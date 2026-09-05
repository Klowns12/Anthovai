//! The ingestion pipeline: parse, normalise, chunk, embed, index.
//!
//! Everything here runs in the worker. An upload request only stores the file
//! and enqueues a job, so a slow PDF never holds an HTTP connection open.

pub mod chunker;
pub mod normalize;
pub mod parsers;
pub mod pipeline;
pub mod tokens;

pub use chunker::{chunk, Block, ChunkConfig, ChunkDraft, ParsedDocument};
pub use normalize::normalize;
pub use parsers::ParserRegistry;
pub use pipeline::{IngestOutcome, IngestPipeline};

use anthovai_core::Result;
use anthovai_knowledge::SourceType;
use async_trait::async_trait;

/// Input handed to a parser: the raw bytes plus what we know about them.
pub struct ParseInput {
    pub bytes: Vec<u8>,
    pub source_type: SourceType,
    pub filename: Option<String>,
    pub source_url: Option<String>,
}

impl ParseInput {
    /// A readable title, falling back to the filename or the URL.
    pub fn title(&self) -> String {
        self.filename
            .clone()
            .or_else(|| self.source_url.clone())
            .unwrap_or_else(|| "Untitled".to_owned())
    }
}

#[async_trait]
pub trait Parser: Send + Sync {
    fn supports(&self, source_type: SourceType) -> bool;
    async fn parse(&self, input: ParseInput) -> Result<ParsedDocument>;
}

/// Why a document failed, and whether trying again could help.
///
/// The distinction is the whole point: a scanned PDF will never parse however
/// many times it is retried, and retrying only delays telling the customer.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("{message}")]
    Transient { code: &'static str, message: String },
    #[error("{message}")]
    Permanent { code: &'static str, message: String },
}

impl IngestError {
    pub fn transient(error: impl std::fmt::Display) -> Self {
        Self::Transient {
            code: error_codes::TEMPORARY_FAILURE,
            message: error.to_string(),
        }
    }

    pub fn transient_with(code: &'static str, message: impl Into<String>) -> Self {
        Self::Transient {
            code,
            message: message.into(),
        }
    }

    pub fn permanent(code: &'static str, message: impl Into<String>) -> Self {
        Self::Permanent {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Transient { code, .. } | Self::Permanent { code, .. } => code,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Transient { .. })
    }
}

/// Error codes surfaced to the customer on a failed document. Kept as one list
/// so the dashboard, the API and the docs agree on the wording.
pub mod error_codes {
    pub const NO_EXTRACTABLE_TEXT: &str = "no_extractable_text";
    pub const FETCH_FAILED: &str = "fetch_failed";
    pub use anthovai_knowledge::url_guard::URL_NOT_ALLOWED;
    pub const PARSE_TIMEOUT: &str = "parse_timeout";
    pub const UNSUPPORTED_FILE_TYPE: &str = "unsupported_file_type";
    pub const EMBEDDING_FAILED: &str = "embedding_failed";
    pub const FILE_TOO_LARGE: &str = "file_too_large";
    pub const DOCUMENT_MISSING: &str = "document_missing";
    pub const FILE_MISSING: &str = "file_missing";
    pub const TEMPORARY_FAILURE: &str = "temporary_failure";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scanned_pdf_is_never_retried() {
        let error = IngestError::permanent(error_codes::NO_EXTRACTABLE_TEXT, "this is a scan");
        assert!(!error.is_retryable());
        assert_eq!(error.code(), "no_extractable_text");
    }

    #[test]
    fn a_provider_outage_is_retried() {
        let error = IngestError::transient_with(error_codes::EMBEDDING_FAILED, "503 from provider");
        assert!(error.is_retryable());
        assert_eq!(error.code(), "embedding_failed");
    }

    #[test]
    fn a_title_falls_back_through_what_is_known() {
        let named = ParseInput {
            bytes: Vec::new(),
            source_type: SourceType::Md,
            filename: Some("handbook.md".into()),
            source_url: None,
        };
        assert_eq!(named.title(), "handbook.md");

        let from_url = ParseInput {
            bytes: Vec::new(),
            source_type: SourceType::Url,
            filename: None,
            source_url: Some("https://abc.ac.th/admissions".into()),
        };
        assert_eq!(from_url.title(), "https://abc.ac.th/admissions");

        let anonymous = ParseInput {
            bytes: Vec::new(),
            source_type: SourceType::Text,
            filename: None,
            source_url: None,
        };
        assert_eq!(anonymous.title(), "Untitled");
    }
}
