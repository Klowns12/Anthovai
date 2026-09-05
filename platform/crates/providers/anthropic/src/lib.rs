//! Anthropic Messages API provider.
//!
//! Endpoint: `POST /v1/messages` with headers `x-api-key` and
//! `anthropic-version: 2023-06-01`. Adaptive thinking is on and depth is
//! controlled with `output_config.effort`, mapped from Anthovai's reasoning
//! level. Nothing above this crate knows any of that.

use std::time::Duration;

use anthovai_inference::types::{
    ChatEvent, ChatRequest, ChatResponse, ChatRole, FinishReason, ProviderCapabilities,
    ProviderError, ProviderId, ReasoningLevel, TokenUsage,
};
use anthovai_inference::ChatProvider;
use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

const API_VERSION: &str = "2023-06-01";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

pub struct AnthropicProvider {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
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

    fn body<'a>(&self, req: &'a ChatRequest, stream: bool) -> MessagesRequest<'a> {
        MessagesRequest {
            model: &req.model,
            max_tokens: req.max_tokens,
            system: &req.system,
            messages: req
                .messages
                .iter()
                .map(|m| WireMessage {
                    role: match m.role {
                        ChatRole::User => "user",
                        ChatRole::Assistant => "assistant",
                    },
                    content: &m.content,
                })
                .collect(),
            stream,
            thinking: Thinking { kind: "adaptive" },
            output_config: OutputConfig {
                effort: effort_for(req.reasoning),
            },
            stop_sequences: req.stop.iter().map(String::as_str).collect(),
            metadata: Metadata {
                user_id: &req.tenant_hash,
            },
        }
    }
}

/// Anthovai's reasoning levels onto Anthropic's effort scale.
fn effort_for(level: ReasoningLevel) -> &'static str {
    match level {
        ReasoningLevel::Fast => "low",
        ReasoningLevel::Balanced => "medium",
        ReasoningLevel::Deep => "high",
    }
}

fn finish_for(stop_reason: Option<&str>) -> FinishReason {
    match stop_reason {
        Some("end_turn") | Some("stop_sequence") => FinishReason::Stop,
        Some("max_tokens") => FinishReason::Length,
        Some("refusal") => FinishReason::Refusal,
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
impl ChatProvider for AnthropicProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            prompt_cache: true,
            vision: true,
            tools: true,
            max_context_tokens: 1_000_000,
        }
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let response = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
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

        let parsed: MessagesResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Decode(e.to_string()))?;

        Ok(ChatResponse {
            text: parsed.text(),
            finish: finish_for(parsed.stop_reason.as_deref()),
            usage: TokenUsage {
                input_tokens: parsed.usage.input_tokens,
                output_tokens: parsed.usage.output_tokens,
                cache_read_tokens: parsed.usage.cache_read_input_tokens.unwrap_or(0),
            },
            model: parsed.model,
            provider_message_id: Some(parsed.id),
        })
    }

    async fn chat_stream(
        &self,
        _req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatEvent, ProviderError>>, ProviderError> {
        // Server-sent event parsing lands with Milestone 7.4; until then the
        // router's streaming filter keeps traffic away from this path.
        Err(ProviderError::BadRequest(
            "streaming is not implemented for the Anthropic provider yet".to_owned(),
        ))
    }
}

// ---- wire types -----------------------------------------------------------

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<WireMessage<'a>>,
    stream: bool,
    thinking: Thinking,
    output_config: OutputConfig,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<&'a str>,
    metadata: Metadata<'a>,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct Thinking {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Serialize)]
struct OutputConfig {
    effort: &'static str,
}

#[derive(Serialize)]
struct Metadata<'a> {
    user_id: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    id: String,
    model: String,
    #[serde(default)]
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
    usage: Usage,
}

impl MessagesResponse {
    /// Text blocks joined; thinking blocks are ignored.
    fn text(&self) -> String {
        self.content
            .iter()
            .filter(|b| b.kind == "text")
            .filter_map(|b| b.text.as_deref())
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_levels_map_onto_effort() {
        assert_eq!(effort_for(ReasoningLevel::Fast), "low");
        assert_eq!(effort_for(ReasoningLevel::Balanced), "medium");
        assert_eq!(effort_for(ReasoningLevel::Deep), "high");
    }

    #[test]
    fn stop_reasons_map_onto_finish_reasons() {
        assert_eq!(finish_for(Some("end_turn")), FinishReason::Stop);
        assert_eq!(finish_for(Some("max_tokens")), FinishReason::Length);
        assert_eq!(finish_for(Some("refusal")), FinishReason::Refusal);
        assert_eq!(finish_for(None), FinishReason::Stop);
        assert!(matches!(
            finish_for(Some("something_new")),
            FinishReason::Other(_)
        ));
    }

    #[test]
    fn http_statuses_map_onto_the_right_error_kinds() {
        assert!(matches!(
            error_for(401, String::new(), None),
            ProviderError::Auth
        ));
        assert!(matches!(
            error_for(400, "bad".into(), None),
            ProviderError::BadRequest(_)
        ));
        assert!(matches!(
            error_for(429, String::new(), Some(Duration::from_secs(3))),
            ProviderError::RateLimited { .. }
        ));
        assert!(matches!(
            error_for(503, String::new(), None),
            ProviderError::Upstream { status: 503, .. }
        ));
    }

    #[test]
    fn only_text_blocks_become_the_answer() {
        let response: MessagesResponse = serde_json::from_value(serde_json::json!({
            "id": "msg_1",
            "model": "claude-sonnet-5",
            "content": [
                {"type": "thinking", "thinking": ""},
                {"type": "text", "text": "12 สัปดาห์"},
                {"type": "text", "text": " [1]"}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 812, "output_tokens": 96}
        }))
        .unwrap();

        assert_eq!(response.text(), "12 สัปดาห์ [1]");
        assert_eq!(response.usage.input_tokens, 812);
    }

    #[test]
    fn the_request_body_carries_adaptive_thinking_and_effort() {
        let provider = AnthropicProvider::new("test-key", None).unwrap();
        let req = ChatRequest {
            model: "claude-sonnet-5".into(),
            system: "sys".into(),
            messages: vec![anthovai_inference::ChatMessage::user("hi")],
            max_tokens: 1024,
            reasoning: ReasoningLevel::Deep,
            stop: vec![],
            tenant_hash: "abc".into(),
            request_id: "req_1".into(),
        };

        let json = serde_json::to_value(provider.body(&req, false)).unwrap();

        assert_eq!(json["thinking"]["type"], "adaptive");
        assert_eq!(json["output_config"]["effort"], "high");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["metadata"]["user_id"], "abc");
        assert!(json.get("stop_sequences").is_none());
    }
}
