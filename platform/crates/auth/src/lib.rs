//! Authentication: API keys for the public API, sessions for the dashboard.
//!
//! Building a `TenantCtx` is this crate's job and no one else's. Everything
//! downstream trusts the context and never re-derives a tenant from a request.

pub mod api_key;
pub mod cache;
pub mod password;
pub mod repo;
pub mod service;
pub mod session;

pub use api_key::{from_authorization_header, generate, hash_key, Environment, GeneratedApiKey};
pub use cache::ApiKeyCache;
pub use service::{AuthConfig, AuthService, CreateApiKey, IssuedApiKey};
pub use session::{NewSession, Session};

use anthovai_core::{
    AgentScope, ApiKeyId, DomainError, OrgId, Plan, Result, Scope, UserId, WorkspaceId,
};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug)]
pub struct User {
    pub id: UserId,
    pub email: String,
    /// `None` for accounts that only ever sign in by magic link.
    pub password_hash: Option<String>,
    pub name: Option<String>,
    pub email_verified_at: Option<DateTime<Utc>>,
}

impl User {
    /// A live API key can move real data, so the address behind the account has
    /// to be proved first.
    pub fn may_create_live_keys(&self) -> bool {
        self.email_verified_at.is_some()
    }
}

/// What a key lookup returns, before it becomes a `TenantCtx`.
#[derive(Clone, Debug)]
pub struct ApiKeyRecord {
    pub id: ApiKeyId,
    pub org_id: OrgId,
    pub workspace_id: WorkspaceId,
    pub environment: Environment,
    pub scopes: Vec<Scope>,
    pub agents: AgentScope,
    pub plan: Plan,
    pub status: KeyStatus,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStatus {
    Active,
    Revoked,
    Expired,
}

impl KeyStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
        }
    }

    /// `expires_at` is the single source of truth for expiry; the stored status
    /// only records revocation. Deriving it on read means nothing has to sweep
    /// the table to keep the two in agreement.
    pub fn effective(self, expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Self {
        match self {
            Self::Active if expires_at.is_some_and(|at| at <= now) => Self::Expired,
            other => other,
        }
    }
}

impl std::str::FromStr for KeyStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "active" => Ok(Self::Active),
            "revoked" => Ok(Self::Revoked),
            "expired" => Ok(Self::Expired),
            other => Err(DomainError::validation(format!(
                "unknown api key status `{other}`"
            ))),
        }
    }
}

impl ApiKeyRecord {
    /// A key is usable when it is active and has not passed its expiry. The
    /// distinct codes let a customer tell a revoked key from an expired one.
    pub fn check_usable(&self, now: DateTime<Utc>) -> Result<()> {
        match self.status.effective(self.expires_at, now) {
            KeyStatus::Revoked => Err(DomainError::Unauthenticated("revoked_api_key")),
            KeyStatus::Expired => Err(DomainError::Unauthenticated("expired_api_key")),
            KeyStatus::Active => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn record(status: KeyStatus, expires_at: Option<DateTime<Utc>>) -> ApiKeyRecord {
        ApiKeyRecord {
            id: ApiKeyId::new(),
            org_id: OrgId::new(),
            workspace_id: WorkspaceId::new(),
            environment: Environment::Live,
            scopes: vec![Scope::Chat],
            agents: AgentScope::All,
            plan: Plan::Free,
            status,
            expires_at,
        }
    }

    #[test]
    fn an_active_key_without_expiry_is_usable() {
        assert!(record(KeyStatus::Active, None)
            .check_usable(Utc::now())
            .is_ok());
    }

    #[test]
    fn a_revoked_key_says_so() {
        let err = record(KeyStatus::Revoked, None)
            .check_usable(Utc::now())
            .unwrap_err();
        assert_eq!(err.to_string(), "unauthenticated: revoked_api_key");
    }

    #[test]
    fn expiry_is_enforced_even_when_the_row_still_says_active() {
        let now = Utc::now();
        assert!(record(KeyStatus::Active, Some(now - Duration::seconds(1)))
            .check_usable(now)
            .is_err());
        assert!(record(KeyStatus::Active, Some(now + Duration::hours(1)))
            .check_usable(now)
            .is_ok());
    }

    #[test]
    fn revocation_outranks_a_future_expiry() {
        let now = Utc::now();
        let key = record(KeyStatus::Revoked, Some(now + Duration::days(30)));
        assert_eq!(
            key.check_usable(now).unwrap_err().to_string(),
            "unauthenticated: revoked_api_key"
        );
    }

    #[test]
    fn status_names_round_trip() {
        for status in [KeyStatus::Active, KeyStatus::Revoked, KeyStatus::Expired] {
            assert_eq!(status.as_str().parse::<KeyStatus>().unwrap(), status);
        }
    }

    #[test]
    fn unverified_accounts_cannot_mint_live_keys() {
        let mut user = User {
            id: UserId::new(),
            email: "owner@abc.ac.th".into(),
            password_hash: None,
            name: None,
            email_verified_at: None,
        };
        assert!(!user.may_create_live_keys());
        user.email_verified_at = Some(Utc::now());
        assert!(user.may_create_live_keys());
    }
}
