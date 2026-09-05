//! Sign-up, sign-in, sign-out and `/me`.

use anthovai_core::DomainError;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::extract::{ReqId, SessionUser};
use crate::state::AppState;

/// Sign-in attempts allowed per window, per address and per client address.
const SIGN_IN_ATTEMPTS: u32 = 5;
const SIGN_IN_WINDOW_SECS: i64 = 900;
/// Sign-ups are limited per client address only: there is no account yet.
const SIGN_UP_ATTEMPTS: u32 = 10;
const SIGN_UP_WINDOW_SECS: i64 = 3_600;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/signup", post(sign_up))
        .route("/auth/login", post(sign_in))
        .route("/auth/logout", post(sign_out))
        .route("/me", get(me))
}

#[derive(Deserialize)]
struct SignUpRequest {
    email: String,
    password: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Serialize)]
struct SignUpResponse {
    user_id: String,
    email: String,
}

async fn sign_up(
    State(state): State<AppState>,
    ReqId(request_id): ReqId,
    headers: HeaderMap,
    Json(body): Json<SignUpRequest>,
) -> Result<Response, ApiError> {
    let client = client_key(&headers);
    enforce(
        &state,
        &format!("signup:{client}"),
        SIGN_UP_ATTEMPTS,
        SIGN_UP_WINDOW_SECS,
        &request_id,
    )?;

    let user_id = state
        .auth
        .sign_up(&body.email, &body.password, body.name.as_deref())
        .await
        .map_err(|e| ApiError::from_domain(e, request_id.clone()))?;

    Ok((
        StatusCode::CREATED,
        Json(SignUpResponse {
            user_id: user_id.to_string(),
            email: body.email.trim().to_lowercase(),
        }),
    )
        .into_response())
}

#[derive(Deserialize)]
struct SignInRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct SignInResponse {
    user_id: String,
    expires_at: String,
}

async fn sign_in(
    State(state): State<AppState>,
    ReqId(request_id): ReqId,
    headers: HeaderMap,
    Json(body): Json<SignInRequest>,
) -> Result<Response, ApiError> {
    let client = client_key(&headers);
    let email_key = format!("login:{}", body.email.trim().to_lowercase());
    let client_key = format!("login-ip:{client}");

    // Both are counted: per-address stops one account being ground down, and
    // per-client stops one machine working through a list of addresses.
    enforce(
        &state,
        &email_key,
        SIGN_IN_ATTEMPTS,
        SIGN_IN_WINDOW_SECS,
        &request_id,
    )?;
    enforce(
        &state,
        &client_key,
        SIGN_IN_ATTEMPTS * 4,
        SIGN_IN_WINDOW_SECS,
        &request_id,
    )?;

    let session = state
        .auth
        .sign_in(
            &body.email,
            &body.password,
            client_ip(&headers).as_deref(),
            headers
                .get(header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(|ua| truncate(ua, 512))
                .as_deref(),
        )
        .await
        .map_err(|e| ApiError::from_domain(e, request_id.clone()))?;

    // A correct password clears the count, so one person's typo does not spend
    // the window for the rest of it.
    state.limits.forget(&email_key);

    let ttl = session.expires_at - state.clock.now();
    let cookie = anthovai_auth::session::cookie_for(&session.token, ttl.max(Duration::zero()));

    let mut response = Json(SignInResponse {
        user_id: session.user_id.to_string(),
        expires_at: session.expires_at.to_rfc3339(),
    })
    .into_response();
    set_cookie(&mut response, &cookie);
    no_store(&mut response);
    Ok(response)
}

async fn sign_out(
    State(state): State<AppState>,
    session: SessionUser,
) -> Result<Response, ApiError> {
    state
        .auth
        .sign_out(&session.token)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    let mut response = StatusCode::NO_CONTENT.into_response();
    set_cookie(&mut response, &anthovai_auth::session::clearing_cookie());
    Ok(response)
}

#[derive(Serialize)]
struct MeResponse {
    user: UserView,
    organizations: Vec<OrganizationView>,
}

#[derive(Serialize)]
struct UserView {
    id: String,
    email: String,
    name: Option<String>,
    email_verified: bool,
}

#[derive(Serialize)]
struct OrganizationView {
    id: String,
    role: String,
}

async fn me(State(state): State<AppState>, session: SessionUser) -> Result<Response, ApiError> {
    let memberships = state
        .tenants
        .list_memberships(session.user.id)
        .await
        .map_err(|e| ApiError::from_domain(e, session.request_id.clone()))?;

    Ok(Json(MeResponse {
        user: UserView {
            id: session.user.id.to_string(),
            email: session.user.email.clone(),
            name: session.user.name.clone(),
            email_verified: session.user.email_verified_at.is_some(),
        },
        organizations: memberships
            .iter()
            // A pending invitation is not yet an organization the user is in.
            .filter(|m| m.is_active())
            .map(|m| OrganizationView {
                id: m.org_id.to_string(),
                role: m.role.as_str().to_owned(),
            })
            .collect(),
    })
    .into_response())
}

fn enforce(
    state: &AppState,
    key: &str,
    limit: u32,
    window_secs: i64,
    request_id: &str,
) -> Result<(), ApiError> {
    let verdict = state.limits.check(key, limit, window_secs);
    if verdict.allowed {
        return Ok(());
    }
    Err(ApiError::from_domain(
        DomainError::RateLimited {
            retry_after_secs: verdict.reset_in_secs,
        },
        request_id.to_owned(),
    ))
}

/// Who to count this attempt against. Behind a load balancer the real address
/// is in `X-Forwarded-For`; the value is only ever used as a counter key, so a
/// forged one costs the forger their own bucket and nobody else's.
fn client_key(headers: &HeaderMap) -> String {
    forwarded_for(headers).unwrap_or_else(|| "unknown".to_owned())
}

/// The same header, but only when it really is an IP address.
///
/// `sessions.ip` is an `inet` column, and this header is set by whoever is in
/// front of us — including, if a proxy is misconfigured, the client. Passing it
/// through unchecked lets anyone turn sign-in into a database error, which is
/// how this was found. Anything that does not parse is simply not recorded.
fn client_ip(headers: &HeaderMap) -> Option<String> {
    forwarded_for(headers).filter(|value| value.parse::<std::net::IpAddr>().is_ok())
}

fn forwarded_for(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        // The header is a chain; the client is the first entry.
        .and_then(|v| v.split(',').next())
        .map(|v| truncate(v.trim(), 64))
        .filter(|v| !v.is_empty())
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn set_cookie(response: &mut Response, cookie: &str) {
    if let Ok(value) = cookie.parse() {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}

/// Responses carrying a credential must not be cached anywhere.
fn no_store(response: &mut Response) {
    if let Ok(value) = "no-store".parse() {
        response.headers_mut().insert(header::CACHE_CONTROL, value);
    }
}
