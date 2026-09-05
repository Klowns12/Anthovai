//! Plans, their limits, and the features they gate.
//!
//! Limits live in `config/plans.toml` in production; the values here are the
//! compiled-in defaults used by tests and local development (P1).

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    Free,
    Starter,
    Business,
    Enterprise,
}

impl Plan {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Starter => "starter",
            Self::Business => "business",
            Self::Enterprise => "enterprise",
        }
    }

    pub fn limits(self) -> PlanLimits {
        match self {
            Self::Free => PlanLimits {
                storage_bytes: 100 * MB,
                documents_per_kb: 50,
                max_file_bytes: 10 * MB,
                max_agents: 1,
                messages_per_month: 1_000,
                requests_per_minute: 20,
                concurrent_streams: 2,
                uploads_per_hour: 20,
            },
            Self::Starter => PlanLimits {
                storage_bytes: GB,
                documents_per_kb: 500,
                max_file_bytes: 25 * MB,
                max_agents: 3,
                messages_per_month: 10_000,
                requests_per_minute: 60,
                concurrent_streams: 5,
                uploads_per_hour: 100,
            },
            Self::Business => PlanLimits {
                storage_bytes: 10 * GB,
                documents_per_kb: 5_000,
                max_file_bytes: 50 * MB,
                max_agents: 10,
                messages_per_month: 100_000,
                requests_per_minute: 300,
                concurrent_streams: 20,
                uploads_per_hour: 1_000,
            },
            Self::Enterprise => PlanLimits {
                storage_bytes: i64::MAX,
                documents_per_kb: i64::MAX,
                max_file_bytes: 200 * MB,
                max_agents: i64::MAX,
                messages_per_month: i64::MAX,
                requests_per_minute: 3_000,
                concurrent_streams: 200,
                uploads_per_hour: 100_000,
            },
        }
    }

    pub fn allows(self, feature: Feature) -> bool {
        match feature {
            Feature::ProviderChoice => self >= Plan::Business,
            Feature::CustomModelPolicy => self == Plan::Enterprise,
            Feature::RevealProviderInResponse => self >= Plan::Business,
            Feature::Webhooks => self >= Plan::Starter,
        }
    }

    pub fn require(self, feature: Feature) -> Result<()> {
        if self.allows(feature) {
            Ok(())
        } else {
            Err(DomainError::PlanRequired(feature.as_str()))
        }
    }
}

impl std::str::FromStr for Plan {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "free" => Ok(Self::Free),
            "starter" => Ok(Self::Starter),
            "business" => Ok(Self::Business),
            "enterprise" => Ok(Self::Enterprise),
            other => Err(DomainError::validation(format!("unknown plan `{other}`"))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    /// Pick OpenAI or Claude explicitly instead of Anthovai Auto.
    ProviderChoice,
    /// Supply a full custom routing policy with fallbacks.
    CustomModelPolicy,
    /// See which provider and model actually answered.
    RevealProviderInResponse,
    Webhooks,
}

impl Feature {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProviderChoice => "provider_choice",
            Self::CustomModelPolicy => "custom_model_policy",
            Self::RevealProviderInResponse => "reveal_provider",
            Self::Webhooks => "webhooks",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanLimits {
    pub storage_bytes: i64,
    pub documents_per_kb: i64,
    pub max_file_bytes: i64,
    pub max_agents: i64,
    pub messages_per_month: i64,
    pub requests_per_minute: u32,
    pub concurrent_streams: u32,
    pub uploads_per_hour: u32,
}

const MB: i64 = 1024 * 1024;
const GB: i64 = 1024 * MB;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_plan_cannot_choose_a_provider() {
        assert!(!Plan::Free.allows(Feature::ProviderChoice));
        assert!(Plan::Business.allows(Feature::ProviderChoice));
    }

    #[test]
    fn only_enterprise_gets_custom_policies() {
        assert!(Plan::Enterprise.allows(Feature::CustomModelPolicy));
        assert!(!Plan::Business.allows(Feature::CustomModelPolicy));
    }

    #[test]
    fn limits_grow_with_the_plan() {
        assert!(Plan::Starter.limits().messages_per_month > Plan::Free.limits().messages_per_month);
        assert!(Plan::Business.limits().max_file_bytes > Plan::Starter.limits().max_file_bytes);
    }

    #[test]
    fn plan_names_round_trip() {
        for plan in [Plan::Free, Plan::Starter, Plan::Business, Plan::Enterprise] {
            assert_eq!(plan.as_str().parse::<Plan>().unwrap(), plan);
        }
    }
}
