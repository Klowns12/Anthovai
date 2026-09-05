//! API key management.
//!
//! The secret appears in exactly one response, from exactly two routes: create
//! and rotate. Every other view of a key shows its prefix.

use anthovai_auth::{CreateApiKey, Environment};
use anthovai_core::{AgentId, AgentScope, ApiKeyId, DomainError, Scope};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::dashboard::organizations::{parse_id, workspace_filter, WorkspaceFilter};
use crate::error::ApiError;
use crate::extract::SessionAuth;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api_keys", get(list).post(create))
        .route("/api_keys/{key_id}/rotate", post(rotate))
        .route("/api_keys/{key_id}/revoke", post(revoke))
}

#[derive(Deserialize)]
struct CreateRequest {
    workspace_id: String,
    name: String,
    #[serde(default = "default_environment")]
    environment: String,
    #[serde(default = "default_scopes")]
    scopes: Vec<String>,
    #[serde(default = "default_true")]
    all_agents: bool,
    #[serde(default)]
    agent_ids: Vec<String>,
    #[serde(default)]
    expires_in_days: Option<i64>,
}

fn default_environment() -> String {
    "live".to_owned()
}

fn default_scopes() -> Vec<String> {
    vec!["chat".to_owned()]
}

fn default_true() -> bool {
    true
}

/// The one response that carries a secret.
#[derive(Serialize)]
struct IssuedKeyView {
    id: String,
    name: String,
    prefix: String,
    environment: String,
    /// Shown once. It is not stored anywhere we can read it back from.
    secret: String,
    warning: &'static str,
}

#[derive(Serialize)]
struct KeyView {
    id: String,
    workspace_id: String,
    name: String,
    prefix: String,
    environment: String,
    scopes: Vec<String>,
    all_agents: bool,
    status: String,
    expires_at: Option<String>,
    last_used_at: Option<String>,
    created_at: String,
}

impl From<&anthovai_auth::repo::ApiKeySummary> for KeyView {
    fn from(key: &anthovai_auth::repo::ApiKeySummary) -> Self {
        Self {
            id: key.id.to_string(),
            workspace_id: key.workspace_id.to_string(),
            name: key.name.clone(),
            prefix: key.prefix.clone(),
            environment: key.environment.as_str().to_owned(),
            scopes: key.scopes.iter().map(|s| s.as_str().to_owned()).collect(),
            all_agents: key.all_agents,
            status: key.status.as_str().to_owned(),
            expires_at: key.expires_at.map(|at| at.to_rfc3339()),
            last_used_at: key.last_used_at.map(|at| at.to_rfc3339()),
            created_at: key.created_at.to_rfc3339(),
        }
    }
}

async fn create(
    State(state): State<AppState>,
    session: SessionAuth,
    Json(body): Json<CreateRequest>,
) -> Result<Response, ApiError> {
    let request = build_request(&body, &session.request_id)?;

    // A live key can move real data, so the address behind the account has to
    // be proved first. Test keys need no such thing.
    if request.environment == Environment::Live && !session.user.may_create_live_keys() {
        return Err(ApiError::from_domain(
            DomainError::Forbidden("email_not_verified"),
            session.request_id,
        ));
    }

    let issued = state
        .auth
        .create_api_key(&session.ctx, request)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(issued_response(issued, StatusCode::CREATED))
}

async fn rotate(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(key_id): Path<String>,
    Json(body): Json<CreateRequest>,
) -> Result<Response, ApiError> {
    let key_id: ApiKeyId = parse_id(&key_id, &session.request_id)?;
    let request = build_request(&body, &session.request_id)?;

    let issued = state
        .auth
        .rotate_api_key(&session.ctx, key_id, request)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(issued_response(issued, StatusCode::CREATED))
}

async fn revoke(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(key_id): Path<String>,
) -> Result<Response, ApiError> {
    let key_id: ApiKeyId = parse_id(&key_id, &session.request_id)?;
    state
        .auth
        .revoke_api_key(&session.ctx, key_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Serialize)]
struct ListResponse {
    data: Vec<KeyView>,
}

async fn list(
    State(state): State<AppState>,
    session: SessionAuth,
    filter: Query<WorkspaceFilter>,
) -> Result<Response, ApiError> {
    let workspace_id = workspace_filter(filter, &session.request_id)?;

    let keys = state
        .auth
        .list_api_keys(&session.ctx, workspace_id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(ListResponse {
        data: keys.iter().map(KeyView::from).collect(),
    })
    .into_response())
}

fn build_request(body: &CreateRequest, request_id: &str) -> Result<CreateApiKey, ApiError> {
    let reject = |err: DomainError| ApiError::from_domain(err, request_id.to_owned());

    let environment: Environment = body
        .environment
        .parse()
        .map_err(|_| reject(DomainError::validation("environment must be live or test")))?;

    let scopes = body
        .scopes
        .iter()
        .map(|s| s.parse::<Scope>())
        .collect::<anthovai_core::Result<Vec<Scope>>>()
        .map_err(reject)?;

    let agents = if body.all_agents {
        AgentScope::All
    } else {
        let ids = body
            .agent_ids
            .iter()
            .map(|raw| parse_id::<AgentId>(raw, request_id))
            .collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty() {
            return Err(reject(DomainError::validation(
                "a key scoped to selected agents needs at least one agent_id",
            )));
        }
        AgentScope::Only(ids)
    };

    Ok(CreateApiKey {
        workspace_id: parse_id(&body.workspace_id, request_id)?,
        name: body.name.clone(),
        environment,
        scopes,
        agents,
        expires_in_days: body.expires_in_days,
    })
}

fn issued_response(issued: anthovai_auth::IssuedApiKey, status: StatusCode) -> Response {
    let mut response = (
        status,
        Json(IssuedKeyView {
            id: issued.id.to_string(),
            name: issued.name,
            prefix: issued.prefix,
            environment: issued.environment.as_str().to_owned(),
            secret: issued.secret,
            warning: "Copy this key now. It will not be shown again.",
        }),
    )
        .into_response();

    // The body holds a credential: it must not sit in a proxy or a browser cache.
    if let Ok(value) = "no-store".parse() {
        response.headers_mut().insert(header::CACHE_CONTROL, value);
    }
    response
}
