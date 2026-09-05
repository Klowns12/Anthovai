//! The one place `DomainError` becomes an HTTP response.
//!
//! The body shape is the public contract from
//! `docs/spec-v0.1/05-api-specification.md` §2.2. Internal errors never leak
//! their message: the caller gets the request id, and the detail goes to logs.

use anthovai_core::DomainError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// The broad family: `authentication_error`, `invalid_request_error`,
    /// `permission_error`, `not_found_error`, `rate_limit_error`,
    /// `service_unavailable`, `api_error`.
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub kind: String,
    /// The stable code to branch on. `agent_not_published`, `scope_missing`,
    /// `url_not_allowed` and so on.
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    pub request_id: String,
    pub doc_url: String,
}

/// An error on its way out.
///
/// The fields live behind a box because this type is the `Err` of nearly every
/// handler's `Result`: unboxed, its size would be paid on every success path in
/// the HTTP layer, for a value that only exists when something has gone wrong.
#[derive(Debug)]
pub struct ApiError(Box<Inner>);

#[derive(Debug)]
struct Inner {
    status: StatusCode,
    kind: &'static str,
    code: String,
    message: String,
    param: Option<String>,
    request_id: String,
    retry_after_secs: Option<u64>,
}

impl ApiError {
    pub fn from_domain(err: DomainError, request_id: String) -> Self {
        let (status, kind) = classify(&err);
        let retry_after_secs = match &err {
            DomainError::RateLimited { retry_after_secs } => Some(*retry_after_secs),
            DomainError::ProviderUnavailable => Some(10),
            _ => None,
        };

        let (code, message) = if err.is_public() {
            (err.code(), err.to_string())
        } else {
            tracing::error!(error = %err, request_id = %request_id, "unhandled internal error");
            (
                "internal_error".to_owned(),
                "An unexpected error occurred. Quote the request id when contacting support."
                    .to_owned(),
            )
        };

        Self(Box::new(Inner {
            status,
            kind,
            code,
            message,
            param: None,
            request_id,
            retry_after_secs,
        }))
    }

    /// Name the field that was wrong, when there is one.
    pub fn with_param(mut self, param: impl Into<String>) -> Self {
        self.0.param = Some(param.into());
        self
    }

    pub fn status(&self) -> StatusCode {
        self.0.status
    }

    pub fn code(&self) -> &str {
        &self.0.code
    }

    pub fn message(&self) -> &str {
        &self.0.message
    }

    pub fn request_id(&self) -> &str {
        &self.0.request_id
    }

    pub fn retry_after_secs(&self) -> Option<u64> {
        self.0.retry_after_secs
    }
}

fn classify(err: &DomainError) -> (StatusCode, &'static str) {
    match err {
        DomainError::Validation(_) | DomainError::Rejected { .. } => {
            (StatusCode::BAD_REQUEST, "invalid_request_error")
        }
        DomainError::Unauthenticated(_) => (StatusCode::UNAUTHORIZED, "authentication_error"),
        DomainError::Forbidden(_) | DomainError::PlanRequired(_) => {
            (StatusCode::FORBIDDEN, "permission_error")
        }
        DomainError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found_error"),
        DomainError::Conflict(_) => (StatusCode::CONFLICT, "conflict_error"),
        DomainError::Gone(_) => (StatusCode::GONE, "gone_error"),
        DomainError::PayloadTooLarge(_) => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
        DomainError::RateLimited { .. } | DomainError::QuotaExceeded(_) => {
            (StatusCode::TOO_MANY_REQUESTS, "rate_limit_error")
        }
        DomainError::ProviderUnavailable | DomainError::RetrievalUnavailable => {
            (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable")
        }
        DomainError::Database(_) | DomainError::Internal(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "api_error")
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let inner = *self.0;

        let mut response = (
            inner.status,
            Json(ErrorBody {
                error: ErrorDetail {
                    kind: inner.kind.to_owned(),
                    doc_url: format!("https://docs.anthovai.com/errors#{}", inner.code),
                    code: inner.code,
                    message: inner.message,
                    param: inner.param,
                    request_id: inner.request_id.clone(),
                },
            }),
        )
            .into_response();

        if let Some(secs) = inner.retry_after_secs {
            if let Ok(value) = secs.to_string().parse() {
                response.headers_mut().insert("retry-after", value);
            }
        }
        if let Ok(value) = inner.request_id.parse() {
            response.headers_mut().insert("x-request-id", value);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_error(err: DomainError) -> ApiError {
        ApiError::from_domain(err, "req_test".to_owned())
    }

    #[test]
    fn a_missing_agent_is_a_404_with_a_specific_code() {
        let err = api_error(DomainError::NotFound("agent"));
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.code(), "agent_not_found");
    }

    #[test]
    fn internal_errors_never_leak_their_message() {
        let err = api_error(DomainError::Internal(anyhow::anyhow!(
            "postgres://user:password@host/db is unreachable"
        )));
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code(), "internal_error");
        assert!(!err.message().contains("password"));
        assert!(err.message().contains("request id"));
    }

    #[test]
    fn validation_messages_are_passed_through() {
        let err = api_error(DomainError::validation("message must not be empty"));
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("must not be empty"));
    }

    #[test]
    fn rate_limits_carry_a_retry_after() {
        let err = api_error(DomainError::RateLimited {
            retry_after_secs: 30,
        });
        assert_eq!(err.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(err.retry_after_secs(), Some(30));
    }

    #[test]
    fn provider_outages_tell_the_caller_to_come_back() {
        let err = api_error(DomainError::ProviderUnavailable);
        assert_eq!(err.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(err.retry_after_secs(), Some(10));
    }

    #[test]
    fn plan_gates_are_forbidden_not_payment_required() {
        let err = api_error(DomainError::PlanRequired("provider_choice"));
        assert_eq!(err.status(), StatusCode::FORBIDDEN);
        assert!(err.code().contains("provider_choice"));
    }

    #[test]
    fn revoked_and_expired_keys_are_distinguishable() {
        assert_eq!(
            api_error(DomainError::Unauthenticated("revoked_api_key")).code(),
            "revoked_api_key"
        );
        assert_eq!(
            api_error(DomainError::Unauthenticated("expired_api_key")).code(),
            "expired_api_key"
        );
    }

    #[test]
    fn the_error_stays_small_enough_to_return_by_value() {
        // Every handler's Result carries this type. Boxing keeps that cheap.
        assert!(
            std::mem::size_of::<ApiError>() <= 16,
            "ApiError grew to {} bytes",
            std::mem::size_of::<ApiError>()
        );
    }
}
