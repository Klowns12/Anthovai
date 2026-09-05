//! Agent management.

use anthovai_agent::{AgentConfig, AgentDetail, CreateAgent, UpdateAgent};
use anthovai_core::{AgentId, DomainError, KnowledgeBaseId};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::dashboard::organizations::{parse_id, workspace_filter, WorkspaceFilter};
use crate::error::ApiError;
use crate::extract::SessionAuth;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/agents", get(list).post(create))
        .route("/agents/{agent_id}", get(get_agent).patch(update))
        .route("/agents/{agent_id}/publish", post(publish))
        .route("/agents/{agent_id}/rollback", post(rollback))
        .route("/agents/{agent_id}/pause", post(pause))
        .route("/agents/{agent_id}/resume", post(resume))
        .route("/agents/{agent_id}/archive", post(archive))
        .route(
            "/agents/{agent_id}/knowledge_bases",
            put(set_knowledge_bases),
        )
}

#[derive(Serialize)]
struct AgentSummaryView {
    id: String,
    workspace_id: String,
    name: String,
    description: Option<String>,
    status: String,
    published: bool,
    updated_at: String,
}

impl From<&anthovai_agent::repo::AgentRow> for AgentSummaryView {
    fn from(agent: &anthovai_agent::repo::AgentRow) -> Self {
        Self {
            id: agent.id.to_string(),
            workspace_id: agent.workspace_id.to_string(),
            name: agent.name.clone(),
            description: agent.description.clone(),
            status: agent.status.as_str().to_owned(),
            published: agent.published_version_id.is_some(),
            updated_at: agent.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
struct VersionView {
    id: String,
    version: i32,
    created_at: String,
}

impl From<&anthovai_agent::repo::AgentVersionRow> for VersionView {
    fn from(version: &anthovai_agent::repo::AgentVersionRow) -> Self {
        Self {
            id: version.id.to_string(),
            version: version.version,
            created_at: version.created_at.to_rfc3339(),
        }
    }
}

/// The dashboard sees both versions: the draft it is editing and the one
/// customers are currently being served.
#[derive(Serialize)]
struct AgentDetailView {
    #[serde(flatten)]
    summary: AgentSummaryView,
    draft_version: Option<i32>,
    published_version: Option<i32>,
    draft_config: Option<AgentConfig>,
    published_config: Option<AgentConfig>,
    knowledge_base_ids: Vec<String>,
    versions: Vec<VersionView>,
}

impl From<&AgentDetail> for AgentDetailView {
    fn from(detail: &AgentDetail) -> Self {
        Self {
            summary: AgentSummaryView::from(&detail.agent),
            draft_version: detail.draft.as_ref().map(|v| v.version),
            published_version: detail.published.as_ref().map(|v| v.version),
            draft_config: detail.draft.as_ref().map(|v| v.config.clone()),
            published_config: detail.published.as_ref().map(|v| v.config.clone()),
            knowledge_base_ids: detail
                .knowledge_base_ids
                .iter()
                .map(|k| k.to_string())
                .collect(),
            versions: detail.versions.iter().map(VersionView::from).collect(),
        }
    }
}

#[derive(Deserialize)]
struct CreateRequest {
    workspace_id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    config: AgentConfig,
}

async fn create(
    State(state): State<AppState>,
    session: SessionAuth,
    Json(body): Json<CreateRequest>,
) -> Result<Response, ApiError> {
    let detail = state
        .agents
        .create(
            &session.ctx,
            CreateAgent {
                workspace_id: parse_id(&body.workspace_id, &session.request_id)?,
                name: body.name,
                description: body.description,
                config: body.config,
            },
        )
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok((StatusCode::CREATED, Json(AgentDetailView::from(&detail))).into_response())
}

#[derive(Serialize)]
struct ListResponse {
    data: Vec<AgentSummaryView>,
}

async fn list(
    State(state): State<AppState>,
    session: SessionAuth,
    filter: Query<WorkspaceFilter>,
) -> Result<Response, ApiError> {
    let workspace_id = workspace_filter(filter, &session.request_id)?;
    let agents = state
        .agents
        .list(&session.ctx, workspace_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(ListResponse {
        data: agents.iter().map(AgentSummaryView::from).collect(),
    })
    .into_response())
}

async fn get_agent(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(agent_id): Path<String>,
) -> Result<Response, ApiError> {
    let agent_id: AgentId = parse_id(&agent_id, &session.request_id)?;
    let detail = state
        .agents
        .get(&session.ctx, agent_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(AgentDetailView::from(&detail)).into_response())
}

#[derive(Deserialize)]
struct UpdateRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    config: Option<AgentConfig>,
}

async fn update(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(agent_id): Path<String>,
    Json(body): Json<UpdateRequest>,
) -> Result<Response, ApiError> {
    let agent_id: AgentId = parse_id(&agent_id, &session.request_id)?;
    let detail = state
        .agents
        .update(
            &session.ctx,
            agent_id,
            UpdateAgent {
                name: body.name,
                description: body.description,
                config: body.config,
            },
        )
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(AgentDetailView::from(&detail)).into_response())
}

async fn publish(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(agent_id): Path<String>,
) -> Result<Response, ApiError> {
    let agent_id: AgentId = parse_id(&agent_id, &session.request_id)?;
    let detail = state
        .agents
        .publish(&session.ctx, agent_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(AgentDetailView::from(&detail)).into_response())
}

#[derive(Deserialize)]
struct RollbackRequest {
    version: i32,
}

async fn rollback(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(agent_id): Path<String>,
    Json(body): Json<RollbackRequest>,
) -> Result<Response, ApiError> {
    let agent_id: AgentId = parse_id(&agent_id, &session.request_id)?;
    let detail = state
        .agents
        .rollback(&session.ctx, agent_id, body.version)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(AgentDetailView::from(&detail)).into_response())
}

async fn pause(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(agent_id): Path<String>,
) -> Result<Response, ApiError> {
    let agent_id: AgentId = parse_id(&agent_id, &session.request_id)?;
    state
        .agents
        .pause(&session.ctx, agent_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn resume(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(agent_id): Path<String>,
) -> Result<Response, ApiError> {
    let agent_id: AgentId = parse_id(&agent_id, &session.request_id)?;
    state
        .agents
        .resume(&session.ctx, agent_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn archive(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(agent_id): Path<String>,
) -> Result<Response, ApiError> {
    let agent_id: AgentId = parse_id(&agent_id, &session.request_id)?;
    state
        .agents
        .archive(&session.ctx, agent_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize)]
struct KnowledgeBasesRequest {
    knowledge_base_ids: Vec<String>,
}

/// Replaces the whole set, which makes the request idempotent: the dashboard
/// sends the list it wants rather than a diff it has to compute.
async fn set_knowledge_bases(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(agent_id): Path<String>,
    Json(body): Json<KnowledgeBasesRequest>,
) -> Result<Response, ApiError> {
    let agent_id: AgentId = parse_id(&agent_id, &session.request_id)?;

    if body.knowledge_base_ids.len() > 50 {
        return Err(ApiError::from_domain(
            DomainError::validation("an agent may read at most 50 knowledge bases"),
            session.request_id,
        ));
    }

    let ids = body
        .knowledge_base_ids
        .iter()
        .map(|raw| parse_id::<KnowledgeBaseId>(raw, &session.request_id))
        .collect::<Result<Vec<_>, _>>()?;

    state
        .agents
        .set_knowledge_bases(&session.ctx, agent_id, &ids)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}
