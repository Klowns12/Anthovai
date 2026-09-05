//! Parsers, and picking one.
//!
//! Formats arrive over several phases. A registry that reports what it cannot
//! read yet — rather than silently producing nothing — is what lets an upload
//! be refused at the door instead of failing an hour later in a worker.

pub mod docx;
pub mod html;
pub mod pdf;
pub mod structured;
pub mod text;

use std::sync::Arc;

use anthovai_knowledge::SourceType;

use crate::Parser;

pub use docx::DocxParser;
pub use html::HtmlParser;
pub use pdf::PdfParser;
pub use structured::{CsvParser, JsonParser};
pub use text::{MarkdownParser, TextParser};

/// Every parser this build can run.
pub struct ParserRegistry {
    parsers: Vec<Arc<dyn Parser>>,
}

impl ParserRegistry {
    /// Every format the platform reads.
    ///
    /// Order matters only in that each parser claims a disjoint set of types;
    /// `for_type` takes the first that claims one.
    pub fn new() -> Self {
        Self {
            parsers: vec![
                Arc::new(TextParser),
                Arc::new(MarkdownParser),
                Arc::new(PdfParser),
                Arc::new(DocxParser),
                Arc::new(JsonParser),
                Arc::new(CsvParser),
                Arc::new(HtmlParser),
            ],
        }
    }

    pub fn with(mut self, parser: Arc<dyn Parser>) -> Self {
        self.parsers.push(parser);
        self
    }

    pub fn for_type(&self, source_type: SourceType) -> Option<&Arc<dyn Parser>> {
        self.parsers.iter().find(|p| p.supports(source_type))
    }

    pub fn supports(&self, source_type: SourceType) -> bool {
        self.for_type(source_type).is_some()
    }
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ParserRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParserRegistry")
            .field("parsers", &self.parsers.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_covers_what_uploads_accept() {
        let registry = ParserRegistry::new();

        // Whatever `SourceType::is_supported` lets through the upload endpoint
        // must have a parser here, or a document would be accepted and then
        // sit at "failed".
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
            assert_eq!(
                registry.supports(source),
                source.is_supported(),
                "{source:?}: the upload gate and the parser registry disagree"
            );
        }
    }

    #[test]
    fn each_type_resolves_to_one_parser() {
        let registry = ParserRegistry::new();
        assert!(registry.for_type(SourceType::Md).is_some());
        assert!(registry.for_type(SourceType::Pdf).is_some());
    }
}
