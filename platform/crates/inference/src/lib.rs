//! Model abstraction and routing.
//!
//! This crate owns the boundary between Anthovai's own contract and the model
//! vendors. Nothing above it knows that OpenAI or Anthropic exist.

pub mod echo;
pub mod health;
pub mod policy;
pub mod registry;
pub mod router;
pub mod types;

pub use echo::{is_echo_model, EchoProvider, ECHO_MODEL};
pub use health::{CircuitState, HealthTracker};
pub use policy::{ModelPolicy, RoutingHints};
pub use registry::{ModelRegistry, ModelSpec};
pub use router::{ChatProvider, ModelRouter, RoutedChat};
pub use types::{
    ChatEvent, ChatMessage, ChatRequest, ChatRequestTemplate, ChatResponse, ChatRole, FinishReason,
    ProviderCapabilities, ProviderError, ProviderId, ReasoningLevel, Tier, TokenUsage,
};
