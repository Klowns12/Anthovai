//! The ingestion pipeline: parse, normalise, chunk, embed, index.
//!
//! Everything here runs in the worker. An upload request only stores the file
//! and enqueues a job, so a slow PDF never holds an HTTP connection open.

pub mod chunker;
pub mod normalize;
pub mod ocr;
pub mod parsers;
pub mod pipeline;
pub mod tokens;

pub use chunker::{chunk, Block, ChunkConfig, ChunkDraft, ParsedDocument};
pub use normalize::normalize;
pub use ocr::{OcrClient, OcrError};
pub use parsers::ParserRegistry;
pub use pipeline::{IngestOutcome, IngestPipeline};

use anthovai_core::{DomainError, Result};
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

    /// What a parser's failure means for the queue.
    ///
    /// Parsers return `DomainError` like every domain crate, and most of what
    /// they say about a file is final: no text, not a PDF, too many pages.
    /// Two things are not about the file at all — a parse that ran out of
    /// time on a busy worker, and an OCR service that was not answering — and
    /// those are worth another attempt.
    pub fn from_parse(error: DomainError) -> Self {
        match error.code().as_str() {
            error_codes::PARSE_TIMEOUT => {
                Self::transient_with(error_codes::PARSE_TIMEOUT, error.to_string())
            }
            error_codes::OCR_UNAVAILABLE => {
                Self::transient_with(error_codes::OCR_UNAVAILABLE, error.to_string())
            }
            _ => Self::permanent(error_codes::NO_EXTRACTABLE_TEXT, error.to_string()),
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
    /// The OCR sidecar was configured but did not answer. Retried: the scan
    /// is fine, the service is not there yet.
    pub const OCR_UNAVAILABLE: &str = "ocr_unavailable";
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
    fn a_parser_that_ran_out_of_time_is_retried_and_a_scan_is_not() {
        let slow = IngestError::from_parse(DomainError::Conflict(error_codes::PARSE_TIMEOUT));
        assert!(slow.is_retryable());
        assert_eq!(slow.code(), "parse_timeout");

        let no_sidecar = IngestError::from_parse(DomainError::rejected(
            error_codes::OCR_UNAVAILABLE,
            "connection refused",
        ));
        assert!(no_sidecar.is_retryable());
        assert_eq!(no_sidecar.code(), "ocr_unavailable");

        let scan = IngestError::from_parse(DomainError::validation(format!(
            "{}: this is a scan",
            error_codes::NO_EXTRACTABLE_TEXT
        )));
        assert!(!scan.is_retryable());
        assert_eq!(scan.code(), "no_extractable_text");
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
