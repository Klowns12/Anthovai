//! Knowledge bases and documents.

use anthovai_core::{DocumentId, DomainError, KnowledgeBaseId};
use anthovai_knowledge::{CreateKnowledgeBase, Document, DocumentStatus, KnowledgeBase};
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::dashboard::organizations::{parse_id, workspace_filter, WorkspaceFilter};
use crate::error::ApiError;
use crate::extract::SessionAuth;
use crate::state::AppState;
use crate::uploads;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/knowledge_bases",
            get(list_knowledge_bases).post(create_knowledge_base),
        )
        .route(
            "/knowledge_bases/{kb_id}",
            get(get_knowledge_base)
                .patch(rename_knowledge_base)
                .delete(delete_knowledge_base),
        )
        .route("/knowledge_bases/{kb_id}/documents", get(list_documents))
}

/// Document routes, mounted at their full path so they can carry the larger
/// body limit an upload needs. Keeping them out of the JSON-sized router means
/// no other endpoint can be used to push a hundred megabytes at us.
pub fn document_routes() -> Router<AppState> {
    Router::new()
        .route("/dashboard/v1/documents", post(upload_document))
        .route(
            "/dashboard/v1/documents/{document_id}",
            get(get_document)
                .patch(reupload_document)
                .delete(delete_document),
        )
        .route(
            "/dashboard/v1/documents/{document_id}/retry",
            post(retry_document),
        )
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct KnowledgeBaseView {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub description: Option<String>,
    /// Which model built the vectors in this base. A base built with one model
    /// cannot be searched with another, so this is what a re-embedding is
    /// decided from.
    pub embedding_model: String,
    pub storage_bytes: i64,
    pub document_count: i32,
}

impl From<&KnowledgeBase> for KnowledgeBaseView {
    fn from(kb: &KnowledgeBase) -> Self {
        Self {
            id: kb.id.to_string(),
            workspace_id: kb.workspace_id.to_string(),
            name: kb.name.clone(),
            description: kb.description.clone(),
            embedding_model: kb.embedding_model.clone(),
            storage_bytes: kb.storage_bytes,
            document_count: kb.document_count,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct DocumentView {
    pub id: String,
    pub knowledge_base_id: String,
    pub title: String,
    /// `pdf`, `docx`, `txt`, `md`, `html`, `url`, `json`, `csv` or `text`.
    pub source_type: String,
    /// `queued`, `processing`, `chunking`, `embedding`, `indexing`, `ready`
    /// or `failed`. Ingestion is asynchronous: an upload returns `queued`.
    pub status: String,
    /// 0–100. Rough, and only meaningful while the status is in progress.
    pub progress: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub version: i32,
    pub size_bytes: i64,
    pub chunk_count: i32,
    pub token_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Document> for DocumentView {
    fn from(doc: &Document) -> Self {
        Self {
            id: doc.id.to_string(),
            knowledge_base_id: doc.knowledge_base_id.to_string(),
            title: doc.title.clone(),
            source_type: doc.source_type.as_str().to_owned(),
            status: doc.status.as_str().to_owned(),
            progress: doc.progress,
            error_code: doc.error_code.clone(),
            error_message: doc.error_message.clone(),
            version: doc.current_version,
            size_bytes: doc.size_bytes,
            chunk_count: doc.chunk_count,
            token_count: doc.token_count,
            language: doc.language.clone(),
            created_at: doc.created_at.to_rfc3339(),
            updated_at: doc.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
struct ListResponse<T> {
    data: Vec<T>,
}

#[derive(Deserialize)]
struct CreateRequest {
    workspace_id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
}

async fn create_knowledge_base(
    State(state): State<AppState>,
    session: SessionAuth,
    Json(body): Json<CreateRequest>,
) -> Result<Response, ApiError> {
    let kb = state
        .knowledge
        .create_knowledge_base(
            &session.ctx,
            CreateKnowledgeBase {
                workspace_id: parse_id(&body.workspace_id, &session.request_id)?,
                name: body.name,
                description: body.description,
            },
        )
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok((StatusCode::CREATED, Json(KnowledgeBaseView::from(&kb))).into_response())
}

async fn list_knowledge_bases(
    State(state): State<AppState>,
    session: SessionAuth,
    filter: Query<WorkspaceFilter>,
) -> Result<Response, ApiError> {
    let workspace_id = workspace_filter(filter, &session.request_id)?;
    let bases = state
        .knowledge
        .list_knowledge_bases(&session.ctx, workspace_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(ListResponse {
        data: bases
            .iter()
            .map(KnowledgeBaseView::from)
            .collect::<Vec<_>>(),
    })
    .into_response())
}

async fn get_knowledge_base(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(kb_id): Path<String>,
) -> Result<Response, ApiError> {
    let kb_id: KnowledgeBaseId = parse_id(&kb_id, &session.request_id)?;
    let kb = state
        .knowledge
        .get_knowledge_base(&session.ctx, kb_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(KnowledgeBaseView::from(&kb)).into_response())
}

#[derive(Deserialize)]
struct RenameRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

async fn rename_knowledge_base(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(kb_id): Path<String>,
    Json(body): Json<RenameRequest>,
) -> Result<Response, ApiError> {
    let kb_id: KnowledgeBaseId = parse_id(&kb_id, &session.request_id)?;
    let kb = state
        .knowledge
        .rename_knowledge_base(&session.ctx, kb_id, &body.name, body.description.as_deref())
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(KnowledgeBaseView::from(&kb)).into_response())
}

async fn delete_knowledge_base(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(kb_id): Path<String>,
) -> Result<Response, ApiError> {
    let kb_id: KnowledgeBaseId = parse_id(&kb_id, &session.request_id)?;
    state
        .knowledge
        .delete_knowledge_base(&session.ctx, kb_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize)]
struct DocumentFilter {
    status: Option<String>,
}

async fn list_documents(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(kb_id): Path<String>,
    Query(filter): Query<DocumentFilter>,
) -> Result<Response, ApiError> {
    let kb_id: KnowledgeBaseId = parse_id(&kb_id, &session.request_id)?;
    let status: Option<DocumentStatus> = filter
        .status
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|e: DomainError| ApiError::from_domain(e, session.request_id.clone()))?;

    let documents = state
        .knowledge
        .list_documents(&session.ctx, kb_id, status)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(ListResponse {
        data: documents.iter().map(DocumentView::from).collect::<Vec<_>>(),
    })
    .into_response())
}

/// Accepted, not done. Ingestion happens in the worker, and the response says
/// where to watch for it.
async fn upload_document(
    State(state): State<AppState>,
    session: SessionAuth,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Response, ApiError> {
    let declared_size = content_length(&headers);

    let document = uploads::receive(
        &state.knowledge,
        &session.ctx,
        &state.fetcher,
        multipart,
        declared_size,
    )
    .await
    .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok((StatusCode::ACCEPTED, Json(DocumentView::from(&document))).into_response())
}

async fn get_document(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(document_id): Path<String>,
) -> Result<Response, ApiError> {
    let document_id: DocumentId = parse_id(&document_id, &session.request_id)?;
    let document = state
        .knowledge
        .get_document(&session.ctx, document_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(DocumentView::from(&document)).into_response())
}

async fn retry_document(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(document_id): Path<String>,
) -> Result<Response, ApiError> {
    let document_id: DocumentId = parse_id(&document_id, &session.request_id)?;
    let document = state
        .knowledge
        .retry(&session.ctx, document_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok((StatusCode::ACCEPTED, Json(DocumentView::from(&document))).into_response())
}

/// Replacing a document is an upload of its own: a new document, and the old
/// one deleted once the new one is safely queued. Versioning within a single
/// document arrives with re-embedding in Phase B.
async fn reupload_document(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(document_id): Path<String>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Response, ApiError> {
    let document_id: DocumentId = parse_id(&document_id, &session.request_id)?;
    let reject = |e| ApiError::from_domain(e, session.request_id.clone());

    // Proves the document exists in this tenant before anything is written.
    state
        .knowledge
        .get_document(&session.ctx, document_id)
        .await
        .map_err(reject)?;

    let declared_size = content_length(&headers);
    let replacement = uploads::receive(
        &state.knowledge,
        &session.ctx,
        &state.fetcher,
        multipart,
        declared_size,
    )
    .await
    .map_err(reject)?;

    state
        .knowledge
        .delete_document(&session.ctx, document_id)
        .await
        .map_err(reject)?;

    Ok((StatusCode::ACCEPTED, Json(DocumentView::from(&replacement))).into_response())
}

async fn delete_document(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(document_id): Path<String>,
) -> Result<Response, ApiError> {
    let document_id: DocumentId = parse_id(&document_id, &session.request_id)?;
    state
        .knowledge
        .delete_document(&session.ctx, document_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The declared size, used to refuse an oversized upload before reading it.
/// It is a claim, not a guarantee — the stream is counted as well.
fn content_length(headers: &HeaderMap) -> Option<i64> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
}
