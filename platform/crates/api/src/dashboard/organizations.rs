//! Organizations and workspaces.

use anthovai_core::WorkspaceId;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::extract::{SessionAuth, SessionUser};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/organizations", post(create_organization))
        .route("/organizations/current", get(current_organization))
        .route("/organizations/current", patch(rename_organization))
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route(
            "/workspaces/{workspace_id}",
            get(get_workspace).delete(delete_workspace),
        )
}

#[derive(Serialize)]
struct OrganizationView {
    id: String,
    slug: String,
    name: String,
    plan: String,
    created_at: String,
}

impl From<&anthovai_tenant::Organization> for OrganizationView {
    fn from(org: &anthovai_tenant::Organization) -> Self {
        Self {
            id: org.id.to_string(),
            slug: org.slug.clone(),
            name: org.name.clone(),
            plan: org.plan.as_str().to_owned(),
            created_at: org.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
struct WorkspaceView {
    id: String,
    name: String,
    slug: String,
}

impl From<&anthovai_tenant::Workspace> for WorkspaceView {
    fn from(workspace: &anthovai_tenant::Workspace) -> Self {
        Self {
            id: workspace.id.to_string(),
            name: workspace.name.clone(),
            slug: workspace.slug.clone(),
        }
    }
}

#[derive(Deserialize)]
struct CreateOrganizationRequest {
    name: String,
    slug: String,
}

#[derive(Serialize)]
struct CreateOrganizationResponse {
    organization: OrganizationView,
    default_workspace: WorkspaceView,
}

/// Creating an organization is the one dashboard write that needs a signed-in
/// user but no organization yet — it is what produces one.
async fn create_organization(
    State(state): State<AppState>,
    session: SessionUser,
    Json(body): Json<CreateOrganizationRequest>,
) -> Result<Response, ApiError> {
    let created = state
        .tenants
        .create_organization(session.user.id, &body.name, &body.slug)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok((
        StatusCode::CREATED,
        Json(CreateOrganizationResponse {
            organization: OrganizationView::from(&created.organization),
            default_workspace: WorkspaceView::from(&created.default_workspace),
        }),
    )
        .into_response())
}

/// "Current" is whichever organization `X-Org-Id` named, and the extractor has
/// already proved membership. There is no route that takes an organization id
/// in the path: that would be a second, weaker place to get this check wrong.
async fn current_organization(
    State(state): State<AppState>,
    session: SessionAuth,
) -> Result<Response, ApiError> {
    let organization = state
        .tenants
        .get_organization(&session.ctx)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(OrganizationView::from(&organization)).into_response())
}

#[derive(Deserialize)]
struct RenameRequest {
    name: String,
}

async fn rename_organization(
    State(state): State<AppState>,
    session: SessionAuth,
    Json(body): Json<RenameRequest>,
) -> Result<Response, ApiError> {
    state
        .tenants
        .rename_organization(&session.ctx, &body.name)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    current_organization(State(state), session).await
}

#[derive(Deserialize)]
struct CreateWorkspaceRequest {
    name: String,
    slug: String,
}

async fn create_workspace(
    State(state): State<AppState>,
    session: SessionAuth,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<Response, ApiError> {
    let workspace = state
        .tenants
        .create_workspace(&session.ctx, &body.name, &body.slug)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok((StatusCode::CREATED, Json(WorkspaceView::from(&workspace))).into_response())
}

#[derive(Serialize)]
struct ListResponse<T> {
    data: Vec<T>,
}

async fn list_workspaces(
    State(state): State<AppState>,
    session: SessionAuth,
) -> Result<Response, ApiError> {
    let workspaces = state
        .tenants
        .list_workspaces(&session.ctx)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(ListResponse {
        data: workspaces
            .iter()
            .map(WorkspaceView::from)
            .collect::<Vec<_>>(),
    })
    .into_response())
}

async fn get_workspace(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(workspace_id): Path<String>,
) -> Result<Response, ApiError> {
    let workspace_id: WorkspaceId = parse_id(&workspace_id, &session.request_id)?;
    let workspace = state
        .tenants
        .get_workspace(&session.ctx, workspace_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(WorkspaceView::from(&workspace)).into_response())
}

async fn delete_workspace(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(workspace_id): Path<String>,
) -> Result<Response, ApiError> {
    let workspace_id: WorkspaceId = parse_id(&workspace_id, &session.request_id)?;
    state
        .tenants
        .delete_workspace(&session.ctx, workspace_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Unused for now; kept so the query shape is settled before agents need it.
#[derive(Deserialize)]
pub struct WorkspaceFilter {
    pub workspace_id: Option<String>,
}

pub fn parse_id<T: std::str::FromStr>(raw: &str, request_id: &str) -> Result<T, ApiError> {
    raw.parse().map_err(|_| {
        ApiError::from_domain(
            anthovai_core::DomainError::validation("that is not a valid id"),
            request_id.to_owned(),
        )
    })
}

/// Read `?workspace_id=` when present.
pub fn workspace_filter(
    Query(filter): Query<WorkspaceFilter>,
    request_id: &str,
) -> Result<Option<WorkspaceId>, ApiError> {
    filter
        .workspace_id
        .as_deref()
        .map(|raw| parse_id(raw, request_id))
        .transpose()
}
