//! Liveness, readiness, and metrics.
//!
//! The two health endpoints answer different questions and must not be
//! conflated. `/internal/health` asks "is this process alive?" and is what an
//! orchestrator restarts on — so it touches nothing that could be slow or
//! briefly unavailable, because restarting the API because the object store
//! hiccupped would turn a small outage into a large one.
//!
//! `/internal/ready` asks "can this process do its job right now?" and is what
//! a load balancer takes out of rotation on. That one really does check the
//! database, storage and the model providers.

use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
    pub uptime_secs: i64,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(Health {
        status: "ok",
        version: state.version,
        uptime_secs: (state.clock.now() - state.started_at).num_seconds(),
    })
}

// ---- readiness ------------------------------------------------------------

#[derive(Serialize)]
struct Readiness {
    status: &'static str,
    version: &'static str,
    uptime_secs: i64,
    checks: Checks,
}

#[derive(Serialize)]
struct Checks {
    database: Check,
    storage: Check,
    providers: Check,
    queue: Check,
}

#[derive(Serialize)]
struct Check {
    status: &'static str,
    took_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl Check {
    fn ok(started: Instant) -> Self {
        Self {
            status: "ok",
            took_ms: started.elapsed().as_millis() as u64,
            detail: None,
        }
    }

    fn with(status: &'static str, started: Instant, detail: impl Into<String>) -> Self {
        Self {
            status,
            took_ms: started.elapsed().as_millis() as u64,
            detail: Some(detail.into()),
        }
    }

    fn is_failing(&self) -> bool {
        self.status == "failing"
    }
}

async fn ready(State(state): State<AppState>) -> Response {
    let checks = Checks {
        database: check_database(&state).await,
        storage: check_storage(&state).await,
        providers: check_providers(&state),
        queue: check_queue(&state).await,
    };

    // Only the checks that make this process *unable to serve* take it out of
    // rotation. A backed-up queue means uploads are slow to index; questions
    // are still answered, and pulling the API would not help the queue.
    let failing = checks.database.is_failing()
        || checks.storage.is_failing()
        || checks.providers.is_failing();

    let (code, status) = if failing {
        (StatusCode::SERVICE_UNAVAILABLE, "failing")
    } else {
        (StatusCode::OK, "ok")
    };

    (
        code,
        Json(Readiness {
            status,
            version: state.version,
            uptime_secs: (state.clock.now() - state.started_at).num_seconds(),
            checks,
        }),
    )
        .into_response()
}

async fn check_database(state: &AppState) -> Check {
    let started = Instant::now();
    match state.diagnostics.db.ping().await {
        Ok(()) => Check::ok(started),
        // The error is not repeated verbatim: a connection error from `sqlx`
        // can carry the connection string, and this endpoint is read by more
        // people than the database password should be.
        Err(e) => {
            tracing::error!(error = %e, "the database is not answering");
            Check::with("failing", started, "the database is not answering")
        }
    }
}

/// Reading a key that does not exist.
///
/// A `false` answer proves the bucket is reachable and our credentials work,
/// which is the whole question. Writing a probe object instead would leave
/// litter in a customer-facing bucket.
async fn check_storage(state: &AppState) -> Check {
    let started = Instant::now();
    match state
        .diagnostics
        .storage
        .exists("_health/probe-does-not-exist")
        .await
    {
        Ok(_) => Check::ok(started),
        Err(e) => {
            tracing::error!(error = %e, "object storage is not answering");
            Check::with("failing", started, "object storage is not answering")
        }
    }
}

/// Whether any model can be reached right now.
///
/// A single model with an open circuit is normal and self-healing. *No* usable
/// model means every question would come back as `provider_unavailable`, and
/// this instance should stop being sent traffic.
fn check_providers(state: &AppState) -> Check {
    let started = Instant::now();
    let usable = state.diagnostics.router.usable_models();

    if usable.is_empty() {
        return Check::with("failing", started, "no model provider is usable");
    }

    let total = state
        .diagnostics
        .router
        .registry()
        .all()
        .iter()
        .filter(|spec| spec.enabled)
        .count();

    if usable.len() < total {
        return Check::with(
            "degraded",
            started,
            format!("{} of {total} models usable", usable.len()),
        );
    }

    Check::ok(started)
}

/// How much work is waiting.
///
/// Reported rather than judged: what counts as a backlog depends on how many
/// workers are running, which this process does not know. Dead jobs are
/// different — they are finished failing, and nobody is coming for them.
async fn check_queue(state: &AppState) -> Check {
    let started = Instant::now();

    match state.diagnostics.jobs.depth().await {
        Ok(depth) => {
            metrics::gauge!("jobs_pending").set(depth.pending as f64);
            metrics::gauge!("jobs_running").set(depth.running as f64);
            metrics::gauge!("jobs_dead").set(depth.dead as f64);

            let status = if depth.dead > 0 { "degraded" } else { "ok" };
            Check::with(
                status,
                started,
                format!(
                    "{} pending, {} running, {} dead",
                    depth.pending, depth.running, depth.dead
                ),
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "the job queue could not be read");
            Check::with("degraded", started, "the job queue could not be read")
        }
    }
}

// ---- metrics --------------------------------------------------------------

/// The Prometheus scrape endpoint.
///
/// Empty rather than an error when no recorder is installed, which is the case
/// in tests: an endpoint that 500s when nothing has been measured would make
/// every test harness look broken.
async fn metrics(State(state): State<AppState>) -> Response {
    let body = match &state.diagnostics.metrics {
        Some(handle) => handle.render(),
        None => String::new(),
    };

    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
        .into_response()
}
