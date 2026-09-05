//! A chat provider with no model behind it.
//!
//! Like the hash embedder, this is not only for tests: a developer with no
//! provider key can run the whole platform and see the shape of a real answer —
//! retrieval, citations, usage, conversation history — without an account or a
//! bill. What it cannot do is tell you whether the *answers* are any good.
//!
//! It answers from the retrieved passages rather than inventing prose, so a
//! broken prompt or a lost citation shows up immediately instead of hiding
//! behind plausible-sounding filler.

use async_trait::async_trait;
use futures::stream::{self, BoxStream};

use crate::types::{
    ChatEvent, ChatRequest, ChatResponse, FinishReason, ProviderCapabilities, ProviderError,
    ProviderId, TokenUsage,
};
use crate::ChatProvider;

/// Namespace for models produced here, so an answer can never be mistaken for
/// one a real model gave.
pub const ECHO_MODEL: &str = "echo:local";

pub struct EchoProvider {
    id: ProviderId,
    /// When set, every call returns this instead of quoting the knowledge.
    canned: Option<String>,
}

impl EchoProvider {
    pub fn new() -> Self {
        Self {
            id: ProviderId::Anthropic,
            canned: None,
        }
    }

    /// Always answer with this. For tests that need to control the answer —
    /// to check citation parsing, say.
    pub fn answering(answer: impl Into<String>) -> Self {
        Self {
            id: ProviderId::Anthropic,
            canned: Some(answer.into()),
        }
    }

    pub fn as_provider(self, id: ProviderId) -> Self {
        Self { id, ..self }
    }
}

impl Default for EchoProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether an answer came from here rather than a real model.
pub fn is_echo_model(model: &str) -> bool {
    model.starts_with("echo:")
}

#[async_trait]
impl ChatProvider for EchoProvider {
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

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let text = self
            .canned
            .clone()
            .unwrap_or_else(|| answer_from_knowledge(&req));

        Ok(ChatResponse {
            usage: TokenUsage {
                input_tokens: rough_tokens(&req.system)
                    + req
                        .messages
                        .iter()
                        .map(|m| rough_tokens(&m.content))
                        .sum::<u32>(),
                output_tokens: rough_tokens(&text),
                cache_read_tokens: 0,
            },
            text,
            finish: FinishReason::Stop,
            model: ECHO_MODEL.to_owned(),
            provider_message_id: None,
        })
    }

    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatEvent, ProviderError>>, ProviderError> {
        let response = self.chat(req).await?;

        // Word by word, so a client rendering a stream has something to render.
        let mut events = vec![Ok(ChatEvent::Start {
            model: response.model.clone(),
        })];
        for word in response.text.split_inclusive(' ') {
            events.push(Ok(ChatEvent::TextDelta(word.to_owned())));
        }
        events.push(Ok(ChatEvent::Usage(response.usage)));
        events.push(Ok(ChatEvent::Done(FinishReason::Stop)));

        Ok(Box::pin(stream::iter(events)))
    }
}

/// Quote the first retrieved passage and cite it.
///
/// Answering from the knowledge — rather than echoing the question — is what
/// makes this useful in development: if retrieval found the wrong passage, the
/// answer is visibly wrong.
fn answer_from_knowledge(req: &ChatRequest) -> String {
    let question = req
        .messages
        .last()
        .map(|m| m.content.as_str())
        .unwrap_or_default();

    match first_passage(&req.system) {
        Some(passage) => format!(
            "(local echo, no model) In answer to \"{}\": {} [1]",
            truncate(question, 80),
            truncate(&passage, 300)
        ),
        None => format!(
            "(local echo, no model) I have no knowledge to answer \"{}\" from.",
            truncate(question, 80)
        ),
    }
}

/// The text of the first `<source>` in the knowledge block.
fn first_passage(system: &str) -> Option<String> {
    let after_open = system.find("<source ")?;
    let content_start = system[after_open..].find('>')? + after_open + 1;
    let content_end = system[content_start..].find("</source>")? + content_start;

    let passage = system[content_start..content_end].trim();
    (!passage.is_empty()).then(|| passage.to_owned())
}

fn truncate(text: &str, max_chars: usize) -> String {
    let taken: String = text.chars().take(max_chars).collect();
    if taken.chars().count() < text.chars().count() {
        format!("{taken}…")
    } else {
        taken
    }
}

/// Enough for a usage record to be non-zero and roughly proportionate.
fn rough_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    words.max(chars / 4) as u32
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;
    use crate::types::{ChatMessage, ReasoningLevel};

    fn request(system: &str, question: &str) -> ChatRequest {
        ChatRequest {
            model: "echo".into(),
            system: system.into(),
            messages: vec![ChatMessage::user(question)],
            max_tokens: 1024,
            reasoning: ReasoningLevel::Balanced,
            stop: vec![],
            tenant_hash: "hash".into(),
            request_id: "req_1".into(),
        }
    }

    const KNOWLEDGE: &str = "Rules:\n- Answer from the knowledge.\n\n\
<knowledge>\n\
<source n=\"1\" doc=\"handbook.md\">\nThe Rust course runs for twelve weeks.\n</source>\n\
<source n=\"2\" doc=\"handbook.md\">\nThe cafeteria opens at seven.\n</source>\n\
</knowledge>";

    #[tokio::test]
    async fn it_answers_from_the_retrieved_passage() {
        let response = EchoProvider::new()
            .chat(request(KNOWLEDGE, "how long is the Rust course?"))
            .await
            .unwrap();

        assert!(response.text.contains("twelve weeks"));
        assert!(
            response.text.contains("[1]"),
            "it should cite, so the citation pipeline is exercised"
        );
    }

    #[tokio::test]
    async fn the_answer_says_it_is_not_a_model() {
        let response = EchoProvider::new()
            .chat(request(KNOWLEDGE, "anything"))
            .await
            .unwrap();

        assert!(response.text.contains("local echo"));
        assert!(is_echo_model(&response.model));
        assert!(!is_echo_model("claude-sonnet-5"));
    }

    #[tokio::test]
    async fn with_no_knowledge_it_says_so_rather_than_inventing() {
        let response = EchoProvider::new()
            .chat(request("Rules:\n<knowledge>\n</knowledge>", "anything"))
            .await
            .unwrap();

        assert!(response.text.contains("no knowledge"));
        assert!(!response.text.contains("[1]"));
    }

    #[tokio::test]
    async fn a_canned_answer_overrides_the_quote() {
        let response = EchoProvider::answering("12 weeks [2]")
            .chat(request(KNOWLEDGE, "anything"))
            .await
            .unwrap();

        assert_eq!(response.text, "12 weeks [2]");
    }

    #[tokio::test]
    async fn usage_is_reported_and_not_zero() {
        let response = EchoProvider::new()
            .chat(request(KNOWLEDGE, "how long is the Rust course?"))
            .await
            .unwrap();

        assert!(response.usage.input_tokens > 0);
        assert!(response.usage.output_tokens > 0);
    }

    #[tokio::test]
    async fn streaming_starts_delivers_and_finishes() {
        let events: Vec<_> = EchoProvider::new()
            .chat_stream(request(KNOWLEDGE, "how long?"))
            .await
            .unwrap()
            .collect()
            .await;

        assert!(matches!(events.first(), Some(Ok(ChatEvent::Start { .. }))));
        assert!(matches!(events.last(), Some(Ok(ChatEvent::Done(_)))));

        let streamed: String = events
            .iter()
            .filter_map(|e| match e {
                Ok(ChatEvent::TextDelta(text)) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(streamed.contains("twelve weeks"));
    }

    #[test]
    fn a_malformed_knowledge_block_is_not_a_panic() {
        assert_eq!(first_passage("no knowledge here"), None);
        assert_eq!(first_passage("<source n=\"1\">unterminated"), None);
        assert_eq!(first_passage("<source n=\"1\"></source>"), None);
    }
}
