//! RAG orchestration: the sequence that turns a question into a grounded answer.
//!
//! `ChatService` itself arrives with Milestone 8, once repositories exist. What
//! lives here now is the part that has no dependencies and carries the product
//! rules: the output shape, and the decision about when not to call a model at all.

pub mod service;

pub use service::{
    AnsweredBy, ChatInput, ChatResult, ChatService, DebugPassage, RetrievalDebug, Version,
};

use anthovai_retrieval::Source;
use serde::{Deserialize, Serialize};

/// What the API returns for one question.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatOutput {
    pub answer: String,
    /// True when the answer cited retrieved knowledge.
    pub grounded: bool,
    pub sources: Vec<Source>,
    /// Set when the agent answered from its fallback rather than the model.
    pub used_fallback: bool,
}

/// Why a request did not reach a model. Every one of these saves a paid call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortCircuit {
    /// Strict agent, nothing relevant retrieved.
    NoRelevantKnowledge,
}

/// A strict agent with no relevant knowledge has nothing to say, and asking a
/// model to say it costs money and invites a hallucination. Answer from the
/// configured fallback instead.
pub fn short_circuit(strict_knowledge: bool, retrieved_sources: usize) -> Option<ShortCircuit> {
    if strict_knowledge && retrieved_sources == 0 {
        Some(ShortCircuit::NoRelevantKnowledge)
    } else {
        None
    }
}

/// Assemble the response for a short-circuited request.
pub fn fallback_output(fallback_message: &str) -> ChatOutput {
    ChatOutput {
        answer: fallback_message.to_owned(),
        grounded: false,
        sources: Vec::new(),
        used_fallback: true,
    }
}

/// Assemble the response for a model answer, keeping only the sources the
/// answer actually cited.
pub fn model_output(answer: String, offered_sources: &[Source], citations: bool) -> ChatOutput {
    let sources = if citations {
        anthovai_retrieval::context::cited_sources(&answer, offered_sources)
    } else {
        Vec::new()
    };
    ChatOutput {
        grounded: !sources.is_empty(),
        answer,
        sources,
        used_fallback: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(index: usize) -> Source {
        Source {
            index,
            document_id: format!("doc_{index}"),
            chunk_id: format!("chk_{index}"),
            title: "Course Catalog".into(),
            page: Some(4),
            url: None,
            snippet: "…".into(),
            score: 0.8,
        }
    }

    #[test]
    fn a_strict_agent_with_nothing_retrieved_never_calls_a_model() {
        assert_eq!(
            short_circuit(true, 0),
            Some(ShortCircuit::NoRelevantKnowledge)
        );
    }

    #[test]
    fn a_strict_agent_with_knowledge_does_call_the_model() {
        assert_eq!(short_circuit(true, 3), None);
    }

    #[test]
    fn a_non_strict_agent_always_calls_the_model() {
        assert_eq!(short_circuit(false, 0), None);
    }

    #[test]
    fn the_fallback_response_is_not_grounded() {
        let out = fallback_output("ขออภัย ฉันไม่มีข้อมูลเรื่องนี้");
        assert!(!out.grounded);
        assert!(out.used_fallback);
        assert!(out.sources.is_empty());
        assert_eq!(out.answer, "ขออภัย ฉันไม่มีข้อมูลเรื่องนี้");
    }

    #[test]
    fn only_cited_sources_are_returned() {
        let offered = vec![source(1), source(2), source(3)];
        let out = model_output("The course runs 12 weeks [2].".into(), &offered, true);
        assert_eq!(out.sources.len(), 1);
        assert_eq!(out.sources[0].index, 2);
        assert!(out.grounded);
    }

    #[test]
    fn an_answer_without_citations_is_not_grounded() {
        let offered = vec![source(1)];
        let out = model_output("I think it is twelve weeks.".into(), &offered, true);
        assert!(out.sources.is_empty());
        assert!(!out.grounded);
    }

    #[test]
    fn citations_can_be_disabled_per_agent() {
        let offered = vec![source(1)];
        let out = model_output("Twelve weeks [1].".into(), &offered, false);
        assert!(out.sources.is_empty());
        assert!(!out.grounded);
    }
}
