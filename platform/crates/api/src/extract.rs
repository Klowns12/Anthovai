//! Request extractors.
//!
//! These are the only place a `TenantCtx` enters a handler, and they are the
//! reason handlers never read a tenant id from a body or a path. Two doors:
//! an API key for customers, a session cookie plus `X-Org-Id` for the dashboard.

use anthovai_core::{DomainError, OrgId, RequestId, TenantCtx};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, Method};

use crate::error::ApiError;
use crate::request_id;
use crate::state::AppState;

/// The request id, resolved once and reused by everything that reports it.
pub struct ReqId(pub String);

impl<S: Send + Sync> FromRequestParts<S> for ReqId {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(ReqId(resolve_request_id(parts)))
    }
}

/// Public API authentication: `Authorization: Bearer av_live_…`.
pub struct ApiKeyAuth {
    pub ctx: TenantCtx,
    pub request_id: String,
}

impl FromRequestParts<AppState> for ApiKeyAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let request_id = resolve_request_id(parts);
        let reject = |err: DomainError| ApiError::from_domain(err, request_id.clone());

        // A key in a query string ends up in access logs, browser history and
        // referrer headers. Refuse it outright rather than quietly accepting a
        // key that should now be considered leaked.
        if let Some(query) = parts.uri.query() {
            if query.contains("api_key=") || query.contains("apikey=") {
                return Err(reject(DomainError::validation(
                    "send the API key in the Authorization header, not the query string",
                )));
            }
        }

        let header_value = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| reject(DomainError::Unauthenticated("missing_bearer_token")))?;

        let ctx = state
            .auth
            .authenticate_api_key(header_value, parse_request_id(&request_id))
            .await
            .map_err(reject)?;

        let key = format!(
            "key:{}",
            ctx.api_key_id().map(|k| k.to_string()).unwrap_or_default()
        );
        let limit = ctx.plan.limits().requests_per_minute;
        let verdict = state.limits.check(&key, limit, 60);
        if !verdict.allowed {
            return Err(ApiError::from_domain(
                DomainError::RateLimited {
                    retry_after_secs: verdict.reset_in_secs,
                },
                request_id,
            ));
        }

        // Every log line inside this request now says which customer it was
        // for, which is the first question asked of any of them.
        crate::observe::record_tenant(ctx.org_id);

        Ok(Self { ctx, request_id })
    }
}

/// Dashboard authentication: the session cookie identifies the user,
/// `X-Org-Id` says which of their organizations they are working in.
pub struct SessionAuth {
    pub ctx: TenantCtx,
    pub user: anthovai_auth::User,
    pub request_id: String,
}

impl FromRequestParts<AppState> for SessionAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let request_id = resolve_request_id(parts);
        let reject = |err: DomainError| ApiError::from_domain(err, request_id.clone());

        check_origin(parts, state).map_err(reject)?;
        let user = current_user(parts, state, &request_id).await?;

        let org_id: OrgId = parts
            .headers
            .get("x-org-id")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| reject(DomainError::validation("X-Org-Id header is required")))?
            .parse()
            .map_err(|_| reject(DomainError::validation("X-Org-Id is not a valid id")))?;

        // A user who is not a member gets NotFound, so the header cannot be
        // used to discover which organizations exist.
        let (role, plan) = state
            .tenants
            .authorize(user.id, org_id)
            .await
            .map_err(reject)?;

        crate::observe::record_tenant(org_id);

        Ok(Self {
            ctx: state.auth.dashboard_context(user.id, org_id, role, plan),
            user,
            request_id,
        })
    }
}

/// A signed-in user with no organization chosen yet: sign-out, `/me`, and
/// creating the first organization.
pub struct SessionUser {
    pub user: anthovai_auth::User,
    pub token: String,
    pub request_id: String,
}

impl FromRequestParts<AppState> for SessionUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let request_id = resolve_request_id(parts);
        check_origin(parts, state).map_err(|err| ApiError::from_domain(err, request_id.clone()))?;

        let token = session_token(parts)
            .ok_or_else(|| {
                ApiError::from_domain(
                    DomainError::Unauthenticated("session_expired"),
                    request_id.clone(),
                )
            })?
            .to_owned();

        let user = state
            .auth
            .verify_session(&token)
            .await
            .map_err(|err| ApiError::from_domain(err, request_id.clone()))?;

        Ok(Self {
            user,
            token,
            request_id,
        })
    }
}

async fn current_user(
    parts: &Parts,
    state: &AppState,
    request_id: &str,
) -> Result<anthovai_auth::User, ApiError> {
    let token = session_token(parts).ok_or_else(|| {
        ApiError::from_domain(
            DomainError::Unauthenticated("session_expired"),
            request_id.to_owned(),
        )
    })?;

    state
        .auth
        .verify_session(token)
        .await
        .map_err(|err| ApiError::from_domain(err, request_id.to_owned()))
}

fn session_token(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(anthovai_auth::session::token_from_cookie_header)
}

/// The dashboard's cookie is `SameSite=Lax`, which already blocks cross-site
/// POSTs from a form. Checking `Origin` as well covers the cases Lax does not,
/// and costs one header comparison.
fn check_origin(parts: &Parts, state: &AppState) -> anthovai_core::Result<()> {
    if matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(());
    }

    let origin = parts
        .headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok());

    match origin {
        // Same-origin requests from a browser omit Origin on some navigations,
        // and server-side callers never send it.
        None => Ok(()),
        Some(value)
            if state
                .dashboard_origins
                .iter()
                .any(|allowed| allowed == value) =>
        {
            Ok(())
        }
        Some(_) => Err(DomainError::Forbidden("origin_not_allowed")),
    }
}

fn resolve_request_id(parts: &Parts) -> String {
    request_id::resolve(
        parts
            .headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok()),
    )
}

/// The public API's `request_id` is a typed id for usage records. A caller's
/// own trace id is echoed in headers but does not become ours.
fn parse_request_id(value: &str) -> RequestId {
    value.parse().unwrap_or_else(|_| RequestId::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caller_trace_id_does_not_become_our_request_id() {
        let mine = RequestId::new();
        assert_eq!(parse_request_id(&mine.to_string()), mine);
        // Anything else gets a fresh one rather than a parse failure.
        assert_ne!(
            parse_request_id("trace-from-their-system").to_string(),
            "trace-from-their-system"
        );
    }
}
