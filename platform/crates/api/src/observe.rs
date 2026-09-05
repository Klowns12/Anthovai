//! What we can see from outside a running server.
//!
//! Two things happen per request here, and they have the same shape for a
//! reason: both need the *route pattern* rather than the path. `/v1/agents/{id}`
//! is one line in a dashboard; `/v1/agents/agt_01M1...` is a million, and each
//! one carries a customer's identifier into a metrics store that was never
//! meant to hold them.
//!
//! What is deliberately absent is as important as what is here. No header is
//! recorded — `Authorization` and `Cookie` would be the two most useful things
//! in the world to an attacker reading our logs — and no request or response
//! body, because the body of a `/v1/chat` call is a customer's question, often
//! about their own health, money or staff.

use std::time::Instant;

use axum::extract::MatchedPath;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use tracing::{field, info_span, Instrument};

use crate::request_id;

/// One span and one timing per request.
pub async fn track(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let route = route_of(&request);
    let request_id = request_id::resolve(
        request
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok()),
    );

    // `tenant_id` and `agent_id` are not known until an extractor has run, so
    // they are declared empty and filled in by `record_tenant` / `record_agent`
    // once authentication resolves. A span field added later is still on the
    // same span, so every line inside the request carries them.
    let span = info_span!(
        "http_request",
        %method,
        route = %route,
        %request_id,
        status = field::Empty,
        tenant_id = field::Empty,
        agent_id = field::Empty,
    );

    // The whole body runs inside the span, not just the call it wraps. Code
    // after an `.instrument(…).await` has left the span again — so the line
    // reporting that the request finished would be emitted outside it, without
    // the fields and, depending on the runtime, without the subscriber that
    // was meant to receive it.
    async move {
        let started = Instant::now();
        let response = next.run(request).await;
        let elapsed = started.elapsed();
        let status = response.status().as_u16();

        tracing::Span::current().record("status", status);

        // One line per request, emitted explicitly rather than left to the
        // subscriber's span-close events: whether those are enabled is a
        // configuration detail, and an access log that silently depends on one
        // is an access log that will one day not be there.
        //
        // Fields only. Nothing derived from a header or a body appears here.
        tracing::info!(latency_ms = elapsed.as_millis() as u64, "request completed");

        // `route` is the matched pattern, never the path: a label built from
        // the path would give the metrics store one series per document id.
        metrics::counter!(
            "http_requests_total",
            "route" => route.clone(),
            "method" => method.to_string(),
            "status" => status.to_string(),
        )
        .increment(1);

        metrics::histogram!(
            "http_request_duration_seconds",
            "route" => route,
            "method" => method.to_string(),
        )
        .record(elapsed.as_secs_f64());

        response
    }
    .instrument(span)
    .await
}

/// The matched route pattern, or a constant for anything unmatched.
///
/// An unmatched request is a 404, and its path is whatever the caller sent —
/// including a scanner walking `/wp-admin`, `/.env` and a thousand others. All
/// of them collapse to one label rather than one series each.
fn route_of(request: &Request) -> String {
    request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".to_owned())
}

/// Attach the tenant to the current request's span.
///
/// Called from the extractors, which are the only place a tenant is resolved.
/// The organization id is not a secret — it is in every dashboard URL — and
/// without it a log line cannot answer "which customer saw this?"
pub fn record_tenant(org_id: anthovai_core::OrgId) {
    tracing::Span::current().record("tenant_id", tracing::field::display(org_id));
}

pub fn record_agent(agent_id: anthovai_core::AgentId) {
    tracing::Span::current().record("agent_id", tracing::field::display(agent_id));
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{header, Request as HttpRequest, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::*;

    /// The label this request would be counted under, observed from where the
    /// real middleware sits — around a nested router, exactly as `app()`
    /// mounts it. Where the layer is attached decides whether the matched path
    /// is there yet, so asserting on `route_of` in isolation would prove
    /// nothing about the running server.
    async fn observed_route(uri: &str) -> String {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let probe = std::sync::Arc::clone(&seen);

        let app = Router::new()
            .nest(
                "/v1",
                Router::new().route("/agents/{agent_id}", get(|| async { "ok" })),
            )
            .layer(axum::middleware::from_fn(
                move |request: Request, next: Next| {
                    let probe = std::sync::Arc::clone(&probe);
                    async move {
                        *probe.lock().unwrap() = route_of(&request);
                        next.run(request).await
                    }
                },
            ));

        app.oneshot(HttpRequest::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        let route = seen.lock().unwrap().clone();
        route
    }

    #[tokio::test]
    async fn a_path_parameter_never_becomes_a_metric_label() {
        // The whole reason the matched path is used: one series for the route,
        // not one per agent a customer happens to own.
        assert_eq!(
            observed_route("/v1/agents/agt_01M1QYJYM9BZHZ93B79VDK43F8").await,
            "/v1/agents/{agent_id}"
        );
    }

    #[tokio::test]
    async fn an_unmatched_path_collapses_to_one_label() {
        // A scanner walking a thousand paths must not create a thousand series.
        assert_eq!(
            observed_route("/wp-admin/setup-config.php").await,
            "unmatched"
        );
    }

    /// Everything the tracing layer emitted while serving one request.
    ///
    /// Collected through a real subscriber rather than by reading the source,
    /// because what matters is what actually reaches a log aggregator — where
    /// it is retained for months and read by more people than the customer
    /// ever agreed to.
    ///
    /// Every test that runs `track` goes through here, and it has to. `tracing`
    /// caches whether a callsite is of interest the first time it is reached,
    /// process-wide: one test running the middleware with no subscriber
    /// installed would mark the request span as never interesting, and every
    /// later test would see it disabled — including this one, whose whole job
    /// is to read what the span emitted.
    fn serve(request: HttpRequest<Body>) -> (StatusCode, String) {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Collector(Arc<Mutex<Vec<u8>>>);

        impl Write for Collector {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Collector {
            type Writer = Self;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let collector = Collector(Arc::clone(&buffer));

        let subscriber = tracing_subscriber::fmt()
            .with_writer(collector)
            .with_max_level(tracing::Level::INFO)
            // Without this the fields arrive wrapped in colour escapes, and a
            // test looking for `route=/v1/chat` never finds it.
            .with_ansi(false)
            .finish();

        let app = Router::new()
            .route(
                "/v1/chat",
                axum::routing::post(|body: String| async move {
                    // A handler that logs what it was given is the mistake this
                    // guards against; the span around it must not do the same.
                    tracing::info!(bytes = body.len(), "answered");
                    "ok"
                }),
            )
            .layer(axum::middleware::from_fn(track));

        // One thread from start to finish. `with_default` sets the subscriber
        // for the current thread only, so the runtime is built inside the
        // closure — a request polled on a runtime worker thread would emit
        // into whatever subscriber that thread had, which is nothing.
        let status = tracing::subscriber::with_default(subscriber, || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(app.oneshot(request))
        })
        .unwrap()
        .status();

        let logged = buffer.lock().unwrap().clone();
        (status, String::from_utf8_lossy(&logged).into_owned())
    }

    #[test]
    fn neither_the_key_nor_the_question_reaches_the_logs() {
        let (_, logs) = serve(
            HttpRequest::builder()
                .method("POST")
                .uri("/v1/chat")
                .header(header::AUTHORIZATION, "Bearer av_live_SECRETKEYVALUE")
                .header(header::COOKIE, "anthovai_session=SECRETSESSIONVALUE")
                .body(Body::from(
                    r#"{"message":"my daughter's medical leave request"}"#,
                ))
                .unwrap(),
        );

        for secret in [
            "SECRETKEYVALUE",
            "SECRETSESSIONVALUE",
            "medical leave",
            "daughter",
        ] {
            assert!(
                !logs.contains(secret),
                "`{secret}` reached the logs:\n{logs}"
            );
        }

        // The line was emitted — otherwise this test would pass by logging
        // nothing at all, which proves nothing.
        // Something was logged, under this request's span — otherwise the test
        // would pass by capturing nothing at all, which proves nothing.
        //
        // Only the span context is asserted, not the trailing access line:
        // One line per request, carrying the span's fields — otherwise this
        // test would pass by capturing nothing at all, which proves nothing.
        assert!(
            logs.contains("request completed") && logs.contains("route=/v1/chat"),
            "the access line is missing:\n{logs}"
        );
    }

    #[test]
    fn the_request_still_reaches_its_handler() {
        let (status, _) = serve(
            HttpRequest::builder()
                .method("POST")
                .uri("/v1/chat")
                .header(header::AUTHORIZATION, "Bearer av_live_secret")
                .body(Body::from("{}"))
                .unwrap(),
        );

        assert_eq!(status, StatusCode::OK);
    }
}
