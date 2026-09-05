//! Model policy: what the customer asked for, expressed without provider names
//! wherever possible.

use anthovai_core::{DomainError, Feature, Plan, Result};
use serde::{Deserialize, Serialize};

use crate::types::{ProviderId, ReasoningLevel};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelPolicy {
    /// The default and the product's selling point: Anthovai picks.
    AnthovaiAuto {
        #[serde(default)]
        reasoning: ReasoningLevel,
    },
    /// Business plan and above: stay inside one provider.
    ProviderOnly {
        provider: ProviderId,
        #[serde(default)]
        reasoning: ReasoningLevel,
    },
    /// Enterprise: an explicit primary with an explicit fallback chain, by
    /// internal model id (never a raw provider model name).
    Custom {
        primary: String,
        #[serde(default)]
        fallback: Vec<String>,
    },
}

impl Default for ModelPolicy {
    fn default() -> Self {
        Self::AnthovaiAuto {
            reasoning: ReasoningLevel::default(),
        }
    }
}

impl ModelPolicy {
    pub fn reasoning(&self) -> ReasoningLevel {
        match self {
            Self::AnthovaiAuto { reasoning } | Self::ProviderOnly { reasoning, .. } => *reasoning,
            Self::Custom { .. } => ReasoningLevel::Balanced,
        }
    }

    /// A policy the customer may not have on their plan is rejected before it
    /// ever reaches the router.
    pub fn check_plan(&self, plan: Plan) -> Result<()> {
        match self {
            Self::AnthovaiAuto { .. } => Ok(()),
            Self::ProviderOnly { .. } => plan.require(Feature::ProviderChoice),
            Self::Custom { primary, fallback } => {
                plan.require(Feature::CustomModelPolicy)?;
                if primary.trim().is_empty() {
                    return Err(DomainError::validation(
                        "custom policy needs a primary model id",
                    ));
                }
                if fallback.iter().any(|f| f.trim().is_empty()) {
                    return Err(DomainError::validation(
                        "custom policy fallback ids must not be empty",
                    ));
                }
                Ok(())
            }
        }
    }

    /// Whether failing over to a different provider is acceptable. A customer
    /// who pinned a provider must not silently be answered by another one.
    pub fn allows_cross_provider_fallback(&self) -> bool {
        matches!(self, Self::AnthovaiAuto { .. })
    }
}

/// Facts about this particular request that the router uses to pick a model.
#[derive(Clone, Debug)]
pub struct RoutingHints {
    pub reasoning: ReasoningLevel,
    pub context_tokens: u32,
    pub needs_streaming: bool,
}

impl RoutingHints {
    pub fn new(reasoning: ReasoningLevel, context_tokens: u32) -> Self {
        Self {
            reasoning,
            context_tokens,
            needs_streaming: false,
        }
    }

    pub fn streaming(mut self, yes: bool) -> Self {
        self.needs_streaming = yes;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_is_available_on_every_plan() {
        let policy = ModelPolicy::default();
        for plan in [Plan::Free, Plan::Starter, Plan::Business, Plan::Enterprise] {
            assert!(policy.check_plan(plan).is_ok());
        }
    }

    #[test]
    fn provider_choice_needs_business() {
        let policy = ModelPolicy::ProviderOnly {
            provider: ProviderId::Anthropic,
            reasoning: ReasoningLevel::Balanced,
        };
        assert!(policy.check_plan(Plan::Starter).is_err());
        assert!(policy.check_plan(Plan::Business).is_ok());
    }

    #[test]
    fn custom_policy_needs_enterprise_and_a_primary() {
        let good = ModelPolicy::Custom {
            primary: "claude-medium".into(),
            fallback: vec!["openai-medium".into()],
        };
        assert!(good.check_plan(Plan::Business).is_err());
        assert!(good.check_plan(Plan::Enterprise).is_ok());

        let empty = ModelPolicy::Custom {
            primary: "  ".into(),
            fallback: vec![],
        };
        assert!(empty.check_plan(Plan::Enterprise).is_err());
    }

    #[test]
    fn pinned_providers_never_fail_over_to_another_vendor() {
        let pinned = ModelPolicy::ProviderOnly {
            provider: ProviderId::OpenAi,
            reasoning: ReasoningLevel::Fast,
        };
        assert!(!pinned.allows_cross_provider_fallback());
        assert!(ModelPolicy::default().allows_cross_provider_fallback());
    }

    #[test]
    fn serialises_with_a_stable_tag() {
        let json = serde_json::to_string(&ModelPolicy::default()).unwrap();
        assert!(json.contains("\"anthovai_auto\""), "got {json}");
        let parsed: ModelPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ModelPolicy::default());
    }
}
