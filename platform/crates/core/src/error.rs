//! The single error type every domain crate returns.
//!
//! The API layer is the only place that maps these onto HTTP status codes and
//! the public error contract described in `docs/spec-v0.1/05-api-specification.md`.

use crate::ids::IdError;

pub type Result<T, E = DomainError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// Resource does not exist, or exists in another tenant. Both cases must
    /// look identical from the outside so we never leak existence across tenants.
    #[error("{0} not found")]
    NotFound(&'static str),

    #[error("forbidden: {0}")]
    Forbidden(&'static str),

    #[error("unauthenticated: {0}")]
    Unauthenticated(&'static str),

    #[error("validation failed: {0}")]
    Validation(String),

    /// A rejected request that carries a code of its own.
    ///
    /// Same status as `Validation`, but the caller can branch on the reason —
    /// `url_not_allowed` is something a dashboard shows differently from a
    /// missing field, and a generic `invalid_request` makes that impossible.
    #[error("{message}")]
    Rejected { code: &'static str, message: String },

    #[error("conflict: {0}")]
    Conflict(&'static str),

    #[error("gone: {0}")]
    Gone(&'static str),

    #[error("payload too large: {0}")]
    PayloadTooLarge(&'static str),

    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },

    #[error("quota exceeded: {0}")]
    QuotaExceeded(&'static str),

    #[error("this feature requires a higher plan: {0}")]
    PlanRequired(&'static str),

    #[error("no model provider is available")]
    ProviderUnavailable,

    #[error("retrieval unavailable")]
    RetrievalUnavailable,

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl DomainError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn rejected(code: &'static str, msg: impl Into<String>) -> Self {
        Self::Rejected {
            code,
            message: msg.into(),
        }
    }

    /// The stable machine-readable code that appears in the public error body
    /// and in our documentation.
    ///
    /// It is defined here rather than at the HTTP boundary so that the code a
    /// test asserts on is the same string a customer will read. A second
    /// definition in the API layer would drift from this one the first time
    /// someone added a variant.
    pub fn code(&self) -> String {
        match self {
            Self::NotFound(what) => format!("{what}_not_found"),
            Self::PlanRequired(feature) => format!("plan_required:{feature}"),

            // These carry their own code already.
            Self::Forbidden(code)
            | Self::Unauthenticated(code)
            | Self::Conflict(code)
            | Self::Gone(code)
            | Self::PayloadTooLarge(code)
            | Self::QuotaExceeded(code) => (*code).to_owned(),

            Self::Validation(_) => "invalid_request".to_owned(),
            Self::Rejected { code, .. } => (*code).to_owned(),
            Self::RateLimited { .. } => "rate_limited".to_owned(),
            Self::ProviderUnavailable => "provider_unavailable".to_owned(),
            Self::RetrievalUnavailable => "retrieval_unavailable".to_owned(),
            Self::Database(_) | Self::Internal(_) => "internal_error".to_owned(),
        }
    }

    /// True when the message is safe to hand to an API caller verbatim.
    pub fn is_public(&self) -> bool {
        !matches!(self, Self::Database(_) | Self::Internal(_))
    }
}

impl From<IdError> for DomainError {
    fn from(err: IdError) -> Self {
        Self::Validation(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_are_never_public() {
        let err = DomainError::Internal(anyhow::anyhow!("connection string leaked"));
        assert!(!err.is_public());
        assert_eq!(err.code(), "internal_error");
    }

    #[test]
    fn domain_errors_are_public() {
        assert!(DomainError::NotFound("agent").is_public());
    }

    #[test]
    fn a_missing_resource_names_what_was_missing() {
        assert_eq!(DomainError::NotFound("agent").code(), "agent_not_found");
        assert_eq!(
            DomainError::NotFound("workspace").code(),
            "workspace_not_found"
        );
    }

    #[test]
    fn errors_that_carry_a_code_keep_it() {
        assert_eq!(
            DomainError::Unauthenticated("revoked_api_key").code(),
            "revoked_api_key"
        );
        assert_eq!(DomainError::Conflict("slug_taken").code(), "slug_taken");
        assert_eq!(
            DomainError::Forbidden("scope_missing").code(),
            "scope_missing"
        );
    }

    #[test]
    fn a_plan_gate_names_the_feature_it_wanted() {
        assert_eq!(
            DomainError::PlanRequired("provider_choice").code(),
            "plan_required:provider_choice"
        );
    }
}
