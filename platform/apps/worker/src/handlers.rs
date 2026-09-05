//! Job handlers.
//!
//! Every handler scopes itself to the job's tenant before touching anything.
//! The queue is the only table the worker reads across tenants; from the moment
//! a job is claimed, the same isolation applies as on a request.

use std::sync::Arc;

use anthovai_core::{OrgId, Plan, TenantCtx};
use anthovai_db::Db;
use anthovai_ingestion::IngestPipeline;
use anthovai_jobs::{Job, JobError, JobHandler, JobPayload, JobQueue};
use anthovai_knowledge::repo as knowledge_repo;
use anthovai_retrieval::chunk_repo;

use async_trait::async_trait;
use tracing::info;

/// How long a retired chunk stays before it is really deleted. Long enough that
/// any request already under way finishes against a consistent set.
const RETENTION_HOURS: i32 = 24;

/// A context for work that belongs to a tenant but was not asked for by anyone
/// in it. Plan limits do not apply: the work was already authorised when it was
/// queued, and refusing it now would leave a document stuck for ever.
fn system_ctx(org_id: OrgId) -> TenantCtx {
    TenantCtx::system(org_id, Plan::Enterprise)
}

/// Turn an uploaded file into searchable chunks.
pub struct IngestDocumentHandler {
    pipeline: Arc<IngestPipeline>,
}

impl IngestDocumentHandler {
    pub fn new(pipeline: Arc<IngestPipeline>) -> Self {
        Self { pipeline }
    }
}

#[async_trait]
impl JobHandler for IngestDocumentHandler {
    fn kind(&self) -> &'static str {
        "ingest_document"
    }

    async fn handle(&self, job: Job) -> Result<(), JobError> {
        let JobPayload::IngestDocument {
            document_id,
            version,
        } = job.payload
        else {
            return Err(JobError::permanent("wrong_payload", "not an ingest job"));
        };

        match self.pipeline.run(job.org_id, document_id, version).await {
            Ok(outcome) => {
                info!(
                    %document_id,
                    version,
                    chunks = outcome.chunks,
                    reused = outcome.reused_vectors,
                    "document ingested"
                );
                Ok(())
            }
            // The pipeline has already recorded the failure on the document.
            // What it returns here decides only whether the queue tries again.
            Err(error) if error.is_retryable() => Err(JobError::Transient(error.to_string())),
            Err(error) => Err(JobError::permanent(error.code(), error.to_string())),
        }
    }
}

/// Remove the chunks of a deleted document.
pub struct DeleteDocumentChunksHandler {
    db: Db,
}

impl DeleteDocumentChunksHandler {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for DeleteDocumentChunksHandler {
    fn kind(&self) -> &'static str {
        "delete_document_chunks"
    }

    async fn handle(&self, job: Job) -> Result<(), JobError> {
        let JobPayload::DeleteDocumentChunks { document_id } = job.payload else {
            return Err(JobError::permanent("wrong_payload", "not a delete job"));
        };

        let ctx = system_ctx(job.org_id);
        let mut db = self
            .db
            .tenant(&ctx)
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        // Marked rather than removed, so a retrieval running right now finishes
        // against a consistent set. The purge job clears them later.
        let marked = chunk_repo::mark_document_chunks_deleted(&mut db, document_id)
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        db.commit()
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        info!(%document_id, chunks = marked, "chunks marked for removal");
        Ok(())
    }
}

/// Housekeeping: actually delete chunks that were marked a day ago.
pub struct PurgeDeletedChunksHandler {
    db: Db,
}

impl PurgeDeletedChunksHandler {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for PurgeDeletedChunksHandler {
    fn kind(&self) -> &'static str {
        "purge_deleted_chunks"
    }

    async fn handle(&self, job: Job) -> Result<(), JobError> {
        let ctx = system_ctx(job.org_id);
        let mut db = self
            .db
            .tenant(&ctx)
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        let removed = chunk_repo::purge_retired(&mut db, RETENTION_HOURS)
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        db.commit()
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        if removed > 0 {
            info!(removed, "purged deleted chunks");
        }
        Ok(())
    }
}

/// Rebuild a knowledge base's vectors with the model configured now.
///
/// Needed whenever the base was built by something else — in practice, by the
/// local stand-in during development before a provider key existed. Those bases
/// answer questions perfectly happily and the answers mean nothing, which is
/// exactly the failure worth having a job for.
///
/// Each document is re-ingested as a new version rather than rebuilt in place,
/// so the pipeline's existing guarantee holds throughout: the old vectors keep
/// serving until the new ones are complete. A base being re-embedded is never
/// briefly unsearchable, and a run that dies halfway leaves every document
/// either on its old version or its new one.
pub struct ReembedKnowledgeBaseHandler {
    db: Db,
    /// The model that will build the new vectors. Recorded on the base as the
    /// work starts, because retrieval groups by it.
    model_id: String,
}

impl ReembedKnowledgeBaseHandler {
    pub fn new(db: Db, model_id: impl Into<String>) -> Self {
        Self {
            db,
            model_id: model_id.into(),
        }
    }
}

#[async_trait]
impl JobHandler for ReembedKnowledgeBaseHandler {
    fn kind(&self) -> &'static str {
        "reembed_knowledge_base"
    }

    async fn handle(&self, job: Job) -> Result<(), JobError> {
        let JobPayload::ReembedKnowledgeBase { knowledge_base_id } = job.payload else {
            return Err(JobError::permanent("wrong_payload", "not a re-embed job"));
        };

        let ctx = system_ctx(job.org_id);
        let mut db = self
            .db
            .tenant(&ctx)
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        let documents = knowledge_repo::list_documents(&mut db, knowledge_base_id, None)
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        // Set first, so the chunks written below are searched by the embedder
        // that wrote them. Doing it at the end would leave every new vector
        // grouped under the old model for the length of the run.
        knowledge_repo::set_embedding_model(&mut db, knowledge_base_id, &self.model_id)
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        let mut work = Vec::new();
        for document in &documents {
            // A document that never finished has no vectors to replace, and one
            // that failed will fail again for its own reasons.
            if document.status != anthovai_knowledge::DocumentStatus::Ready {
                continue;
            }

            let version = knowledge_repo::next_version(&mut db, document.id)
                .await
                .map_err(|e| JobError::Transient(e.to_string()))?;
            work.push((document.id, version));
        }

        db.commit()
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        // A second transaction, as the system role: `jobs` is the one table the
        // application role has no grant on, because a worker claims work before
        // it knows whose it is.
        //
        // Split like this, a crash between the two leaves the base pointing at
        // the new model with its old vectors intact — still searchable, still
        // wrong in the way it already was — and a retry re-queues the lot. The
        // pipeline discards a half-written version before it writes one, so a
        // document queued twice is ingested once.
        let mut system = self
            .db
            .system()
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        for (document_id, version) in &work {
            JobQueue::enqueue_in(
                &mut system,
                job.org_id,
                &JobPayload::IngestDocument {
                    document_id: *document_id,
                    version: *version,
                },
            )
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;
        }

        system
            .commit()
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        let queued = work.len();

        info!(
            %knowledge_base_id,
            model = %self.model_id,
            documents = queued,
            skipped = documents.len() - queued,
            "queued a knowledge base for re-embedding"
        );
        Ok(())
    }
}
