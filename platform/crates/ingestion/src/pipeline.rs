//! The ingestion pipeline: bytes in storage to chunks in the index.
//!
//! Every step reports its progress to the document row, because from the
//! customer's side this is a progress bar that has to move. And every failure
//! has to leave the document in a state they can act on — a code and a message,
//! not a spinner that never stops.
//!
//! The previous version keeps serving throughout. It is only retired once the
//! new one is written, so a re-upload never leaves a gap where the document
//! cannot be found, and a failed re-ingestion leaves the old version in place.

use std::sync::Arc;

use anthovai_core::{DocumentId, OrgId, TenantCtx};
use anthovai_db::Db;
use anthovai_embeddings::{EmbeddingRunner, RunnerConfig};
use anthovai_knowledge::{repo as knowledge_repo, DocumentStatus};
use anthovai_retrieval::chunk_repo::{self, ChunkToInsert, StoredVectors};
use anthovai_storage::Storage;
use tracing::{debug, info, warn};

use crate::chunker::{chunk, ChunkConfig, ChunkDraft};
use crate::parsers::ParserRegistry;
use crate::{error_codes, IngestError, ParseInput};

pub struct IngestPipeline {
    db: Db,
    storage: Storage,
    parsers: ParserRegistry,
    embeddings: Arc<EmbeddingRunner>,
    chunk_config: ChunkConfig,
}

/// What one run produced, for the log and for the usage record.
#[derive(Clone, Copy, Debug, Default)]
pub struct IngestOutcome {
    pub chunks: usize,
    pub tokens: u32,
    pub reused_vectors: usize,
    pub billable_tokens: u32,
}

impl IngestPipeline {
    pub fn new(
        db: Db,
        storage: Storage,
        embeddings: Arc<EmbeddingRunner>,
        chunk_config: ChunkConfig,
    ) -> Self {
        Self {
            db,
            storage,
            parsers: ParserRegistry::new(),
            embeddings,
            chunk_config,
        }
    }

    pub fn with_parsers(mut self, parsers: ParserRegistry) -> Self {
        self.parsers = parsers;
        self
    }

    /// Run one document through, recording the outcome on the document row.
    ///
    /// Errors are recorded here as well as returned: the queue decides whether
    /// to retry, but the customer needs to see what happened either way.
    pub async fn run(
        &self,
        org_id: OrgId,
        document_id: DocumentId,
        version: i32,
    ) -> std::result::Result<IngestOutcome, IngestError> {
        match self.ingest(org_id, document_id, version).await {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.record_failure(org_id, document_id, version, &error)
                    .await;
                Err(error)
            }
        }
    }

    async fn ingest(
        &self,
        org_id: OrgId,
        document_id: DocumentId,
        version: i32,
    ) -> std::result::Result<IngestOutcome, IngestError> {
        let ctx = TenantCtx::system(org_id, anthovai_core::Plan::Enterprise);

        // ---- load ---------------------------------------------------------
        let mut db = self.db.tenant(&ctx).await.map_err(IngestError::transient)?;
        let document = knowledge_repo::find_document_including_deleted(&mut db, document_id)
            .await
            .map_err(|e| IngestError::permanent(error_codes::DOCUMENT_MISSING, e.to_string()))?;

        // Deleted while the job waited in the queue. Nothing to do, and nothing
        // to report: this is an ordinary race, not a failure.
        if document.status == DocumentStatus::Deleted {
            debug!(%document_id, "document was deleted before ingestion began");
            return Ok(IngestOutcome::default());
        }

        let knowledge_base_id = document.knowledge_base_id;
        let storage_key = document.storage_key.clone().ok_or_else(|| {
            IngestError::permanent(
                error_codes::DOCUMENT_MISSING,
                "the document has no stored file",
            )
        })?;

        knowledge_repo::set_status(&mut db, document_id, DocumentStatus::Processing)
            .await
            .map_err(IngestError::transient)?;
        db.commit().await.map_err(IngestError::transient)?;

        // ---- parse --------------------------------------------------------
        let bytes = self
            .storage
            .get(&storage_key)
            .await
            .map_err(|e| IngestError::permanent(error_codes::FILE_MISSING, e.to_string()))?;

        let parser = self
            .parsers
            .for_type(document.source_type)
            .ok_or_else(|| {
                IngestError::permanent(
                    error_codes::UNSUPPORTED_FILE_TYPE,
                    format!("no parser for `{}` files", document.source_type.as_str()),
                )
            })?
            .clone();

        let parsed = parser
            .parse(ParseInput {
                bytes,
                source_type: document.source_type,
                filename: Some(document.title.clone()),
                source_url: document.source_url.clone(),
            })
            .await
            .map_err(|e| IngestError::permanent(error_codes::NO_EXTRACTABLE_TEXT, e.to_string()))?;

        // ---- chunk --------------------------------------------------------
        self.set_status(org_id, document_id, DocumentStatus::Chunking)
            .await?;

        let drafts = chunk(&parsed, &self.chunk_config);
        if drafts.is_empty() {
            return Err(IngestError::permanent(
                error_codes::NO_EXTRACTABLE_TEXT,
                "the document produced no chunks",
            ));
        }

        // ---- embed --------------------------------------------------------
        self.set_status(org_id, document_id, DocumentStatus::Embedding)
            .await?;

        let texts: Vec<String> = drafts.iter().map(|d| d.content.clone()).collect();
        let token_counts: Vec<u32> = drafts.iter().map(|d| d.token_count as u32).collect();

        let cache = StoredVectors::new(self.db.clone(), org_id, knowledge_base_id);
        let run = self
            .embeddings
            .run(&texts, &token_counts, &cache)
            .await
            .map_err(|e| {
                IngestError::transient_with(error_codes::EMBEDDING_FAILED, e.to_string())
            })?;

        // ---- index --------------------------------------------------------
        self.set_status(org_id, document_id, DocumentStatus::Indexing)
            .await?;

        let to_insert: Vec<ChunkToInsert> = drafts
            .iter()
            .zip(run.embedded.iter())
            .map(|(draft, embedded)| ChunkToInsert {
                chunk_index: draft.index as i32,
                content: draft.content.clone(),
                content_hash: embedded.content_hash.clone(),
                token_count: draft.token_count as i32,
                vector: embedded.vector.clone(),
                metadata: metadata_for(draft, &parsed, document.source_type.as_str()),
            })
            .collect();

        let total_tokens: u32 = token_counts.iter().sum();

        let mut db = self.db.tenant(&ctx).await.map_err(IngestError::transient)?;

        // Guard against the race the whole way through: a document deleted
        // during a slow embedding call must not come back with fresh chunks.
        let current = knowledge_repo::find_document_including_deleted(&mut db, document_id)
            .await
            .map_err(|e| IngestError::permanent(error_codes::DOCUMENT_MISSING, e.to_string()))?;
        if current.status == DocumentStatus::Deleted {
            debug!(%document_id, "document was deleted during ingestion");
            return Ok(IngestOutcome::default());
        }

        // A retry may have left a partial version behind.
        chunk_repo::discard_version(&mut db, document_id, version)
            .await
            .map_err(IngestError::transient)?;

        chunk_repo::insert_chunks(&mut db, knowledge_base_id, document_id, version, &to_insert)
            .await
            .map_err(IngestError::transient)?;

        // Only now is the previous version retired: until this point a search
        // was still being served by it.
        chunk_repo::retire_other_versions(&mut db, document_id, version)
            .await
            .map_err(IngestError::transient)?;

        knowledge_repo::set_ready(
            &mut db,
            document_id,
            version,
            to_insert.len() as i32,
            total_tokens as i32,
            parsed.language.as_deref(),
        )
        .await
        .map_err(IngestError::transient)?;

        db.commit().await.map_err(IngestError::transient)?;

        info!(
            %document_id,
            version,
            chunks = to_insert.len(),
            reused = run.reused,
            "document indexed"
        );

        Ok(IngestOutcome {
            chunks: to_insert.len(),
            tokens: total_tokens,
            reused_vectors: run.reused,
            billable_tokens: run.billable_tokens,
        })
    }

    async fn set_status(
        &self,
        org_id: OrgId,
        document_id: DocumentId,
        status: DocumentStatus,
    ) -> std::result::Result<(), IngestError> {
        let ctx = TenantCtx::system(org_id, anthovai_core::Plan::Enterprise);
        let mut db = self.db.tenant(&ctx).await.map_err(IngestError::transient)?;
        knowledge_repo::set_status(&mut db, document_id, status)
            .await
            .map_err(IngestError::transient)?;
        db.commit().await.map_err(IngestError::transient)
    }

    /// Put the failure on the document, and clear away anything half-written.
    ///
    /// Best-effort by design: the job has already failed, and failing to record
    /// that should not turn a retryable error into a lost one.
    async fn record_failure(
        &self,
        org_id: OrgId,
        document_id: DocumentId,
        version: i32,
        error: &IngestError,
    ) {
        let ctx = TenantCtx::system(org_id, anthovai_core::Plan::Enterprise);

        let Ok(mut db) = self.db.tenant(&ctx).await else {
            warn!(%document_id, "could not record the failure: no database connection");
            return;
        };

        if let Err(e) = chunk_repo::discard_version(&mut db, document_id, version).await {
            warn!(error = %e, %document_id, "could not clear a half-written version");
        }
        if let Err(e) =
            knowledge_repo::set_failed(&mut db, document_id, error.code(), &error.to_string()).await
        {
            warn!(error = %e, %document_id, "could not record the failure on the document");
        }
        let _ = db.commit().await;
    }
}

/// What is stored alongside a chunk, and what a citation is built from.
fn metadata_for(
    draft: &ChunkDraft,
    parsed: &crate::chunker::ParsedDocument,
    source_type: &str,
) -> serde_json::Value {
    serde_json::json!({
        "source_type": source_type,
        "title": parsed.title,
        "heading_path": draft.heading_path,
        "page": draft.page,
        "record_key": draft.record_key,
        "language": parsed.language,
    })
}

impl std::fmt::Debug for IngestPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestPipeline")
            .field("embeddings", &self.embeddings)
            .field("parsers", &self.parsers)
            .finish()
    }
}

/// The default chunking configuration, from `config/default.toml`.
pub fn chunk_config_from(target_tokens: usize, overlap_tokens: usize) -> ChunkConfig {
    ChunkConfig {
        target_tokens,
        overlap_tokens,
        max_tokens: target_tokens + target_tokens / 5,
    }
}

/// Sensible batching for the runner, from settings.
pub fn runner_config(batch_size: usize, concurrency: usize) -> RunnerConfig {
    RunnerConfig {
        batch_size: batch_size.max(1),
        concurrency: concurrency.max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_maximum_chunk_leaves_room_above_the_target() {
        let config = chunk_config_from(500, 80);
        assert_eq!(config.target_tokens, 500);
        assert!(config.max_tokens > config.target_tokens);
    }

    #[test]
    fn batching_never_degenerates_to_zero() {
        let config = runner_config(0, 0);
        assert_eq!(config.batch_size, 1);
        assert_eq!(config.concurrency, 1);
    }
}
