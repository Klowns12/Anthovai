//! Provider-agnostic chat types. Nothing here mentions a specific vendor: the
//! public API contract is Anthovai's, so providers map onto these, never the
//! other way round.

use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    OpenAi,
    Anthropic,
}

impl ProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ProviderId {
    type Err = anthovai_core::DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openai" => Ok(Self::OpenAi),
            "anthropic" => Ok(Self::Anthropic),
            other => Err(anthovai_core::DomainError::validation(format!(
                "unknown provider `{other}`"
            ))),
        }
    }
}

/// How hard the model should think. Providers map this onto their own control
/// (Anthropic: `output_config.effort`; OpenAI: its reasoning control).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningLevel {
    Fast,
    #[default]
    Balanced,
    Deep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Small,
    Medium,
    Large,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Small => "small",
            Tier::Medium => "medium",
            Tier::Large => "large",
        }
    }

    pub fn for_reasoning(level: ReasoningLevel) -> Self {
        match level {
            ReasoningLevel::Fast => Tier::Small,
            ReasoningLevel::Balanced => Tier::Medium,
            ReasoningLevel::Deep => Tier::Large,
        }
    }

    /// Ordering used when a request needs a bigger context window than the
    /// chosen tier offers.
    pub fn next_up(self) -> Option<Self> {
        match self {
            Tier::Small => Some(Tier::Medium),
            Tier::Medium => Some(Tier::Large),
            Tier::Large => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }
}

/// Everything except the model name, which the router fills in once it has
/// picked a candidate.
#[derive(Clone, Debug)]
pub struct ChatRequestTemplate {
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub reasoning: ReasoningLevel,
    pub stop: Vec<String>,
    /// Opaque per-tenant identifier passed to providers for abuse tracking.
    /// Must be a hash, never a raw tenant id.
    pub tenant_hash: String,
    pub request_id: String,
}

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub reasoning: ReasoningLevel,
    pub stop: Vec<String>,
    pub tenant_hash: String,
    pub request_id: String,
}

impl ChatRequestTemplate {
    pub fn with_model(self, model: impl Into<String>) -> ChatRequest {
        ChatRequest {
            model: model.into(),
            system: self.system,
            messages: self.messages,
            max_tokens: self.max_tokens,
            reasoning: self.reasoning,
            stop: self.stop,
            tenant_hash: self.tenant_hash,
            request_id: self.request_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_read_tokens: u32,
}

impl TokenUsage {
    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    /// The provider declined the request. Treated as a completed turn, not an error.
    Refusal,
    Other(String),
}

#[derive(Clone, Debug)]
pub struct ChatResponse {
    pub text: String,
    pub finish: FinishReason,
    pub usage: TokenUsage,
    /// The provider-side model name that actually answered.
    pub model: String,
    pub provider_message_id: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ChatEvent {
    Start { model: String },
    TextDelta(String),
    Usage(TokenUsage),
    Done(FinishReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub prompt_cache: bool,
    pub vision: bool,
    pub tools: bool,
    pub max_context_tokens: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("rate limited by provider")]
    RateLimited { retry_after: Option<Duration> },

    #[error("provider returned {status}")]
    Upstream { status: u16, body: String },

    #[error("provider request timed out")]
    Timeout,

    /// Our request was wrong. Retrying or failing over will not help.
    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("provider rejected our credentials")]
    Auth,

    #[error("could not decode the provider response: {0}")]
    Decode(String),

    #[error(transparent)]
    Transport(#[from] reqwest::Error),
}

impl ProviderError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::RateLimited { .. } | Self::Timeout | Self::Transport(_) => true,
            Self::Upstream { status, .. } => *status >= 500,
            Self::BadRequest(_) | Self::Auth | Self::Decode(_) => false,
        }
    }

    /// Whether this failure should count against the provider's health, i.e.
    /// whether it says anything about the provider rather than about us.
    ///
    /// `BadRequest` and `Auth` are both ours: a model name we got wrong, a
    /// payload we built wrong, a key that was revoked or never set. Counting
    /// them would open the circuit on a model that is working perfectly, and —
    /// worse — would make the health numbers say "this model is down" to
    /// whoever reads them during an incident, sending them after a provider
    /// outage that is not happening.
    ///
    /// Failing over does not help either: with a bad key, every model of that
    /// provider fails identically and just as fast.
    pub fn counts_against_health(&self) -> bool {
        !matches!(self, Self::BadRequest(_) | Self::Auth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_own_mistakes_do_not_open_a_circuit() {
        // The circuit breaker routes around a failing model. Neither of these
        // says anything about the model.
        assert!(!ProviderError::BadRequest("bad schema".into()).counts_against_health());
        assert!(!ProviderError::Auth.counts_against_health());

        // These do.
        assert!(ProviderError::Timeout.counts_against_health());
        assert!(ProviderError::RateLimited { retry_after: None }.counts_against_health());
        assert!(ProviderError::Upstream {
            status: 503,
            body: String::new()
        }
        .counts_against_health());
        // A response we cannot parse is the provider changing under us.
        assert!(ProviderError::Decode("unexpected shape".into()).counts_against_health());
    }

    #[test]
    fn bad_requests_are_not_retried() {
        assert!(!ProviderError::BadRequest("bad schema".into()).is_retryable());
        assert!(!ProviderError::Auth.is_retryable());
    }

    #[test]
    fn server_errors_are_retried_but_client_errors_are_not() {
        assert!(ProviderError::Upstream {
            status: 503,
            body: String::new()
        }
        .is_retryable());
        assert!(!ProviderError::Upstream {
            status: 400,
            body: String::new()
        }
        .is_retryable());
    }

    #[test]
    fn tiers_climb_for_larger_contexts() {
        assert_eq!(Tier::Small.next_up(), Some(Tier::Medium));
        assert_eq!(Tier::Large.next_up(), None);
    }
}
