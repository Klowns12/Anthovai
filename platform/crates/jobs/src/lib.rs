//! The background job queue.
//!
//! P1 keeps the queue in PostgreSQL: `FOR UPDATE SKIP LOCKED` gives competing
//! workers safe hand-off without another moving part to operate, and the jobs
//! live in the same transaction log as the data they are about — so a job is
//! only queued if the work that produced it committed.

use std::time::Duration;

pub mod queue;
pub mod runtime;

pub use queue::{JobQueue, QueueDepth};
pub use runtime::{drain, Handlers, WorkerConfig, WorkerRuntime};

use anthovai_core::{DocumentId, JobId, OrgId, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Failed,
    /// Out of attempts. A human has to look at it.
    Dead,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Dead => "dead",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobPayload {
    IngestDocument {
        document_id: DocumentId,
        version: i32,
    },
    DeleteDocumentChunks {
        document_id: DocumentId,
    },
    ReembedKnowledgeBase {
        knowledge_base_id: anthovai_core::KnowledgeBaseId,
    },
    PurgeDeletedChunks,
}

impl JobPayload {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::IngestDocument { .. } => "ingest_document",
            Self::DeleteDocumentChunks { .. } => "delete_document_chunks",
            Self::ReembedKnowledgeBase { .. } => "reembed_knowledge_base",
            Self::PurgeDeletedChunks => "purge_deleted_chunks",
        }
    }

    /// Lower runs first. Customer-visible work outranks housekeeping.
    pub fn priority(&self) -> i16 {
        match self {
            Self::IngestDocument { .. } => 1,
            Self::DeleteDocumentChunks { .. } => 5,
            Self::ReembedKnowledgeBase { .. } => 7,
            Self::PurgeDeletedChunks => 9,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Job {
    pub id: JobId,
    pub org_id: OrgId,
    pub payload: JobPayload,
    pub attempts: i32,
    pub max_attempts: i32,
    pub created_at: DateTime<Utc>,
}

impl Job {
    /// Backoff after a failed attempt: 30s, then 2m, then 10m.
    pub fn retry_delay(attempts: i32) -> Duration {
        match attempts {
            0 | 1 => Duration::from_secs(30),
            2 => Duration::from_secs(120),
            _ => Duration::from_secs(600),
        }
    }

    /// Whether a further attempt is allowed after this failure.
    pub fn should_retry(&self) -> bool {
        self.attempts < self.max_attempts
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// Try again later: a provider was down, the database blinked.
    #[error("transient failure: {0}")]
    Transient(String),
    /// Retrying cannot help: the file does not parse, the document is gone.
    #[error("permanent failure ({code}): {message}")]
    Permanent { code: &'static str, message: String },
}

impl JobError {
    pub fn permanent(code: &'static str, message: impl Into<String>) -> Self {
        Self::Permanent {
            code,
            message: message.into(),
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Transient(_))
    }
}

#[async_trait]
pub trait JobHandler: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn handle(&self, job: Job) -> std::result::Result<(), JobError>;
}

/// Enqueue side, used by the API.
#[async_trait]
pub trait Enqueue: Send + Sync {
    async fn enqueue(&self, org_id: OrgId, payload: JobPayload) -> Result<JobId>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(attempts: i32, max_attempts: i32) -> Job {
        Job {
            id: JobId::new(),
            org_id: OrgId::new(),
            payload: JobPayload::IngestDocument {
                document_id: DocumentId::new(),
                version: 1,
            },
            attempts,
            max_attempts,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn backoff_grows_with_each_attempt() {
        assert_eq!(Job::retry_delay(1), Duration::from_secs(30));
        assert_eq!(Job::retry_delay(2), Duration::from_secs(120));
        assert_eq!(Job::retry_delay(3), Duration::from_secs(600));
        assert_eq!(Job::retry_delay(9), Duration::from_secs(600));
    }

    #[test]
    fn a_job_stops_retrying_once_attempts_run_out() {
        assert!(job(1, 3).should_retry());
        assert!(!job(3, 3).should_retry());
    }

    #[test]
    fn permanent_failures_are_not_retried() {
        assert!(!JobError::permanent("no_extractable_text", "scanned pdf").is_retryable());
        assert!(JobError::Transient("provider 503".into()).is_retryable());
    }

    #[test]
    fn ingestion_outranks_housekeeping() {
        let ingest = JobPayload::IngestDocument {
            document_id: DocumentId::new(),
            version: 1,
        };
        assert!(ingest.priority() < JobPayload::PurgeDeletedChunks.priority());
    }

    #[test]
    fn payloads_round_trip_through_json() {
        let payload = JobPayload::IngestDocument {
            document_id: DocumentId::new(),
            version: 3,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("ingest_document"));
        assert_eq!(serde_json::from_str::<JobPayload>(&json).unwrap(), payload);
    }
}
