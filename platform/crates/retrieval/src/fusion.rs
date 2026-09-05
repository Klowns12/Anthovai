//! Combining and trimming candidate lists.
//!
//! Two ranked lists come back from the database, one from vector similarity and
//! one from keyword search. Reciprocal Rank Fusion merges them without needing
//! the two score scales to be comparable, then Maximal Marginal Relevance drops
//! near-duplicates so the context window is not spent three times on the same
//! paragraph.

use std::collections::HashMap;

/// One candidate chunk as it comes out of a search.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub chunk_id: String,
    pub document_id: String,
    pub content: String,
    pub token_count: u32,
    /// Cosine similarity, present only for vector hits.
    pub vector_score: Option<f32>,
    /// Fused score, filled in by `reciprocal_rank_fusion`.
    pub score: f32,
    pub embedding: Vec<f32>,
    /// What ingestion recorded about this chunk: the document title, the
    /// heading it sits under, a page number. This is what a citation is built
    /// from, so it travels with the chunk rather than being looked up again.
    pub metadata: serde_json::Value,
}

impl Candidate {
    pub fn new(chunk_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            chunk_id: chunk_id.into(),
            document_id: String::new(),
            content: content.into(),
            token_count: 0,
            vector_score: None,
            score: 0.0,
            embedding: Vec::new(),
            metadata: serde_json::Value::Null,
        }
    }

    /// A string field from the chunk's metadata.
    pub fn meta_str(&self, key: &str) -> Option<&str> {
        self.metadata.get(key)?.as_str()
    }

    /// A number field from the chunk's metadata.
    pub fn meta_u32(&self, key: &str) -> Option<u32> {
        self.metadata
            .get(key)?
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
    }
}

/// Merge ranked lists. `k` damps the influence of the top of each list; 60 is
/// the value from the original paper and the one the spec fixes.
pub fn reciprocal_rank_fusion(lists: &[Vec<Candidate>], k: f32) -> Vec<Candidate> {
    let mut scores: HashMap<&str, f32> = HashMap::new();
    let mut best: HashMap<&str, &Candidate> = HashMap::new();

    for list in lists {
        for (rank, candidate) in list.iter().enumerate() {
            let contribution = 1.0 / (k + (rank as f32) + 1.0);
            *scores.entry(&candidate.chunk_id).or_insert(0.0) += contribution;
            best.entry(&candidate.chunk_id).or_insert(candidate);
        }
    }

    let mut fused: Vec<Candidate> = best
        .into_iter()
        .map(|(id, candidate)| {
            let mut merged = candidate.clone();
            merged.score = scores[id];
            merged
        })
        .collect();

    // Ties are broken by chunk id so the ordering is deterministic and tests
    // are not flaky.
    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chunk_id.cmp(&b.chunk_id))
    });
    fused
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

/// Greedy Maximal Marginal Relevance. `lambda` weighs relevance against
/// novelty: 1.0 keeps the original order, 0.0 maximises diversity.
/// Candidates without embeddings fall back to relevance order.
pub fn maximal_marginal_relevance(
    candidates: &[Candidate],
    lambda: f32,
    limit: usize,
) -> Vec<Candidate> {
    if candidates.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut remaining: Vec<&Candidate> = candidates.iter().collect();
    let mut selected: Vec<Candidate> = Vec::new();

    // The most relevant candidate is always taken first.
    let first = remaining.remove(0);
    selected.push(first.clone());

    while selected.len() < limit && !remaining.is_empty() {
        let mut best_index = 0;
        let mut best_value = f32::NEG_INFINITY;

        for (i, candidate) in remaining.iter().enumerate() {
            let max_similarity = selected
                .iter()
                .map(|s| cosine_similarity(&candidate.embedding, &s.embedding))
                .fold(0.0_f32, f32::max);
            let value = lambda * candidate.score - (1.0 - lambda) * max_similarity;
            if value > best_value {
                best_value = value;
                best_index = i;
            }
        }

        selected.push(remaining.remove(best_index).clone());
    }

    selected
}

/// Take candidates in order until the token budget or the count limit is hit.
/// A single candidate larger than the whole budget is skipped, not truncated.
pub fn select_within_budget(
    candidates: &[Candidate],
    token_budget: u32,
    max_items: usize,
) -> Vec<Candidate> {
    let mut used = 0_u32;
    let mut out = Vec::new();
    for candidate in candidates {
        if out.len() >= max_items {
            break;
        }
        if used + candidate.token_count > token_budget {
            continue;
        }
        used += candidate.token_count;
        out.push(candidate.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, tokens: u32, embedding: Vec<f32>) -> Candidate {
        Candidate {
            chunk_id: id.into(),
            document_id: "doc".into(),
            content: format!("content of {id}"),
            token_count: tokens,
            vector_score: None,
            score: 0.0,
            embedding,
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn fusion_rewards_appearing_in_both_lists() {
        let vector = vec![Candidate::new("a", ""), Candidate::new("b", "")];
        let keyword = vec![Candidate::new("c", ""), Candidate::new("a", "")];

        let fused = reciprocal_rank_fusion(&[vector, keyword], 60.0);

        assert_eq!(fused[0].chunk_id, "a", "a is in both lists so it wins");
        assert_eq!(fused.len(), 3);
    }

    #[test]
    fn fusion_is_deterministic_for_ties() {
        let one = vec![Candidate::new("b", ""), Candidate::new("a", "")];
        let two = vec![Candidate::new("a", ""), Candidate::new("b", "")];
        let first = reciprocal_rank_fusion(&[one.clone(), two.clone()], 60.0);
        let second = reciprocal_rank_fusion(&[one, two], 60.0);
        let ids = |v: &[Candidate]| v.iter().map(|c| c.chunk_id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&first), ids(&second));
    }

    #[test]
    fn fusion_of_an_empty_input_is_empty() {
        assert!(reciprocal_rank_fusion(&[], 60.0).is_empty());
    }

    #[test]
    fn cosine_handles_degenerate_vectors() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[1.0, 0.0, 0.0]), 0.0);
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mmr_drops_a_near_duplicate_in_favour_of_something_new() {
        let mut top = candidate("a", 10, vec![1.0, 0.0]);
        top.score = 0.9;
        let mut duplicate = candidate("a-copy", 10, vec![0.99, 0.01]);
        duplicate.score = 0.85;
        let mut different = candidate("b", 10, vec![0.0, 1.0]);
        different.score = 0.6;

        let picked = maximal_marginal_relevance(&[top, duplicate, different], 0.7, 2);

        let ids: Vec<&str> = picked.iter().map(|c| c.chunk_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn mmr_with_lambda_one_keeps_relevance_order() {
        let mut a = candidate("a", 10, vec![1.0, 0.0]);
        a.score = 0.9;
        let mut b = candidate("b", 10, vec![0.99, 0.0]);
        b.score = 0.8;
        let picked = maximal_marginal_relevance(&[a, b], 1.0, 2);
        assert_eq!(picked[1].chunk_id, "b");
    }

    #[test]
    fn budget_selection_stops_at_the_token_limit() {
        let candidates = vec![
            candidate("a", 400, vec![]),
            candidate("b", 400, vec![]),
            candidate("c", 400, vec![]),
        ];
        let picked = select_within_budget(&candidates, 900, 10);
        assert_eq!(picked.len(), 2);
    }

    #[test]
    fn budget_selection_skips_an_oversized_chunk_and_keeps_going() {
        let candidates = vec![
            candidate("huge", 5_000, vec![]),
            candidate("small", 100, vec![]),
        ];
        let picked = select_within_budget(&candidates, 1_000, 10);
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].chunk_id, "small");
    }

    #[test]
    fn budget_selection_respects_max_items() {
        let candidates: Vec<Candidate> = (0..20)
            .map(|i| candidate(&format!("c{i}"), 1, vec![]))
            .collect();
        assert_eq!(select_within_budget(&candidates, 10_000, 8).len(), 8);
    }
}
