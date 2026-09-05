//! Storing and removing chunks.
//!
//! Versions matter here. Re-uploading a document produces a new version, and
//! the old one keeps serving searches until the new one is complete — so a
//! document is never briefly unsearchable, and a failed re-ingestion leaves the
//! previous version in place rather than an empty knowledge base.

use std::collections::HashMap;

use anthovai_core::{ChunkId, DocumentId, DomainError, KnowledgeBaseId, Result};
use anthovai_db::TenantDb;
use anthovai_embeddings::VectorCache;
use pgvector::Vector;
use sqlx::Row;

/// One chunk ready to be written.
#[derive(Clone, Debug)]
pub struct ChunkToInsert {
    pub chunk_index: i32,
    pub content: String,
    pub content_hash: String,
    pub token_count: i32,
    pub vector: Vec<f32>,
    pub metadata: serde_json::Value,
}

/// How many rows go in one statement. Large enough that a big document is a
/// handful of round trips, small enough to stay well inside PostgreSQL's limit
/// on bind parameters.
const INSERT_BATCH: usize = 100;

/// Write a version's chunks. The caller's transaction decides whether they
/// become visible, so a failure part-way leaves nothing behind.
pub async fn insert_chunks(
    db: &mut TenantDb<'_>,
    knowledge_base_id: KnowledgeBaseId,
    document_id: DocumentId,
    version: i32,
    chunks: &[ChunkToInsert],
) -> Result<usize> {
    if chunks.is_empty() {
        return Ok(0);
    }
    let tenant = db.tenant_key();
    let mut written = 0;

    for batch in chunks.chunks(INSERT_BATCH) {
        let mut query = sqlx::QueryBuilder::new(
            "INSERT INTO document_chunks
               (id, tenant_id, knowledge_base_id, document_id, document_version,
                chunk_index, content, content_hash, token_count, embedding, metadata) ",
        );

        query.push_values(batch, |mut row, chunk| {
            row.push_bind(ChunkId::new().to_db())
                .push_bind(tenant.clone())
                .push_bind(knowledge_base_id.to_db())
                .push_bind(document_id.to_db())
                .push_bind(version)
                .push_bind(chunk.chunk_index)
                .push_bind(chunk.content.clone())
                .push_bind(chunk.content_hash.clone())
                .push_bind(chunk.token_count)
                .push_bind(Vector::from(chunk.vector.clone()))
                .push_bind(chunk.metadata.clone());
        });

        written += query.build().execute(db.conn()).await?.rows_affected() as usize;
    }

    Ok(written)
}

/// Retire every version of this document except `keep`.
///
/// Marked rather than deleted: a retrieval that is running right now finishes
/// against a consistent set, and the purge job clears the rows a day later.
pub async fn retire_other_versions(
    db: &mut TenantDb<'_>,
    document_id: DocumentId,
    keep: i32,
) -> Result<u64> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "UPDATE document_chunks SET deleted_at = now()
         WHERE document_id = $1 AND tenant_id = $2
           AND document_version <> $3 AND deleted_at IS NULL",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .bind(keep)
    .execute(db.conn())
    .await?
    .rows_affected();

    Ok(affected)
}

/// Drop a half-written version outright.
///
/// Used when ingestion fails part-way: these chunks were never visible to a
/// search, so there is nothing to keep consistent and no reason to wait a day.
pub async fn discard_version(
    db: &mut TenantDb<'_>,
    document_id: DocumentId,
    version: i32,
) -> Result<u64> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "DELETE FROM document_chunks
         WHERE document_id = $1 AND tenant_id = $2 AND document_version = $3",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .bind(version)
    .execute(db.conn())
    .await?
    .rows_affected();

    Ok(affected)
}

pub async fn mark_document_chunks_deleted(
    db: &mut TenantDb<'_>,
    document_id: DocumentId,
) -> Result<u64> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "UPDATE document_chunks SET deleted_at = now()
         WHERE document_id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .execute(db.conn())
    .await?
    .rows_affected();

    Ok(affected)
}

/// Remove chunks retired more than a day ago.
pub async fn purge_retired(db: &mut TenantDb<'_>, older_than_hours: i32) -> Result<u64> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "DELETE FROM document_chunks
         WHERE tenant_id = $1 AND deleted_at IS NOT NULL
           AND deleted_at < now() - make_interval(hours => $2)",
    )
    .bind(&tenant)
    .bind(older_than_hours)
    .execute(db.conn())
    .await?
    .rows_affected();

    Ok(affected)
}

/// How many live chunks a document has.
pub async fn count_live_chunks(db: &mut TenantDb<'_>, document_id: DocumentId) -> Result<i64> {
    let tenant = db.tenant_key();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM document_chunks
         WHERE document_id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .fetch_one(db.conn())
    .await?;

    Ok(count)
}

/// Vectors this tenant already has for these texts.
///
/// Scoped to the knowledge base, not just the tenant, because a vector is only
/// interchangeable with another produced by the same embedding model — and the
/// model is a property of the knowledge base.
pub async fn lookup_vectors(
    db: &mut TenantDb<'_>,
    knowledge_base_id: KnowledgeBaseId,
    hashes: &[String],
) -> Result<HashMap<String, Vec<f32>>> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let tenant = db.tenant_key();

    let rows = sqlx::query(
        "SELECT DISTINCT ON (content_hash) content_hash, embedding
         FROM document_chunks
         WHERE tenant_id = $1 AND knowledge_base_id = $2
           AND content_hash = ANY($3) AND deleted_at IS NULL",
    )
    .bind(&tenant)
    .bind(knowledge_base_id.to_db())
    .bind(hashes)
    .fetch_all(db.conn())
    .await?;

    rows.iter()
        .map(|row| {
            let hash: String = row
                .try_get("content_hash")
                .map_err(|e| DomainError::Internal(e.into()))?;
            let vector: Vector = row
                .try_get("embedding")
                .map_err(|e| DomainError::Internal(e.into()))?;
            Ok((hash, vector.to_vec()))
        })
        .collect()
}

/// A [`VectorCache`] backed by chunks already stored for one knowledge base.
///
/// It takes its own connection rather than borrowing the ingestion
/// transaction: the lookup happens before any write, and holding the write
/// transaction open across an embedding call would keep it open for as long as
/// the provider takes.
pub struct StoredVectors {
    db: anthovai_db::Db,
    org_id: anthovai_core::OrgId,
    knowledge_base_id: KnowledgeBaseId,
}

impl StoredVectors {
    pub fn new(
        db: anthovai_db::Db,
        org_id: anthovai_core::OrgId,
        knowledge_base_id: KnowledgeBaseId,
    ) -> Self {
        Self {
            db,
            org_id,
            knowledge_base_id,
        }
    }
}

#[async_trait::async_trait]
impl VectorCache for StoredVectors {
    async fn lookup(&self, hashes: &[String]) -> Result<HashMap<String, Vec<f32>>> {
        let mut db = self.db.tenant_for(self.org_id).await?;
        let found = lookup_vectors(&mut db, self.knowledge_base_id, hashes).await?;
        db.commit().await?;
        Ok(found)
    }
}
