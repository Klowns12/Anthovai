//! Knowledge services: creating knowledge bases, and getting a document from
//! an HTTP request into the queue.
//!
//! An upload is deliberately two steps. `start_upload` reserves the document
//! row and hands back where the bytes go; `finish_upload` records what actually
//! arrived and queues the work. Doing it in one step would mean either buffering
//! the whole file in memory or leaving a row behind whenever a transfer broke
//! off half way.

use std::sync::Arc;

use anthovai_core::config::EmbeddingSettings;
use anthovai_core::{
    DocumentId, DomainError, KnowledgeBaseId, Permission, Result, TenantCtx, WorkspaceId,
};
use anthovai_db::Db;
use anthovai_jobs::{JobPayload, JobQueue};
use anthovai_storage::{Storage, StorageKey};

use crate::repo::{self, NewDocument};
use crate::{Document, DocumentStatus, KnowledgeBase, SourceType};

pub struct KnowledgeService {
    db: Db,
    storage: Storage,
    embeddings: EmbeddingSettings,
}

#[derive(Clone, Debug)]
pub struct CreateKnowledgeBase {
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub description: Option<String>,
}

/// Where an upload's bytes came from.
#[derive(Clone, Debug)]
pub enum UploadTarget {
    /// A file. The bytes are streamed by the caller.
    File {
        filename: String,
        mime_type: Option<String>,
        /// From `Content-Length`. Checked against the plan before any bytes are
        /// read, so an oversized upload costs nothing.
        declared_size: Option<i64>,
    },
    /// Text pasted into the dashboard.
    Text { title: String },
    /// A page the customer asked us to fetch. The URL has already passed the
    /// SSRF guard by the time it reaches here; this only records it.
    Url { url: String, title: Option<String> },
}

/// What the caller needs in order to write the bytes.
#[derive(Clone, Debug)]
pub struct StartUpload {
    pub document_id: DocumentId,
    pub storage_key: String,
    pub source_type: SourceType,
    /// The largest this upload may be, from the plan.
    pub max_bytes: i64,
}

impl KnowledgeService {
    pub fn new(db: Db, storage: Storage, embeddings: EmbeddingSettings) -> Self {
        Self {
            db,
            storage,
            embeddings,
        }
    }

    // ---- knowledge bases --------------------------------------------------

    pub async fn create_knowledge_base(
        &self,
        ctx: &TenantCtx,
        request: CreateKnowledgeBase,
    ) -> Result<KnowledgeBase> {
        ctx.require(Permission::KnowledgeWrite)?;
        let name = validated_name(&request.name)?;

        let kb = KnowledgeBase {
            id: KnowledgeBaseId::new(),
            org_id: ctx.org_id,
            workspace_id: request.workspace_id,
            name,
            description: request.description,
            // Pinned now, for the life of the knowledge base: a query must be
            // embedded by the same model as the chunks it is searching.
            embedding_model: self.embeddings.default_model.clone(),
            embedding_dim: self.embeddings.dimension as i32,
            storage_bytes: 0,
            document_count: 0,
        };

        let mut db = self.db.tenant(ctx).await?;
        repo::insert_knowledge_base(&mut db, &kb).await?;
        db.commit().await?;
        Ok(kb)
    }

    pub async fn list_knowledge_bases(
        &self,
        ctx: &TenantCtx,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Vec<KnowledgeBase>> {
        ctx.require(Permission::KnowledgeRead)?;
        let mut db = self.db.tenant(ctx).await?;
        let bases = repo::list_knowledge_bases(&mut db, workspace_id).await?;
        db.commit().await?;
        Ok(bases)
    }

    pub async fn get_knowledge_base(
        &self,
        ctx: &TenantCtx,
        kb_id: KnowledgeBaseId,
    ) -> Result<KnowledgeBase> {
        ctx.require(Permission::KnowledgeRead)?;
        let mut db = self.db.tenant(ctx).await?;
        let kb = repo::find_knowledge_base(&mut db, kb_id).await?;
        db.commit().await?;
        Ok(kb)
    }

    pub async fn rename_knowledge_base(
        &self,
        ctx: &TenantCtx,
        kb_id: KnowledgeBaseId,
        name: &str,
        description: Option<&str>,
    ) -> Result<KnowledgeBase> {
        ctx.require(Permission::KnowledgeWrite)?;
        let name = validated_name(name)?;

        let mut db = self.db.tenant(ctx).await?;
        repo::rename_knowledge_base(&mut db, kb_id, &name, description).await?;
        let kb = repo::find_knowledge_base(&mut db, kb_id).await?;
        db.commit().await?;
        Ok(kb)
    }

    /// Delete a knowledge base and everything in it. The chunks go through the
    /// queue rather than the request, because there may be a great many.
    pub async fn delete_knowledge_base(
        &self,
        ctx: &TenantCtx,
        kb_id: KnowledgeBaseId,
    ) -> Result<()> {
        ctx.require(Permission::KnowledgeWrite)?;

        let mut db = self.db.tenant(ctx).await?;
        let documents = repo::list_documents(&mut db, kb_id, None).await?;
        for document in &documents {
            repo::mark_deleted(&mut db, document.id).await?;
        }
        repo::soft_delete_knowledge_base(&mut db, kb_id).await?;
        db.commit().await?;

        let mut system = self.db.system().await?;
        for document in &documents {
            JobQueue::enqueue_in(
                &mut system,
                ctx.org_id,
                &JobPayload::DeleteDocumentChunks {
                    document_id: document.id,
                },
            )
            .await?;
        }
        system.commit().await?;

        // The objects themselves are not on the request path either, but there
        // is no job kind for them yet, so this is done here for now.
        let prefix = format!("tenant/{}/{}/", ctx.org_id.to_db(), kb_id.to_db());
        if let Err(e) = self.storage.delete_prefix(&prefix).await {
            tracing::warn!(error = %e, %prefix, "could not remove stored files for a deleted knowledge base");
        }
        Ok(())
    }

    // ---- uploads ----------------------------------------------------------

    /// Reserve a document and say where its bytes belong.
    ///
    /// Every plan limit is checked here, before a single byte is read.
    pub async fn start_upload(
        &self,
        ctx: &TenantCtx,
        kb_id: KnowledgeBaseId,
        target: UploadTarget,
    ) -> Result<StartUpload> {
        ctx.require(Permission::KnowledgeWrite)?;

        let limits = ctx.plan.limits();
        let (title, source_type, mime_type) = describe(&target)?;

        if let UploadTarget::File {
            declared_size: Some(size),
            ..
        } = &target
        {
            if *size > limits.max_file_bytes {
                return Err(DomainError::PayloadTooLarge("file_too_large"));
            }
        }

        if !source_type.is_supported() {
            return Err(DomainError::validation(format!(
                "`{}` files cannot be processed yet",
                source_type.as_str()
            )));
        }

        let document_id = DocumentId::new();
        let mut db = self.db.tenant(ctx).await?;

        // Reading the knowledge base first also proves it belongs to this
        // tenant, which the foreign key would not.
        repo::find_knowledge_base(&mut db, kb_id).await?;

        if repo::count_documents(&mut db, kb_id).await? >= limits.documents_per_kb {
            return Err(DomainError::QuotaExceeded("document_limit_reached"));
        }
        if repo::total_storage_bytes(&mut db).await? >= limits.storage_bytes {
            return Err(DomainError::QuotaExceeded("storage_limit_reached"));
        }

        repo::insert_document(
            &mut db,
            &NewDocument {
                id: document_id,
                knowledge_base_id: kb_id,
                title,
                source_type,
                source_url: match &target {
                    UploadTarget::Url { url, .. } => Some(url.clone()),
                    _ => None,
                },
                mime_type,
                created_by: ctx.user_id(),
            },
        )
        .await?;
        db.commit().await?;

        Ok(StartUpload {
            document_id,
            storage_key: StorageKey::new(ctx.org_id, kb_id, document_id, 1).original(),
            source_type,
            max_bytes: limits.max_file_bytes,
        })
    }

    /// The bytes are in storage. Record what arrived and queue the ingestion.
    ///
    /// The job is enqueued in the same transaction as the status change, so a
    /// document is never left queued for work that was rolled back, nor marked
    /// ready for work that was never queued.
    pub async fn finish_upload(
        &self,
        ctx: &TenantCtx,
        document_id: DocumentId,
        storage_key: &str,
        size_bytes: i64,
        content_hash: &str,
    ) -> Result<Document> {
        let mut db = self.db.tenant(ctx).await?;
        let document = repo::find_document(&mut db, document_id).await?;

        repo::record_upload(&mut db, document_id, storage_key, size_bytes, content_hash).await?;
        repo::adjust_counters(&mut db, document.knowledge_base_id, size_bytes, 1).await?;
        let updated = repo::find_document(&mut db, document_id).await?;
        db.commit().await?;

        let mut system = self.db.system().await?;
        JobQueue::enqueue_in(
            &mut system,
            ctx.org_id,
            &JobPayload::IngestDocument {
                document_id,
                version: updated.current_version,
            },
        )
        .await?;
        system.commit().await?;

        Ok(updated)
    }

    /// Abandon a reservation whose upload never completed.
    pub async fn abandon_upload(&self, ctx: &TenantCtx, document_id: DocumentId) -> Result<()> {
        let mut db = self.db.tenant(ctx).await?;
        repo::mark_deleted(&mut db, document_id).await?;
        db.commit().await
    }

    pub async fn get_document(&self, ctx: &TenantCtx, document_id: DocumentId) -> Result<Document> {
        ctx.require(Permission::KnowledgeRead)?;
        let mut db = self.db.tenant(ctx).await?;
        let document = repo::find_document(&mut db, document_id).await?;
        db.commit().await?;
        Ok(document)
    }

    pub async fn list_documents(
        &self,
        ctx: &TenantCtx,
        kb_id: KnowledgeBaseId,
        status: Option<DocumentStatus>,
    ) -> Result<Vec<Document>> {
        ctx.require(Permission::KnowledgeRead)?;
        let mut db = self.db.tenant(ctx).await?;
        repo::find_knowledge_base(&mut db, kb_id).await?;
        let documents = repo::list_documents(&mut db, kb_id, status).await?;
        db.commit().await?;
        Ok(documents)
    }

    /// Queue a failed document again. Only a failure can be retried: re-running
    /// one that is already working would duplicate its chunks.
    pub async fn retry(&self, ctx: &TenantCtx, document_id: DocumentId) -> Result<Document> {
        ctx.require(Permission::KnowledgeWrite)?;

        let mut db = self.db.tenant(ctx).await?;
        let document = repo::find_document(&mut db, document_id).await?;

        if !document.status.is_retryable() {
            return Err(DomainError::Conflict("document_not_failed"));
        }
        repo::set_status(&mut db, document_id, DocumentStatus::Queued).await?;
        db.commit().await?;

        let mut system = self.db.system().await?;
        JobQueue::enqueue_in(
            &mut system,
            ctx.org_id,
            &JobPayload::IngestDocument {
                document_id,
                version: document.current_version,
            },
        )
        .await?;
        system.commit().await?;

        self.get_document(ctx, document_id).await
    }

    pub async fn delete_document(&self, ctx: &TenantCtx, document_id: DocumentId) -> Result<()> {
        ctx.require(Permission::KnowledgeWrite)?;

        let mut db = self.db.tenant(ctx).await?;
        let document = repo::mark_deleted(&mut db, document_id).await?;
        repo::adjust_counters(
            &mut db,
            document.knowledge_base_id,
            -document.size_bytes,
            -1,
        )
        .await?;
        db.commit().await?;

        let mut system = self.db.system().await?;
        JobQueue::enqueue_in(
            &mut system,
            ctx.org_id,
            &JobPayload::DeleteDocumentChunks { document_id },
        )
        .await?;
        system.commit().await?;

        if let Some(key) = &document.storage_key {
            if let Err(e) = self.storage.delete(key).await {
                tracing::warn!(error = %e, key, "could not remove a deleted document's file");
            }
        }
        Ok(())
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }
}

impl std::fmt::Debug for KnowledgeService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnowledgeService")
            .field("embedding_model", &self.embeddings.default_model)
            .finish()
    }
}

impl Clone for KnowledgeService {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            storage: Arc::clone(&self.storage),
            embeddings: self.embeddings.clone(),
        }
    }
}

/// Title, kind and mime type for an upload.
fn describe(target: &UploadTarget) -> Result<(String, SourceType, Option<String>)> {
    match target {
        UploadTarget::Text { title } => Ok((
            validated_name(title)?,
            SourceType::Text,
            Some("text/plain".to_owned()),
        )),
        UploadTarget::Url { url, title } => {
            // Until the page is fetched its own `<title>` is unknown, so the
            // URL stands in. A name the customer gave beats both.
            let name = title.clone().unwrap_or_else(|| url.clone());
            Ok((validated_name(&name)?, SourceType::Url, None))
        }
        UploadTarget::File {
            filename,
            mime_type,
            ..
        } => {
            let title = validated_name(filename)?;
            // The extension only picks a parser. What the bytes actually are is
            // confirmed from their content once they arrive.
            let source_type = SourceType::from_extension(filename).ok_or_else(|| {
                DomainError::validation(
                    "unrecognised file type: name the file with a known extension",
                )
            })?;
            Ok((title, source_type, mime_type.clone()))
        }
    }
}

fn validated_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DomainError::validation("a name is required"));
    }
    if trimmed.chars().count() > 200 {
        return Err(DomainError::validation(
            "the name must be at most 200 characters",
        ));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_document_is_text() {
        let (title, source, mime) = describe(&UploadTarget::Text {
            title: "  Course notes ".into(),
        })
        .unwrap();

        assert_eq!(title, "Course notes");
        assert_eq!(source, SourceType::Text);
        assert_eq!(mime.as_deref(), Some("text/plain"));
    }

    #[test]
    fn a_file_is_typed_by_its_extension() {
        let (title, source, _) = describe(&UploadTarget::File {
            filename: "handbook.md".into(),
            mime_type: None,
            declared_size: None,
        })
        .unwrap();

        assert_eq!(title, "handbook.md");
        assert_eq!(source, SourceType::Md);
    }

    #[test]
    fn a_file_with_no_recognisable_extension_is_refused() {
        let err = describe(&UploadTarget::File {
            filename: "mystery".into(),
            mime_type: None,
            declared_size: None,
        })
        .unwrap_err();

        assert_eq!(err.code(), "invalid_request");
    }

    #[test]
    fn empty_names_are_refused() {
        assert!(validated_name("   ").is_err());
        assert!(validated_name(&"a".repeat(201)).is_err());
        assert!(validated_name("Student Handbook 2026").is_ok());
    }

    #[test]
    fn thai_names_are_measured_in_characters() {
        assert!(validated_name(&"ก".repeat(200)).is_ok());
    }
}
