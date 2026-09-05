//! Knowledge over the public API.
//!
//! This exists so a customer can keep their knowledge base in step with their
//! own system — a nightly sync of a course catalogue, a webhook when a handbook
//! changes — without a person opening the dashboard.

use anthovai_core::{DocumentId, DomainError, KnowledgeBaseId, Scope};
use anthovai_knowledge::{CreateKnowledgeBase, DocumentStatus};
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::dashboard::knowledge::{DocumentView, KnowledgeBaseView};
use crate::dashboard::organizations::parse_id;
use crate::error::ApiError;
use crate::extract::ApiKeyAuth;
use crate::state::AppState;
use crate::uploads;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/knowledge_bases", get(list_knowledge_bases).post(create))
        .route(
            "/knowledge_bases/{kb_id}",
            get(get_knowledge_base).delete(delete_knowledge_base),
        )
}

/// Document routes, at their full path, so they can carry the upload-sized
/// body limit rather than the JSON one.
pub fn document_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/documents", get(list_documents).post(upload))
        .route(
            "/v1/documents/{document_id}",
            get(get_document).delete(delete_document),
        )
}

#[derive(Serialize, ToSchema)]
pub struct KnowledgeBaseListResponse {
    pub data: Vec<crate::dashboard::knowledge::KnowledgeBaseView>,
}

#[derive(Serialize, ToSchema)]
pub struct DocumentListResponse {
    pub data: Vec<crate::dashboard::knowledge::DocumentView>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateKnowledgeBaseRequest {
    /// Omit to use the workspace the key belongs to.
    pub workspace_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/knowledge_bases",
    tag = "Knowledge",
    request_body = CreateKnowledgeBaseRequest,
    responses(
        (status = 201, description = "Created", body = crate::dashboard::knowledge::KnowledgeBaseView),
        (status = 403, description = "The key lacks the `knowledge:write` scope", body = crate::error::ErrorBody),
        (status = 429, description = "The plan's knowledge base limit is reached", body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn create(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Json(body): Json<CreateKnowledgeBaseRequest>,
) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, auth.request_id.clone());

    // A key belongs to one workspace, so that is where its knowledge bases go.
    // Naming a different one is refused rather than quietly ignored.
    let workspace_id = match body.workspace_id.as_deref() {
        Some(raw) => {
            let named: anthovai_core::WorkspaceId = parse_id(raw, &auth.request_id)?;
            if Some(named) != auth.ctx.workspace_id {
                return Err(reject(DomainError::NotFound("workspace")));
            }
            named
        }
        None => auth
            .ctx
            .workspace_id
            .ok_or_else(|| reject(DomainError::validation("workspace_id is required")))?,
    };

    let kb = state
        .knowledge
        .create_knowledge_base(
            &auth.ctx,
            CreateKnowledgeBase {
                workspace_id,
                name: body.name,
                description: body.description,
            },
        )
        .await
        .map_err(reject)?;

    Ok((StatusCode::CREATED, Json(KnowledgeBaseView::from(&kb))).into_response())
}

#[utoipa::path(
    get,
    path = "/v1/knowledge_bases",
    tag = "Knowledge",
    responses((status = 200, body = KnowledgeBaseListResponse)),
    security(("api_key" = []))
)]
async fn list_knowledge_bases(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
) -> Result<Response, ApiError> {
    let bases = state
        .knowledge
        .list_knowledge_bases(&auth.ctx, auth.ctx.workspace_id)
        .await
        .map_err(|e| ApiError::from_domain(e, auth.request_id.clone()))?;

    Ok(Json(KnowledgeBaseListResponse {
        data: bases
            .iter()
            .map(KnowledgeBaseView::from)
            .collect::<Vec<_>>(),
    })
    .into_response())
}

#[utoipa::path(
    get,
    path = "/v1/knowledge_bases/{kb_id}",
    tag = "Knowledge",
    params(("kb_id" = String, Path, description = "The knowledge base id, `kb_…`")),
    responses(
        (status = 200, body = crate::dashboard::knowledge::KnowledgeBaseView),
        (status = 404, body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn get_knowledge_base(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Path(kb_id): Path<String>,
) -> Result<Response, ApiError> {
    let kb_id: KnowledgeBaseId = parse_id(&kb_id, &auth.request_id)?;
    let kb = state
        .knowledge
        .get_knowledge_base(&auth.ctx, kb_id)
        .await
        .map_err(|e| ApiError::from_domain(e, auth.request_id.clone()))?;

    Ok(Json(KnowledgeBaseView::from(&kb)).into_response())
}

#[utoipa::path(
    delete,
    path = "/v1/knowledge_bases/{kb_id}",
    tag = "Knowledge",
    params(("kb_id" = String, Path, description = "The knowledge base id, `kb_…`")),
    responses(
        (status = 204, description = "Deleted, along with every document in it"),
        (status = 404, body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn delete_knowledge_base(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Path(kb_id): Path<String>,
) -> Result<Response, ApiError> {
    let kb_id: KnowledgeBaseId = parse_id(&kb_id, &auth.request_id)?;
    state
        .knowledge
        .delete_knowledge_base(&auth.ctx, kb_id)
        .await
        .map_err(|e| ApiError::from_domain(e, auth.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct DocumentQuery {
    pub knowledge_base_id: String,
    /// Filter by ingestion status, e.g. `failed`.
    pub status: Option<String>,
}

#[utoipa::path(
    get,
    path = "/v1/documents",
    tag = "Knowledge",
    params(DocumentQuery),
    responses((status = 200, body = DocumentListResponse)),
    security(("api_key" = []))
)]
async fn list_documents(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Query(query): Query<DocumentQuery>,
) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, auth.request_id.clone());

    let kb_id: KnowledgeBaseId = parse_id(&query.knowledge_base_id, &auth.request_id)?;
    let status: Option<DocumentStatus> = query
        .status
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(reject)?;

    let documents = state
        .knowledge
        .list_documents(&auth.ctx, kb_id, status)
        .await
        .map_err(reject)?;

    Ok(Json(DocumentListResponse {
        data: documents.iter().map(DocumentView::from).collect::<Vec<_>>(),
    })
    .into_response())
}

/// Add a document to a knowledge base.
///
/// `multipart/form-data` with `knowledge_base_id` first, then exactly one of
/// `file`, `text` (with `title`), or `url`. The order matters: the plan limit
/// is checked against the knowledge base before any bytes are stored.
#[utoipa::path(
    post,
    path = "/v1/documents",
    tag = "Knowledge",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(
        (status = 202, description = "Accepted, not finished. Ingestion runs in a worker; poll the document for its status.", body = crate::dashboard::knowledge::DocumentView),
        (status = 400, description = "`url_not_allowed` for an address that is not publicly reachable; `unsupported_file_type` for a format with no parser", body = crate::error::ErrorBody),
        (status = 403, description = "The key lacks the `knowledge:write` scope", body = crate::error::ErrorBody),
        (status = 413, description = "Past the plan's per-file limit", body = crate::error::ErrorBody),
        (status = 429, description = "The plan's document or storage limit is reached", body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn upload(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, auth.request_id.clone());
    auth.ctx
        .require_scope(Scope::KnowledgeWrite)
        .map_err(reject)?;

    let declared_size = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());

    let document = uploads::receive(
        &state.knowledge,
        &auth.ctx,
        &state.fetcher,
        multipart,
        declared_size,
    )
    .await
    .map_err(reject)?;

    Ok((StatusCode::ACCEPTED, Json(DocumentView::from(&document))).into_response())
}

#[utoipa::path(
    get,
    path = "/v1/documents/{document_id}",
    tag = "Knowledge",
    params(("document_id" = String, Path, description = "The document id, `doc_…`")),
    responses(
        (status = 200, description = "The document, including how its ingestion is going", body = crate::dashboard::knowledge::DocumentView),
        (status = 404, body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn get_document(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Path(document_id): Path<String>,
) -> Result<Response, ApiError> {
    let document_id: DocumentId = parse_id(&document_id, &auth.request_id)?;
    let document = state
        .knowledge
        .get_document(&auth.ctx, document_id)
        .await
        .map_err(|e| ApiError::from_domain(e, auth.request_id.clone()))?;

    Ok(Json(DocumentView::from(&document)).into_response())
}

#[utoipa::path(
    delete,
    path = "/v1/documents/{document_id}",
    tag = "Knowledge",
    params(("document_id" = String, Path, description = "The document id, `doc_…`")),
    responses(
        (status = 204, description = "Deleted. Its chunks are removed by the worker shortly after."),
        (status = 404, body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn delete_document(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Path(document_id): Path<String>,
) -> Result<Response, ApiError> {
    let document_id: DocumentId = parse_id(&document_id, &auth.request_id)?;
    state
        .knowledge
        .delete_document(&auth.ctx, document_id)
        .await
        .map_err(|e| ApiError::from_domain(e, auth.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}
