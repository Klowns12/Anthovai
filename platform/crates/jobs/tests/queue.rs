//! The job queue, against a real PostgreSQL.
//!
//! `FOR UPDATE SKIP LOCKED` is the whole reason a table is enough here, and it
//! is not something a mock can demonstrate. Neither is the retry policy, which
//! lives half in Rust and half in a SQL `UPDATE`.

use std::sync::OnceLock;

use anthovai_core::{DocumentId, OrgId};
use anthovai_db::{sqlx, Db};
use anthovai_jobs::{Job, JobError, JobPayload, JobQueue, JobStatus};
use anthovai_testkit::db_test;
use chrono::Duration;
use tokio::sync::{Mutex, MutexGuard};

/// The queue is one shared table by design — that is what lets several workers
/// share the work. It also means a test that claims jobs would claim another
/// test's, so these run one at a time.
async fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().await
}

/// A tenant to hang jobs from. The queue itself is cross-tenant, but the rows
/// still reference an organization.
async fn seed_org(db: &Db) -> OrgId {
    let org_id = OrgId::new();
    let mut system = db.system().await.unwrap();
    sqlx::query("INSERT INTO organizations (id, slug, name) VALUES ($1, $2, $3)")
        .bind(org_id.to_db())
        .bind(format!("q-{}", org_id.to_db().to_lowercase()))
        .bind("Queue test")
        .execute(system.conn())
        .await
        .expect("insert organization");
    system.commit().await.unwrap();
    org_id
}

fn ingest_job() -> JobPayload {
    JobPayload::IngestDocument {
        document_id: DocumentId::new(),
        version: 1,
    }
}

/// Empty the queue.
///
/// Other test binaries leave work in it — an upload test queues an ingestion
/// job and never runs a worker — and these tests assert on what a fetch
/// returns. They hold the lock above, so clearing it here is safe.
async fn clear_pending(db: &Db) {
    let mut system = db.system().await.unwrap();
    sqlx::query("DELETE FROM jobs WHERE status = 'pending'")
        .execute(system.conn())
        .await
        .expect("clear pending jobs");
    system.commit().await.unwrap();
}

/// The queue is shared, so tests filter to the jobs they created themselves.
async fn status_of(db: &Db, job_id: anthovai_core::JobId) -> (String, i32, Option<String>) {
    let mut system = db.system().await.unwrap();
    let row: (String, i32, Option<String>) =
        sqlx::query_as("SELECT status, attempts, last_error FROM jobs WHERE id = $1")
            .bind(job_id.to_db())
            .fetch_one(system.conn())
            .await
            .expect("read job");
    system.commit().await.unwrap();
    row
}

db_test!(async fn an_enqueued_job_can_be_claimed(db) {
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());
    let payload = ingest_job();

    let job_id = queue.enqueue(org_id, &payload).await.expect("enqueue");

    let claimed = queue.fetch_batch("worker-a", 10).await.expect("fetch");
    let mine = claimed
        .iter()
        .find(|j| j.id == job_id)
        .expect("our job should have been claimed");

    assert_eq!(mine.org_id, org_id);
    assert_eq!(mine.payload, payload);
    assert_eq!(mine.attempts, 1, "claiming counts as an attempt");
});

db_test!(async fn two_workers_never_get_the_same_job(db) {
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());

    let mut mine = Vec::new();
    for _ in 0..10 {
        mine.push(queue.enqueue(org_id, &ingest_job()).await.unwrap());
    }

    // Both workers go for the queue at once, which is what SKIP LOCKED exists
    // to make safe.
    let (a, b) = tokio::join!(
        queue.fetch_batch("worker-a", 10),
        queue.fetch_batch("worker-b", 10)
    );

    let a: Vec<_> = a.unwrap().into_iter().filter(|j| mine.contains(&j.id)).collect();
    let b: Vec<_> = b.unwrap().into_iter().filter(|j| mine.contains(&j.id)).collect();

    for job in &a {
        assert!(
            !b.iter().any(|other| other.id == job.id),
            "job {} was handed to both workers",
            job.id
        );
    }
    assert_eq!(a.len() + b.len(), mine.len(), "every job went to exactly one worker");
});

db_test!(async fn a_completed_job_is_not_handed_out_again(db) {
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());
    let job_id = queue.enqueue(org_id, &ingest_job()).await.unwrap();

    let claimed = queue.fetch_batch("worker-a", 50).await.unwrap();
    let job = claimed.into_iter().find(|j| j.id == job_id).unwrap();
    queue.complete(job.id).await.unwrap();

    let again = queue.fetch_batch("worker-a", 50).await.unwrap();
    assert!(!again.iter().any(|j| j.id == job_id));

    let (status, _, _) = status_of(&db, job_id).await;
    assert_eq!(status, "done");
});

db_test!(async fn a_transient_failure_is_scheduled_for_later(db) {
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());
    let job_id = queue.enqueue(org_id, &ingest_job()).await.unwrap();

    let job = queue
        .fetch_batch("worker-a", 50)
        .await
        .unwrap()
        .into_iter()
        .find(|j| j.id == job_id)
        .unwrap();

    let next = queue
        .fail(&job, &JobError::Transient("the provider returned 503".into()))
        .await
        .unwrap();

    assert_eq!(next, JobStatus::Pending);

    let (status, attempts, error) = status_of(&db, job_id).await;
    assert_eq!(status, "pending");
    assert_eq!(attempts, 1);
    assert!(error.unwrap().contains("503"));

    // Backed off, so it is not picked up immediately.
    let immediately = queue.fetch_batch("worker-a", 50).await.unwrap();
    assert!(
        !immediately.iter().any(|j| j.id == job_id),
        "a job that just failed should wait out its backoff"
    );
});

db_test!(async fn a_permanent_failure_is_never_retried(db) {
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());
    let job_id = queue.enqueue(org_id, &ingest_job()).await.unwrap();

    let job = queue
        .fetch_batch("worker-a", 50)
        .await
        .unwrap()
        .into_iter()
        .find(|j| j.id == job_id)
        .unwrap();

    // Retrying a PDF that does not parse only burns attempts and delays the
    // answer the customer is waiting for.
    let next = queue
        .fail(
            &job,
            &JobError::permanent("no_extractable_text", "this PDF is a scan"),
        )
        .await
        .unwrap();

    assert_eq!(next, JobStatus::Dead);
    assert_eq!(status_of(&db, job_id).await.0, "dead");
});

db_test!(async fn a_job_gives_up_after_its_last_attempt(db) {
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());
    let job_id = queue.enqueue(org_id, &ingest_job()).await.unwrap();

    // Fail it as though it had already used every attempt.
    let job = Job {
        id: job_id,
        org_id,
        payload: ingest_job(),
        attempts: 3,
        max_attempts: 3,
        created_at: chrono::Utc::now(),
    };

    let next = queue
        .fail(&job, &JobError::Transient("still failing".into()))
        .await
        .unwrap();

    assert_eq!(next, JobStatus::Dead);
});

db_test!(async fn a_job_abandoned_by_a_dead_worker_comes_back(db) {
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());
    let job_id = queue.enqueue(org_id, &ingest_job()).await.unwrap();

    // Claimed, then the worker vanishes without completing or failing it.
    queue.fetch_batch("worker-that-died", 50).await.unwrap();
    assert_eq!(status_of(&db, job_id).await.0, "running");

    // Nothing is stale yet.
    queue.reap_stale(Duration::minutes(15)).await.unwrap();
    assert_eq!(status_of(&db, job_id).await.0, "running");

    // With a threshold of zero, everything running counts as abandoned.
    let reaped = queue.reap_stale(Duration::zero()).await.unwrap();
    assert!(reaped >= 1);

    let (status, attempts, error) = status_of(&db, job_id).await;
    assert_eq!(status, "pending", "the job should be available again");
    assert_eq!(attempts, 1, "the attempt it already used still counts");
    assert!(error.unwrap().contains("worker stopped"));
});

db_test!(async fn higher_priority_work_is_taken_first(db) {
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());

    // Priority decides which rows a worker claims, not the order they come back
    // in — `UPDATE ... RETURNING` makes no promise about that. So this asks the
    // question that matters: with room for one job, which one is taken?
    clear_pending(&db).await;

    // Housekeeping first, so the answer cannot be an accident of insertion order.
    queue
        .enqueue(org_id, &JobPayload::PurgeDeletedChunks)
        .await
        .unwrap();
    let ingest = queue.enqueue(org_id, &ingest_job()).await.unwrap();

    let claimed = queue.fetch_batch("worker-a", 1).await.unwrap();

    assert_eq!(claimed.len(), 1);
    assert_eq!(
        claimed[0].id, ingest,
        "a customer waiting on an upload outranks a cleanup job"
    );
});

db_test!(async fn the_queue_reports_its_depth(db) {
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());

    let before = queue.depth().await.unwrap();
    queue.enqueue(org_id, &ingest_job()).await.unwrap();
    let after = queue.depth().await.unwrap();

    assert_eq!(after.pending, before.pending + 1);
});

db_test!(async fn a_job_enqueued_in_a_rolled_back_transaction_never_appears(db) {
    // The reason enqueue can join a caller's transaction: a job must not point
    // at work that was rolled back.
    let _guard = exclusive().await;
    clear_pending(&db).await;
    let org_id = seed_org(&db).await;
    let queue = JobQueue::new(db.clone());

    let mut system = db.system().await.unwrap();
    let job_id = JobQueue::enqueue_in(&mut system, org_id, &ingest_job())
        .await
        .unwrap();
    system.rollback().await.unwrap();

    let claimed = queue.fetch_batch("worker-a", 100).await.unwrap();
    assert!(!claimed.iter().any(|j| j.id == job_id));
});
