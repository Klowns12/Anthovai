//! Hybrid retrieval: vector search, keyword search, fusion, diversification and
//! context assembly.
//!
//! Every query here is tenant-scoped at the repository boundary: the SQL in
//! `search` binds `tenant_id` from the transaction, never from anything a
//! caller supplies, and row-level security stands behind it.

pub mod chunk_repo;
pub mod context;
pub mod fusion;
pub mod retriever;
pub mod search;

pub use context::{ContextBuilder, RetrievedContext, Source};
pub use fusion::{
    cosine_similarity, maximal_marginal_relevance, reciprocal_rank_fusion, select_within_budget,
    Candidate,
};
pub use retriever::{Retrieved, Retriever};
pub use search::SearchFilters;

/// Retrieval knobs, mirrored from the agent's stored configuration.
#[derive(Clone, Copy, Debug)]
pub struct RetrievalConfig {
    pub top_k: usize,
    pub context_token_budget: u32,
    pub min_relevance: f32,
    pub hybrid: bool,
    pub mmr_lambda: f32,
    pub vector_top: usize,
    pub keyword_top: usize,
    pub rrf_k: f32,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 8,
            context_token_budget: 6_000,
            min_relevance: 0.25,
            hybrid: true,
            mmr_lambda: 0.7,
            vector_top: 30,
            keyword_top: 30,
            rrf_k: 60.0,
        }
    }
}

/// Fuse, diversify and trim. The database work happens before this; this is the
/// pure part, so it is fully unit-testable.
pub fn rank(
    vector_hits: Vec<Candidate>,
    keyword_hits: Vec<Candidate>,
    config: &RetrievalConfig,
) -> Vec<Candidate> {
    let relevant: Vec<Candidate> = vector_hits
        .into_iter()
        .filter(|c| c.vector_score.is_none_or(|s| s >= config.min_relevance))
        .collect();

    let lists = if config.hybrid {
        vec![relevant, keyword_hits]
    } else {
        vec![relevant]
    };

    let fused = reciprocal_rank_fusion(&lists, config.rrf_k);
    let diversified = maximal_marginal_relevance(&fused, config.mmr_lambda, config.top_k * 2);
    select_within_budget(&diversified, config.context_token_budget, config.top_k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, score: f32) -> Candidate {
        Candidate {
            chunk_id: id.into(),
            document_id: "doc".into(),
            content: format!("body {id}"),
            token_count: 100,
            vector_score: Some(score),
            score: 0.0,
            embedding: vec![1.0, 0.0],
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn irrelevant_vector_hits_are_dropped_before_fusion() {
        let config = RetrievalConfig::default();
        let ranked = rank(vec![hit("weak", 0.05)], vec![], &config);
        assert!(ranked.is_empty(), "0.05 is below the 0.25 threshold");
    }

    #[test]
    fn keyword_only_hits_still_reach_the_context() {
        let config = RetrievalConfig::default();
        let mut keyword = Candidate::new("kw", "keyword body");
        keyword.token_count = 50;
        let ranked = rank(vec![], vec![keyword], &config);
        assert_eq!(ranked.len(), 1);
    }

    #[test]
    fn hybrid_can_be_switched_off() {
        let config = RetrievalConfig {
            hybrid: false,
            ..RetrievalConfig::default()
        };
        let ranked = rank(
            vec![hit("v", 0.9)],
            vec![Candidate::new("kw", "x")],
            &config,
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].chunk_id, "v");
    }

    #[test]
    fn the_result_never_exceeds_top_k() {
        let config = RetrievalConfig::default();
        let hits: Vec<Candidate> = (0..40).map(|i| hit(&format!("c{i}"), 0.9)).collect();
        assert_eq!(rank(hits, vec![], &config).len(), config.top_k);
    }
}
