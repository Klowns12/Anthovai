//! The model registry: the only place that knows real provider model names.
//!
//! Loaded from `config/models.toml` so a model can be renamed, repriced, or
//! disabled without touching code or breaking the customer-facing API.

use serde::Deserialize;

use crate::types::{ProviderId, Tier};

#[derive(Clone, Debug, Deserialize)]
pub struct ModelSpec {
    /// Stable internal id, e.g. `claude-medium`. Never exposed to customers.
    pub id: String,
    pub provider: ProviderId,
    /// The provider's own model name, e.g. `claude-sonnet-5`.
    pub name: String,
    pub tier: Tier,
    pub max_context_tokens: u32,
    pub max_output_tokens: u32,
    /// Price in micro-USD per million tokens, so costs stay integral.
    pub input_price_micro_per_mtok: u64,
    pub output_price_micro_per_mtok: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Lower sorts first when several models share a tier.
    #[serde(default)]
    pub priority: i32,
    /// The date the prices above were last checked against the provider's own
    /// published figures, as `YYYY-MM-DD`.
    ///
    /// Absent means nobody has confirmed them. Every usage record and every
    /// invoice is computed from those two numbers, so a guess here becomes a
    /// wrong bill that nothing else in the system would ever contradict —
    /// which is why production refuses to start with an enabled model that has
    /// no date. See [`ModelRegistry::unpriced`].
    #[serde(default)]
    pub priced_on: Option<String>,
}

fn default_true() -> bool {
    true
}

impl ModelSpec {
    /// Whether anybody has checked this model's prices against the provider.
    pub fn is_priced(&self) -> bool {
        self.priced_on.is_some()
    }

    /// Cost of one call, in micro-USD.
    pub fn cost_micro(&self, input_tokens: u32, output_tokens: u32) -> u64 {
        let per_mtok = |tokens: u32, price: u64| -> u64 { (u64::from(tokens) * price) / 1_000_000 };
        per_mtok(input_tokens, self.input_price_micro_per_mtok)
            + per_mtok(output_tokens, self.output_price_micro_per_mtok)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ModelRegistry {
    #[serde(default, rename = "models")]
    specs: Vec<ModelSpec>,
}

impl ModelRegistry {
    pub fn new(mut specs: Vec<ModelSpec>) -> Self {
        specs.sort_by_key(|s| s.priority);
        Self { specs }
    }

    pub fn from_toml(text: &str) -> Result<Self, toml_error::TomlError> {
        // Parsing is delegated to `config` so the same file can be layered with
        // environment overrides alongside the rest of the settings.
        let registry: ModelRegistry = config::Config::builder()
            .add_source(config::File::from_str(text, config::FileFormat::Toml))
            .build()
            .map_err(|e| toml_error::TomlError(e.to_string()))?
            .try_deserialize()
            .map_err(|e| toml_error::TomlError(e.to_string()))?;
        Ok(Self::new(registry.specs))
    }

    /// A registry holding only the local echo model.
    ///
    /// Used when no provider is configured, so development still has a routable
    /// model. Priced at zero because nothing is spent, which keeps the usage
    /// records honest rather than showing a cost that was never incurred.
    ///
    /// Registered in every tier. The router walks upward from the tier an
    /// agent's policy asks for, so an echo model that existed only in `Small`
    /// would be unreachable for every agent on the default policy — which reads
    /// as "no model provider is available" rather than as a missing key.
    pub fn echo_only() -> Self {
        Self::new(
            [Tier::Small, Tier::Medium, Tier::Large]
                .into_iter()
                .map(|tier| ModelSpec {
                    id: format!("echo-local-{}", tier.as_str()),
                    provider: ProviderId::Anthropic,
                    name: crate::echo::ECHO_MODEL.to_owned(),
                    tier,
                    max_context_tokens: 1_000_000,
                    max_output_tokens: 64_000,
                    input_price_micro_per_mtok: 0,
                    output_price_micro_per_mtok: 0,
                    enabled: true,
                    priority: 0,
                    // Zero really is the price of the echo model, and saying so
                    // keeps it out of the unpriced list.
                    priced_on: Some("1970-01-01".to_owned()),
                })
                .collect(),
        )
    }

    pub fn all(&self) -> &[ModelSpec] {
        &self.specs
    }

    pub fn by_id(&self, id: &str) -> Option<&ModelSpec> {
        self.specs.iter().find(|s| s.id == id && s.enabled)
    }

    /// Enabled models in a tier, best first.
    pub fn in_tier(&self, tier: Tier) -> Vec<&ModelSpec> {
        self.specs
            .iter()
            .filter(|s| s.enabled && s.tier == tier)
            .collect()
    }

    /// Enabled models in a tier from one provider, best first.
    pub fn in_tier_for(&self, tier: Tier, provider: ProviderId) -> Vec<&ModelSpec> {
        self.specs
            .iter()
            .filter(|s| s.enabled && s.tier == tier && s.provider == provider)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.iter().all(|s| !s.enabled)
    }

    /// Enabled models whose prices nobody has confirmed.
    ///
    /// A model here will still answer questions perfectly well. What it cannot
    /// do is be billed for: `cost_usd_micro` on every usage record it produces
    /// is computed from numbers that were guessed, and no other part of the
    /// system will ever notice they are wrong.
    pub fn unpriced(&self) -> Vec<&ModelSpec> {
        self.specs
            .iter()
            .filter(|s| s.enabled && !s.is_priced())
            .collect()
    }
}

pub mod toml_error {
    #[derive(Debug, thiserror::Error)]
    #[error("could not read the model registry: {0}")]
    pub struct TomlError(pub String);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[models]]
id = "small-a"
provider = "openai"
name = "some-small-model"
tier = "small"
max_context_tokens = 128000
max_output_tokens = 16000
input_price_micro_per_mtok = 150000
output_price_micro_per_mtok = 600000
priority = 0

[[models]]
id = "medium-a"
provider = "anthropic"
name = "claude-sonnet-5"
tier = "medium"
max_context_tokens = 1000000
max_output_tokens = 64000
input_price_micro_per_mtok = 2000000
output_price_micro_per_mtok = 10000000
priority = 0

[[models]]
id = "medium-b"
provider = "openai"
name = "some-medium-model"
tier = "medium"
max_context_tokens = 400000
max_output_tokens = 32000
input_price_micro_per_mtok = 1000000
output_price_micro_per_mtok = 4000000
priority = 1

[[models]]
id = "retired"
provider = "openai"
name = "gone"
tier = "large"
max_context_tokens = 8000
max_output_tokens = 2000
input_price_micro_per_mtok = 1
output_price_micro_per_mtok = 1
enabled = false
"#;

    fn registry() -> ModelRegistry {
        ModelRegistry::from_toml(SAMPLE).expect("sample registry parses")
    }

    #[test]
    fn parses_and_orders_by_priority() {
        let reg = registry();
        let medium = reg.in_tier(Tier::Medium);
        assert_eq!(medium.len(), 2);
        assert_eq!(medium[0].id, "medium-a");
        assert_eq!(medium[1].id, "medium-b");
    }

    #[test]
    fn disabled_models_are_invisible() {
        let reg = registry();
        assert!(reg.by_id("retired").is_none());
        assert!(reg.in_tier(Tier::Large).is_empty());
    }

    #[test]
    fn filters_by_provider() {
        let reg = registry();
        let only_openai = reg.in_tier_for(Tier::Medium, ProviderId::OpenAi);
        assert_eq!(only_openai.len(), 1);
        assert_eq!(only_openai[0].provider, ProviderId::OpenAi);
    }

    #[test]
    fn computes_cost_in_micro_usd() {
        let reg = registry();
        let spec = reg.by_id("medium-a").unwrap();
        // 1M input tokens at 2.0 USD, 1M output at 10.0 USD.
        assert_eq!(spec.cost_micro(1_000_000, 0), 2_000_000);
        assert_eq!(spec.cost_micro(0, 1_000_000), 10_000_000);
        assert_eq!(spec.cost_micro(500_000, 100_000), 1_000_000 + 1_000_000);
    }
}
