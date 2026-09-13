//! Photographs and scans.
//!
//! A delivery note photographed on a building site is the most common document
//! in the cost-control product, and it carries no text at all — only pixels.
//! The OCR sidecar reads it; this parser is the thin end of that. Without a
//! sidecar the file is refused with a reason, though the upload endpoint
//! should already have refused it further upstream (`KnowledgeService` gates
//! images on the same setting), so reaching that branch means the two
//! disagree and the message says so.
//!
//! One picture is one page. Every block says page 1, so a citation on an OCR'd
//! photo reads the same as one on a scanned PDF.

use std::sync::Arc;

use anthovai_core::{DomainError, Result};
use anthovai_knowledge::SourceType;
use async_trait::async_trait;
use tracing::info;

use crate::chunker::{Block, ParsedDocument};
use crate::ocr::{self, OcrClient};
use crate::parsers::text::detect_language;
use crate::{error_codes, ParseInput, Parser};

/// Below this, whatever OCR returned is not a document — the same bar the PDF
/// parser holds text to, for the same reason: a blank page and a page of
/// stray marks must not become a knowledge base that answers nothing.
const MIN_TEXT_CHARS: usize = 32;

pub struct ImageParser {
    ocr: Option<Arc<OcrClient>>,
}

impl ImageParser {
    /// Refuses every image with a reason: there is nothing to read one with.
    pub fn new() -> Self {
        Self { ocr: None }
    }

    /// Reads images with the OCR sidecar.
    pub fn with_ocr(ocr: Arc<OcrClient>) -> Self {
        Self { ocr: Some(ocr) }
    }
}

impl Default for ImageParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Parser for ImageParser {
    fn supports(&self, source_type: SourceType) -> bool {
        matches!(source_type, SourceType::Image)
    }

    async fn parse(&self, input: ParseInput) -> Result<ParsedDocument> {
        let title = input.title();

        let Some(ocr) = &self.ocr else {
            return Err(no_ocr());
        };

        info!(document = %title, "sending the picture to OCR");
        let markdown = ocr
            .ocr_image(&input.bytes)
            .await
            .map_err(ocr::to_domain_error)?;

        let blocks = ocr::blocks_from_pages(std::slice::from_ref(&markdown));
        let characters: usize = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { text, .. } => Some(text.chars().count()),
                _ => None,
            })
            .sum();
        if characters < MIN_TEXT_CHARS {
            return Err(nothing_recovered());
        }

        info!(document = %title, characters, "text recovered from the picture");
        Ok(ParsedDocument {
            title,
            language: detect_language(&markdown),
            blocks,
            ocr: true,
        })
    }
}

fn no_ocr() -> DomainError {
    DomainError::validation(format!(
        "{}: a photograph or scan can only be read by OCR, and OCR is not \
         enabled on this deployment. Upload the text instead, or ask an \
         administrator to enable OCR.",
        error_codes::NO_EXTRACTABLE_TEXT
    ))
}

fn nothing_recovered() -> DomainError {
    DomainError::validation(format!(
        "{}: OCR found no readable text in this picture. It may be blank, out \
         of focus, or taken from too far away — a closer, sharper photograph \
         usually reads.",
        error_codes::NO_EXTRACTABLE_TEXT
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::ocr::testing;

    fn input(bytes: &[u8]) -> ParseInput {
        ParseInput {
            bytes: bytes.to_vec(),
            source_type: SourceType::Image,
            filename: Some("ใบส่งของ.jpg".to_owned()),
            source_url: None,
        }
    }

    /// Not a real JPEG — the stand-in sidecar never looks at the bytes.
    const PHOTO: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];

    async fn client(url: String) -> Arc<OcrClient> {
        Arc::new(OcrClient::new(url, Duration::from_secs(5)).unwrap())
    }

    fn paragraphs(doc: &ParsedDocument) -> Vec<(&str, Option<u32>)> {
        doc.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { text, page } => Some((text.as_str(), *page)),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_photograph_becomes_a_one_page_document() {
        let url = testing::serve(testing::sidecar_answering(vec![(
            1,
            "ร้านวัสดุก่อสร้าง พี.เอส. ค้าส่ง\n\nใบส่งของ เลขที่ DN-690912-03\n\n\
             <table><tr><td>1</td><td>อิฐมอญ</td><td>4,000</td><td>ก้อน</td></tr></table>",
        )]))
        .await;
        let parser = ImageParser::with_ocr(client(url).await);

        let doc = parser.parse(input(PHOTO)).await.unwrap();

        assert!(doc.ocr);
        assert_eq!(doc.language.as_deref(), Some("tha"));
        let paragraphs = paragraphs(&doc);
        assert!(
            paragraphs.iter().all(|(_, page)| *page == Some(1)),
            "one picture is one page: {paragraphs:?}"
        );
        assert!(
            paragraphs
                .iter()
                .any(|(text, _)| text.contains("อิฐมอญ | 4,000 | ก้อน")),
            "the table's numbers must survive: {paragraphs:?}"
        );
    }

    #[tokio::test]
    async fn without_a_sidecar_a_picture_is_refused_and_says_what_to_do() {
        let err = ImageParser::new().parse(input(PHOTO)).await.unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains(error_codes::NO_EXTRACTABLE_TEXT),
            "{message}"
        );
        assert!(message.contains("enable OCR"), "{message}");
        assert!(!crate::IngestError::from_parse(err).is_retryable());
    }

    #[tokio::test]
    async fn a_picture_too_small_to_read_is_refused_for_good() {
        let url = testing::serve(testing::sidecar_refusing()).await;
        let parser = ImageParser::with_ocr(client(url).await);

        let err = parser.parse(input(PHOTO)).await.unwrap_err();
        assert!(err.to_string().contains("too small"), "{err}");
        assert!(
            !crate::IngestError::from_parse(err).is_retryable(),
            "retrying the same blurry photo cannot help"
        );
    }

    #[tokio::test]
    async fn a_sidecar_that_is_down_means_retry_not_failure() {
        let parser = ImageParser::with_ocr(client(testing::dead_url().await).await);

        let err = parser.parse(input(PHOTO)).await.unwrap_err();
        assert_eq!(err.code(), error_codes::OCR_UNAVAILABLE, "{err}");
        assert!(crate::IngestError::from_parse(err).is_retryable());
    }

    #[tokio::test]
    async fn a_picture_of_nothing_is_not_a_document() {
        let url = testing::serve(testing::sidecar_answering(vec![(1, "  ")])).await;
        let parser = ImageParser::with_ocr(client(url).await);

        let err = parser.parse(input(PHOTO)).await.unwrap_err();
        assert!(err.to_string().contains("no readable text"), "{err}");
    }

    #[test]
    fn the_parser_only_claims_images() {
        assert!(ImageParser::new().supports(SourceType::Image));
        assert!(!ImageParser::new().supports(SourceType::Pdf));
    }

    /// The real thing, when it is running.
    ///
    /// The unit tests above prove the plumbing against a stand-in; this one
    /// proves the model reads Thai off an actual page. Start the sidecar
    /// (`ocr-sidecar/README.md`) and run:
    ///
    /// ```text
    /// ANTHOVAI_OCR_URL=http://127.0.0.1:9090 \
    /// ANTHOVAI_OCR_FIXTURE=/path/to/a-thai-page.png \
    /// cargo test -p anthovai-ingestion --lib real_sidecar -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs the OCR sidecar running and ANTHOVAI_OCR_FIXTURE pointing at a picture"]
    async fn a_real_thai_photograph_is_read_by_the_real_sidecar() {
        let (Ok(url), Ok(fixture)) = (
            std::env::var("ANTHOVAI_OCR_URL"),
            std::env::var("ANTHOVAI_OCR_FIXTURE"),
        ) else {
            println!("ANTHOVAI_OCR_URL / ANTHOVAI_OCR_FIXTURE not set; nothing to check");
            return;
        };
        let bytes = std::fs::read(&fixture).expect("read the fixture");
        let parser = ImageParser::with_ocr(Arc::new(
            OcrClient::new(url, Duration::from_secs(600)).unwrap(),
        ));

        let doc = parser.parse(input(&bytes)).await.expect("OCR the picture");

        let text: String = paragraphs(&doc)
            .iter()
            .map(|(t, _)| *t)
            .collect::<Vec<_>>()
            .join("\n");
        println!(
            "--- first 800 characters ---\n{}",
            text.chars().take(800).collect::<String>()
        );
        println!(
            "language: {:?}  ocr: {}  blocks: {}",
            doc.language,
            doc.ocr,
            doc.blocks.len()
        );

        let thai = text
            .chars()
            .filter(|c| ('\u{0E00}'..='\u{0E7F}').contains(c))
            .count();
        assert!(
            thai as f32 / text.chars().count().max(1) as f32 > 0.3,
            "less than a third of the recovered text is Thai"
        );
        assert!(doc.ocr);
    }
}
