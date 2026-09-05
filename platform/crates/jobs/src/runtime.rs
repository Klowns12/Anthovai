//! The worker loop.
//!
//! Waits on a PostgreSQL `LISTEN` so a fresh upload starts immediately, with a
//! poll interval as the backstop for anything the notification missed — one
//! delivered while the worker was busy, or a retry whose `run_after` has simply
//! come around.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anthovai_core::Result;
use anthovai_db::sqlx::postgres::PgListener;
use anthovai_db::Db;
use chrono::Duration as ChronoDuration;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::queue::{JobQueue, NEW_JOB_CHANNEL};
use crate::{Job, JobError, JobHandler, JobStatus};

/// A job still running after this long is assumed to belong to a dead worker.
/// Comfortably longer than the slowest thing we do, which is a large PDF.
const STALE_AFTER_MINUTES: i64 = 15;

pub type Handlers = HashMap<&'static str, Arc<dyn JobHandler>>;

#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub concurrency: usize,
    pub poll_interval: Duration,
    /// Identifies this worker in `jobs.locked_by`, for debugging a stuck queue.
    pub worker_id: String,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            poll_interval: Duration::from_millis(500),
            worker_id: format!("worker-{}", anthovai_core::JobId::new()),
        }
    }
}

pub struct WorkerRuntime {
    queue: JobQueue,
    handlers: Handlers,
    config: WorkerConfig,
}

impl WorkerRuntime {
    pub fn new(db: Db, config: WorkerConfig) -> Self {
        Self {
            queue: JobQueue::new(db),
            handlers: Handlers::new(),
            config,
        }
    }

    pub fn register(mut self, handler: Arc<dyn JobHandler>) -> Self {
        self.handlers.insert(handler.kind(), handler);
        self
    }

    pub fn queue(&self) -> &JobQueue {
        &self.queue
    }

    /// Run until `shutdown` resolves, then let in-flight jobs finish.
    pub async fn run(self, database_url: &str, shutdown: impl std::future::Future<Output = ()>) {
        let permits = Arc::new(Semaphore::new(self.config.concurrency));
        let mut listener = open_listener(database_url).await;
        let mut shutdown = Box::pin(shutdown);

        // Anything a previous worker abandoned goes back in the queue before we
        // start, so a crash does not leave documents stuck at "processing".
        match self
            .queue
            .reap_stale(ChronoDuration::minutes(STALE_AFTER_MINUTES))
            .await
        {
            Ok(0) => {}
            Ok(n) => warn!(jobs = n, "returned jobs abandoned by a previous worker"),
            Err(e) => error!(error = %e, "could not reap stale jobs"),
        }

        info!(
            worker_id = %self.config.worker_id,
            concurrency = self.config.concurrency,
            "worker ready"
        );

        loop {
            // Claiming happens outside the select. Cancelling it half-way would
            // leave jobs marked running that nobody is running.
            let claimed = self.claim_and_spawn(&permits).await;

            let idle = if claimed == 0 {
                // Nothing waiting: sleep until notified or the interval is up.
                wait_for_work(listener.as_mut(), self.config.poll_interval)
            } else {
                // A full batch usually means more is waiting. Go straight back,
                // pausing only long enough to notice a shutdown.
                wait_for_work(None, Duration::ZERO)
            };

            tokio::select! {
                biased;
                _ = &mut shutdown => break,
                _ = idle => {}
            }
        }

        info!("worker draining");
        // Every permit back means every job has finished.
        let _ = permits
            .acquire_many(self.config.concurrency as u32)
            .await
            .expect("the semaphore is never closed");
        info!("worker stopped");
    }

    /// Take what we have capacity for, and run each job on its own task.
    async fn claim_and_spawn(&self, permits: &Arc<Semaphore>) -> usize {
        let available = permits.available_permits();
        if available == 0 {
            return 0;
        }

        let jobs = match self
            .queue
            .fetch_batch(&self.config.worker_id, available as i64)
            .await
        {
            Ok(jobs) => jobs,
            Err(e) => {
                error!(error = %e, "could not fetch jobs");
                return 0;
            }
        };

        for job in &jobs {
            let permit = Arc::clone(permits)
                .try_acquire_owned()
                .expect("we asked for no more jobs than there were permits");

            let handler = self.handlers.get(job.payload.kind()).cloned();
            let queue = self.queue.clone();
            let job = job.clone();

            tokio::spawn(async move {
                let _permit = permit;
                run_job(queue, handler, job).await;
            });
        }

        jobs.len()
    }
}

async fn run_job(queue: JobQueue, handler: Option<Arc<dyn JobHandler>>, job: Job) {
    let kind = job.payload.kind();

    let Some(handler) = handler else {
        // A job nobody handles will never succeed, however many times it runs.
        error!(kind, job_id = %job.id, "no handler registered for this job kind");
        let _ = queue
            .fail(
                &job,
                &JobError::permanent("no_handler", format!("no handler for `{kind}`")),
            )
            .await;
        return;
    };

    debug!(kind, job_id = %job.id, attempt = job.attempts, "running job");

    match handler.handle(job.clone()).await {
        Ok(()) => {
            if let Err(e) = queue.complete(job.id).await {
                error!(error = %e, job_id = %job.id, "job finished but could not be marked done");
            }
        }
        Err(err) => match queue.fail(&job, &err).await {
            // The `outcome` label is what separates noise from an alert: a job
            // that will be retried is usually a provider having a bad minute,
            // and one that gave up is work nobody is coming back for.
            Ok(JobStatus::Dead) => {
                metrics::counter!("jobs_failed_total", "kind" => kind.to_owned(), "outcome" => "dead")
                    .increment(1);
                error!(kind, job_id = %job.id, error = %err, "job gave up");
            }
            Ok(_) => {
                metrics::counter!("jobs_failed_total", "kind" => kind.to_owned(), "outcome" => "retrying")
                    .increment(1);
                warn!(kind, job_id = %job.id, error = %err, "job failed, will retry");
            }
            Err(e) => error!(error = %e, job_id = %job.id, "could not record job failure"),
        },
    }
}

/// Wake on a notification, or when the poll interval is up.
async fn wait_for_work(listener: Option<&mut PgListener>, poll_interval: Duration) {
    match listener {
        Some(listener) => {
            tokio::select! {
                result = listener.recv() => {
                    if let Err(e) = result {
                        warn!(error = %e, "job notifications stopped, falling back to polling");
                        tokio::time::sleep(poll_interval).await;
                    }
                }
                _ = tokio::time::sleep(poll_interval) => {}
            }
        }
        None => tokio::time::sleep(poll_interval).await,
    }
}

/// Subscribing is an optimisation, not a requirement: without it polling still
/// finds every job, just less promptly. So a failure here warns and continues.
async fn open_listener(database_url: &str) -> Option<PgListener> {
    let mut listener = match PgListener::connect(database_url).await {
        Ok(listener) => listener,
        Err(e) => {
            warn!(error = %e, "could not open a listener connection, polling only");
            return None;
        }
    };

    match listener.listen(NEW_JOB_CHANNEL).await {
        Ok(()) => Some(listener),
        Err(e) => {
            warn!(error = %e, "could not subscribe to job notifications, polling only");
            None
        }
    }
}

/// Run everything currently queued to completion, then return how many ran.
///
/// This is what tests use, and what a one-shot `--drain` run would use: the
/// same code path as the loop, without the waiting.
pub async fn drain(queue: &JobQueue, handlers: &Handlers, worker_id: &str) -> Result<usize> {
    let mut done = 0;
    loop {
        let jobs = queue.fetch_batch(worker_id, 10).await?;
        if jobs.is_empty() {
            return Ok(done);
        }
        for job in jobs {
            let handler = handlers.get(job.payload.kind()).cloned();
            run_job(queue.clone(), handler, job).await;
            done += 1;
        }
    }
}
