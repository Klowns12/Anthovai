//! The stored agent configuration.
//!
//! This is the JSON in `agent_versions.config`. It is a versioned schema: any
//! change adds a field with a default, or bumps `schema_version` and migrates.

use anthovai_core::{DomainError, Plan, Result};
use anthovai_inference::{ModelPolicy, ReasoningLevel};
use serde::{Deserialize, Serialize};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "current_schema_version")]
    pub schema_version: u32,
    pub instructions: String,
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub model_policy: ModelPolicy,
    #[serde(default)]
    pub response: ResponseConfig,
    #[serde(default)]
    pub retrieval: RetrievalConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub guardrails: GuardrailConfig,
    /// Reserved for P5. Present so stored configs do not need a schema bump.
    #[serde(default)]
    pub tools: Vec<serde_json::Value>,
}

fn current_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            instructions: String::new(),
            language: Language::default(),
            model_policy: ModelPolicy::default(),
            response: ResponseConfig::default(),
            retrieval: RetrievalConfig::default(),
            behavior: BehaviorConfig::default(),
            guardrails: GuardrailConfig::default(),
            tools: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Answer in whatever language the user wrote in.
    #[default]
    Auto,
    Th,
    En,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseConfig {
    pub length: ResponseLength,
    pub format: ResponseFormat,
}

impl Default for ResponseConfig {
    fn default() -> Self {
        Self {
            length: ResponseLength::Balanced,
            format: ResponseFormat::Markdown,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseLength {
    Short,
    #[default]
    Balanced,
    Detailed,
}

impl ResponseLength {
    /// The sentence appended to the system prompt.
    pub fn instruction(self) -> &'static str {
        match self {
            Self::Short => "Answer in one or two sentences.",
            Self::Balanced => "Answer in a short paragraph.",
            Self::Detailed => "Answer thoroughly, using lists where they help.",
        }
    }

    pub fn max_output_tokens(self) -> u32 {
        match self {
            Self::Short => 512,
            Self::Balanced => 1_536,
            Self::Detailed => 4_096,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseFormat {
    #[default]
    Markdown,
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub top_k: usize,
    pub context_token_budget: u32,
    pub min_relevance: f32,
    pub hybrid: bool,
    pub mmr_lambda: f32,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 8,
            context_token_budget: 6_000,
            min_relevance: 0.25,
            hybrid: true,
            mmr_lambda: 0.7,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorConfig {
    /// When true, the agent answers only from retrieved knowledge and returns
    /// `fallback_message` when nothing relevant was found.
    pub strict_knowledge: bool,
    pub citations: bool,
    pub fallback_message: String,
    pub history_turns: usize,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            strict_knowledge: true,
            citations: true,
            fallback_message: "ขออภัย ฉันไม่มีข้อมูลเรื่องนี้".to_owned(),
            history_turns: 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardrailConfig {
    pub block_pii_output: bool,
    pub max_input_chars: usize,
}

impl Default for GuardrailConfig {
    fn default() -> Self {
        Self {
            block_pii_output: false,
            max_input_chars: 4_000,
        }
    }
}

impl AgentConfig {
    pub fn reasoning(&self) -> ReasoningLevel {
        self.model_policy.reasoning()
    }

    /// Structural validation plus the plan gate on the model policy.
    pub fn validate(&self, plan: Plan) -> Result<()> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(DomainError::validation(format!(
                "unsupported config schema_version {}",
                self.schema_version
            )));
        }
        if self.instructions.chars().count() > 20_000 {
            return Err(DomainError::validation(
                "instructions must be at most 20000 characters",
            ));
        }
        if !(1..=50).contains(&self.retrieval.top_k) {
            return Err(DomainError::validation("retrieval.top_k must be 1..=50"));
        }
        if !(500..=100_000).contains(&self.retrieval.context_token_budget) {
            return Err(DomainError::validation(
                "retrieval.context_token_budget must be 500..=100000",
            ));
        }
        if !(0.0..=1.0).contains(&self.retrieval.min_relevance) {
            return Err(DomainError::validation(
                "retrieval.min_relevance must be between 0 and 1",
            ));
        }
        if !(0.0..=1.0).contains(&self.retrieval.mmr_lambda) {
            return Err(DomainError::validation(
                "retrieval.mmr_lambda must be between 0 and 1",
            ));
        }
        if self.behavior.strict_knowledge && self.behavior.fallback_message.trim().is_empty() {
            return Err(DomainError::validation(
                "a strict-knowledge agent needs a fallback_message",
            ));
        }
        if self.behavior.history_turns > 50 {
            return Err(DomainError::validation(
                "behavior.history_turns must be at most 50",
            ));
        }
        if !self.tools.is_empty() {
            return Err(DomainError::validation("agent tools are not available yet"));
        }
        self.model_policy.check_plan(plan)
    }
}

#[cfg(test)]
mod tests {
    use anthovai_inference::ProviderId;

    use super::*;

    fn config() -> AgentConfig {
        AgentConfig {
            instructions: "You help students of ABC School.".into(),
            ..AgentConfig::default()
        }
    }

    #[test]
    fn a_default_config_is_valid_on_the_free_plan() {
        assert!(config().validate(Plan::Free).is_ok());
    }

    #[test]
    fn defaults_match_the_specification() {
        let c = AgentConfig::default();
        assert_eq!(c.retrieval.top_k, 8);
        assert_eq!(c.retrieval.context_token_budget, 6_000);
        assert!(c.behavior.strict_knowledge);
        assert!(c.behavior.citations);
        assert!(matches!(c.model_policy, ModelPolicy::AnthovaiAuto { .. }));
    }

    #[test]
    fn out_of_range_retrieval_settings_are_rejected() {
        let mut c = config();
        c.retrieval.top_k = 0;
        assert!(c.validate(Plan::Free).is_err());

        let mut c = config();
        c.retrieval.min_relevance = 1.5;
        assert!(c.validate(Plan::Free).is_err());

        let mut c = config();
        c.retrieval.context_token_budget = 10;
        assert!(c.validate(Plan::Free).is_err());
    }

    #[test]
    fn strict_agents_must_have_something_to_fall_back_to() {
        let mut c = config();
        c.behavior.fallback_message = "  ".into();
        assert!(c.validate(Plan::Free).is_err());

        c.behavior.strict_knowledge = false;
        assert!(c.validate(Plan::Free).is_ok());
    }

    #[test]
    fn choosing_a_provider_is_gated_on_the_plan() {
        let mut c = config();
        c.model_policy = ModelPolicy::ProviderOnly {
            provider: ProviderId::Anthropic,
            reasoning: ReasoningLevel::Deep,
        };
        assert!(c.validate(Plan::Starter).is_err());
        assert!(c.validate(Plan::Business).is_ok());
    }

    #[test]
    fn an_unknown_schema_version_is_refused() {
        let mut c = config();
        c.schema_version = 99;
        assert!(c.validate(Plan::Enterprise).is_err());
    }

    #[test]
    fn tools_are_refused_until_they_are_implemented() {
        let mut c = config();
        c.tools = vec![serde_json::json!({"type": "http"})];
        assert!(c.validate(Plan::Enterprise).is_err());
    }

    #[test]
    fn a_stored_config_missing_new_fields_still_loads() {
        let stored = serde_json::json!({
            "schema_version": 1,
            "instructions": "hello"
        });
        let parsed: AgentConfig = serde_json::from_value(stored).unwrap();
        assert_eq!(parsed.retrieval.top_k, 8);
        assert!(parsed.validate(Plan::Free).is_ok());
    }

    #[test]
    fn round_trips_through_json() {
        let original = config();
        let json = serde_json::to_string(&original).unwrap();
        let parsed: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }
}
