//! The job queue, on PostgreSQL.
//!
//! `FOR UPDATE SKIP LOCKED` is what makes this safe with several workers: each
//! one takes rows nobody else has locked, and no two workers can claim the same
//! job. That is the whole reason a queue table is enough here — no broker, one
//! fewer thing to operate, and the jobs are in the same transaction log as the
//! data they are about.

use anthovai_core::{DomainError, JobId, OrgId, Result};
use anthovai_db::{Db, SystemDb};
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;

use crate::{Job, JobError, JobPayload, JobStatus};

/// PostgreSQL channel the API notifies so an idle worker wakes at once instead
/// of waiting out its poll interval.
pub const NEW_JOB_CHANNEL: &str = "jobs_new";

#[derive(Clone, Debug)]
pub struct JobQueue {
    db: Db,
}

impl JobQueue {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Enqueue inside an existing transaction, so a job is only queued if the
    /// work that produced it committed. Enqueuing separately would let a failed
    /// upload leave a job pointing at a document that was rolled back.
    pub async fn enqueue_in(
        db: &mut SystemDb<'_>,
        org_id: OrgId,
        payload: &JobPayload,
    ) -> Result<JobId> {
        let job_id = JobId::new();
        let json = serde_json::to_value(payload).map_err(|e| {
            DomainError::Internal(anyhow::anyhow!("could not serialise job payload: {e}"))
        })?;

        sqlx::query(
            "INSERT INTO jobs (id, tenant_id, kind, payload, priority) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(job_id.to_db())
        .bind(org_id.to_db())
        .bind(payload.kind())
        .bind(json)
        .bind(payload.priority())
        .execute(db.conn())
        .await?;

        // Delivered when the transaction commits, which is exactly when the job
        // becomes real.
        sqlx::query(&format!("NOTIFY {NEW_JOB_CHANNEL}"))
            .execute(db.conn())
            .await?;

        Ok(job_id)
    }

    /// Enqueue on its own. For schedulers and retries, where there is no
    /// surrounding transaction to join.
    pub async fn enqueue(&self, org_id: OrgId, payload: &JobPayload) -> Result<JobId> {
        let mut db = self.db.system().await?;
        let job_id = Self::enqueue_in(&mut db, org_id, payload).await?;
        db.commit().await?;
        Ok(job_id)
    }

    /// Claim up to `limit` jobs. Marks them running and counts the attempt in
    /// the same statement, so a worker that dies mid-job cannot retry for ever.
    pub async fn fetch_batch(&self, worker_id: &str, limit: i64) -> Result<Vec<Job>> {
        let mut db = self.db.system().await?;

        let rows = sqlx::query(
            "UPDATE jobs SET status = 'running', locked_by = $1, locked_at = now(),
                             attempts = attempts + 1
             WHERE id IN (
                 SELECT id FROM jobs
                 WHERE status = 'pending' AND run_after <= now()
                 ORDER BY priority, run_after
                 LIMIT $2
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, tenant_id, payload, attempts, max_attempts, created_at",
        )
        .bind(worker_id)
        .bind(limit)
        .fetch_all(db.conn())
        .await?;

        let jobs = rows.iter().map(job_from_row).collect::<Result<Vec<_>>>()?;
        db.commit().await?;
        Ok(jobs)
    }

    pub async fn complete(&self, job_id: JobId) -> Result<()> {
        let mut db = self.db.system().await?;
        sqlx::query(
            "UPDATE jobs SET status = 'done', finished_at = now(), locked_by = NULL WHERE id = $1",
        )
        .bind(job_id.to_db())
        .execute(db.conn())
        .await?;
        db.commit().await
    }

    /// Record a failure and decide what happens next. A permanent failure goes
    /// straight to `dead`: retrying a PDF that does not parse just burns the
    /// attempts and delays the answer the customer needs.
    pub async fn fail(&self, job: &Job, error: &JobError) -> Result<JobStatus> {
        let retryable = error.is_retryable() && job.should_retry();
        let next = if retryable {
            JobStatus::Pending
        } else {
            JobStatus::Dead
        };

        let run_after = if retryable {
            Utc::now()
                + Duration::from_std(Job::retry_delay(job.attempts))
                    .unwrap_or_else(|_| Duration::seconds(30))
        } else {
            Utc::now()
        };

        let mut db = self.db.system().await?;
        sqlx::query(
            "UPDATE jobs
             SET status = $2, last_error = $3, run_after = $4, locked_by = NULL, locked_at = NULL,
                 finished_at = CASE WHEN $2 = 'dead' THEN now() ELSE NULL END
             WHERE id = $1",
        )
        .bind(job.id.to_db())
        .bind(next.as_str())
        .bind(error.to_string())
        .bind(run_after)
        .execute(db.conn())
        .await?;
        db.commit().await?;

        Ok(next)
    }

    /// Return jobs abandoned by a worker that died while holding them.
    ///
    /// The attempt has already been counted, so a job that keeps killing its
    /// worker still runs out of attempts rather than looping for ever.
    pub async fn reap_stale(&self, older_than: Duration) -> Result<u64> {
        let cutoff = Utc::now() - older_than;
        let mut db = self.db.system().await?;

        let affected = sqlx::query(
            "UPDATE jobs
             SET status = CASE WHEN attempts >= max_attempts THEN 'dead' ELSE 'pending' END,
                 locked_by = NULL, locked_at = NULL,
                 last_error = 'worker stopped before finishing this job'
             WHERE status = 'running' AND locked_at < $1",
        )
        .bind(cutoff)
        .execute(db.conn())
        .await?
        .rows_affected();

        db.commit().await?;
        Ok(affected)
    }

    /// How much work is waiting. Reported by `/internal/health`.
    pub async fn depth(&self) -> Result<QueueDepth> {
        let mut db = self.db.system().await?;
        let row = sqlx::query(
            "SELECT
               count(*) FILTER (WHERE status = 'pending') AS pending,
               count(*) FILTER (WHERE status = 'running') AS running,
               count(*) FILTER (WHERE status = 'dead')    AS dead
             FROM jobs",
        )
        .fetch_one(db.conn())
        .await?;

        let depth = QueueDepth {
            pending: row.try_get("pending").unwrap_or(0),
            running: row.try_get("running").unwrap_or(0),
            dead: row.try_get("dead").unwrap_or(0),
        };
        db.commit().await?;
        Ok(depth)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueDepth {
    pub pending: i64,
    pub running: i64,
    pub dead: i64,
}

fn job_from_row(row: &sqlx::postgres::PgRow) -> Result<Job> {
    let payload: serde_json::Value = row
        .try_get("payload")
        .map_err(|e| DomainError::Internal(e.into()))?;
    let payload: JobPayload = serde_json::from_value(payload).map_err(|e| {
        DomainError::Internal(anyhow::anyhow!("stored job payload is unreadable: {e}"))
    })?;

    Ok(Job {
        id: anthovai_db::repo::id(row, "id")?,
        org_id: anthovai_db::repo::id(row, "tenant_id")?,
        payload,
        attempts: row
            .try_get("attempts")
            .map_err(|e| DomainError::Internal(e.into()))?,
        max_attempts: row
            .try_get("max_attempts")
            .map_err(|e| DomainError::Internal(e.into()))?,
        created_at: row
            .try_get::<DateTime<Utc>, _>("created_at")
            .map_err(|e| DomainError::Internal(e.into()))?,
    })
}
