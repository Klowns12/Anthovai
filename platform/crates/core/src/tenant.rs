//! Tenant context, actors, roles and permissions.
//!
//! Every domain function takes `&TenantCtx` as its first argument. The context
//! is built once, during authentication, and is the only source of `org_id`
//! anywhere below the API layer. Never read a tenant id from a request body.

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};
use crate::ids::{AgentId, ApiKeyId, OrgId, RequestId, UserId, WorkspaceId};
use crate::plan::Plan;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Editor,
    Admin,
    Owner,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Editor => "editor",
            Self::Admin => "admin",
            Self::Owner => "owner",
        }
    }
}

impl std::str::FromStr for Role {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "viewer" => Ok(Self::Viewer),
            "editor" => Ok(Self::Editor),
            "admin" => Ok(Self::Admin),
            "owner" => Ok(Self::Owner),
            other => Err(DomainError::validation(format!("unknown role `{other}`"))),
        }
    }
}

/// Capabilities checked in the service layer, never in HTTP handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    OrgManage,
    MemberManage,
    WorkspaceManage,
    AgentRead,
    AgentWrite,
    AgentPublish,
    KnowledgeRead,
    KnowledgeWrite,
    ApiKeyManage,
    UsageRead,
    Chat,
}

impl Permission {
    /// The API-key scope that grants this permission, if any. Permissions with
    /// no scope are dashboard-only by design.
    pub fn scope(self) -> Option<Scope> {
        match self {
            Self::AgentRead => Some(Scope::AgentsRead),
            Self::KnowledgeRead => Some(Scope::KnowledgeRead),
            Self::KnowledgeWrite => Some(Scope::KnowledgeWrite),
            Self::UsageRead => Some(Scope::UsageRead),
            Self::Chat => Some(Scope::Chat),
            Self::OrgManage
            | Self::MemberManage
            | Self::WorkspaceManage
            | Self::AgentWrite
            | Self::AgentPublish
            | Self::ApiKeyManage => None,
        }
    }
}

impl Role {
    pub fn grants(self, perm: Permission) -> bool {
        use Permission::*;
        match perm {
            OrgManage => self == Role::Owner,
            MemberManage | WorkspaceManage | ApiKeyManage => self >= Role::Admin,
            AgentWrite | AgentPublish | KnowledgeWrite => self >= Role::Editor,
            AgentRead | KnowledgeRead | UsageRead | Chat => true,
        }
    }
}

/// Scopes attached to an API key. A key holds the subset its creator chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Chat,
    AgentsRead,
    KnowledgeRead,
    KnowledgeWrite,
    UsageRead,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::AgentsRead => "agents:read",
            Self::KnowledgeRead => "knowledge:read",
            Self::KnowledgeWrite => "knowledge:write",
            Self::UsageRead => "usage:read",
        }
    }
}

impl std::str::FromStr for Scope {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "chat" => Ok(Self::Chat),
            "agents:read" => Ok(Self::AgentsRead),
            "knowledge:read" => Ok(Self::KnowledgeRead),
            "knowledge:write" => Ok(Self::KnowledgeWrite),
            "usage:read" => Ok(Self::UsageRead),
            other => Err(DomainError::validation(format!("unknown scope `{other}`"))),
        }
    }
}

/// Which agents an API key may address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentScope {
    All,
    Only(Vec<AgentId>),
}

impl AgentScope {
    pub fn allows(&self, agent: AgentId) -> bool {
        match self {
            Self::All => true,
            Self::Only(ids) => ids.contains(&agent),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Actor {
    User {
        user_id: UserId,
        role: Role,
    },
    ApiKey {
        key_id: ApiKeyId,
        scopes: Vec<Scope>,
        agents: AgentScope,
    },
    /// Background workers and cleanup jobs. Bypasses permission checks, so it
    /// must never be constructed from anything a request controls.
    System,
}

#[derive(Clone, Debug)]
pub struct TenantCtx {
    pub org_id: OrgId,
    pub workspace_id: Option<WorkspaceId>,
    pub actor: Actor,
    pub plan: Plan,
    pub request_id: RequestId,
}

impl TenantCtx {
    pub fn system(org_id: OrgId, plan: Plan) -> Self {
        Self {
            org_id,
            workspace_id: None,
            actor: Actor::System,
            plan,
            request_id: RequestId::new(),
        }
    }

    pub fn user_id(&self) -> Option<UserId> {
        match &self.actor {
            Actor::User { user_id, .. } => Some(*user_id),
            _ => None,
        }
    }

    pub fn api_key_id(&self) -> Option<ApiKeyId> {
        match &self.actor {
            Actor::ApiKey { key_id, .. } => Some(*key_id),
            _ => None,
        }
    }

    /// Can this actor do this?
    ///
    /// The two actor kinds answer it differently — a user by their role, a key
    /// by its scopes — but callers should not have to know which one they have.
    /// A service that asked only about roles would reject every API key, which
    /// is a trap: the rejection looks like a permission bug rather than a
    /// design one, and only shows up once something is wired to the public API.
    pub fn require(&self, perm: Permission) -> Result<()> {
        match &self.actor {
            Actor::System => Ok(()),
            Actor::User { role, .. } => {
                if role.grants(perm) {
                    Ok(())
                } else {
                    Err(DomainError::Forbidden("role_insufficient"))
                }
            }
            Actor::ApiKey { scopes, .. } => match perm.scope() {
                Some(needed) if scopes.contains(&needed) => Ok(()),
                Some(_) => Err(DomainError::Forbidden("scope_missing")),
                // Everything a key has no scope for: creating agents, managing
                // members, minting more keys. A leaked key must not be able to
                // entrench itself.
                None => Err(DomainError::Forbidden("api_key_cannot_perform_action")),
            },
        }
    }

    /// Scope check for API-key actors. Dashboard users pass their role check instead.
    pub fn require_scope(&self, scope: Scope) -> Result<()> {
        match &self.actor {
            Actor::System => Ok(()),
            Actor::ApiKey { scopes, .. } => {
                if scopes.contains(&scope) {
                    Ok(())
                } else {
                    Err(DomainError::Forbidden("scope_missing"))
                }
            }
            Actor::User { .. } => Ok(()),
        }
    }

    /// An API key may only address the agents it was scoped to. Returns
    /// `NotFound` rather than `Forbidden` so a key cannot probe for agent ids
    /// belonging to another workspace.
    pub fn require_agent(&self, agent: AgentId) -> Result<()> {
        match &self.actor {
            Actor::ApiKey { agents, .. } if !agents.allows(agent) => {
                Err(DomainError::NotFound("agent"))
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_ctx(agents: AgentScope, scopes: Vec<Scope>) -> TenantCtx {
        TenantCtx {
            org_id: OrgId::new(),
            workspace_id: None,
            actor: Actor::ApiKey {
                key_id: ApiKeyId::new(),
                scopes,
                agents,
            },
            plan: Plan::Free,
            request_id: RequestId::new(),
        }
    }

    #[test]
    fn viewer_cannot_publish_an_agent() {
        assert!(!Role::Viewer.grants(Permission::AgentPublish));
        assert!(Role::Editor.grants(Permission::AgentPublish));
    }

    #[test]
    fn only_owner_manages_the_org() {
        assert!(Role::Owner.grants(Permission::OrgManage));
        assert!(!Role::Admin.grants(Permission::OrgManage));
    }

    #[test]
    fn scoped_key_hides_agents_outside_its_scope() {
        let allowed = AgentId::new();
        let other = AgentId::new();
        let ctx = key_ctx(AgentScope::Only(vec![allowed]), vec![Scope::Chat]);
        assert!(ctx.require_agent(allowed).is_ok());
        assert!(matches!(
            ctx.require_agent(other),
            Err(DomainError::NotFound("agent"))
        ));
    }

    #[test]
    fn missing_scope_is_rejected() {
        let ctx = key_ctx(AgentScope::All, vec![Scope::Chat]);
        assert!(ctx.require_scope(Scope::Chat).is_ok());
        assert!(ctx.require_scope(Scope::KnowledgeWrite).is_err());
    }

    #[test]
    fn api_keys_cannot_perform_role_actions() {
        let ctx = key_ctx(AgentScope::All, vec![Scope::Chat]);
        assert!(ctx.require(Permission::ApiKeyManage).is_err());
        assert!(ctx.require(Permission::AgentWrite).is_err());
        assert!(ctx.require(Permission::OrgManage).is_err());
    }

    #[test]
    fn a_key_can_do_what_its_scopes_allow() {
        let ctx = key_ctx(AgentScope::All, vec![Scope::Chat, Scope::AgentsRead]);
        assert!(ctx.require(Permission::Chat).is_ok());
        assert!(ctx.require(Permission::AgentRead).is_ok());
    }

    #[test]
    fn a_key_is_refused_permissions_outside_its_scopes() {
        let ctx = key_ctx(AgentScope::All, vec![Scope::Chat]);
        let err = ctx.require(Permission::AgentRead).unwrap_err();
        assert_eq!(err.code(), "scope_missing");
    }

    #[test]
    fn every_read_permission_maps_to_a_scope() {
        // A read permission with no scope would silently lock every API key
        // out of the endpoint that used it.
        for perm in [
            Permission::AgentRead,
            Permission::KnowledgeRead,
            Permission::UsageRead,
            Permission::Chat,
        ] {
            assert!(perm.scope().is_some(), "{perm:?} needs a scope");
        }
    }

    #[test]
    fn management_permissions_have_no_scope() {
        for perm in [
            Permission::OrgManage,
            Permission::MemberManage,
            Permission::WorkspaceManage,
            Permission::AgentWrite,
            Permission::AgentPublish,
            Permission::ApiKeyManage,
        ] {
            assert!(perm.scope().is_none(), "{perm:?} must stay dashboard-only");
        }
    }
}
