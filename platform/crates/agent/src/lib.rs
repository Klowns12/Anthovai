//! Agents: configuration, versions, and the published/draft split.
//!
//! An agent is configuration, not a model. Public traffic always runs the
//! published version; the dashboard playground always runs the draft.

pub mod config;
pub mod prompt;
pub mod repo;
pub mod service;

pub use config::{
    AgentConfig, BehaviorConfig, GuardrailConfig, Language, ResponseConfig, ResponseFormat,
    ResponseLength, RetrievalConfig, CURRENT_SCHEMA_VERSION,
};
pub use prompt::PromptBuilder;
pub use service::{AgentDetail, AgentService, CreateAgent, UpdateAgent};

use anthovai_core::{AgentId, AgentVersionId, OrgId, WorkspaceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Draft,
    Active,
    Paused,
    Archived,
}

impl AgentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Archived => "archived",
        }
    }

    /// Whether public API traffic may reach this agent.
    pub fn is_publicly_callable(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl std::str::FromStr for AgentStatus {
    type Err = anthovai_core::DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(Self::Draft),
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "archived" => Ok(Self::Archived),
            other => Err(anthovai_core::DomainError::validation(format!(
                "unknown agent status `{other}`"
            ))),
        }
    }
}

/// An agent plus the version that is about to run.
#[derive(Clone, Debug)]
pub struct ResolvedAgent {
    pub id: AgentId,
    pub org_id: OrgId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub status: AgentStatus,
    pub version_id: AgentVersionId,
    pub version: i32,
    pub config: AgentConfig,
    pub knowledge_base_ids: Vec<anthovai_core::KnowledgeBaseId>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_active_agents_serve_public_traffic() {
        assert!(AgentStatus::Active.is_publicly_callable());
        for status in [
            AgentStatus::Draft,
            AgentStatus::Paused,
            AgentStatus::Archived,
        ] {
            assert!(!status.is_publicly_callable(), "{status:?} must be refused");
        }
    }

    #[test]
    fn status_names_round_trip() {
        for status in [
            AgentStatus::Draft,
            AgentStatus::Active,
            AgentStatus::Paused,
            AgentStatus::Archived,
        ] {
            assert_eq!(status.as_str().parse::<AgentStatus>().unwrap(), status);
        }
    }
}
