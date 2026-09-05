//! Repositories for agents, their versions, and the knowledge bases they read.

use anthovai_core::{
    AgentId, AgentVersionId, DomainError, KnowledgeBaseId, Result, UserId, WorkspaceId,
};
use anthovai_db::repo::{id, opt_id, parsed};
use anthovai_db::{on_missing_reference, TenantDb};
use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::{AgentConfig, AgentStatus};

/// An agent row, without its configuration.
#[derive(Clone, Debug)]
pub struct AgentRow {
    pub id: AgentId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub description: Option<String>,
    pub status: AgentStatus,
    pub published_version_id: Option<AgentVersionId>,
    pub draft_version_id: Option<AgentVersionId>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct AgentVersionRow {
    pub id: AgentVersionId,
    pub version: i32,
    pub config: AgentConfig,
    pub created_at: DateTime<Utc>,
}

// ---- agents ---------------------------------------------------------------

pub async fn insert_agent(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    workspace_id: WorkspaceId,
    name: &str,
    description: Option<&str>,
    created_by: Option<UserId>,
) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "INSERT INTO agents (id, tenant_id, workspace_id, name, description, status, created_by)
         VALUES ($1, $2, $3, $4, $5, 'draft', $6)",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .bind(workspace_id.to_db())
    .bind(name)
    .bind(description)
    .bind(created_by.map(|u| u.to_db()))
    .execute(db.conn())
    .await
    // The workspace is the only reference a caller controls here, and row-level
    // security hides another tenant's workspaces, so a bad id lands as missing.
    .map_err(|e| on_missing_reference(e, "workspace"))?;
    Ok(())
}

pub async fn find_agent(db: &mut TenantDb<'_>, agent_id: AgentId) -> Result<AgentRow> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "SELECT id, workspace_id, name, description, status,
                published_version_id, draft_version_id, updated_at
         FROM agents
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("agent"))?;

    agent_row(&row)
}

pub async fn list_agents(
    db: &mut TenantDb<'_>,
    workspace_id: Option<WorkspaceId>,
) -> Result<Vec<AgentRow>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT id, workspace_id, name, description, status,
                published_version_id, draft_version_id, updated_at
         FROM agents
         WHERE tenant_id = $1
           AND ($2::text IS NULL OR workspace_id = $2)
           AND deleted_at IS NULL
           AND status <> 'archived'
         ORDER BY created_at DESC",
    )
    .bind(&tenant)
    .bind(workspace_id.map(|w| w.to_db()))
    .fetch_all(db.conn())
    .await?;

    rows.iter().map(agent_row).collect()
}

/// How many agents count against the plan limit. Archived ones do not.
pub async fn count_agents(db: &mut TenantDb<'_>) -> Result<i64> {
    let tenant = db.tenant_key();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agents
         WHERE tenant_id = $1 AND deleted_at IS NULL AND status <> 'archived'",
    )
    .bind(&tenant)
    .fetch_one(db.conn())
    .await?;
    Ok(count)
}

pub async fn update_agent_details(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    name: &str,
    description: Option<&str>,
) -> Result<()> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "UPDATE agents SET name = $3, description = $4, updated_at = now()
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .bind(name)
    .bind(description)
    .execute(db.conn())
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(DomainError::NotFound("agent"));
    }
    Ok(())
}

pub async fn set_status(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    status: AgentStatus,
) -> Result<()> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "UPDATE agents SET status = $3, updated_at = now()
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .bind(status.as_str())
    .execute(db.conn())
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(DomainError::NotFound("agent"));
    }
    Ok(())
}

pub async fn set_draft_version(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    version_id: AgentVersionId,
) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "UPDATE agents SET draft_version_id = $3, updated_at = now()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .bind(version_id.to_db())
    .execute(db.conn())
    .await?;
    Ok(())
}

/// Point the agent at a published version, and make it live if it was not.
/// A paused or archived agent keeps its status: publishing must not silently
/// bring an agent back online that someone deliberately took down.
pub async fn set_published_version(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    version_id: AgentVersionId,
) -> Result<()> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "UPDATE agents
         SET published_version_id = $3,
             status = CASE WHEN status = 'draft' THEN 'active' ELSE status END,
             updated_at = now()
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .bind(version_id.to_db())
    .execute(db.conn())
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(DomainError::NotFound("agent"));
    }
    Ok(())
}

/// Archiving sets the status and nothing else.
///
/// `deleted_at` is reserved for an actual purge, so that each column means one
/// thing: archived agents stay readable, which is what lets the public API
/// answer "this is gone" instead of "this never existed".
pub async fn archive_agent(db: &mut TenantDb<'_>, agent_id: AgentId) -> Result<()> {
    let tenant = db.tenant_key();
    let affected = sqlx::query(
        "UPDATE agents SET status = 'archived', updated_at = now()
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .execute(db.conn())
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(DomainError::NotFound("agent"));
    }
    Ok(())
}

// ---- versions -------------------------------------------------------------

pub async fn next_version_number(db: &mut TenantDb<'_>, agent_id: AgentId) -> Result<i32> {
    let tenant = db.tenant_key();
    let current: Option<i32> = sqlx::query_scalar(
        "SELECT max(version) FROM agent_versions WHERE agent_id = $1 AND tenant_id = $2",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .fetch_one(db.conn())
    .await?;
    Ok(current.unwrap_or(0) + 1)
}

pub async fn insert_version(
    db: &mut TenantDb<'_>,
    version_id: AgentVersionId,
    agent_id: AgentId,
    version: i32,
    config: &AgentConfig,
    created_by: Option<UserId>,
) -> Result<()> {
    let tenant = db.tenant_key();
    let json = serde_json::to_value(config)
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("could not serialise config: {e}")))?;

    sqlx::query(
        "INSERT INTO agent_versions (id, tenant_id, agent_id, version, config, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(version_id.to_db())
    .bind(&tenant)
    .bind(agent_id.to_db())
    .bind(version)
    .bind(json)
    .bind(created_by.map(|u| u.to_db()))
    .execute(db.conn())
    .await?;
    Ok(())
}

pub async fn find_version(
    db: &mut TenantDb<'_>,
    version_id: AgentVersionId,
) -> Result<AgentVersionRow> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "SELECT id, version, config, created_at
         FROM agent_versions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(version_id.to_db())
    .bind(&tenant)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("agent_version"))?;

    version_row(&row)
}

pub async fn find_version_by_number(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    version: i32,
) -> Result<AgentVersionRow> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "SELECT id, version, config, created_at
         FROM agent_versions WHERE agent_id = $1 AND tenant_id = $2 AND version = $3",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .bind(version)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("agent_version"))?;

    version_row(&row)
}

pub async fn list_versions(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    limit: i64,
) -> Result<Vec<AgentVersionRow>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT id, version, config, created_at
         FROM agent_versions
         WHERE agent_id = $1 AND tenant_id = $2
         ORDER BY version DESC
         LIMIT $3",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .bind(limit)
    .fetch_all(db.conn())
    .await?;

    rows.iter().map(version_row).collect()
}

// ---- knowledge bases ------------------------------------------------------

pub async fn list_knowledge_base_ids(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
) -> Result<Vec<KnowledgeBaseId>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT knowledge_base_id FROM agent_knowledge_bases
         WHERE agent_id = $1 AND tenant_id = $2",
    )
    .bind(agent_id.to_db())
    .bind(&tenant)
    .fetch_all(db.conn())
    .await?;

    rows.iter()
        .map(|row| id(row, "knowledge_base_id"))
        .collect()
}

/// Replace the whole set. Every id is checked against this tenant first: the
/// foreign key will not do it for us, because PostgreSQL runs referential
/// integrity checks with the referenced table's owner privileges and so sees
/// rows that row-level security hides.
pub async fn set_knowledge_bases(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    knowledge_base_ids: &[KnowledgeBaseId],
) -> Result<()> {
    let tenant = db.tenant_key();

    if !knowledge_base_ids.is_empty() {
        let ids: Vec<String> = knowledge_base_ids.iter().map(|k| k.to_db()).collect();
        let found: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM knowledge_bases
             WHERE id = ANY($1) AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(&ids)
        .bind(&tenant)
        .fetch_one(db.conn())
        .await?;

        if found != knowledge_base_ids.len() as i64 {
            return Err(DomainError::NotFound("knowledge_base"));
        }
    }

    sqlx::query("DELETE FROM agent_knowledge_bases WHERE agent_id = $1 AND tenant_id = $2")
        .bind(agent_id.to_db())
        .bind(&tenant)
        .execute(db.conn())
        .await?;

    for kb_id in knowledge_base_ids {
        sqlx::query(
            "INSERT INTO agent_knowledge_bases (tenant_id, agent_id, knowledge_base_id)
             VALUES ($1, $2, $3)",
        )
        .bind(&tenant)
        .bind(agent_id.to_db())
        .bind(kb_id.to_db())
        .execute(db.conn())
        .await
        .map_err(|e| on_missing_reference(e, "knowledge_base"))?;
    }
    Ok(())
}

// ---- row mapping ----------------------------------------------------------

fn agent_row(row: &sqlx::postgres::PgRow) -> Result<AgentRow> {
    Ok(AgentRow {
        id: id(row, "id")?,
        workspace_id: id(row, "workspace_id")?,
        name: row.try_get("name").map_err(sql)?,
        description: row.try_get("description").map_err(sql)?,
        status: parsed(row, "status")?,
        published_version_id: opt_id(row, "published_version_id")?,
        draft_version_id: opt_id(row, "draft_version_id")?,
        updated_at: row.try_get("updated_at").map_err(sql)?,
    })
}

fn version_row(row: &sqlx::postgres::PgRow) -> Result<AgentVersionRow> {
    let json: serde_json::Value = row.try_get("config").map_err(sql)?;
    let config: AgentConfig = serde_json::from_value(json).map_err(|e| {
        DomainError::Internal(anyhow::anyhow!("stored agent config is unreadable: {e}"))
    })?;

    Ok(AgentVersionRow {
        id: id(row, "id")?,
        version: row.try_get("version").map_err(sql)?,
        config,
        created_at: row.try_get("created_at").map_err(sql)?,
    })
}

fn sql(err: sqlx::Error) -> DomainError {
    DomainError::Database(err)
}
