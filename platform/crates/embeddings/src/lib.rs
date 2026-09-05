//! Embedding generation.
//!
//! Chat and embedding models are separate concerns and separate traits: a
//! provider may serve one and not the other, and today Anthropic serves chat
//! only. Embeddings are also content-addressed, so re-uploading an unchanged
//! document costs nothing.

use anthovai_core::Result;
use async_trait::async_trait;
use sha2::{Digest, Sha256};

pub mod batching;
pub mod hash_embedder;
pub mod runner;

pub use batching::{plan_batches, BatchPlan};
pub use hash_embedder::{is_fake_model, HashEmbedder, FAKE_MODEL_PREFIX};
pub use runner::{Embedded, EmbeddingRun, EmbeddingRunner, NoCache, RunnerConfig, VectorCache};

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Namespaced model id, e.g. `openai:text-embedding-3-small`. Stored on the
    /// knowledge base so a query is always embedded with the same model as the
    /// chunks it is searching.
    fn model_id(&self) -> &str;
    fn dimension(&self) -> usize;
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>>;

    async fn embed_one(&self, input: &str) -> Result<Vec<f32>> {
        let mut out = self.embed_batch(&[input.to_owned()]).await?;
        out.pop().ok_or_else(|| {
            anthovai_core::DomainError::Internal(anyhow::anyhow!(
                "embedding provider returned no vectors"
            ))
        })
    }
}

/// Content hash used to skip re-embedding identical chunks.
pub fn content_hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_hashes_identically() {
        assert_eq!(content_hash("หลักสูตร Rust"), content_hash("หลักสูตร Rust"));
    }

    #[test]
    fn different_text_hashes_differently() {
        assert_ne!(content_hash("a"), content_hash("b"));
    }

    #[test]
    fn whitespace_matters_because_it_changes_the_tokens() {
        assert_ne!(content_hash("a b"), content_hash("a  b"));
    }
}
