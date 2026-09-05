//! Public read-only view of agents.
//!
//! This is what a customer's own code sees. It shows what an integration needs
//! to know — which agents exist, whether they are live, what knowledge they
//! read — and nothing about how they are configured. Instructions and model
//! policy are ours and the customer dashboard's business, not something to hand
//! to whatever is holding the key.

use anthovai_core::{AgentId, Scope};
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use crate::dashboard::organizations::parse_id;
use crate::error::ApiError;
use crate::extract::ApiKeyAuth;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/agents", get(list))
        .route("/agents/{agent_id}", get(get_agent))
}

/// An agent as an integration sees it.
#[derive(Serialize, ToSchema)]
pub struct AgentView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// `active` or `paused`. An archived agent is not listed at all.
    pub status: String,
}

#[derive(Serialize, ToSchema)]
pub struct AgentDetailView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub knowledge_base_count: usize,
    pub published_version: i32,
}

#[derive(Serialize, ToSchema)]
pub struct AgentListResponse {
    pub data: Vec<AgentView>,
}

#[utoipa::path(
    get,
    path = "/v1/agents",
    tag = "Agents",
    responses(
        (status = 200, description = "The agents this key may use", body = AgentListResponse),
        (status = 401, description = "The key is missing, unknown or revoked", body = crate::error::ErrorBody),
        (status = 403, description = "The key lacks the `agents:read` scope", body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn list(State(state): State<AppState>, auth: ApiKeyAuth) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, auth.request_id.clone());
    auth.ctx.require_scope(Scope::AgentsRead).map_err(reject)?;

    let agents = state
        .agents
        .list(&auth.ctx, auth.ctx.workspace_id)
        .await
        .map_err(reject)?;

    // A key scoped to selected agents sees only those, so the list cannot be
    // used to discover the names of the rest.
    let data = agents
        .iter()
        .filter(|agent| auth.ctx.require_agent(agent.id).is_ok())
        .filter(|agent| agent.status.is_publicly_callable())
        .map(|agent| AgentView {
            id: agent.id.to_string(),
            name: agent.name.clone(),
            description: agent.description.clone(),
            status: agent.status.as_str().to_owned(),
        })
        .collect();

    Ok(Json(AgentListResponse { data }).into_response())
}

#[utoipa::path(
    get,
    path = "/v1/agents/{agent_id}",
    tag = "Agents",
    params(("agent_id" = String, Path, description = "The agent id, `agt_…`")),
    responses(
        (status = 200, description = "The agent", body = AgentDetailView),
        (status = 403, description = "The key is scoped to other agents, or the agent is not published", body = crate::error::ErrorBody),
        (status = 404, description = "No such agent in this organization", body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn get_agent(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Path(agent_id): Path<String>,
) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, auth.request_id.clone());
    auth.ctx.require_scope(Scope::AgentsRead).map_err(reject)?;

    let agent_id: AgentId = parse_id(&agent_id, &auth.request_id)?;
    let resolved = state
        .agents
        .load_published(&auth.ctx, agent_id)
        .await
        .map_err(reject)?;

    Ok(Json(AgentDetailView {
        id: resolved.id.to_string(),
        name: resolved.name,
        description: None,
        status: resolved.status.as_str().to_owned(),
        knowledge_base_count: resolved.knowledge_base_ids.len(),
        published_version: resolved.version,
    })
    .into_response())
}
