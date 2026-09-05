//! Finding the passages that should answer a question.
//!
//! The one rule that shapes everything here: a query has to be embedded by the
//! same model as the chunks it is searching. Vectors from two different models
//! are not comparable — the distances they produce are meaningless rather than
//! merely inaccurate — so the embedding model is a property of the knowledge
//! base, and a base whose model we can no longer run is reported rather than
//! quietly skipped.

use std::collections::HashMap;
use std::sync::Arc;

use anthovai_core::{DomainError, KnowledgeBaseId, Result, TenantCtx};
use anthovai_db::Db;
use anthovai_embeddings::EmbeddingProvider;
use tracing::debug;

use crate::context::{ContextBuilder, RetrievedContext};
use crate::fusion::Candidate;
use crate::search::{self, SearchFilters};
use crate::{rank, RetrievalConfig};

pub struct Retriever {
    db: Db,
    /// Keyed by the namespaced model id a knowledge base records, so a base
    /// built with a model this deployment no longer runs is recognised.
    embedders: HashMap<String, Arc<dyn EmbeddingProvider>>,
    context: ContextBuilder,
}

/// What one retrieval produced, and enough about how to explain it.
#[derive(Clone, Debug, Default)]
pub struct Retrieved {
    pub context: RetrievedContext,
    pub candidates: Vec<Candidate>,
    /// Tokens spent embedding the question. Small, but it is real money and it
    /// belongs on the usage record.
    pub embedding_tokens: u32,
}

impl Retrieved {
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

impl Retriever {
    pub fn new(db: Db, embedders: Vec<Arc<dyn EmbeddingProvider>>) -> Self {
        Self {
            db,
            embedders: embedders
                .into_iter()
                .map(|e| (e.model_id().to_owned(), e))
                .collect(),
            context: ContextBuilder::new(),
        }
    }

    /// Search the agent's knowledge for a question.
    ///
    /// Knowledge bases are grouped by embedding model, so the question is
    /// embedded once per distinct model rather than once per base.
    pub async fn retrieve(
        &self,
        ctx: &TenantCtx,
        knowledge_base_ids: &[KnowledgeBaseId],
        query: &str,
        filters: &SearchFilters,
        config: &RetrievalConfig,
    ) -> Result<Retrieved> {
        if knowledge_base_ids.is_empty() || query.trim().is_empty() {
            return Ok(Retrieved::default());
        }

        // Timed from here rather than from the caller: what is measured is the
        // embed-and-search itself, which is the part that can quietly get slow
        // as a knowledge base grows past what the index handles well.
        let started = std::time::Instant::now();
        let by_model = self.group_by_model(ctx, knowledge_base_ids).await?;

        let mut vector_hits: Vec<Candidate> = Vec::new();
        let mut keyword_hits: Vec<Candidate> = Vec::new();
        let mut embedding_tokens = 0_u32;

        for (model_id, bases) in by_model {
            let embedder = self.embedders.get(&model_id).ok_or_else(|| {
                // Searching the bases we *can* read and staying silent about
                // the rest would answer from half the customer's knowledge and
                // look entirely successful.
                debug!(%model_id, "no embedder for a knowledge base's model");
                DomainError::Conflict("knowledge_base_needs_reembedding")
            })?;

            let query_vector = embedder.embed_one(query).await?;
            embedding_tokens = embedding_tokens.saturating_add(estimate_query_tokens(query));

            let mut db = self.db.tenant(ctx).await?;

            vector_hits.extend(
                search::vector_search(
                    &mut db,
                    &bases,
                    &query_vector,
                    filters,
                    config.vector_top as i64,
                )
                .await?,
            );

            if config.hybrid {
                keyword_hits.extend(
                    search::keyword_search(
                        &mut db,
                        &bases,
                        query,
                        filters,
                        config.keyword_top as i64,
                    )
                    .await?,
                );
            }

            db.commit().await?;
        }

        let candidates = rank(vector_hits, keyword_hits, config);
        let context = self.context.build(&candidates);

        metrics::histogram!("retrieval_duration_seconds").record(started.elapsed().as_secs_f64());

        Ok(Retrieved {
            context,
            candidates,
            embedding_tokens,
        })
    }

    /// Which model each knowledge base was built with.
    ///
    /// Reading them in one query also proves they all belong to this tenant:
    /// a base from another one simply does not come back, and the count check
    /// turns that into a plain "not found".
    async fn group_by_model(
        &self,
        ctx: &TenantCtx,
        knowledge_base_ids: &[KnowledgeBaseId],
    ) -> Result<Vec<(String, Vec<KnowledgeBaseId>)>> {
        let mut db = self.db.tenant(ctx).await?;
        let tenant = db.tenant_key();
        let ids: Vec<String> = knowledge_base_ids.iter().map(|k| k.to_db()).collect();

        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, embedding_model FROM knowledge_bases
             WHERE tenant_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        )
        .bind(&tenant)
        .bind(&ids)
        .fetch_all(db.conn())
        .await?;
        db.commit().await?;

        if rows.len() != knowledge_base_ids.len() {
            return Err(DomainError::NotFound("knowledge_base"));
        }

        let mut grouped: HashMap<String, Vec<KnowledgeBaseId>> = HashMap::new();
        for (id, model) in rows {
            grouped
                .entry(model)
                .or_default()
                .push(KnowledgeBaseId::from_db(&id)?);
        }

        // Deterministic order, so a debug trace of two identical requests looks
        // the same and a test can rely on it.
        let mut grouped: Vec<(String, Vec<KnowledgeBaseId>)> = grouped.into_iter().collect();
        grouped.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(grouped)
    }
}

/// Roughly what embedding a question cost.
///
/// The providers do report exact usage; this is the estimate used until that is
/// threaded through, and a question is short enough that the difference is
/// immaterial next to the chat call it precedes.
fn estimate_query_tokens(query: &str) -> u32 {
    let words = query.split_whitespace().count();
    let chars = query.chars().count();
    words.max(chars / 4).max(1) as u32
}

impl std::fmt::Debug for Retriever {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Retriever")
            .field("models", &self.embedders.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_always_costs_at_least_one_token() {
        assert_eq!(estimate_query_tokens(""), 1);
    }

    #[test]
    fn thai_questions_are_not_counted_as_one_token() {
        let thai = "หลักสูตรนี้ใช้เวลาเรียนกี่สัปดาห์";
        assert_eq!(thai.split_whitespace().count(), 1);
        assert!(estimate_query_tokens(thai) > 5);
    }
}
