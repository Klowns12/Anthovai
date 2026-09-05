//! A deterministic embedder with no network behind it.
//!
//! This lives here rather than in the test kit because it is not only for
//! tests: a developer with no provider key can still run the whole platform
//! against it, and see documents ingest and questions retrieve. What it cannot
//! do is tell you whether retrieval is any *good* — the vectors carry word
//! overlap and nothing else. Anything measuring answer quality has to run
//! against a real model.

use anthovai_core::Result;
use async_trait::async_trait;

use crate::EmbeddingProvider;

/// Namespace for models produced here. A knowledge base built with one of these
/// carries the name, so it can never be mistaken for real embeddings — and can
/// be found and re-embedded when a key arrives.
pub const FAKE_MODEL_PREFIX: &str = "fake:";

pub struct HashEmbedder {
    dimension: usize,
    model_id: String,
}

impl HashEmbedder {
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            model_id: format!("{FAKE_MODEL_PREFIX}hash-{dimension}"),
        }
    }
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self::new(1536)
    }
}

#[async_trait]
impl EmbeddingProvider for HashEmbedder {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(inputs.iter().map(|t| embed(t, self.dimension)).collect())
    }
}

/// A bag-of-words vector: each word lands in one dimension, then the whole
/// thing is normalised so cosine similarity behaves.
fn embed(text: &str, dimension: usize) -> Vec<f32> {
    let mut vector = vec![0.0_f32; dimension];

    for word in text.to_lowercase().split_whitespace() {
        let mut hash: u64 = 1469598103934665603;
        for byte in word.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(1099511628211);
        }
        vector[(hash as usize) % dimension] += 1.0;
    }

    let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

/// Whether a knowledge base was built with one of these, and so needs
/// re-embedding once a real provider is configured.
pub fn is_fake_model(model_id: &str) -> bool {
    model_id.starts_with(FAKE_MODEL_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_same_text_always_gives_the_same_vector() {
        let embedder = HashEmbedder::new(64);
        let first = embedder.embed_one("rust course").await.unwrap();
        let second = embedder.embed_one("rust course").await.unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[tokio::test]
    async fn shared_words_bring_vectors_closer() {
        let embedder = HashEmbedder::new(256);
        let query = embedder.embed_one("rust course duration").await.unwrap();
        let related = embedder
            .embed_one("the rust course duration is twelve weeks")
            .await
            .unwrap();
        let unrelated = embedder
            .embed_one("cafeteria menu and opening hours")
            .await
            .unwrap();

        let similarity =
            |a: &[f32], b: &[f32]| -> f32 { a.iter().zip(b).map(|(x, y)| x * y).sum() };

        assert!(similarity(&query, &related) > similarity(&query, &unrelated));
    }

    #[tokio::test]
    async fn a_batch_gives_one_vector_per_input() {
        let embedder = HashEmbedder::new(32);
        let vectors = embedder
            .embed_batch(&["one".into(), "two".into(), "three".into()])
            .await
            .unwrap();

        assert_eq!(vectors.len(), 3);
        assert!(vectors.iter().all(|v| v.len() == 32));
    }

    #[tokio::test]
    async fn an_empty_batch_is_not_an_error() {
        assert!(HashEmbedder::new(32)
            .embed_batch(&[])
            .await
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_model_id_says_it_is_not_real() {
        let embedder = HashEmbedder::new(1536);
        assert!(is_fake_model(embedder.model_id()));
        assert!(!is_fake_model("openai:text-embedding-3-small"));
    }
}
