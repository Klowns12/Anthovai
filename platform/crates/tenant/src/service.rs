//! Tenant services: the operations a handler calls.

use anthovai_core::{
    DomainError, OrgId, Permission, Plan, Result, Role, TenantCtx, UserId, WorkspaceId,
};
use anthovai_db::Db;
use chrono::Utc;

use crate::repo;
use crate::{validate_slug, Membership, Organization, Workspace};

#[derive(Clone, Debug)]
pub struct TenantService {
    db: Db,
}

/// What signing up produces: the organization, its first workspace, and the
/// owner membership that lets the user back in.
#[derive(Clone, Debug)]
pub struct CreatedOrganization {
    pub organization: Organization,
    pub default_workspace: Workspace,
}

impl TenantService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Create an organization, its default workspace and the owner membership
    /// in one transaction. A half-created organization would leave a user with
    /// a tenant they cannot enter, so this is all-or-nothing by construction.
    pub async fn create_organization(
        &self,
        owner: UserId,
        name: &str,
        slug: &str,
    ) -> Result<CreatedOrganization> {
        validate_slug(slug)?;
        if name.trim().is_empty() {
            return Err(DomainError::validation("organization name is required"));
        }

        let organization = Organization {
            id: OrgId::new(),
            slug: slug.to_owned(),
            name: name.trim().to_owned(),
            plan: Plan::Free,
            created_at: Utc::now(),
        };
        let default_workspace = Workspace {
            id: WorkspaceId::new(),
            org_id: organization.id,
            name: "Default".to_owned(),
            slug: "default".to_owned(),
        };

        let mut db = self.db.system().await?;
        repo::insert_organization(&mut db, &organization).await?;
        repo::insert_workspace_system(&mut db, &default_workspace).await?;
        repo::insert_membership(&mut db, owner, organization.id, Role::Owner, true).await?;
        db.commit().await?;

        Ok(CreatedOrganization {
            organization,
            default_workspace,
        })
    }

    pub async fn get_organization(&self, ctx: &TenantCtx) -> Result<Organization> {
        let mut db = self.db.tenant(ctx).await?;
        let organization = repo::get_organization(&mut db).await?;
        db.commit().await?;
        Ok(organization)
    }

    pub async fn rename_organization(&self, ctx: &TenantCtx, name: &str) -> Result<()> {
        ctx.require(Permission::OrgManage)?;
        if name.trim().is_empty() {
            return Err(DomainError::validation("organization name is required"));
        }
        let mut db = self.db.tenant(ctx).await?;
        repo::rename_organization(&mut db, name.trim()).await?;
        db.commit().await
    }

    /// The organizations this user can act in. Read through the system role
    /// because at this point no tenant has been chosen yet — choosing one is
    /// what this list is for.
    pub async fn list_memberships(&self, user_id: UserId) -> Result<Vec<Membership>> {
        let mut db = self.db.system().await?;
        let memberships = repo::list_memberships(&mut db, user_id).await?;
        db.commit().await?;
        Ok(memberships)
    }

    /// Resolve the role a user holds in an organization. An unaccepted
    /// invitation grants nothing, and a user with no membership must not learn
    /// whether the organization exists.
    pub async fn authorize(&self, user_id: UserId, org_id: OrgId) -> Result<(Role, Plan)> {
        let mut db = self.db.system().await?;
        let membership = repo::find_membership(&mut db, user_id, org_id).await?;
        let membership = match membership {
            Some(m) if m.is_active() => m,
            _ => return Err(DomainError::NotFound("organization")),
        };
        let plan = repo::get_plan(&mut db, org_id).await?;
        db.commit().await?;
        Ok((membership.role, plan))
    }

    pub async fn create_workspace(
        &self,
        ctx: &TenantCtx,
        name: &str,
        slug: &str,
    ) -> Result<Workspace> {
        ctx.require(Permission::WorkspaceManage)?;
        validate_slug(slug)?;
        if name.trim().is_empty() {
            return Err(DomainError::validation("workspace name is required"));
        }

        let mut db = self.db.tenant(ctx).await?;
        let workspace = repo::insert_workspace(&mut db, name.trim(), slug).await?;
        db.commit().await?;
        Ok(workspace)
    }

    pub async fn list_workspaces(&self, ctx: &TenantCtx) -> Result<Vec<Workspace>> {
        let mut db = self.db.tenant(ctx).await?;
        let workspaces = repo::list_workspaces(&mut db).await?;
        db.commit().await?;
        Ok(workspaces)
    }

    pub async fn get_workspace(&self, ctx: &TenantCtx, id: WorkspaceId) -> Result<Workspace> {
        let mut db = self.db.tenant(ctx).await?;
        let workspace = repo::get_workspace(&mut db, id).await?;
        db.commit().await?;
        Ok(workspace)
    }

    pub async fn delete_workspace(&self, ctx: &TenantCtx, id: WorkspaceId) -> Result<()> {
        ctx.require(Permission::WorkspaceManage)?;
        let mut db = self.db.tenant(ctx).await?;
        repo::soft_delete_workspace(&mut db, id).await?;
        db.commit().await
    }
}
