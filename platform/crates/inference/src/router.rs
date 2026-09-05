//! The model router: turns a policy plus request facts into an ordered list of
//! candidates, then calls them until one answers.

use std::collections::HashMap;
use std::sync::Arc;

use anthovai_core::{DomainError, Result};
use async_trait::async_trait;
use futures::stream::BoxStream;
use tracing::{debug, warn};

use crate::health::HealthTracker;
use crate::policy::{ModelPolicy, RoutingHints};
use crate::registry::{ModelRegistry, ModelSpec};
use crate::types::{
    ChatEvent, ChatRequest, ChatRequestTemplate, ChatResponse, ProviderCapabilities, ProviderError,
    ProviderId, Tier,
};

#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> ProviderCapabilities;
    async fn chat(&self, req: ChatRequest) -> std::result::Result<ChatResponse, ProviderError>;
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> std::result::Result<
        BoxStream<'static, std::result::Result<ChatEvent, ProviderError>>,
        ProviderError,
    >;
}

/// What actually happened, for the usage record and for debugging.
#[derive(Clone, Debug)]
pub struct RoutedChat {
    pub response: ChatResponse,
    pub model_id: String,
    pub provider: ProviderId,
    pub used_fallback: bool,
    pub attempts: u32,
}

pub struct ModelRouter {
    registry: ModelRegistry,
    providers: HashMap<ProviderId, Arc<dyn ChatProvider>>,
    health: HealthTracker,
}

impl ModelRouter {
    pub fn new(
        registry: ModelRegistry,
        providers: HashMap<ProviderId, Arc<dyn ChatProvider>>,
        health: HealthTracker,
    ) -> Self {
        Self {
            registry,
            providers,
            health,
        }
    }

    pub fn registry(&self) -> &ModelRegistry {
        &self.registry
    }

    /// Every model that could answer a question right now: enabled, with a
    /// configured provider, and with a closed circuit.
    ///
    /// Reported by the readiness endpoint. An empty list means every question
    /// would come back as `provider_unavailable`, which is the one provider
    /// condition worth taking an instance out of rotation for.
    pub fn usable_models(&self) -> Vec<&ModelSpec> {
        self.registry
            .all()
            .iter()
            .filter(|spec| spec.enabled)
            .filter(|spec| self.providers.contains_key(&spec.provider))
            .filter(|spec| self.health.is_usable(&health_key(spec)))
            .collect()
    }

    /// Ordered candidates for this policy: primary first, then fallbacks.
    /// Models whose circuit is open, whose provider is not configured, or whose
    /// context window is too small for this request are filtered out.
    pub fn plan(&self, policy: &ModelPolicy, hints: &RoutingHints) -> Vec<&ModelSpec> {
        let ordered: Vec<&ModelSpec> = match policy {
            ModelPolicy::AnthovaiAuto { reasoning } => {
                self.tier_chain(Tier::for_reasoning(*reasoning), None)
            }
            ModelPolicy::ProviderOnly {
                provider,
                reasoning,
            } => self.tier_chain(Tier::for_reasoning(*reasoning), Some(*provider)),
            ModelPolicy::Custom { primary, fallback } => std::iter::once(primary)
                .chain(fallback.iter())
                .filter_map(|id| self.registry.by_id(id))
                .collect(),
        };

        ordered
            .into_iter()
            .filter(|spec| self.providers.contains_key(&spec.provider))
            .filter(|spec| spec.max_context_tokens >= hints.context_tokens)
            .filter(|spec| {
                !hints.needs_streaming
                    || self
                        .providers
                        .get(&spec.provider)
                        .is_some_and(|p| p.capabilities().streaming)
            })
            .filter(|spec| self.health.is_usable(&health_key(spec)))
            .collect()
    }

    /// Candidates for a tier, then the tiers above it. Climbing lets a long
    /// context find a model that can hold it.
    fn tier_chain(&self, start: Tier, provider: Option<ProviderId>) -> Vec<&ModelSpec> {
        let mut out = Vec::new();
        let mut tier = Some(start);
        while let Some(current) = tier {
            let specs = match provider {
                Some(p) => self.registry.in_tier_for(current, p),
                None => self.registry.in_tier(current),
            };
            out.extend(specs);
            tier = current.next_up();
        }
        out
    }

    /// Try each candidate in order. Within a candidate a retryable failure is
    /// retried once before moving on.
    pub async fn chat(
        &self,
        policy: &ModelPolicy,
        hints: &RoutingHints,
        template: ChatRequestTemplate,
    ) -> Result<RoutedChat> {
        let candidates = self.plan(policy, hints);
        if candidates.is_empty() {
            warn!(?policy, "no candidate model is available for this policy");
            return Err(DomainError::ProviderUnavailable);
        }

        let mut attempts = 0;
        for (index, spec) in candidates.iter().enumerate() {
            let provider = self
                .providers
                .get(&spec.provider)
                .expect("plan() filtered out unconfigured providers");
            let key = health_key(spec);

            for attempt in 0..2 {
                attempts += 1;
                let request = template.clone().with_model(&spec.name);
                let started = std::time::Instant::now();
                let outcome = provider.chat(request).await;

                // Labelled by our model id, never by the provider's name: the
                // registry is what a dashboard is read against, and swapping
                // the underlying model should not break the graph.
                metrics::histogram!(
                    "provider_latency_seconds",
                    "provider" => spec.provider.as_str(),
                    "model" => spec.id.clone(),
                )
                .record(started.elapsed().as_secs_f64());
                metrics::counter!(
                    "provider_requests_total",
                    "provider" => spec.provider.as_str(),
                    "model" => spec.id.clone(),
                    "outcome" => if outcome.is_ok() { "ok" } else { "error" },
                )
                .increment(1);

                match outcome {
                    Ok(response) => {
                        self.health.record_success(&key);
                        return Ok(RoutedChat {
                            response,
                            model_id: spec.id.clone(),
                            provider: spec.provider,
                            used_fallback: index > 0,
                            attempts,
                        });
                    }
                    Err(err) => {
                        if err.counts_against_health() {
                            self.health.record_failure(&key);
                        }
                        debug!(model = %spec.id, attempt, error = %err, "model call failed");
                        if !err.is_retryable() {
                            break;
                        }
                    }
                }
            }
        }

        Err(DomainError::ProviderUnavailable)
    }
}

impl std::fmt::Debug for ModelRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelRouter")
            .field("models", &self.registry.all().len())
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .finish()
    }
}

fn health_key(spec: &ModelSpec) -> String {
    format!("{}:{}", spec.provider, spec.id)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use anthovai_core::Clock;
    use futures::stream;

    use super::*;
    use crate::types::{ChatMessage, FinishReason, ReasoningLevel, TokenUsage};

    struct StubProvider {
        id: ProviderId,
        /// Each call pops one outcome; an empty queue means "succeed".
        outcomes: std::sync::Mutex<Vec<Outcome>>,
        calls: AtomicU32,
    }

    #[derive(Clone, Copy)]
    enum Outcome {
        Ok,
        Retryable,
        Fatal,
    }

    impl StubProvider {
        fn new(id: ProviderId, outcomes: Vec<Outcome>) -> Arc<Self> {
            Arc::new(Self {
                id,
                outcomes: std::sync::Mutex::new(outcomes),
                calls: AtomicU32::new(0),
            })
        }
    }

    #[async_trait]
    impl ChatProvider for StubProvider {
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
            self.calls.fetch_add(1, Ordering::SeqCst);
            let outcome = {
                let mut queue = self.outcomes.lock().unwrap();
                if queue.is_empty() {
                    Outcome::Ok
                } else {
                    queue.remove(0)
                }
            };
            match outcome {
                Outcome::Ok => Ok(ChatResponse {
                    text: format!("answered by {}", req.model),
                    finish: FinishReason::Stop,
                    usage: TokenUsage {
                        input_tokens: 10,
                        output_tokens: 5,
                        cache_read_tokens: 0,
                    },
                    model: req.model,
                    provider_message_id: None,
                }),
                Outcome::Retryable => Err(ProviderError::Upstream {
                    status: 503,
                    body: "down".into(),
                }),
                Outcome::Fatal => Err(ProviderError::BadRequest("nope".into())),
            }
        }

        async fn chat_stream(
            &self,
            _req: ChatRequest,
        ) -> std::result::Result<
            BoxStream<'static, std::result::Result<ChatEvent, ProviderError>>,
            ProviderError,
        > {
            Ok(Box::pin(stream::empty()))
        }
    }

    const MODELS: &str = r#"
[[models]]
id = "openai-medium"
provider = "openai"
name = "openai-m"
tier = "medium"
max_context_tokens = 128000
max_output_tokens = 16000
input_price_micro_per_mtok = 1000000
output_price_micro_per_mtok = 4000000
priority = 0

[[models]]
id = "claude-medium"
provider = "anthropic"
name = "claude-sonnet-5"
tier = "medium"
max_context_tokens = 1000000
max_output_tokens = 64000
input_price_micro_per_mtok = 2000000
output_price_micro_per_mtok = 10000000
priority = 1

[[models]]
id = "claude-large"
provider = "anthropic"
name = "claude-opus-5"
tier = "large"
max_context_tokens = 1000000
max_output_tokens = 64000
input_price_micro_per_mtok = 5000000
output_price_micro_per_mtok = 25000000
priority = 0
"#;

    fn template() -> ChatRequestTemplate {
        ChatRequestTemplate {
            system: "you are a test".into(),
            messages: vec![ChatMessage::user("hello")],
            max_tokens: 1024,
            reasoning: ReasoningLevel::Balanced,
            stop: vec![],
            tenant_hash: "hash".into(),
            request_id: "req_test".into(),
        }
    }

    fn router_with(providers: Vec<(ProviderId, Arc<dyn ChatProvider>)>) -> ModelRouter {
        ModelRouter::new(
            ModelRegistry::from_toml(MODELS).unwrap(),
            providers.into_iter().collect(),
            HealthTracker::new(Clock::system()),
        )
    }

    #[test]
    fn auto_policy_prefers_the_matching_tier_then_climbs() {
        let router = router_with(vec![
            (
                ProviderId::OpenAi,
                StubProvider::new(ProviderId::OpenAi, vec![]),
            ),
            (
                ProviderId::Anthropic,
                StubProvider::new(ProviderId::Anthropic, vec![]),
            ),
        ]);
        let plan = router.plan(
            &ModelPolicy::AnthovaiAuto {
                reasoning: ReasoningLevel::Balanced,
            },
            &RoutingHints::new(ReasoningLevel::Balanced, 1_000),
        );
        let ids: Vec<&str> = plan.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["openai-medium", "claude-medium", "claude-large"]);
    }

    #[test]
    fn a_long_context_skips_models_that_cannot_hold_it() {
        let router = router_with(vec![
            (
                ProviderId::OpenAi,
                StubProvider::new(ProviderId::OpenAi, vec![]),
            ),
            (
                ProviderId::Anthropic,
                StubProvider::new(ProviderId::Anthropic, vec![]),
            ),
        ]);
        let plan = router.plan(
            &ModelPolicy::default(),
            &RoutingHints::new(ReasoningLevel::Balanced, 500_000),
        );
        let ids: Vec<&str> = plan.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["claude-medium", "claude-large"]);
    }

    #[test]
    fn pinning_a_provider_excludes_the_other_one() {
        let router = router_with(vec![
            (
                ProviderId::OpenAi,
                StubProvider::new(ProviderId::OpenAi, vec![]),
            ),
            (
                ProviderId::Anthropic,
                StubProvider::new(ProviderId::Anthropic, vec![]),
            ),
        ]);
        let plan = router.plan(
            &ModelPolicy::ProviderOnly {
                provider: ProviderId::Anthropic,
                reasoning: ReasoningLevel::Balanced,
            },
            &RoutingHints::new(ReasoningLevel::Balanced, 1_000),
        );
        assert!(plan.iter().all(|s| s.provider == ProviderId::Anthropic));
    }

    #[test]
    fn unconfigured_providers_are_never_planned() {
        let router = router_with(vec![(
            ProviderId::Anthropic,
            StubProvider::new(ProviderId::Anthropic, vec![]),
        )]);
        let plan = router.plan(
            &ModelPolicy::default(),
            &RoutingHints::new(ReasoningLevel::Balanced, 1_000),
        );
        assert!(plan.iter().all(|s| s.provider == ProviderId::Anthropic));
    }

    #[tokio::test]
    async fn falls_over_to_the_next_provider_when_the_first_is_down() {
        let openai = StubProvider::new(
            ProviderId::OpenAi,
            vec![Outcome::Retryable, Outcome::Retryable],
        );
        let anthropic = StubProvider::new(ProviderId::Anthropic, vec![]);
        let router = router_with(vec![
            (ProviderId::OpenAi, openai.clone()),
            (ProviderId::Anthropic, anthropic.clone()),
        ]);

        let routed = router
            .chat(
                &ModelPolicy::default(),
                &RoutingHints::new(ReasoningLevel::Balanced, 1_000),
                template(),
            )
            .await
            .expect("the fallback provider should answer");

        assert_eq!(routed.provider, ProviderId::Anthropic);
        assert!(routed.used_fallback);
        // Two attempts on the primary, one on the fallback.
        assert_eq!(openai.calls.load(Ordering::SeqCst), 2);
        assert_eq!(anthropic.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_fatal_error_is_not_retried_on_the_same_model() {
        let openai = StubProvider::new(ProviderId::OpenAi, vec![Outcome::Fatal]);
        let anthropic = StubProvider::new(ProviderId::Anthropic, vec![]);
        let router = router_with(vec![
            (ProviderId::OpenAi, openai.clone()),
            (ProviderId::Anthropic, anthropic.clone()),
        ]);

        router
            .chat(
                &ModelPolicy::default(),
                &RoutingHints::new(ReasoningLevel::Balanced, 1_000),
                template(),
            )
            .await
            .unwrap();

        assert_eq!(openai.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_pinned_provider_that_is_down_returns_unavailable() {
        let openai = StubProvider::new(
            ProviderId::OpenAi,
            vec![Outcome::Retryable, Outcome::Retryable],
        );
        let anthropic = StubProvider::new(ProviderId::Anthropic, vec![]);
        let router = router_with(vec![
            (ProviderId::OpenAi, openai),
            (ProviderId::Anthropic, anthropic.clone()),
        ]);

        let err = router
            .chat(
                &ModelPolicy::ProviderOnly {
                    provider: ProviderId::OpenAi,
                    reasoning: ReasoningLevel::Balanced,
                },
                &RoutingHints::new(ReasoningLevel::Balanced, 1_000),
                template(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::ProviderUnavailable));
        // The customer pinned OpenAI, so Anthropic must not have been called.
        assert_eq!(anthropic.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn no_candidates_means_provider_unavailable() {
        let router = router_with(vec![]);
        let err = router
            .chat(
                &ModelPolicy::default(),
                &RoutingHints::new(ReasoningLevel::Balanced, 1_000),
                template(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::ProviderUnavailable));
    }
}
