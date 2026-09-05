//! Database access.
//!
//! Two ways in, and no third:
//!
//! * [`Db::tenant`] — everything a request does. Opens a transaction, switches
//!   to the restricted `anthovai_app` role and pins `app.tenant_id`, so every
//!   row-level security policy applies. Repositories still write
//!   `WHERE tenant_id = $1` themselves; RLS is the second line, not the first.
//! * [`Db::system`] — the three operations that genuinely have no tenant yet:
//!   creating an organization, finding an API key by its hash, and the job
//!   queue. Runs as `anthovai_system`, which policy allows past isolation only
//!   for exactly those cases.
//!
//! The `SET LOCAL ROLE` is not decoration. Connecting as the database owner (as
//! every developer does locally) would otherwise bypass every policy, and the
//! isolation we rely on would be untested until it mattered.

use anthovai_core::{DomainError, OrgId, Result, TenantCtx};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{Postgres, Transaction};

pub mod repo;

pub use sqlx;

const APP_ROLE: &str = "anthovai_app";
const SYSTEM_ROLE: &str = "anthovai_system";

#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Migrations and health checks only. Anything touching customer data goes
    /// through [`Db::tenant`] or [`Db::system`].
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.into()))
    }

    /// Open a tenant-scoped transaction. Commit with [`TenantDb::commit`];
    /// dropping it rolls back.
    pub async fn tenant<'a>(&'a self, ctx: &TenantCtx) -> Result<TenantDb<'a>> {
        self.tenant_for(ctx.org_id).await
    }

    /// The same thing for the worker, which gets its tenant from the job it
    /// picked up rather than from a request.
    pub async fn tenant_for(&self, org_id: OrgId) -> Result<TenantDb<'_>> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(&format!("SET LOCAL ROLE {APP_ROLE}"))
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
            .bind(org_id.to_db())
            .execute(&mut *tx)
            .await?;

        Ok(TenantDb { tx, org_id })
    }

    /// A transaction for the operations that have no tenant to scope to. Use it
    /// where the alternative would be to take a tenant id from the caller and
    /// hope it is right.
    pub async fn system(&self) -> Result<SystemDb<'_>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(&format!("SET LOCAL ROLE {SYSTEM_ROLE}"))
            .execute(&mut *tx)
            .await?;
        Ok(SystemDb { tx })
    }
}

/// A transaction pinned to one tenant.
pub struct TenantDb<'a> {
    tx: Transaction<'a, Postgres>,
    org_id: OrgId,
}

impl TenantDb<'_> {
    /// The tenant this transaction is bound to. Repositories bind this into
    /// every statement rather than accepting a tenant id as an argument, so
    /// there is no parameter for a caller to get wrong.
    pub fn org_id(&self) -> OrgId {
        self.org_id
    }

    /// The tenant id in the form the database stores.
    pub fn tenant_key(&self) -> String {
        self.org_id.to_db()
    }

    pub fn conn(&mut self) -> &mut sqlx::PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> Result<()> {
        self.tx.commit().await?;
        Ok(())
    }

    pub async fn rollback(self) -> Result<()> {
        self.tx.rollback().await?;
        Ok(())
    }
}

impl std::fmt::Debug for TenantDb<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantDb")
            .field("org_id", &self.org_id)
            .finish()
    }
}

/// A transaction for cross-tenant work.
pub struct SystemDb<'a> {
    tx: Transaction<'a, Postgres>,
}

impl SystemDb<'_> {
    pub fn conn(&mut self) -> &mut sqlx::PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> Result<()> {
        self.tx.commit().await?;
        Ok(())
    }

    pub async fn rollback(self) -> Result<()> {
        self.tx.rollback().await?;
        Ok(())
    }
}

impl std::fmt::Debug for SystemDb<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SystemDb")
    }
}

/// Turn a unique-violation into a domain conflict, leaving every other database
/// error alone.
pub fn on_unique_violation(err: sqlx::Error, code: &'static str) -> DomainError {
    match &err {
        sqlx::Error::Database(db) if db.is_unique_violation() => DomainError::Conflict(code),
        _ => DomainError::Database(err),
    }
}

/// Turn a foreign-key violation into "that thing does not exist".
///
/// A request naming a row from another tenant, or one that was deleted between
/// the check and the write, arrives here. It is the caller's mistake, not ours,
/// so it must not surface as a 500.
pub fn on_missing_reference(err: sqlx::Error, what: &'static str) -> DomainError {
    match &err {
        sqlx::Error::Database(db) if db.is_foreign_key_violation() => DomainError::NotFound(what),
        _ => DomainError::Database(err),
    }
}
