//! Organizations, workspaces and memberships.
//!
//! The organization is the isolation boundary; a workspace is only a grouping
//! and an API-key scope.

pub mod repo;
pub mod service;

pub use service::{CreatedOrganization, TenantService};

use anthovai_core::{OrgId, Plan, Role, UserId, WorkspaceId};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug)]
pub struct Organization {
    pub id: OrgId,
    pub slug: String,
    pub name: String,
    pub plan: Plan,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub org_id: OrgId,
    pub slug: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Membership {
    pub user_id: UserId,
    pub org_id: OrgId,
    pub role: Role,
    pub accepted_at: Option<DateTime<Utc>>,
}

impl Membership {
    /// An invitation that has not been accepted grants nothing.
    pub fn is_active(&self) -> bool {
        self.accepted_at.is_some()
    }
}

/// Slugs appear in URLs and must not collide with our own route segments.
pub fn validate_slug(slug: &str) -> anthovai_core::Result<()> {
    use anthovai_core::DomainError;

    const RESERVED: &[&str] = &["api", "app", "www", "admin", "internal", "dashboard", "v1"];

    if !(2..=48).contains(&slug.len()) {
        return Err(DomainError::validation("slug must be 2 to 48 characters"));
    }
    if !slug
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(DomainError::validation(
            "slug may contain only lowercase letters, digits and hyphens",
        ));
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(DomainError::validation(
            "slug must not start or end with a hyphen",
        ));
    }
    if RESERVED.contains(&slug) {
        return Err(DomainError::Conflict("slug_reserved"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pending_invitation_is_not_an_active_membership() {
        let mut membership = Membership {
            user_id: UserId::new(),
            org_id: OrgId::new(),
            role: Role::Admin,
            accepted_at: None,
        };
        assert!(!membership.is_active());
        membership.accepted_at = Some(Utc::now());
        assert!(membership.is_active());
    }

    #[test]
    fn accepts_ordinary_slugs() {
        assert!(validate_slug("abc-school").is_ok());
        assert!(validate_slug("kg2").is_ok());
    }

    #[test]
    fn rejects_slugs_that_would_break_urls() {
        assert!(validate_slug("a").is_err());
        assert!(validate_slug("Has Capitals").is_err());
        assert!(validate_slug("-leading").is_err());
        assert!(validate_slug("trailing-").is_err());
        assert!(validate_slug("under_score").is_err());
    }

    #[test]
    fn reserves_our_own_subdomains() {
        assert!(validate_slug("api").is_err());
        assert!(validate_slug("app").is_err());
    }
}
