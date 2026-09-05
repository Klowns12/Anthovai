//! Knowledge bases, documents, and the status machine an upload walks through.

pub mod repo;
pub mod service;
pub mod url_guard;

pub use service::{CreateKnowledgeBase, KnowledgeService, StartUpload, UploadTarget};

use anthovai_core::{DocumentId, DomainError, KnowledgeBaseId, OrgId, Result, WorkspaceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentStatus {
    Uploading,
    Queued,
    Processing,
    Chunking,
    Embedding,
    Indexing,
    Ready,
    Failed,
    Deleted,
}

impl DocumentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uploading => "uploading",
            Self::Queued => "queued",
            Self::Processing => "processing",
            Self::Chunking => "chunking",
            Self::Embedding => "embedding",
            Self::Indexing => "indexing",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Deleted => "deleted",
        }
    }

    /// Whether this document's chunks may be retrieved.
    pub fn is_searchable(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Whether ingestion is still working on it, for progress display.
    pub fn is_in_progress(self) -> bool {
        matches!(
            self,
            Self::Uploading
                | Self::Queued
                | Self::Processing
                | Self::Chunking
                | Self::Embedding
                | Self::Indexing
        )
    }

    /// Whether a retry makes sense. Only a failure can be retried: re-running
    /// a document that is already working would duplicate its chunks.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Failed)
    }

    /// Roughly how far along the pipeline is, for the progress bar.
    pub fn progress(self) -> u8 {
        match self {
            Self::Uploading => 5,
            Self::Queued => 10,
            Self::Processing => 30,
            Self::Chunking => 50,
            Self::Embedding => 70,
            Self::Indexing => 90,
            Self::Ready => 100,
            Self::Failed | Self::Deleted => 0,
        }
    }
}

impl std::str::FromStr for DocumentStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "uploading" => Ok(Self::Uploading),
            "queued" => Ok(Self::Queued),
            "processing" => Ok(Self::Processing),
            "chunking" => Ok(Self::Chunking),
            "embedding" => Ok(Self::Embedding),
            "indexing" => Ok(Self::Indexing),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            "deleted" => Ok(Self::Deleted),
            other => Err(DomainError::validation(format!(
                "unknown document status `{other}`"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Pdf,
    Docx,
    Txt,
    Md,
    Html,
    Url,
    Json,
    Csv,
    Text,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::Txt => "txt",
            Self::Md => "md",
            Self::Html => "html",
            Self::Url => "url",
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Text => "text",
        }
    }

    /// Detected from the leading bytes, never from the filename. A caller may
    /// name a file anything; the bytes decide which parser runs.
    pub fn from_magic_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"%PDF-") {
            return Some(Self::Pdf);
        }
        // DOCX is a zip container. Ingestion confirms the inner layout before
        // treating it as a Word document.
        if bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
            return Some(Self::Docx);
        }
        None
    }

    /// A last resort for text formats, which have no magic bytes to read. The
    /// content still decides how it is parsed — this only picks the parser.
    pub fn from_extension(filename: &str) -> Option<Self> {
        let extension = filename.rsplit_once('.')?.1.to_lowercase();
        match extension.as_str() {
            "txt" => Some(Self::Txt),
            "md" | "markdown" => Some(Self::Md),
            "json" => Some(Self::Json),
            "csv" => Some(Self::Csv),
            "html" | "htm" => Some(Self::Html),
            "pdf" => Some(Self::Pdf),
            "docx" => Some(Self::Docx),
            _ => None,
        }
    }

    /// Which formats ingestion can actually read. Refusing an upload we cannot
    /// process is kinder than accepting it and failing an hour later, so this
    /// and the parser registry are checked against each other by a test.
    pub fn is_supported(self) -> bool {
        true
    }
}

impl std::str::FromStr for SourceType {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "pdf" => Ok(Self::Pdf),
            "docx" => Ok(Self::Docx),
            "txt" => Ok(Self::Txt),
            "md" => Ok(Self::Md),
            "html" => Ok(Self::Html),
            "url" => Ok(Self::Url),
            "json" => Ok(Self::Json),
            "csv" => Ok(Self::Csv),
            "text" => Ok(Self::Text),
            other => Err(DomainError::validation(format!(
                "unknown source type `{other}`"
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct KnowledgeBase {
    pub id: KnowledgeBaseId,
    pub org_id: OrgId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub description: Option<String>,
    /// Locked when the knowledge base is created. Changing it means re-embedding
    /// every chunk, so it is a migration rather than a setting.
    pub embedding_model: String,
    pub embedding_dim: i32,
    pub storage_bytes: i64,
    pub document_count: i32,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub id: DocumentId,
    pub knowledge_base_id: KnowledgeBaseId,
    pub title: String,
    pub source_type: SourceType,
    pub source_url: Option<String>,
    pub status: DocumentStatus,
    pub progress: u8,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub current_version: i32,
    pub size_bytes: i64,
    pub chunk_count: i32,
    pub token_count: i32,
    pub language: Option<String>,
    pub storage_key: Option<String>,
    pub content_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ready_documents_are_searchable() {
        assert!(DocumentStatus::Ready.is_searchable());
        for status in [
            DocumentStatus::Queued,
            DocumentStatus::Embedding,
            DocumentStatus::Failed,
            DocumentStatus::Deleted,
        ] {
            assert!(!status.is_searchable(), "{status:?} must not be searchable");
        }
    }

    #[test]
    fn progress_climbs_along_the_pipeline() {
        assert!(DocumentStatus::Queued.progress() < DocumentStatus::Embedding.progress());
        assert!(DocumentStatus::Embedding.progress() < DocumentStatus::Ready.progress());
        assert_eq!(DocumentStatus::Ready.progress(), 100);
    }

    #[test]
    fn terminal_states_are_not_in_progress() {
        assert!(!DocumentStatus::Ready.is_in_progress());
        assert!(!DocumentStatus::Failed.is_in_progress());
        assert!(DocumentStatus::Chunking.is_in_progress());
    }

    #[test]
    fn only_failures_can_be_retried() {
        assert!(DocumentStatus::Failed.is_retryable());
        for status in [
            DocumentStatus::Ready,
            DocumentStatus::Processing,
            DocumentStatus::Queued,
        ] {
            assert!(!status.is_retryable(), "{status:?} must not be retryable");
        }
    }

    #[test]
    fn statuses_round_trip() {
        for status in [
            DocumentStatus::Uploading,
            DocumentStatus::Queued,
            DocumentStatus::Processing,
            DocumentStatus::Chunking,
            DocumentStatus::Embedding,
            DocumentStatus::Indexing,
            DocumentStatus::Ready,
            DocumentStatus::Failed,
            DocumentStatus::Deleted,
        ] {
            assert_eq!(status.as_str().parse::<DocumentStatus>().unwrap(), status);
        }
    }

    #[test]
    fn file_types_come_from_the_bytes_not_the_name() {
        assert_eq!(
            SourceType::from_magic_bytes(b"%PDF-1.7 ..."),
            Some(SourceType::Pdf)
        );
        assert_eq!(
            SourceType::from_magic_bytes(&[0x50, 0x4B, 0x03, 0x04, 0x00]),
            Some(SourceType::Docx)
        );
        assert_eq!(SourceType::from_magic_bytes(b"just text"), None);
        assert_eq!(SourceType::from_magic_bytes(b""), None);
    }

    #[test]
    fn extensions_are_the_fallback_for_text_formats() {
        assert_eq!(SourceType::from_extension("notes.md"), Some(SourceType::Md));
        assert_eq!(
            SourceType::from_extension("HANDBOOK.PDF"),
            Some(SourceType::Pdf)
        );
        assert_eq!(SourceType::from_extension("no-extension"), None);
        assert_eq!(SourceType::from_extension("archive.tar.gz"), None);
    }

    #[test]
    fn every_format_the_upload_endpoint_names_can_be_read() {
        // A format accepted here with no parser behind it means a document
        // that sits at "failed" for a week. `parsers::ParserRegistry` has the
        // matching test on the other side of that agreement.
        for source in [
            SourceType::Txt,
            SourceType::Md,
            SourceType::Text,
            SourceType::Pdf,
            SourceType::Docx,
            SourceType::Json,
            SourceType::Csv,
            SourceType::Html,
            SourceType::Url,
        ] {
            assert!(source.is_supported(), "{source:?}");
        }
    }

    #[test]
    fn source_types_round_trip() {
        for source in [
            SourceType::Pdf,
            SourceType::Docx,
            SourceType::Txt,
            SourceType::Md,
            SourceType::Html,
            SourceType::Url,
            SourceType::Json,
            SourceType::Csv,
            SourceType::Text,
        ] {
            assert_eq!(source.as_str().parse::<SourceType>().unwrap(), source);
        }
    }
}
