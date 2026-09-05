//! Repositories for knowledge bases and documents.

use anthovai_core::{DocumentId, DomainError, KnowledgeBaseId, OrgId, Result, UserId, WorkspaceId};
use anthovai_db::repo::{id, parsed};
use anthovai_db::{on_missing_reference, on_unique_violation, SystemDb, TenantDb};
use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::{Document, DocumentStatus, KnowledgeBase, SourceType};

// ---- knowledge bases ------------------------------------------------------

pub async fn insert_knowledge_base(db: &mut TenantDb<'_>, kb: &KnowledgeBase) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "INSERT INTO knowledge_bases
           (id, tenant_id, workspace_id, name, description, embedding_model, embedding_dim)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(kb.id.to_db())
    .bind(&tenant)
    .bind(kb.workspace_id.to_db())
    .bind(&kb.name)
    .bind(&kb.description)
    .bind(&kb.embedding_model)
    .bind(kb.embedding_dim)
    .execute(db.conn())
    .await
    .map_err(|e| on_missing_reference(e, "workspace"))?;
    Ok(())
}

pub async fn find_knowledge_base(
    db: &mut TenantDb<'_>,
    kb_id: KnowledgeBaseId,
) -> Result<KnowledgeBase> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "SELECT id, tenant_id, workspace_id, name, description, embedding_model, embedding_dim,
                storage_bytes, document_count
         FROM knowledge_bases
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(kb_id.to_db())
    .bind(&tenant)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("knowledge_base"))?;

    knowledge_base_row(&row)
}

pub async fn list_knowledge_bases(
    db: &mut TenantDb<'_>,
    workspace_id: Option<WorkspaceId>,
) -> Result<Vec<KnowledgeBase>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT id, tenant_id, workspace_id, name, description, embedding_model, embedding_dim,
                storage_bytes, document_count
         FROM knowledge_bases
         WHERE tenant_id = $1
           AND ($2::text IS NULL OR workspace_id = $2)
           AND deleted_at IS NULL
         ORDER BY created_at DESC",
    )
    .bind(&tenant)
    .bind(workspace_id.map(|w| w.to_db()))
    .fetch_all(db.conn())
    .await?;

    rows.iter().map(knowledge_base_row).collect()
}

pub async fn rename_knowledge_base(
    db: &mut TenantDb<'_>,
    kb_id: KnowledgeBaseId,
    name: &str,
    description: Option<&str>,
) -> Result<()> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "UPDATE knowledge_bases SET name = $3, description = $4, updated_at = now()
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(kb_id.to_db())
    .bind(&tenant)
    .bind(name)
    .bind(description)
    .execute(db.conn())
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(DomainError::NotFound("knowledge_base"));
    }
    Ok(())
}

pub async fn soft_delete_knowledge_base(
    db: &mut TenantDb<'_>,
    kb_id: KnowledgeBaseId,
) -> Result<()> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "UPDATE knowledge_bases SET deleted_at = now()
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(kb_id.to_db())
    .bind(&tenant)
    .execute(db.conn())
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(DomainError::NotFound("knowledge_base"));
    }
    Ok(())
}

/// Storage and document counts, kept as columns so a plan check is one read
/// rather than an aggregate over every document the tenant owns.
pub async fn adjust_counters(
    db: &mut TenantDb<'_>,
    kb_id: KnowledgeBaseId,
    bytes_delta: i64,
    documents_delta: i32,
) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "UPDATE knowledge_bases
         SET storage_bytes = GREATEST(0, storage_bytes + $3),
             document_count = GREATEST(0, document_count + $4),
             updated_at = now()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(kb_id.to_db())
    .bind(&tenant)
    .bind(bytes_delta)
    .bind(documents_delta)
    .execute(db.conn())
    .await?;
    Ok(())
}

/// Total bytes this tenant is storing, for the plan limit.
pub async fn total_storage_bytes(db: &mut TenantDb<'_>) -> Result<i64> {
    let tenant = db.tenant_key();
    // `sum()` over a bigint widens to numeric in PostgreSQL, so it is cast back
    // explicitly rather than left to surprise the decoder.
    let total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(storage_bytes), 0)::bigint FROM knowledge_bases
         WHERE tenant_id = $1 AND deleted_at IS NULL",
    )
    .bind(&tenant)
    .fetch_one(db.conn())
    .await?;
    Ok(total)
}

// ---- documents ------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct NewDocument {
    pub id: DocumentId,
    pub knowledge_base_id: KnowledgeBaseId,
    pub title: String,
    pub source_type: SourceType,
    pub source_url: Option<String>,
    pub mime_type: Option<String>,
    pub created_by: Option<UserId>,
}

/// The knowledge base is checked explicitly rather than left to the foreign
/// key: referential integrity runs with the referenced table's owner
/// privileges, so it sees rows row-level security hides from us and would
/// happily accept another tenant's knowledge base id.
pub async fn insert_document(db: &mut TenantDb<'_>, doc: &NewDocument) -> Result<()> {
    find_knowledge_base(db, doc.knowledge_base_id).await?;

    let tenant = db.tenant_key();
    sqlx::query(
        "INSERT INTO documents
           (id, tenant_id, knowledge_base_id, title, source_type, source_url, mime_type,
            status, current_version, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'uploading', 1, $8)",
    )
    .bind(doc.id.to_db())
    .bind(&tenant)
    .bind(doc.knowledge_base_id.to_db())
    .bind(&doc.title)
    .bind(doc.source_type.as_str())
    .bind(&doc.source_url)
    .bind(&doc.mime_type)
    .bind(doc.created_by.map(|u| u.to_db()))
    .execute(db.conn())
    .await
    .map_err(|e| on_unique_violation(e, "document_already_exists"))?;
    Ok(())
}

pub async fn find_document(db: &mut TenantDb<'_>, document_id: DocumentId) -> Result<Document> {
    find(db, document_id, false).await
}

/// The same, but a deleted document is returned rather than hidden.
///
/// Ingestion needs the difference: a document deleted while its job waited in
/// the queue is an ordinary race with nothing to do, while one that never
/// existed is a bug worth reporting. Without this they look identical.
pub async fn find_document_including_deleted(
    db: &mut TenantDb<'_>,
    document_id: DocumentId,
) -> Result<Document> {
    find(db, document_id, true).await
}

async fn find(
    db: &mut TenantDb<'_>,
    document_id: DocumentId,
    include_deleted: bool,
) -> Result<Document> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "SELECT id, knowledge_base_id, title, source_type, source_url, status, progress,
                error_code, error_message, current_version, size_bytes, chunk_count,
                token_count, language, storage_key, content_hash, created_at, updated_at
         FROM documents
         WHERE id = $1 AND tenant_id = $2 AND ($3 OR status <> 'deleted')",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .bind(include_deleted)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("document"))?;

    document_row(&row)
}

pub async fn list_documents(
    db: &mut TenantDb<'_>,
    kb_id: KnowledgeBaseId,
    status: Option<DocumentStatus>,
) -> Result<Vec<Document>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT id, knowledge_base_id, title, source_type, source_url, status, progress,
                error_code, error_message, current_version, size_bytes, chunk_count,
                token_count, language, storage_key, content_hash, created_at, updated_at
         FROM documents
         WHERE tenant_id = $1 AND knowledge_base_id = $2
           AND status <> 'deleted'
           AND ($3::text IS NULL OR status = $3)
         ORDER BY created_at DESC",
    )
    .bind(&tenant)
    .bind(kb_id.to_db())
    .bind(status.map(|s| s.as_str()))
    .fetch_all(db.conn())
    .await?;

    rows.iter().map(document_row).collect()
}

pub async fn count_documents(db: &mut TenantDb<'_>, kb_id: KnowledgeBaseId) -> Result<i64> {
    let tenant = db.tenant_key();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM documents
         WHERE tenant_id = $1 AND knowledge_base_id = $2 AND status <> 'deleted'",
    )
    .bind(&tenant)
    .bind(kb_id.to_db())
    .fetch_one(db.conn())
    .await?;
    Ok(count)
}

/// Record where the bytes landed and how big they were.
pub async fn record_upload(
    db: &mut TenantDb<'_>,
    document_id: DocumentId,
    storage_key: &str,
    size_bytes: i64,
    content_hash: &str,
) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "UPDATE documents
         SET storage_key = $3, size_bytes = $4, content_hash = $5,
             status = 'queued', progress = 10, error_code = NULL, error_message = NULL,
             updated_at = now()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .bind(storage_key)
    .bind(size_bytes)
    .bind(content_hash)
    .execute(db.conn())
    .await?;
    Ok(())
}

/// Move a document along the pipeline. `progress` comes from the status itself,
/// so the two cannot drift apart.
pub async fn set_status(
    db: &mut TenantDb<'_>,
    document_id: DocumentId,
    status: DocumentStatus,
) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "UPDATE documents SET status = $3, progress = $4, updated_at = now()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .bind(status.as_str())
    .bind(i16::from(status.progress()))
    .execute(db.conn())
    .await?;
    Ok(())
}

/// As with `set_ready`, a document deleted mid-ingestion stays deleted rather
/// than reappearing as a failure.
pub async fn set_failed(
    db: &mut TenantDb<'_>,
    document_id: DocumentId,
    error_code: &str,
    error_message: &str,
) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "UPDATE documents
         SET status = 'failed', progress = 0, error_code = $3, error_message = $4,
             updated_at = now()
         WHERE id = $1 AND tenant_id = $2 AND status <> 'deleted'",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .bind(error_code)
    // Truncated: a parser can produce a very long message, and this is shown
    // in the dashboard rather than used for debugging.
    .bind(error_message.chars().take(1_000).collect::<String>())
    .execute(db.conn())
    .await?;
    Ok(())
}

/// Ingestion finished: record what it produced and mark the document ready.
///
/// A deleted document stays deleted. Ingestion can finish after someone has
/// removed the document it was working on, and marking it ready then would
/// bring it back — visible in the dashboard, and searchable.
pub async fn set_ready(
    db: &mut TenantDb<'_>,
    document_id: DocumentId,
    version: i32,
    chunk_count: i32,
    token_count: i32,
    language: Option<&str>,
) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "UPDATE documents
         SET status = 'ready', progress = 100, current_version = $3,
             chunk_count = $4, token_count = $5, language = $6,
             error_code = NULL, error_message = NULL, updated_at = now()
         WHERE id = $1 AND tenant_id = $2 AND status <> 'deleted'",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .bind(version)
    .bind(chunk_count)
    .bind(token_count)
    .bind(language)
    .execute(db.conn())
    .await?;
    Ok(())
}

/// A re-upload becomes a new version. The old one keeps serving until the new
/// one is ready, so a document is never briefly unsearchable.
pub async fn next_version(db: &mut TenantDb<'_>, document_id: DocumentId) -> Result<i32> {
    let tenant = db.tenant_key();
    let current: i32 = sqlx::query_scalar(
        "SELECT current_version FROM documents WHERE id = $1 AND tenant_id = $2",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("document"))?;

    Ok(current + 1)
}

pub async fn mark_deleted(db: &mut TenantDb<'_>, document_id: DocumentId) -> Result<Document> {
    let document = find_document(db, document_id).await?;
    let tenant = db.tenant_key();

    sqlx::query(
        "UPDATE documents SET status = 'deleted', deleted_at = now(), updated_at = now()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(document_id.to_db())
    .bind(&tenant)
    .execute(db.conn())
    .await?;

    Ok(document)
}

// ---- row mapping ----------------------------------------------------------

fn knowledge_base_row(row: &sqlx::postgres::PgRow) -> Result<KnowledgeBase> {
    Ok(KnowledgeBase {
        id: id(row, "id")?,
        org_id: id(row, "tenant_id")?,
        workspace_id: id(row, "workspace_id")?,
        name: row.try_get("name").map_err(sql)?,
        description: row.try_get("description").map_err(sql)?,
        embedding_model: row.try_get("embedding_model").map_err(sql)?,
        embedding_dim: row.try_get("embedding_dim").map_err(sql)?,
        storage_bytes: row.try_get("storage_bytes").map_err(sql)?,
        document_count: row.try_get("document_count").map_err(sql)?,
    })
}

fn document_row(row: &sqlx::postgres::PgRow) -> Result<Document> {
    Ok(Document {
        id: id(row, "id")?,
        knowledge_base_id: id(row, "knowledge_base_id")?,
        title: row.try_get("title").map_err(sql)?,
        source_type: parsed(row, "source_type")?,
        source_url: row.try_get("source_url").map_err(sql)?,
        status: parsed(row, "status")?,
        progress: row.try_get::<i16, _>("progress").map_err(sql)? as u8,
        error_code: row.try_get("error_code").map_err(sql)?,
        error_message: row.try_get("error_message").map_err(sql)?,
        current_version: row.try_get("current_version").map_err(sql)?,
        size_bytes: row.try_get("size_bytes").map_err(sql)?,
        chunk_count: row.try_get("chunk_count").map_err(sql)?,
        token_count: row.try_get("token_count").map_err(sql)?,
        language: row.try_get("language").map_err(sql)?,
        storage_key: row.try_get("storage_key").map_err(sql)?,
        content_hash: row.try_get("content_hash").map_err(sql)?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at").map_err(sql)?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at").map_err(sql)?,
    })
}

fn sql(err: sqlx::Error) -> DomainError {
    DomainError::Database(err)
}

/// Knowledge bases whose vectors were built by a stand-in rather than a model.
///
/// A base here is searchable and answers questions; the answers just mean
/// nothing, because the vectors encode word overlap rather than meaning. Found
/// by the model id the base recorded when it was created, which is why that id
/// carries a `fake:` namespace at all.
///
/// Read as the system role: this runs at startup across every tenant, before
/// any request has chosen one.
pub async fn knowledge_bases_needing_reembedding(
    db: &mut SystemDb<'_>,
) -> Result<Vec<(OrgId, KnowledgeBaseId)>> {
    let rows = sqlx::query(
        "SELECT tenant_id, id FROM knowledge_bases
         WHERE embedding_model LIKE 'fake:%' AND deleted_at IS NULL
         ORDER BY created_at",
    )
    .fetch_all(db.conn())
    .await?;

    // `id`, not `parse`: the column holds a bare ULID. The prefixed form is
    // what an id looks like on the wire, and only the wire.
    rows.iter()
        .map(|row| Ok((id(row, "tenant_id")?, id(row, "id")?)))
        .collect()
}

/// Point a knowledge base at the model that will now build its vectors.
///
/// Called when re-embedding starts, not when it finishes: from this moment the
/// documents are re-ingested one at a time, and each one's new chunks are
/// written with the new model. Retrieval groups by the base's model, so leaving
/// the old id would send the new vectors to be searched by the old embedder.
pub async fn set_embedding_model(
    db: &mut TenantDb<'_>,
    kb_id: KnowledgeBaseId,
    model_id: &str,
) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "UPDATE knowledge_bases SET embedding_model = $3, updated_at = now()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(kb_id.to_db())
    .bind(&tenant)
    .bind(model_id)
    .execute(db.conn())
    .await?;
    Ok(())
}
