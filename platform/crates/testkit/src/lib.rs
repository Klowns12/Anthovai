//! Test doubles and test-database plumbing.
//!
//! Integration tests must not call a paid provider: the answers would not be
//! deterministic and the bill would grow with the test suite. These fakes are
//! deterministic and free.
//!
//! The database is the opposite case — see [`db`] for why those tests use a
//! real PostgreSQL rather than a mock.

pub mod db;

use anthovai_core::Result;
use anthovai_embeddings::EmbeddingProvider;
use anthovai_inference::types::{
    ChatEvent, ChatRequest, ChatResponse, FinishReason, ProviderCapabilities, ProviderError,
    ProviderId, TokenUsage,
};
use anthovai_inference::ChatProvider;
use async_trait::async_trait;
use futures::stream::{self, BoxStream};

/// A chat provider that answers from the prompt it was given, so a test can
/// assert on what the pipeline actually assembled.
pub struct FakeChatProvider {
    id: ProviderId,
    /// When set, every call returns this instead of the echo.
    canned: Option<String>,
}

impl FakeChatProvider {
    pub fn new(id: ProviderId) -> Self {
        Self { id, canned: None }
    }

    pub fn answering(id: ProviderId, answer: impl Into<String>) -> Self {
        Self {
            id,
            canned: Some(answer.into()),
        }
    }
}

#[async_trait]
impl ChatProvider for FakeChatProvider {
    fn id(&self) -> ProviderId {
        self.id
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            prompt_cache: false,
            vision: false,
            tools: false,
            max_context_tokens: 1_000_000,
        }
    }

    async fn chat(&self, req: ChatRequest) -> std::result::Result<ChatResponse, ProviderError> {
        let text = self.canned.clone().unwrap_or_else(|| {
            let question = req
                .messages
                .last()
                .map(|m| m.content.as_str())
                .unwrap_or_default();
            format!("echo: {question}")
        });

        Ok(ChatResponse {
            usage: TokenUsage {
                input_tokens: (req.system.len() / 4) as u32,
                output_tokens: (text.len() / 4) as u32,
                cache_read_tokens: 0,
            },
            text,
            finish: FinishReason::Stop,
            model: req.model,
            provider_message_id: Some("msg_fake".into()),
        })
    }

    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> std::result::Result<
        BoxStream<'static, std::result::Result<ChatEvent, ProviderError>>,
        ProviderError,
    > {
        let response = self.chat(req).await?;
        let events = vec![
            Ok(ChatEvent::Start {
                model: response.model.clone(),
            }),
            Ok(ChatEvent::TextDelta(response.text)),
            Ok(ChatEvent::Usage(response.usage)),
            Ok(ChatEvent::Done(FinishReason::Stop)),
        ];
        Ok(Box::pin(stream::iter(events)))
    }
}

/// Embeddings derived from the text itself: same text, same vector, and texts
/// that share words land near each other. Enough to exercise retrieval ranking
/// without a network call.
pub struct FakeEmbeddingProvider {
    dimension: usize,
    model_id: String,
}

impl FakeEmbeddingProvider {
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            model_id: format!("fake:hash-{dimension}"),
        }
    }
}

impl Default for FakeEmbeddingProvider {
    fn default() -> Self {
        Self::new(1536)
    }
}

#[async_trait]
impl EmbeddingProvider for FakeEmbeddingProvider {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(inputs.iter().map(|t| embed(t, self.dimension)).collect())
    }
}

/// A bag-of-words vector: each word contributes to one dimension, then the
/// vector is normalised so cosine similarity behaves.
fn embed(text: &str, dimension: usize) -> Vec<f32> {
    let mut vector = vec![0.0_f32; dimension];
    for word in text.to_lowercase().split_whitespace() {
        let mut hash: u64 = 1469598103934665603;
        for byte in word.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(1099511628211);
        }
        vector[(hash as usize) % dimension] += 1.0;
    }
    let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(question: &str) -> ChatRequest {
        ChatRequest {
            model: "fake-model".into(),
            system: "system prompt".into(),
            messages: vec![anthovai_inference::ChatMessage::user(question)],
            max_tokens: 256,
            reasoning: anthovai_inference::ReasoningLevel::Balanced,
            stop: vec![],
            tenant_hash: "hash".into(),
            request_id: "req_1".into(),
        }
    }

    #[tokio::test]
    async fn the_fake_provider_echoes_the_question() {
        let provider = FakeChatProvider::new(ProviderId::OpenAi);
        let response = provider
            .chat(request("how long is the course?"))
            .await
            .unwrap();
        assert!(response.text.contains("how long is the course?"));
        assert_eq!(response.finish, FinishReason::Stop);
    }

    #[tokio::test]
    async fn a_canned_answer_overrides_the_echo() {
        let provider = FakeChatProvider::answering(ProviderId::Anthropic, "12 weeks [1]");
        let response = provider.chat(request("anything")).await.unwrap();
        assert_eq!(response.text, "12 weeks [1]");
    }

    #[tokio::test]
    async fn streaming_ends_with_a_done_event() {
        use futures::StreamExt;

        let provider = FakeChatProvider::answering(ProviderId::OpenAi, "hello");
        let events: Vec<_> = provider
            .chat_stream(request("hi"))
            .await
            .unwrap()
            .collect()
            .await;

        assert!(matches!(events.first(), Some(Ok(ChatEvent::Start { .. }))));
        assert!(matches!(events.last(), Some(Ok(ChatEvent::Done(_)))));
    }

    #[tokio::test]
    async fn embeddings_are_deterministic() {
        let provider = FakeEmbeddingProvider::new(64);
        let first = provider.embed_one("rust course").await.unwrap();
        let second = provider.embed_one("rust course").await.unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[tokio::test]
    async fn similar_text_embeds_more_closely_than_unrelated_text() {
        use anthovai_retrieval::cosine_similarity;

        let provider = FakeEmbeddingProvider::new(256);
        let query = provider.embed_one("rust course duration").await.unwrap();
        let related = provider
            .embed_one("the rust course duration is 12 weeks")
            .await
            .unwrap();
        let unrelated = provider
            .embed_one("cafeteria menu and opening hours")
            .await
            .unwrap();

        assert!(
            cosine_similarity(&query, &related) > cosine_similarity(&query, &unrelated),
            "related text should score higher"
        );
    }

    #[tokio::test]
    async fn a_batch_returns_one_vector_per_input() {
        let provider = FakeEmbeddingProvider::new(32);
        let vectors = provider
            .embed_batch(&["a".to_owned(), "b".to_owned(), "c".to_owned()])
            .await
            .unwrap();
        assert_eq!(vectors.len(), 3);
    }
}
