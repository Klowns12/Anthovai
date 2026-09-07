//! Repositories for users, sessions and API keys.

use anthovai_core::{
    AgentId, AgentScope, ApiKeyId, DomainError, Result, Scope, UserId, WorkspaceId,
};
use anthovai_db::repo::{id, parsed};
use anthovai_db::{on_missing_reference, on_unique_violation, SystemDb, TenantDb};
use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::api_key::Environment;
use crate::session::{NewSession, Session};
use crate::verification::{NewVerification, Verification};
use crate::{ApiKeyRecord, KeyStatus, User};

// ---- users ----------------------------------------------------------------

pub async fn insert_user(
    db: &mut SystemDb<'_>,
    user_id: UserId,
    email: &str,
    password_hash: Option<&str>,
    name: Option<&str>,
) -> Result<()> {
    sqlx::query("INSERT INTO users (id, email, password_hash, name) VALUES ($1, $2, $3, $4)")
        .bind(user_id.to_db())
        .bind(email)
        .bind(password_hash)
        .bind(name)
        .execute(db.conn())
        .await
        .map_err(|e| on_unique_violation(e, "email_taken"))?;
    Ok(())
}

pub async fn find_user_by_email(db: &mut SystemDb<'_>, email: &str) -> Result<Option<User>> {
    let row = sqlx::query(
        "SELECT id, email, password_hash, name, email_verified_at
         FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(db.conn())
    .await?;

    row.map(|row| {
        Ok(User {
            id: id(&row, "id")?,
            email: row.try_get("email").map_err(sql)?,
            password_hash: row.try_get("password_hash").map_err(sql)?,
            name: row.try_get("name").map_err(sql)?,
            email_verified_at: row.try_get("email_verified_at").map_err(sql)?,
        })
    })
    .transpose()
}

pub async fn find_user(db: &mut SystemDb<'_>, user_id: UserId) -> Result<Option<User>> {
    let row = sqlx::query(
        "SELECT id, email, password_hash, name, email_verified_at
         FROM users WHERE id = $1",
    )
    .bind(user_id.to_db())
    .fetch_optional(db.conn())
    .await?;

    row.map(|row| {
        Ok(User {
            id: id(&row, "id")?,
            email: row.try_get("email").map_err(sql)?,
            password_hash: row.try_get("password_hash").map_err(sql)?,
            name: row.try_get("name").map_err(sql)?,
            email_verified_at: row.try_get("email_verified_at").map_err(sql)?,
        })
    })
    .transpose()
}

pub async fn mark_email_verified(db: &mut SystemDb<'_>, user_id: UserId) -> Result<()> {
    sqlx::query("UPDATE users SET email_verified_at = now(), updated_at = now() WHERE id = $1")
        .bind(user_id.to_db())
        .execute(db.conn())
        .await?;
    Ok(())
}

// ---- sessions -------------------------------------------------------------

pub async fn insert_session(
    db: &mut SystemDb<'_>,
    session: &NewSession,
    ip: Option<&str>,
    user_agent: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at, ip, user_agent)
         VALUES ($1, $2, $3, $4::inet, $5)",
    )
    .bind(&session.token_hash)
    .bind(session.user_id.to_db())
    .bind(session.expires_at)
    .bind(ip)
    .bind(user_agent)
    .execute(db.conn())
    .await?;
    Ok(())
}

pub async fn find_session(db: &mut SystemDb<'_>, token_hash: &str) -> Result<Option<Session>> {
    let row = sqlx::query("SELECT id, user_id, expires_at FROM sessions WHERE id = $1")
        .bind(token_hash)
        .fetch_optional(db.conn())
        .await?;

    row.map(|row| {
        Ok(Session {
            token_hash: row.try_get("id").map_err(sql)?,
            user_id: id(&row, "user_id")?,
            expires_at: row.try_get("expires_at").map_err(sql)?,
        })
    })
    .transpose()
}

pub async fn extend_session(
    db: &mut SystemDb<'_>,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query("UPDATE sessions SET expires_at = $2 WHERE id = $1")
        .bind(token_hash)
        .bind(expires_at)
        .execute(db.conn())
        .await?;
    Ok(())
}

pub async fn delete_session(db: &mut SystemDb<'_>, token_hash: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(token_hash)
        .execute(db.conn())
        .await?;
    Ok(())
}

/// Sign out everywhere: used when a password changes or an account is
/// compromised.
pub async fn delete_sessions_for_user(db: &mut SystemDb<'_>, user_id: UserId) -> Result<u64> {
    let affected = sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id.to_db())
        .execute(db.conn())
        .await?
        .rows_affected();
    Ok(affected)
}

pub async fn purge_expired_sessions(db: &mut SystemDb<'_>, now: DateTime<Utc>) -> Result<u64> {
    let affected = sqlx::query("DELETE FROM sessions WHERE expires_at <= $1")
        .bind(now)
        .execute(db.conn())
        .await?
        .rows_affected();
    Ok(affected)
}

// ---- API keys -------------------------------------------------------------

/// What the caller supplies when minting a key.
#[derive(Clone, Debug)]
pub struct NewApiKey {
    pub id: ApiKeyId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub key_hash: String,
    pub prefix: String,
    pub environment: Environment,
    pub scopes: Vec<Scope>,
    pub agents: AgentScope,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: Option<UserId>,
    pub rotated_from: Option<ApiKeyId>,
}

/// Written under the tenant, because by now we know which one it is.
pub async fn insert_api_key(db: &mut TenantDb<'_>, key: &NewApiKey) -> Result<()> {
    let tenant = db.tenant_key();
    let scopes: Vec<String> = key.scopes.iter().map(|s| s.as_str().to_owned()).collect();
    let all_agents = matches!(key.agents, AgentScope::All);

    sqlx::query(
        "INSERT INTO api_keys
           (id, tenant_id, workspace_id, name, key_hash, prefix, environment,
            scopes, all_agents, expires_at, created_by, rotated_from)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(key.id.to_db())
    .bind(&tenant)
    .bind(key.workspace_id.to_db())
    .bind(&key.name)
    .bind(&key.key_hash)
    .bind(&key.prefix)
    .bind(key.environment.as_str())
    .bind(&scopes)
    .bind(all_agents)
    .bind(key.expires_at)
    .bind(key.created_by.map(|u| u.to_db()))
    .bind(key.rotated_from.map(|k| k.to_db()))
    .execute(db.conn())
    .await
    .map_err(|e| on_unique_violation(e, "key_already_exists"))?;

    if let AgentScope::Only(agent_ids) = &key.agents {
        // Checked explicitly, because the foreign key will not do it for us:
        // PostgreSQL runs referential integrity checks with the privileges of
        // the referenced table's owner, so they see rows that row-level
        // security hides from us. Without this, one tenant could mint a key
        // naming another tenant's agent id.
        assert_agents_belong_to_tenant(db, agent_ids).await?;

        for agent_id in agent_ids {
            sqlx::query("INSERT INTO api_key_agents (api_key_id, agent_id) VALUES ($1, $2)")
                .bind(key.id.to_db())
                .bind(agent_id.to_db())
                .execute(db.conn())
                .await
                .map_err(|e| on_missing_reference(e, "agent"))?;
        }
    }
    Ok(())
}

/// Every id must name a live agent inside this tenant. Reports `NotFound` for
/// an agent belonging to another tenant as well as for one that does not exist:
/// the two must be indistinguishable from outside.
async fn assert_agents_belong_to_tenant(
    db: &mut TenantDb<'_>,
    agent_ids: &[AgentId],
) -> Result<()> {
    if agent_ids.is_empty() {
        return Ok(());
    }
    let tenant = db.tenant_key();
    let ids: Vec<String> = agent_ids.iter().map(|a| a.to_db()).collect();

    let found: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agents
         WHERE id = ANY($1) AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(&ids)
    .bind(&tenant)
    .fetch_one(db.conn())
    .await?;

    if found != agent_ids.len() as i64 {
        return Err(DomainError::NotFound("agent"));
    }
    Ok(())
}

/// Resolve a key by its hash. This is the one read that cannot be tenant-scoped
/// — the hash is all we have, and the tenant is what it returns — so it runs as
/// the system role under a policy that allows exactly this, read-only.
pub async fn find_api_key_by_hash(
    db: &mut SystemDb<'_>,
    key_hash: &str,
) -> Result<Option<ApiKeyRecord>> {
    let row = sqlx::query(
        "SELECT k.id, k.tenant_id, k.workspace_id, k.environment, k.scopes,
                k.all_agents, k.status, k.expires_at, o.plan, o.deleted_at AS org_deleted_at
         FROM api_keys k
         JOIN organizations o ON o.id = k.tenant_id
         WHERE k.key_hash = $1",
    )
    .bind(key_hash)
    .fetch_optional(db.conn())
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    // A deleted organization's keys are dead, whatever the key row still says.
    let org_deleted: Option<DateTime<Utc>> = row.try_get("org_deleted_at").map_err(sql)?;
    if org_deleted.is_some() {
        return Ok(None);
    }

    let key_id: ApiKeyId = id(&row, "id")?;
    let all_agents: bool = row.try_get("all_agents").map_err(sql)?;
    let agents = if all_agents {
        AgentScope::All
    } else {
        AgentScope::Only(scoped_agents(db, key_id).await?)
    };

    let raw_scopes: Vec<String> = row.try_get("scopes").map_err(sql)?;
    let scopes = raw_scopes
        .iter()
        .map(|s| s.parse::<Scope>())
        .collect::<Result<Vec<_>>>()?;

    Ok(Some(ApiKeyRecord {
        id: key_id,
        org_id: id(&row, "tenant_id")?,
        workspace_id: id(&row, "workspace_id")?,
        environment: parsed(&row, "environment")?,
        scopes,
        agents,
        plan: parsed(&row, "plan")?,
        status: parsed(&row, "status")?,
        expires_at: row.try_get("expires_at").map_err(sql)?,
    }))
}

async fn scoped_agents(db: &mut SystemDb<'_>, key_id: ApiKeyId) -> Result<Vec<AgentId>> {
    let rows = sqlx::query("SELECT agent_id FROM api_key_agents WHERE api_key_id = $1")
        .bind(key_id.to_db())
        .fetch_all(db.conn())
        .await?;
    rows.iter().map(|row| id(row, "agent_id")).collect()
}

/// Listed for the dashboard. The secret is not here, and cannot be: only its
/// hash was ever stored.
#[derive(Clone, Debug)]
pub struct ApiKeySummary {
    pub id: ApiKeyId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub prefix: String,
    pub environment: Environment,
    pub scopes: Vec<Scope>,
    pub all_agents: bool,
    pub status: KeyStatus,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// `now` decides the reported status: `expires_at` is the single source of
/// truth for expiry, and the stored `status` only records revocation. Nothing
/// has to sweep the table to keep the two in step.
pub async fn list_api_keys(
    db: &mut TenantDb<'_>,
    workspace_id: Option<WorkspaceId>,
    now: DateTime<Utc>,
) -> Result<Vec<ApiKeySummary>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT id, workspace_id, name, prefix, environment, scopes, all_agents,
                status, expires_at, last_used_at, created_at
         FROM api_keys
         WHERE tenant_id = $1 AND ($2::text IS NULL OR workspace_id = $2)
         ORDER BY created_at DESC",
    )
    .bind(&tenant)
    .bind(workspace_id.map(|w| w.to_db()))
    .fetch_all(db.conn())
    .await?;

    rows.iter()
        .map(|row| {
            let raw_scopes: Vec<String> = row.try_get("scopes").map_err(sql)?;
            let stored_status: KeyStatus = parsed(row, "status")?;
            let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at").map_err(sql)?;

            Ok(ApiKeySummary {
                id: id(row, "id")?,
                workspace_id: id(row, "workspace_id")?,
                name: row.try_get("name").map_err(sql)?,
                prefix: row.try_get("prefix").map_err(sql)?,
                environment: parsed(row, "environment")?,
                scopes: raw_scopes
                    .iter()
                    .map(|s| s.parse::<Scope>())
                    .collect::<Result<Vec<_>>>()?,
                all_agents: row.try_get("all_agents").map_err(sql)?,
                status: stored_status.effective(expires_at, now),
                expires_at,
                last_used_at: row.try_get("last_used_at").map_err(sql)?,
                created_at: row.try_get("created_at").map_err(sql)?,
            })
        })
        .collect()
}

/// Returns the key's hash so the caller can evict it from the cache. Without
/// that, a revoked key would keep working until its cache entry aged out.
pub async fn revoke_api_key(db: &mut TenantDb<'_>, key_id: ApiKeyId) -> Result<String> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "UPDATE api_keys
         SET status = 'revoked', revoked_at = now()
         WHERE id = $1 AND tenant_id = $2 AND status = 'active'
         RETURNING key_hash",
    )
    .bind(key_id.to_db())
    .bind(&tenant)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("api_key"))?;

    row.try_get("key_hash").map_err(sql)
}

/// Give an existing key a deadline. Used when rotating: the old key keeps
/// working for a grace period so a deployment can be updated without downtime.
pub async fn set_api_key_expiry(
    db: &mut TenantDb<'_>,
    key_id: ApiKeyId,
    expires_at: DateTime<Utc>,
) -> Result<String> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "UPDATE api_keys SET expires_at = $3
         WHERE id = $1 AND tenant_id = $2 AND status = 'active'
         RETURNING key_hash",
    )
    .bind(key_id.to_db())
    .bind(&tenant)
    .bind(expires_at)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("api_key"))?;

    row.try_get("key_hash").map_err(sql)
}

/// Record that a key was used, at most once a minute per key. Writing on every
/// request would put a row update on the hot path to no purpose.
pub async fn touch_api_key(db: &mut TenantDb<'_>, key_id: ApiKeyId) -> Result<()> {
    let tenant = db.tenant_key();
    sqlx::query(
        "UPDATE api_keys SET last_used_at = now()
         WHERE id = $1 AND tenant_id = $2
           AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')",
    )
    .bind(key_id.to_db())
    .bind(&tenant)
    .execute(db.conn())
    .await?;
    Ok(())
}

fn sql(err: sqlx::Error) -> DomainError {
    DomainError::Database(err)
}

// ---- email verification ---------------------------------------------------

pub async fn insert_verification(
    db: &mut SystemDb<'_>,
    verification: &NewVerification,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO email_verifications (token_hash, user_id, email, expires_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(&verification.token_hash)
    .bind(verification.user_id.to_db())
    .bind(&verification.email)
    .bind(verification.expires_at)
    .execute(db.conn())
    .await
    .map_err(|e| on_missing_reference(e, "user"))?;
    Ok(())
}

pub async fn find_verification(
    db: &mut SystemDb<'_>,
    token_hash: &str,
) -> Result<Option<Verification>> {
    let row = sqlx::query(
        "SELECT user_id, email, expires_at, consumed_at
         FROM email_verifications WHERE token_hash = $1",
    )
    .bind(token_hash)
    .fetch_optional(db.conn())
    .await?;

    row.map(|row| {
        Ok(Verification {
            user_id: id(&row, "user_id")?,
            email: row.try_get("email")?,
            expires_at: row.try_get("expires_at")?,
            consumed_at: row.try_get("consumed_at")?,
        })
    })
    .transpose()
}

/// Mark a token used, but only if it has not been used already.
///
/// The condition is in the UPDATE rather than in a preceding SELECT, so two
/// requests arriving together cannot both find it unconsumed and both proceed.
/// Returns whether this call is the one that consumed it.
pub async fn consume_verification(
    db: &mut SystemDb<'_>,
    token_hash: &str,
    now: DateTime<Utc>,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE email_verifications SET consumed_at = $2
         WHERE token_hash = $1 AND consumed_at IS NULL",
    )
    .bind(token_hash)
    .bind(now)
    .execute(db.conn())
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Void every outstanding token for a user.
///
/// Called when a new one is issued, so asking for a second email does not leave
/// the first link alive — a customer who requested a new one because they
/// suspected the old had gone astray would otherwise have achieved nothing.
pub async fn invalidate_verifications(
    db: &mut SystemDb<'_>,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE email_verifications SET consumed_at = $2
         WHERE user_id = $1 AND consumed_at IS NULL",
    )
    .bind(user_id.to_db())
    .bind(now)
    .execute(db.conn())
    .await?;

    Ok(result.rows_affected())
}

pub async fn purge_expired_verifications(
    db: &mut SystemDb<'_>,
    now: DateTime<Utc>,
) -> Result<u64> {
    let result = sqlx::query("DELETE FROM email_verifications WHERE expires_at < $1")
        .bind(now)
        .execute(db.conn())
        .await?;
    Ok(result.rows_affected())
}
