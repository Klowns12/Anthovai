//! Turning a document's chunks into vectors.
//!
//! Two things make this more than a loop over the provider. Chunks whose text
//! we have embedded before are reused rather than paid for again — re-uploading
//! a handbook with one paragraph changed should cost one paragraph. And the
//! rest go out in batches with a bounded number in flight, because a thousand
//! chunks sent one at a time is a thousand round trips, and sent all at once is
//! a rate limit.

use std::collections::HashMap;
use std::sync::Arc;

use anthovai_core::{DomainError, Result};
use futures::stream::{self, StreamExt, TryStreamExt};

use crate::batching::plan_batches;
use crate::{content_hash, EmbeddingProvider};

/// Where a vector for an already-seen chunk can be found.
///
/// Implemented against `document_chunks`, and kept as a trait so this crate
/// does not need the database — and so a test can decide exactly what was
/// already known.
#[async_trait::async_trait]
pub trait VectorCache: Send + Sync {
    /// Vectors for whichever of these content hashes are already stored.
    async fn lookup(&self, hashes: &[String]) -> Result<HashMap<String, Vec<f32>>>;
}

/// Nothing is ever reused. The honest default for a fresh knowledge base.
pub struct NoCache;

#[async_trait::async_trait]
impl VectorCache for NoCache {
    async fn lookup(&self, _hashes: &[String]) -> Result<HashMap<String, Vec<f32>>> {
        Ok(HashMap::new())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RunnerConfig {
    pub batch_size: usize,
    pub concurrency: usize,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            batch_size: 64,
            concurrency: 4,
        }
    }
}

/// One embedded chunk.
#[derive(Clone, Debug)]
pub struct Embedded {
    pub content_hash: String,
    pub vector: Vec<f32>,
    /// True when this came from the cache and cost nothing.
    pub reused: bool,
}

/// What a run produced, and what it cost.
#[derive(Clone, Debug)]
pub struct EmbeddingRun {
    pub embedded: Vec<Embedded>,
    pub reused: usize,
    /// Tokens sent to the provider. Zero when everything was reused.
    pub billable_tokens: u32,
}

pub struct EmbeddingRunner {
    provider: Arc<dyn EmbeddingProvider>,
    config: RunnerConfig,
}

impl EmbeddingRunner {
    pub fn new(provider: Arc<dyn EmbeddingProvider>, config: RunnerConfig) -> Self {
        Self { provider, config }
    }

    pub fn model_id(&self) -> &str {
        self.provider.model_id()
    }

    pub fn dimension(&self) -> usize {
        self.provider.dimension()
    }

    /// Embed these texts, in order, reusing whatever the cache already knows.
    ///
    /// `token_count` is what the caller already computed while chunking; it is
    /// passed in rather than recomputed so the same number reaches the usage
    /// record.
    pub async fn run(
        &self,
        texts: &[String],
        token_counts: &[u32],
        cache: &dyn VectorCache,
    ) -> Result<EmbeddingRun> {
        if texts.is_empty() {
            return Ok(EmbeddingRun {
                embedded: Vec::new(),
                reused: 0,
                billable_tokens: 0,
            });
        }
        if texts.len() != token_counts.len() {
            return Err(DomainError::Internal(anyhow::anyhow!(
                "{} texts but {} token counts",
                texts.len(),
                token_counts.len()
            )));
        }

        let hashes: Vec<String> = texts.iter().map(|t| content_hash(t)).collect();
        let known = cache.lookup(&hashes).await?;

        // Only what is genuinely new goes to the provider, and only once each:
        // a document that repeats a paragraph should not pay for it twice.
        let mut wanted: Vec<String> = Vec::new();
        let mut wanted_index: HashMap<&str, usize> = HashMap::new();
        let mut billable_tokens: u32 = 0;

        for (i, hash) in hashes.iter().enumerate() {
            if known.contains_key(hash) || wanted_index.contains_key(hash.as_str()) {
                continue;
            }
            wanted_index.insert(hash, wanted.len());
            wanted.push(texts[i].clone());
            billable_tokens = billable_tokens.saturating_add(token_counts[i]);
        }

        let fresh = self.embed_all(&wanted).await?;

        let embedded = hashes
            .iter()
            .map(|hash| match known.get(hash) {
                Some(vector) => Ok(Embedded {
                    content_hash: hash.clone(),
                    vector: vector.clone(),
                    reused: true,
                }),
                None => {
                    let index = wanted_index[hash.as_str()];
                    Ok(Embedded {
                        content_hash: hash.clone(),
                        vector: fresh[index].clone(),
                        reused: false,
                    })
                }
            })
            .collect::<Result<Vec<_>>>()?;

        let reused = embedded.iter().filter(|e| e.reused).count();

        Ok(EmbeddingRun {
            embedded,
            reused,
            billable_tokens,
        })
    }

    /// Batch, run a bounded number at a time, and reassemble in order.
    async fn embed_all(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let batches = plan_batches(texts.len(), self.config.batch_size);
        let expected = self.provider.dimension();

        let results: Vec<Vec<Vec<f32>>> = stream::iter(batches.into_iter().map(|batch| {
            let provider = Arc::clone(&self.provider);
            let slice = texts[batch.start..batch.end].to_vec();

            async move {
                let vectors = provider.embed_batch(&slice).await?;

                // A provider returning the wrong number of vectors, or vectors
                // of the wrong width, would corrupt the index quietly: chunks
                // would be stored against the wrong text.
                if vectors.len() != slice.len() {
                    return Err(DomainError::Internal(anyhow::anyhow!(
                        "asked for {} embeddings, got {}",
                        slice.len(),
                        vectors.len()
                    )));
                }
                if let Some(bad) = vectors.iter().find(|v| v.len() != expected) {
                    return Err(DomainError::Internal(anyhow::anyhow!(
                        "expected {expected}-dimensional vectors, got {}",
                        bad.len()
                    )));
                }
                Ok(vectors)
            }
        }))
        .buffered(self.config.concurrency.max(1))
        .try_collect()
        .await?;

        Ok(results.into_iter().flatten().collect())
    }
}

impl std::fmt::Debug for EmbeddingRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingRunner")
            .field("model", &self.provider.model_id())
            .field("dimension", &self.provider.dimension())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::hash_embedder::HashEmbedder;

    /// Counts what it was asked to do, so a test can prove work was skipped.
    struct CountingProvider {
        inner: HashEmbedder,
        calls: AtomicUsize,
        texts: AtomicUsize,
    }

    impl CountingProvider {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                inner: HashEmbedder::new(32),
                calls: AtomicUsize::new(0),
                texts: AtomicUsize::new(0),
            })
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for CountingProvider {
        fn model_id(&self) -> &str {
            self.inner.model_id()
        }

        fn dimension(&self) -> usize {
            self.inner.dimension()
        }

        async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.texts.fetch_add(inputs.len(), Ordering::SeqCst);
            self.inner.embed_batch(inputs).await
        }
    }

    struct Cached(HashMap<String, Vec<f32>>);

    #[async_trait::async_trait]
    impl VectorCache for Cached {
        async fn lookup(&self, hashes: &[String]) -> Result<HashMap<String, Vec<f32>>> {
            Ok(hashes
                .iter()
                .filter_map(|h| self.0.get(h).map(|v| (h.clone(), v.clone())))
                .collect())
        }
    }

    fn texts(n: usize) -> (Vec<String>, Vec<u32>) {
        let texts: Vec<String> = (0..n).map(|i| format!("chunk number {i}")).collect();
        let counts = vec![10_u32; n];
        (texts, counts)
    }

    #[tokio::test]
    async fn every_text_gets_a_vector_in_order() {
        let provider = CountingProvider::new();
        let runner = EmbeddingRunner::new(provider.clone(), RunnerConfig::default());
        let (texts, counts) = texts(5);

        let run = runner.run(&texts, &counts, &NoCache).await.unwrap();

        assert_eq!(run.embedded.len(), 5);
        assert_eq!(run.reused, 0);
        for (i, embedded) in run.embedded.iter().enumerate() {
            assert_eq!(embedded.content_hash, content_hash(&texts[i]));
            assert_eq!(embedded.vector.len(), 32);
        }
    }

    #[tokio::test]
    async fn work_is_batched_rather_than_sent_one_at_a_time() {
        let provider = CountingProvider::new();
        let runner = EmbeddingRunner::new(
            provider.clone(),
            RunnerConfig {
                batch_size: 10,
                concurrency: 2,
            },
        );
        let (texts, counts) = texts(25);

        runner.run(&texts, &counts, &NoCache).await.unwrap();

        assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
        assert_eq!(provider.texts.load(Ordering::SeqCst), 25);
    }

    #[tokio::test]
    async fn text_we_have_seen_before_costs_nothing() {
        let provider = CountingProvider::new();
        let runner = EmbeddingRunner::new(provider.clone(), RunnerConfig::default());
        let (texts, counts) = texts(4);

        // Everything but the last chunk is already known — the shape of a
        // re-upload with one paragraph changed.
        let mut known = HashMap::new();
        for text in texts.iter().take(3) {
            known.insert(content_hash(text), vec![0.5_f32; 32]);
        }

        let run = runner.run(&texts, &counts, &Cached(known)).await.unwrap();

        assert_eq!(run.reused, 3);
        assert_eq!(provider.texts.load(Ordering::SeqCst), 1);
        assert_eq!(
            run.billable_tokens, 10,
            "only the new chunk should be billed"
        );
        assert!(run.embedded[0].reused);
        assert!(!run.embedded[3].reused);
    }

    #[tokio::test]
    async fn nothing_is_called_when_everything_is_cached() {
        let provider = CountingProvider::new();
        let runner = EmbeddingRunner::new(provider.clone(), RunnerConfig::default());
        let (texts, counts) = texts(3);

        let known: HashMap<String, Vec<f32>> = texts
            .iter()
            .map(|t| (content_hash(t), vec![0.1_f32; 32]))
            .collect();

        let run = runner.run(&texts, &counts, &Cached(known)).await.unwrap();

        assert_eq!(run.reused, 3);
        assert_eq!(run.billable_tokens, 0);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn duplicate_text_within_one_document_is_only_paid_for_once() {
        let provider = CountingProvider::new();
        let runner = EmbeddingRunner::new(provider.clone(), RunnerConfig::default());

        let texts = vec![
            "the same boilerplate".to_owned(),
            "something different".to_owned(),
            "the same boilerplate".to_owned(),
        ];
        let counts = vec![5, 5, 5];

        let run = runner.run(&texts, &counts, &NoCache).await.unwrap();

        assert_eq!(run.embedded.len(), 3);
        assert_eq!(provider.texts.load(Ordering::SeqCst), 2);
        assert_eq!(run.billable_tokens, 10);
        assert_eq!(
            run.embedded[0].vector, run.embedded[2].vector,
            "identical text must get an identical vector"
        );
    }

    #[tokio::test]
    async fn an_empty_run_does_nothing() {
        let provider = CountingProvider::new();
        let runner = EmbeddingRunner::new(provider.clone(), RunnerConfig::default());

        let run = runner.run(&[], &[], &NoCache).await.unwrap();

        assert!(run.embedded.is_empty());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn mismatched_inputs_are_refused_rather_than_guessed_at() {
        let runner = EmbeddingRunner::new(CountingProvider::new(), RunnerConfig::default());
        let err = runner
            .run(&["one".to_owned()], &[], &NoCache)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "internal_error");
    }

    /// A provider that returns the wrong shape.
    struct BrokenProvider;

    #[async_trait::async_trait]
    impl EmbeddingProvider for BrokenProvider {
        fn model_id(&self) -> &str {
            "broken"
        }

        fn dimension(&self) -> usize {
            1536
        }

        async fn embed_batch(&self, _inputs: &[String]) -> Result<Vec<Vec<f32>>> {
            // One vector, of the wrong width, for however many were asked for.
            Ok(vec![vec![0.0; 8]])
        }
    }

    #[tokio::test]
    async fn a_provider_returning_the_wrong_shape_fails_loudly() {
        let runner = EmbeddingRunner::new(Arc::new(BrokenProvider), RunnerConfig::default());
        let (texts, counts) = texts(3);

        // Storing these would put vectors against the wrong text, and the only
        // symptom would be retrieval quietly returning nonsense.
        let err = runner.run(&texts, &counts, &NoCache).await.unwrap_err();
        assert_eq!(err.code(), "internal_error");
    }
}
