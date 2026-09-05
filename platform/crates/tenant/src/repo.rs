//! Repositories for organizations, workspaces and memberships.
//!
//! Every statement here binds its tenant from the transaction, never from an
//! argument. A repository function that accepted a tenant id would put the
//! isolation guarantee in the hands of each caller.

use anthovai_core::{DomainError, OrgId, Plan, Result, Role, UserId, WorkspaceId};
use anthovai_db::repo::{id, parsed};
use anthovai_db::{on_unique_violation, SystemDb, TenantDb};
use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::{Membership, Organization, Workspace};

// ---- organizations --------------------------------------------------------

/// Insert the organization. This is the one write with no tenant to scope to —
/// the row being written is what creates the tenant — so it runs as the system
/// role.
pub async fn insert_organization(db: &mut SystemDb<'_>, org: &Organization) -> Result<()> {
    sqlx::query("INSERT INTO organizations (id, slug, name, plan) VALUES ($1, $2, $3, $4)")
        .bind(org.id.to_db())
        .bind(&org.slug)
        .bind(&org.name)
        .bind(org.plan.as_str())
        .execute(db.conn())
        .await
        .map_err(|e| on_unique_violation(e, "slug_taken"))?;
    Ok(())
}

/// Read the organization this transaction is pinned to. There is no "by id"
/// variant on purpose: the id is the transaction's, not the caller's.
pub async fn get_organization(db: &mut TenantDb<'_>) -> Result<Organization> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "SELECT id, slug, name, plan, created_at
         FROM organizations
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(&tenant)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("organization"))?;

    Ok(Organization {
        id: id(&row, "id")?,
        slug: row.try_get("slug").map_err(sql)?,
        name: row.try_get("name").map_err(sql)?,
        plan: parsed(&row, "plan")?,
        created_at: row.try_get("created_at").map_err(sql)?,
    })
}

/// The plan drives quota and feature gates, and is read on the authentication
/// path, so it gets its own narrow query rather than loading the whole row.
pub async fn get_plan(db: &mut SystemDb<'_>, org_id: OrgId) -> Result<Plan> {
    let row = sqlx::query("SELECT plan FROM organizations WHERE id = $1 AND deleted_at IS NULL")
        .bind(org_id.to_db())
        .fetch_optional(db.conn())
        .await?
        .ok_or(DomainError::NotFound("organization"))?;
    parsed(&row, "plan")
}

pub async fn rename_organization(db: &mut TenantDb<'_>, name: &str) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query("UPDATE organizations SET name = $2, updated_at = now() WHERE id = $1")
        .bind(&tenant)
        .bind(name)
        .execute(db.conn())
        .await?;
    Ok(())
}

// ---- workspaces -----------------------------------------------------------

pub async fn insert_workspace_system(db: &mut SystemDb<'_>, workspace: &Workspace) -> Result<()> {
    sqlx::query("INSERT INTO workspaces (id, tenant_id, name, slug) VALUES ($1, $2, $3, $4)")
        .bind(workspace.id.to_db())
        .bind(workspace.org_id.to_db())
        .bind(&workspace.name)
        .bind(&workspace.slug)
        .execute(db.conn())
        .await
        .map_err(|e| on_unique_violation(e, "slug_taken"))?;
    Ok(())
}

pub async fn insert_workspace(db: &mut TenantDb<'_>, name: &str, slug: &str) -> Result<Workspace> {
    let workspace = Workspace {
        id: WorkspaceId::new(),
        org_id: db.org_id(),
        name: name.to_owned(),
        slug: slug.to_owned(),
    };
    let tenant = db.tenant_key();

    sqlx::query("INSERT INTO workspaces (id, tenant_id, name, slug) VALUES ($1, $2, $3, $4)")
        .bind(workspace.id.to_db())
        .bind(&tenant)
        .bind(&workspace.name)
        .bind(&workspace.slug)
        .execute(db.conn())
        .await
        .map_err(|e| on_unique_violation(e, "slug_taken"))?;

    Ok(workspace)
}

pub async fn list_workspaces(db: &mut TenantDb<'_>) -> Result<Vec<Workspace>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT id, tenant_id, name, slug
         FROM workspaces
         WHERE tenant_id = $1 AND deleted_at IS NULL
         ORDER BY created_at",
    )
    .bind(&tenant)
    .fetch_all(db.conn())
    .await?;

    rows.iter()
        .map(|row| {
            Ok(Workspace {
                id: id(row, "id")?,
                org_id: id(row, "tenant_id")?,
                name: row.try_get("name").map_err(sql)?,
                slug: row.try_get("slug").map_err(sql)?,
            })
        })
        .collect()
}

/// A workspace from another tenant is reported as missing, not as forbidden:
/// telling a caller that an id exists elsewhere is itself a leak.
pub async fn get_workspace(db: &mut TenantDb<'_>, workspace_id: WorkspaceId) -> Result<Workspace> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "SELECT id, tenant_id, name, slug
         FROM workspaces
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(workspace_id.to_db())
    .bind(&tenant)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("workspace"))?;

    Ok(Workspace {
        id: id(&row, "id")?,
        org_id: id(&row, "tenant_id")?,
        name: row.try_get("name").map_err(sql)?,
        slug: row.try_get("slug").map_err(sql)?,
    })
}

pub async fn soft_delete_workspace(db: &mut TenantDb<'_>, workspace_id: WorkspaceId) -> Result<()> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "UPDATE workspaces SET deleted_at = now()
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(workspace_id.to_db())
    .bind(&tenant)
    .execute(db.conn())
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(DomainError::NotFound("workspace"));
    }
    Ok(())
}

// ---- memberships ----------------------------------------------------------

/// Memberships have no tenant column of their own to police — they are the
/// mapping that decides which tenants a user may enter — so they are written
/// through the system role and always read by user id.
pub async fn insert_membership(
    db: &mut SystemDb<'_>,
    user_id: UserId,
    org_id: OrgId,
    role: Role,
    accepted: bool,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO memberships (user_id, tenant_id, role, accepted_at)
         VALUES ($1, $2, $3, CASE WHEN $4 THEN now() ELSE NULL END)",
    )
    .bind(user_id.to_db())
    .bind(org_id.to_db())
    .bind(role.as_str())
    .bind(accepted)
    .execute(db.conn())
    .await
    .map_err(|e| on_unique_violation(e, "already_a_member"))?;
    Ok(())
}

/// The organizations a user may act in, with the role they hold in each.
pub async fn list_memberships(db: &mut SystemDb<'_>, user_id: UserId) -> Result<Vec<Membership>> {
    let rows = sqlx::query(
        "SELECT m.user_id, m.tenant_id, m.role, m.accepted_at
         FROM memberships m
         JOIN organizations o ON o.id = m.tenant_id
         WHERE m.user_id = $1 AND o.deleted_at IS NULL
         ORDER BY m.created_at",
    )
    .bind(user_id.to_db())
    .fetch_all(db.conn())
    .await?;

    rows.iter()
        .map(|row| {
            Ok(Membership {
                user_id: id(row, "user_id")?,
                org_id: id(row, "tenant_id")?,
                role: parsed(row, "role")?,
                accepted_at: row
                    .try_get::<Option<DateTime<Utc>>, _>("accepted_at")
                    .map_err(sql)?,
            })
        })
        .collect()
}

/// The membership that authorises this user in this organization, if any.
/// Returns `None` rather than an error so the caller decides whether a missing
/// membership is a 401 or a 404.
pub async fn find_membership(
    db: &mut SystemDb<'_>,
    user_id: UserId,
    org_id: OrgId,
) -> Result<Option<Membership>> {
    let row = sqlx::query(
        "SELECT m.user_id, m.tenant_id, m.role, m.accepted_at
         FROM memberships m
         JOIN organizations o ON o.id = m.tenant_id
         WHERE m.user_id = $1 AND m.tenant_id = $2 AND o.deleted_at IS NULL",
    )
    .bind(user_id.to_db())
    .bind(org_id.to_db())
    .fetch_optional(db.conn())
    .await?;

    match row {
        None => Ok(None),
        Some(row) => Ok(Some(Membership {
            user_id: id(&row, "user_id")?,
            org_id: id(&row, "tenant_id")?,
            role: parsed(&row, "role")?,
            accepted_at: row
                .try_get::<Option<DateTime<Utc>>, _>("accepted_at")
                .map_err(sql)?,
        })),
    }
}

fn sql(err: sqlx::Error) -> DomainError {
    DomainError::Database(err)
}
