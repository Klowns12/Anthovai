//! Headers that tell a browser what this response is allowed to do.
//!
//! They matter here because the dashboard is authenticated by a cookie, and a
//! cookie is sent by the browser whatever caused the request. Framing, MIME
//! sniffing and referrer leakage are all ways to turn "the user is signed in"
//! into "an attacker's page can act as them".
//!
//! The public API is JSON read by a customer's server, not by a browser, but it
//! gets the same treatment: a header that costs nothing on a response nobody
//! renders is cheaper than remembering which routes are which.

use axum::http::{header, HeaderName, HeaderValue};
use axum::response::Response;

/// Applied to every response.
///
/// `Strict-Transport-Security` is deliberately not here. It is set by whatever
/// terminates TLS, which is the only component that knows the deployment is
/// actually reachable over HTTPS — sending it from a server running on plain
/// HTTP in development would lock a developer's browser out of `localhost` for
/// six months.
const HEADERS: &[(HeaderName, &str)] = &[
    // The API returns JSON and nothing else. Without this, a browser asked to
    // navigate to a response is free to decide the bytes look like HTML and
    // execute them.
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    // Nothing here is meant to be embedded. `frame-ancestors` in the policy
    // below is the modern form; this is for the browsers that predate it.
    (header::X_FRAME_OPTIONS, "DENY"),
    // A dashboard URL contains the organization id. Sending it to whatever a
    // customer clicks through to would hand that id to a third party.
    (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
    // An API response is JSON. It loads nothing, runs nothing, and may not be
    // framed — so the policy that describes it is almost entirely denials.
    // The dashboard frontend is a separate deployment and sets its own, which
    // has to be far more permissive.
    (
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
    ),
];

/// Add them, without overwriting anything a handler set deliberately.
pub async fn apply(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    for (name, value) in HEADERS {
        // `entry`, not `insert`: a route that has its own opinion — a future
        // one serving an embeddable widget, say — keeps it.
        if !headers.contains_key(name) {
            headers.insert(name, HeaderValue::from_static(value));
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::*;

    async fn headers_of(app: Router) -> axum::http::HeaderMap {
        app.oneshot(Request::builder().uri("/x").body(Body::empty()).unwrap())
            .await
            .unwrap()
            .headers()
            .clone()
    }

    #[tokio::test]
    async fn every_response_carries_them() {
        let app = Router::new()
            .route("/x", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(apply));

        let headers = headers_of(app).await;

        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
        assert_eq!(
            headers[header::REFERRER_POLICY],
            "strict-origin-when-cross-origin"
        );
        assert!(headers[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'"));
    }

    #[tokio::test]
    async fn an_error_response_carries_them_too() {
        // The 404 for an unknown path is a response a browser can be pointed
        // at just as easily as any other.
        let app = Router::new()
            .route("/other", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(apply));

        let response = app
            .oneshot(Request::builder().uri("/x").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.headers()[header::X_FRAME_OPTIONS], "DENY");
    }

    #[tokio::test]
    async fn a_handler_that_sets_its_own_keeps_it() {
        let app = Router::new()
            .route(
                "/x",
                get(|| async { ([(header::X_FRAME_OPTIONS, "SAMEORIGIN")], "ok").into_response() }),
            )
            .layer(axum::middleware::from_fn(apply));

        let headers = headers_of(app).await;
        assert_eq!(headers[header::X_FRAME_OPTIONS], "SAMEORIGIN");
    }

    #[test]
    fn hsts_is_not_set_here() {
        // Sending it from a development server on plain HTTP would lock a
        // developer's browser out of localhost for six months. It belongs to
        // whatever terminates TLS.
        assert!(
            !HEADERS
                .iter()
                .any(|(name, _)| name == header::STRICT_TRANSPORT_SECURITY),
            "HSTS must be set by the TLS terminator, not by the application"
        );
    }
}
