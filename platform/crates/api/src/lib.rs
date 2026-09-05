//! The HTTP layer.
//!
//! Axum types stop here: domain crates take and return their own structs, and
//! this crate maps them onto the wire. Three routers, three audiences:
//! `/v1` for customers, `/dashboard/v1` for our own frontend, `/internal` for us.
//!
//! The split matters beyond tidiness. `/v1` is a contract we cannot break
//! without a version bump, so it exposes as little as it can get away with;
//! `/dashboard/v1` is ours and can change with the frontend.

pub mod dashboard;
pub mod error;
pub mod extract;
pub mod fetch;
pub mod health;
pub mod metrics;
pub mod observe;
pub mod openapi;
pub mod public;
pub mod rate_limit;
pub mod request_id;
pub mod security_headers;
pub mod state;
pub mod uploads;

pub use error::{ApiError, ErrorBody};
pub use rate_limit::RateLimiter;
pub use state::{AppState, Services};

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// JSON bodies are small; uploads are not. The two limits are applied to
/// different routes, because one generous limit would let any endpoint be used
/// to push a hundred megabytes at us.
const MAX_JSON_BODY_BYTES: usize = 1024 * 1024;

/// Customer-facing API. Authenticated by API key.
pub fn public_router(state: AppState) -> Router {
    public::router().with_state(state)
}

/// Dashboard API. Authenticated by session cookie plus `X-Org-Id`.
pub fn dashboard_router(state: AppState) -> Router {
    dashboard::router().with_state(state)
}

/// Health and metrics. Not exposed publicly in production.
pub fn internal_router(state: AppState) -> Router {
    health::router(state)
}

/// Everything mounted together, as the server runs it.
pub fn app(state: AppState) -> Router {
    let timeout = std::time::Duration::from_secs(65);

    let uploads = Router::new()
        .merge(dashboard::knowledge::document_routes())
        .merge(public::knowledge::document_routes())
        .with_state(state.clone())
        // Axum applies its own 2MB limit to extractors like `Multipart`. Left
        // on, it would reject a large upload before our own counter ever ran,
        // and the rejection would arrive as a broken stream rather than as
        // "too large" — so the caller would be told the wrong thing.
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(state::max_upload_bytes()));

    let rest = Router::new()
        .nest("/v1", public_router(state.clone()))
        .nest("/dashboard/v1", dashboard_router(state.clone()))
        .nest("/internal", internal_router(state))
        .layer(RequestBodyLimitLayer::new(MAX_JSON_BODY_BYTES));

    Router::new()
        .merge(uploads)
        .merge(rest)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::SERVICE_UNAVAILABLE,
            timeout,
        ))
        // Our own span carries the request id and, once authentication has
        // run, the tenant. `TraceLayer` sits under it for the connection-level
        // events it reports; neither records a header or a body.
        .layer(axum::middleware::from_fn(security_headers::apply))
        .layer(axum::middleware::from_fn(observe::track))
        .layer(TraceLayer::new_for_http())
}
