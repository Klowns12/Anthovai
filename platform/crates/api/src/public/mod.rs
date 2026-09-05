//! The public API, `/v1`.
//!
//! A contract with customers: additions are free, removals and renames need a
//! new version.

pub mod agents;
pub mod chat;
pub mod knowledge;

use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(agents::router())
        .merge(knowledge::router())
        .merge(chat::router())
        .route("/openapi.json", get(openapi))
}

/// The contract, served from the running server.
///
/// Unauthenticated on purpose: it describes how to authenticate, and a
/// customer writing an integration should not need a key to read it. It says
/// nothing about any particular organization.
async fn openapi() -> Response {
    (
        [
            (axum::http::header::CONTENT_TYPE, "application/json"),
            // The document changes only when the server does, and the version
            // is in the URL when it changes incompatibly.
            (axum::http::header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        crate::openapi::document(),
    )
        .into_response()
}
