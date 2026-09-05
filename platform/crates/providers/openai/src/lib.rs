//! OpenAI provider: chat completions and embeddings.
//!
//! This is the only crate that serves both traits, which is why embeddings and
//! chat are separate abstractions in the first place.

use std::time::Duration;

use anthovai_core::{DomainError, Result};
use anthovai_embeddings::EmbeddingProvider;
use anthovai_inference::types::{
    ChatEvent, ChatRequest, ChatResponse, ChatRole, FinishReason, ProviderCapabilities,
    ProviderError, ProviderId, TokenUsage,
};
use anthovai_inference::ChatProvider;
use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

pub struct OpenAiProvider {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenAiProvider {
    pub fn new(
        api_key: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<Self, ProviderError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self {
            http,
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_owned()),
        })
    }

    fn body<'a>(&self, req: &'a ChatRequest, stream: bool) -> ChatCompletionRequest<'a> {
        let mut messages = Vec::with_capacity(req.messages.len() + 1);
        if !req.system.is_empty() {
            messages.push(WireMessage {
                role: "system",
                content: &req.system,
            });
        }
        messages.extend(req.messages.iter().map(|m| WireMessage {
            role: match m.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
            },
            content: &m.content,
        }));

        ChatCompletionRequest {
            model: &req.model,
            max_completion_tokens: req.max_tokens,
            messages,
            stream,
            stop: req.stop.iter().map(String::as_str).collect(),
            user: &req.tenant_hash,
        }
    }
}

fn finish_for(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("stop") => FinishReason::Stop,
        Some("length") => FinishReason::Length,
        Some("content_filter") => FinishReason::ContentFilter,
        Some(other) => FinishReason::Other(other.to_owned()),
        None => FinishReason::Stop,
    }
}

fn error_for(status: u16, body: String, retry_after: Option<Duration>) -> ProviderError {
    match status {
        401 | 403 => ProviderError::Auth,
        400 | 404 | 422 => ProviderError::BadRequest(body),
        429 => ProviderError::RateLimited { retry_after },
        _ => ProviderError::Upstream { status, body },
    }
}

#[async_trait]
impl ChatProvider for OpenAiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenAi
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            prompt_cache: true,
            vision: true,
            tools: true,
            max_context_tokens: 128_000,
        }
    }

    async fn chat(&self, req: ChatRequest) -> std::result::Result<ChatResponse, ProviderError> {
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&self.body(&req, false))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Transport(e)
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(Duration::from_secs);
            let body = response.text().await.unwrap_or_default();
            return Err(error_for(status.as_u16(), body, retry_after));
        }

        let parsed: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Decode(e.to_string()))?;

        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::Decode("response contained no choices".into()))?;

        Ok(ChatResponse {
            text: choice.message.content.unwrap_or_default(),
            finish: finish_for(choice.finish_reason.as_deref()),
            usage: TokenUsage {
                input_tokens: parsed.usage.prompt_tokens,
                output_tokens: parsed.usage.completion_tokens,
                cache_read_tokens: 0,
            },
            model: parsed.model,
            provider_message_id: Some(parsed.id),
        })
    }

    async fn chat_stream(
        &self,
        _req: ChatRequest,
    ) -> std::result::Result<
        BoxStream<'static, std::result::Result<ChatEvent, ProviderError>>,
        ProviderError,
    > {
        Err(ProviderError::BadRequest(
            "streaming is not implemented for the OpenAI provider yet".to_owned(),
        ))
    }
}

pub struct OpenAiEmbeddings {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
    model_id: String,
    dimension: usize,
}

impl OpenAiEmbeddings {
    pub fn new(
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
        dimension: usize,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| DomainError::Internal(e.into()))?;
        let model = model.into();
        Ok(Self {
            http,
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_owned()),
            model_id: format!("openai:{model}"),
            model,
            dimension,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbeddings {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let response = self
            .http
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&EmbeddingRequest {
                model: &self.model,
                input: inputs,
                dimensions: self.dimension,
            })
            .send()
            .await
            .map_err(|e| DomainError::Internal(e.into()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::Internal(anyhow::anyhow!(
                "embedding request failed with {status}: {body}"
            )));
        }

        let parsed: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Internal(e.into()))?;

        // The API may return items out of order; `index` is authoritative.
        let mut data = parsed.data;
        data.sort_by_key(|d| d.index);
        Ok(data.into_iter().map(|d| d.embedding).collect())
    }
}

// ---- wire types -----------------------------------------------------------

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    max_completion_tokens: u32,
    messages: Vec<WireMessage<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<&'a str>,
    user: &'a str,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    id: String,
    model: String,
    choices: Vec<Choice>,
    usage: Usage,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
    dimensions: usize,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_prompt_becomes_the_first_message() {
        let provider = OpenAiProvider::new("k", None).unwrap();
        let req = ChatRequest {
            model: "m".into(),
            system: "you are a test".into(),
            messages: vec![anthovai_inference::ChatMessage::user("hi")],
            max_tokens: 100,
            reasoning: anthovai_inference::ReasoningLevel::Balanced,
            stop: vec![],
            tenant_hash: "hash".into(),
            request_id: "req".into(),
        };

        let json = serde_json::to_value(provider.body(&req, false)).unwrap();

        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][1]["role"], "user");
        assert_eq!(json["user"], "hash");
    }

    #[test]
    fn finish_reasons_map_across() {
        assert_eq!(finish_for(Some("stop")), FinishReason::Stop);
        assert_eq!(finish_for(Some("length")), FinishReason::Length);
        assert_eq!(
            finish_for(Some("content_filter")),
            FinishReason::ContentFilter
        );
    }

    #[test]
    fn embeddings_are_reordered_by_index() {
        let parsed: EmbeddingResponse = serde_json::from_value(serde_json::json!({
            "data": [
                {"index": 1, "embedding": [0.2]},
                {"index": 0, "embedding": [0.1]}
            ]
        }))
        .unwrap();

        let mut data = parsed.data;
        data.sort_by_key(|d| d.index);
        assert_eq!(data[0].embedding, vec![0.1]);
        assert_eq!(data[1].embedding, vec![0.2]);
    }

    #[test]
    fn the_model_id_is_namespaced() {
        let embeddings = OpenAiEmbeddings::new("k", None, "text-embedding-3-small", 1536).unwrap();
        assert_eq!(embeddings.model_id(), "openai:text-embedding-3-small");
        assert_eq!(embeddings.dimension(), 1536);
    }
}
