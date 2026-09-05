//! The dashboard API, `/dashboard/v1`.
//!
//! Used only by our own frontend, so it can change freely — unlike `/v1`,
//! which is a promise to customers.

pub mod agents;
pub mod api_keys;
pub mod auth;
pub mod knowledge;
pub mod organizations;
pub mod playground;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(auth::router())
        .merge(organizations::router())
        .merge(api_keys::router())
        .merge(agents::router())
        .merge(knowledge::router())
        .merge(playground::router())
}
