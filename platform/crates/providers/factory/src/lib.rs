//! Choosing providers from configuration.
//!
//! This is the one place that knows both the provider implementations and the
//! settings, so the API and the worker make the same choice from the same
//! rules — including the important one: **a production deployment with no
//! provider key must not start.** Falling back to a fake embedder there would
//! fill a customer's knowledge base with vectors that mean nothing, and the
//! only symptom would be retrieval that never quite works.

use std::collections::HashMap;
use std::sync::Arc;

use anthovai_core::config::{EmbeddingSettings, ProviderSettings};
use anthovai_core::Clock;
use anthovai_embeddings::{EmbeddingProvider, HashEmbedder};
use anthovai_inference::{
    ChatProvider, EchoProvider, HealthTracker, ModelRegistry, ModelRouter, ProviderId,
};
use anthovai_provider_anthropic::AnthropicProvider;
use anthovai_provider_openai::{OpenAiEmbeddings, OpenAiProvider};
use tracing::{info, warn};

/// Which deployment this is. Only production refuses to run without real keys;
/// everywhere else a developer should be able to work offline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Environment {
    Local,
    Staging,
    Production,
}

impl Environment {
    /// From `ANTHOVAI_ENV`, defaulting to local.
    pub fn from_env() -> Self {
        match std::env::var("ANTHOVAI_ENV").unwrap_or_default().as_str() {
            "production" | "prod" => Self::Production,
            "staging" => Self::Staging,
            _ => Self::Local,
        }
    }

    fn allows_fakes(self) -> bool {
        !matches!(self, Self::Production)
    }

    /// For the callers that gate on production without caring about fakes —
    /// durable storage, say, which staging can reasonably do without.
    pub fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

/// The embedding provider this deployment should use.
///
/// The knowledge base records whichever model this returns, so a base built
/// against the hash embedder is marked as such and can be found and re-embedded
/// once a key is configured.
pub fn embedding_provider(
    providers: &ProviderSettings,
    embeddings: &EmbeddingSettings,
    environment: Environment,
) -> anyhow::Result<Arc<dyn EmbeddingProvider>> {
    let openai = providers.openai.as_ref().filter(|p| p.enabled);
    let key = openai.and_then(|p| p.api_key());

    match (openai, key) {
        (Some(config), Some(key)) => {
            let model = model_name(&embeddings.default_model);
            let provider = OpenAiEmbeddings::new(
                key,
                Some(config.base_url.clone()),
                model,
                embeddings.dimension,
            )?;
            info!(model = %provider.model_id(), "embeddings ready");
            Ok(Arc::new(provider))
        }

        _ if environment.allows_fakes() => {
            warn!(
                "no embedding provider is configured: using deterministic local \
                 embeddings. Retrieval will work but its quality means nothing. \
                 Set OPENAI_API_KEY to use real embeddings."
            );
            Ok(Arc::new(HashEmbedder::new(embeddings.dimension)))
        }

        _ => anyhow::bail!(
            "no embedding provider is configured. Set OPENAI_API_KEY, or run with \
             ANTHOVAI_ENV unset to use local embeddings for development."
        ),
    }
}

/// The model router this deployment should use.
///
/// A provider with no key is left out rather than registered and failed on
/// first use, so the router's own fallback logic sees an honest picture of what
/// is available. If that leaves nothing, development gets the local echo
/// provider and production refuses to start.
pub fn chat_router(
    providers: &ProviderSettings,
    registry: ModelRegistry,
    clock: Clock,
    environment: Environment,
) -> anyhow::Result<ModelRouter> {
    let mut configured: HashMap<ProviderId, Arc<dyn ChatProvider>> = HashMap::new();

    if let Some(config) = providers.anthropic.as_ref().filter(|p| p.enabled) {
        if let Some(key) = config.api_key() {
            configured.insert(
                ProviderId::Anthropic,
                Arc::new(AnthropicProvider::new(key, Some(config.base_url.clone()))?),
            );
            info!("Anthropic ready");
        }
    }

    if let Some(config) = providers.openai.as_ref().filter(|p| p.enabled) {
        if let Some(key) = config.api_key() {
            configured.insert(
                ProviderId::OpenAi,
                Arc::new(OpenAiProvider::new(key, Some(config.base_url.clone()))?),
            );
            info!("OpenAI ready");
        }
    }

    // A key on its own is not enough: the registry has to name a model that key
    // can actually reach. Otherwise the router has candidates for nothing and
    // every question comes back as `provider_unavailable`, which reads like an
    // outage rather than like a configuration file that was never filled in.
    let reachable = registry
        .all()
        .iter()
        .any(|spec| spec.enabled && configured.contains_key(&spec.provider));

    if configured.is_empty() || !reachable {
        if !environment.allows_fakes() {
            anyhow::bail!(
                "no chat provider is configured. Set ANTHROPIC_API_KEY or OPENAI_API_KEY, \
                 enable the matching entry in config/models.toml, and check that the model \
                 names there are current."
            );
        }

        if configured.is_empty() {
            warn!(
                "no chat provider is configured: answering from retrieved passages locally. \
                 The pipeline is exercised end to end, but nothing is generated. Set a \
                 provider key for real answers."
            );
        } else {
            warn!(
                configured = ?configured.keys().map(|p| p.as_str()).collect::<Vec<_>>(),
                "a provider key is set but config/models.toml enables no model for it: \
                 answering from retrieved passages locally. Enable a row for this provider \
                 and check that its model name is current."
            );
        }

        return Ok(ModelRouter::new(
            ModelRegistry::echo_only(),
            HashMap::from([(
                ProviderId::Anthropic,
                Arc::new(EchoProvider::new()) as Arc<dyn ChatProvider>,
            )]),
            HealthTracker::new(clock),
        ));
    }

    // Every usage record this deployment writes is priced from the registry.
    // A model whose prices nobody has checked produces invoices that look
    // exactly like correct ones, so production does not start with one — the
    // mistake is unrecoverable once a customer has been billed, and silent
    // until they add it up themselves.
    let unpriced: Vec<&str> = registry
        .unpriced()
        .iter()
        .map(|spec| spec.id.as_str())
        .collect();

    if !unpriced.is_empty() {
        if !environment.allows_fakes() {
            anyhow::bail!(
                "these models are enabled but their prices have never been confirmed: {unpriced:?}. \
                 Check them against the provider's published figures, then set `priced_on` in \
                 config/models.toml to today's date."
            );
        }
        warn!(
            models = ?unpriced,
            "these models have unconfirmed prices: their usage records will carry a \
             cost that was guessed. Production will not start until `priced_on` is set."
        );
    }

    // A registry naming models we cannot reach is a configuration mistake worth
    // saying out loud: the symptom otherwise is every request failing over.
    let unreachable: Vec<&str> = registry
        .all()
        .iter()
        .filter(|spec| spec.enabled && !configured.contains_key(&spec.provider))
        .map(|spec| spec.id.as_str())
        .collect();
    if !unreachable.is_empty() {
        warn!(
            models = ?unreachable,
            "these models are enabled but their provider has no key, and will be skipped"
        );
    }

    Ok(ModelRouter::new(
        registry,
        configured,
        HealthTracker::new(clock),
    ))
}

/// Read the model registry from `config/models.toml`.
pub fn model_registry(path: &str) -> anyhow::Result<ModelRegistry> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("could not read the model registry at `{path}`: {e}"))?;
    Ok(ModelRegistry::from_toml(&text)?)
}

/// Strip the `provider:` namespace from a configured model id.
///
/// The platform stores `openai:text-embedding-3-small`; the provider wants
/// `text-embedding-3-small`. Keeping the namespace out of the wire call is what
/// lets a knowledge base record which provider built it.
fn model_name(model_id: &str) -> &str {
    model_id.split_once(':').map_or(model_id, |(_, name)| name)
}

#[cfg(test)]
mod tests {
    use anthovai_core::config::ProviderEntry;

    use super::*;

    fn settings(enabled: bool) -> ProviderSettings {
        ProviderSettings {
            openai: Some(ProviderEntry {
                api_key_env: "ANTHOVAI_TEST_MISSING_KEY".to_owned(),
                base_url: "https://api.openai.com/v1".to_owned(),
                enabled,
            }),
            anthropic: None,
        }
    }

    fn embedding_settings() -> EmbeddingSettings {
        EmbeddingSettings {
            default_model: "openai:text-embedding-3-small".to_owned(),
            dimension: 1536,
            batch_size: 64,
            concurrency: 4,
        }
    }

    #[test]
    fn the_namespace_is_not_sent_to_the_provider() {
        assert_eq!(
            model_name("openai:text-embedding-3-small"),
            "text-embedding-3-small"
        );
        assert_eq!(
            model_name("text-embedding-3-small"),
            "text-embedding-3-small"
        );
        assert_eq!(model_name("fake:hash-1536"), "hash-1536");
    }

    #[test]
    fn development_falls_back_to_local_embeddings() {
        let provider =
            embedding_provider(&settings(true), &embedding_settings(), Environment::Local)
                .expect("development should not need a key");

        assert!(anthovai_embeddings::is_fake_model(provider.model_id()));
        assert_eq!(provider.dimension(), 1536);
    }

    #[test]
    fn production_refuses_to_start_without_a_key() {
        // Fake vectors in a customer's knowledge base would look like working
        // software and retrieve nothing useful, so this has to be fatal.
        let result = embedding_provider(
            &settings(true),
            &embedding_settings(),
            Environment::Production,
        );

        let error = result
            .err()
            .expect("production must not start without a provider");

        assert!(error.to_string().contains("OPENAI_API_KEY"));
    }

    #[test]
    fn a_disabled_provider_is_the_same_as_none() {
        assert!(embedding_provider(
            &settings(false),
            &embedding_settings(),
            Environment::Production
        )
        .is_err());
    }

    /// The state this repository is actually in: a key is present, but every
    /// row that would use it is still a placeholder. Without this check the
    /// router accepts the key, finds no candidate, and answers every question
    /// with `provider_unavailable`.
    #[test]
    fn a_key_with_no_enabled_model_is_the_same_as_no_provider() {
        let providers = ProviderSettings {
            openai: Some(ProviderEntry {
                api_key_env: "ANTHOVAI_TEST_FACTORY_KEY".to_owned(),
                base_url: "https://api.openai.com/v1".to_owned(),
                enabled: true,
            }),
            anthropic: None,
        };
        // SAFETY: single-threaded test, and the variable is unique to it.
        unsafe { std::env::set_var("ANTHOVAI_TEST_FACTORY_KEY", "sk-test") };

        // A registry whose only enabled model belongs to the *other* provider.
        let registry = ModelRegistry::new(vec![anthovai_inference::ModelSpec {
            id: "claude-medium".to_owned(),
            provider: ProviderId::Anthropic,
            name: "claude-sonnet-5".to_owned(),
            tier: anthovai_inference::Tier::Medium,
            max_context_tokens: 200_000,
            max_output_tokens: 32_000,
            input_price_micro_per_mtok: 0,
            output_price_micro_per_mtok: 0,
            enabled: true,
            priority: 0,
            priced_on: Some("2026-09-05".to_owned()),
        }]);

        assert!(
            chat_router(
                &providers,
                registry.clone(),
                Clock::system(),
                Environment::Production
            )
            .is_err(),
            "production must not start with a router that can reach nothing"
        );

        // Development gets the echo model rather than a router with no
        // candidates, so the pipeline can still be exercised.
        let router = chat_router(&providers, registry, Clock::system(), Environment::Local)
            .expect("development should fall back");
        assert!(router
            .registry()
            .all()
            .iter()
            .all(|spec| spec.name == anthovai_inference::ECHO_MODEL));

        unsafe { std::env::remove_var("ANTHOVAI_TEST_FACTORY_KEY") };
    }

    /// The prices in `config/models.toml` are what every invoice is computed
    /// from. A row nobody has checked must not reach a deployment that bills.
    #[test]
    fn production_will_not_start_with_a_model_nobody_has_priced() {
        let providers = ProviderSettings {
            openai: Some(ProviderEntry {
                api_key_env: "ANTHOVAI_TEST_PRICING_KEY".to_owned(),
                base_url: "https://api.openai.com/v1".to_owned(),
                enabled: true,
            }),
            anthropic: None,
        };
        // SAFETY: single-threaded test, and the variable is unique to it.
        unsafe { std::env::set_var("ANTHOVAI_TEST_PRICING_KEY", "sk-test") };

        let unpriced = || {
            ModelRegistry::new(vec![anthovai_inference::ModelSpec {
                id: "openai-medium".to_owned(),
                provider: ProviderId::OpenAi,
                name: "gpt-5.4-mini".to_owned(),
                tier: anthovai_inference::Tier::Medium,
                max_context_tokens: 400_000,
                max_output_tokens: 128_000,
                input_price_micro_per_mtok: 0,
                output_price_micro_per_mtok: 0,
                enabled: true,
                priority: 0,
                priced_on: None,
            }])
        };

        let error = chat_router(
            &providers,
            unpriced(),
            Clock::system(),
            Environment::Production,
        )
        .expect_err("production must not start with an unpriced model");
        assert!(error.to_string().contains("priced_on"), "{error}");

        // Development runs anyway — a developer needs to be able to try a new
        // model before anyone has looked up what it costs — but is told.
        assert!(chat_router(&providers, unpriced(), Clock::system(), Environment::Local).is_ok());

        let mut priced = unpriced();
        priced = ModelRegistry::new(
            priced
                .all()
                .iter()
                .cloned()
                .map(|mut spec| {
                    spec.priced_on = Some("2026-09-05".to_owned());
                    spec
                })
                .collect(),
        );
        assert!(
            chat_router(&providers, priced, Clock::system(), Environment::Production).is_ok(),
            "a priced model should start in production"
        );

        unsafe { std::env::remove_var("ANTHOVAI_TEST_PRICING_KEY") };
    }

    #[test]
    fn only_production_insists_on_a_real_provider() {
        assert!(Environment::Local.allows_fakes());
        assert!(Environment::Staging.allows_fakes());
        assert!(!Environment::Production.allows_fakes());
    }
}
