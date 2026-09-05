//! Usage accounting and quota.
//!
//! Cost is stored as micro-USD integers. Floating point money drifts, and these
//! numbers are summed over millions of rows.

pub mod repo;

pub use repo::DailyUsage;

use anthovai_core::{AgentId, ApiKeyId, OrgId, Plan, RequestId, Result};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageKind {
    Chat,
    EmbeddingIngest,
    EmbeddingQuery,
    Test,
}

impl UsageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::EmbeddingIngest => "embedding_ingest",
            Self::EmbeddingQuery => "embedding_query",
            Self::Test => "test",
        }
    }

    /// Playground traffic and ingestion cost us money but do not spend the
    /// customer's message allowance.
    pub fn counts_towards_message_quota(self) -> bool {
        matches!(self, Self::Chat)
    }
}

#[derive(Clone, Debug)]
pub struct UsageRecord {
    pub org_id: OrgId,
    pub agent_id: Option<AgentId>,
    pub api_key_id: Option<ApiKeyId>,
    pub request_id: RequestId,
    pub kind: UsageKind,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub embedding_tokens: i32,
    pub latency_ms: Option<i32>,
    pub cost_usd_micro: i64,
    pub created_at: DateTime<Utc>,
}

/// Running totals for the current billing period.
#[derive(Clone, Copy, Debug, Default)]
pub struct UsageCounters {
    pub messages: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd_micro: i64,
}

/// Quota periods are calendar months in UTC.
pub fn period_start(now: DateTime<Utc>) -> NaiveDate {
    NaiveDate::from_ymd_opt(now.year(), now.month(), 1).expect("the first of a month always exists")
}

/// Checked before the request reaches a paid model call, not after.
pub fn check_message_quota(plan: Plan, counters: &UsageCounters) -> Result<()> {
    if counters.messages >= plan.limits().messages_per_month {
        return Err(anthovai_core::DomainError::QuotaExceeded("quota_exceeded"));
    }
    Ok(())
}

/// Fraction of the allowance consumed, for the warning at 80%.
pub fn quota_fraction(plan: Plan, counters: &UsageCounters) -> f64 {
    let limit = plan.limits().messages_per_month;
    if limit <= 0 || limit == i64::MAX {
        return 0.0;
    }
    counters.messages as f64 / limit as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingestion_does_not_eat_the_message_allowance() {
        assert!(UsageKind::Chat.counts_towards_message_quota());
        assert!(!UsageKind::EmbeddingIngest.counts_towards_message_quota());
        assert!(!UsageKind::Test.counts_towards_message_quota());
    }

    #[test]
    fn the_free_plan_stops_at_its_limit() {
        let under = UsageCounters {
            messages: 999,
            ..Default::default()
        };
        assert!(check_message_quota(Plan::Free, &under).is_ok());

        let at_limit = UsageCounters {
            messages: 1_000,
            ..Default::default()
        };
        assert!(check_message_quota(Plan::Free, &at_limit).is_err());
    }

    #[test]
    fn enterprise_is_effectively_uncapped() {
        let heavy = UsageCounters {
            messages: 50_000_000,
            ..Default::default()
        };
        assert!(check_message_quota(Plan::Enterprise, &heavy).is_ok());
        assert_eq!(quota_fraction(Plan::Enterprise, &heavy), 0.0);
    }

    #[test]
    fn the_warning_threshold_can_be_computed() {
        let counters = UsageCounters {
            messages: 800,
            ..Default::default()
        };
        assert!((quota_fraction(Plan::Free, &counters) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn periods_start_on_the_first_of_the_month() {
        let now = DateTime::parse_from_rfc3339("2026-09-17T13:45:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            period_start(now),
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
        );
    }
}
