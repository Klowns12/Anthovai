//! PDFs.
//!
//! The two things that go wrong here are both worth naming. A PDF is arbitrary
//! untrusted input handed to a parser that does real work, so it runs on a
//! blocking thread under a timeout and a page cap rather than in the async
//! runtime. And a scanned PDF — a photograph of a page — contains no text at
//! all; it must be refused with a reason the customer can act on, not retried
//! forever, because no number of attempts will make text appear.
//!
//! Page numbers are kept on every block. A citation that can say "page 14" is
//! worth a great deal in a hundred-page handbook.

use std::panic::AssertUnwindSafe;
use std::time::Duration;

use anthovai_core::{DomainError, Result};
use anthovai_knowledge::SourceType;
use async_trait::async_trait;
use tracing::warn;

use crate::chunker::{Block, ParsedDocument};
use crate::normalize::normalize;
use crate::parsers::text::detect_language;
use crate::{error_codes, ParseInput, Parser};

/// The longest we will spend on one document.
///
/// A pathological PDF can keep a parser busy indefinitely. Past this the job
/// fails as retryable, because the same file on a less loaded worker may well
/// finish.
const PARSE_TIMEOUT: Duration = Duration::from_secs(120);

/// The most pages we will read. Larger than any handbook and small enough that
/// one document cannot monopolise a worker.
const MAX_PAGES: usize = 2_000;

/// Below this, whatever came out is not text.
///
/// A scanned PDF is not always empty — it often yields a handful of stray
/// glyphs from a watermark or a form field. Treating those few characters as a
/// document produces a knowledge base that looks populated and answers nothing.
const MIN_TEXT_CHARS: usize = 32;

pub struct PdfParser;

#[async_trait]
impl Parser for PdfParser {
    fn supports(&self, source_type: SourceType) -> bool {
        matches!(source_type, SourceType::Pdf)
    }

    async fn parse(&self, input: ParseInput) -> Result<ParsedDocument> {
        let title = input.title();
        let bytes = input.bytes;

        let work = tokio::task::spawn_blocking(move || {
            // `pdf-extract` panics on some malformed files rather than
            // returning an error. Unwinding out of a worker thread would take
            // the job down without a reason the customer could read.
            std::panic::catch_unwind(AssertUnwindSafe(|| pages_of(&bytes)))
                .unwrap_or_else(|_| Err(unreadable()))
        });

        // The blocking thread cannot be cancelled — it runs to completion in the
        // background either way. What the timeout buys is that the job stops
        // waiting and is retried rather than held open indefinitely.
        let pages = match tokio::time::timeout(PARSE_TIMEOUT, work).await {
            Ok(Ok(result)) => result?,
            Ok(Err(_)) => return Err(unreadable()),
            Err(_) => {
                warn!(document = %title, "the PDF did not parse within the time limit");
                return Err(timed_out());
            }
        };

        let blocks = blocks_from(&pages);
        let characters: usize = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { text, .. } => Some(text.chars().count()),
                _ => None,
            })
            .sum();

        if characters < MIN_TEXT_CHARS {
            return Err(no_text());
        }

        let sample: String = pages.iter().take(5).cloned().collect::<Vec<_>>().join("\n");

        Ok(ParsedDocument {
            title,
            language: detect_language(&sample),
            blocks,
        })
    }
}

/// The text of each page, in order.
fn pages_of(bytes: &[u8]) -> Result<Vec<String>> {
    // The page count is read first: a 50,000-page document should be refused
    // before any of it is rendered, not after.
    let document = pdf_extract::Document::load_mem(bytes).map_err(|e| {
        DomainError::validation(format!(
            "{}: this file is not a readable PDF ({e})",
            error_codes::UNSUPPORTED_FILE_TYPE
        ))
    })?;

    let count = document.get_pages().len();
    if count > MAX_PAGES {
        return Err(too_many_pages(count));
    }

    pdf_extract::extract_text_from_mem_by_pages(bytes).map_err(|e| {
        DomainError::validation(format!(
            "{}: the PDF's text could not be extracted ({e})",
            error_codes::NO_EXTRACTABLE_TEXT
        ))
    })
}

/// Paragraphs, each carrying the page it came from.
///
/// A PDF has no paragraph structure to recover — only lines that happened to be
/// laid out near each other — so a blank line is the only boundary available.
fn blocks_from(pages: &[String]) -> Vec<Block> {
    let mut blocks = Vec::new();

    for (index, page) in pages.iter().enumerate() {
        let page_number = (index + 1) as u32;
        let text = normalize(page);

        for paragraph in text.split("\n\n") {
            let paragraph = paragraph.trim();
            if paragraph.is_empty() {
                continue;
            }
            blocks.push(Block::Paragraph {
                text: paragraph.to_owned(),
                page: Some(page_number),
            });
        }
    }

    blocks
}

fn no_text() -> DomainError {
    DomainError::validation(format!(
        "{}: this PDF holds no selectable text. It is most likely a scan or a \
         set of page images; run it through OCR and upload the result.",
        error_codes::NO_EXTRACTABLE_TEXT
    ))
}

fn unreadable() -> DomainError {
    DomainError::validation(format!(
        "{}: this file could not be read as a PDF",
        error_codes::UNSUPPORTED_FILE_TYPE
    ))
}

fn timed_out() -> DomainError {
    DomainError::Conflict(error_codes::PARSE_TIMEOUT)
}

fn too_many_pages(count: usize) -> DomainError {
    DomainError::validation(format!(
        "{}: {count} pages is past the limit of {MAX_PAGES}. Split the document \
         and upload the parts.",
        error_codes::FILE_TOO_LARGE
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(bytes: Vec<u8>) -> ParseInput {
        ParseInput {
            bytes,
            source_type: SourceType::Pdf,
            filename: Some("handbook.pdf".to_owned()),
            source_url: None,
        }
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

    /// A PDF whose pages hold the given lines, built here so the tests do not
    /// depend on a binary fixture nobody can read in a diff.
    fn pdf(pages: &[&[&str]]) -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let page_ids: Vec<Object> = pages
            .iter()
            .map(|lines| {
                let mut operations = vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 14.into()]),
                    Operation::new("Td", vec![72.into(), 720.into()]),
                    Operation::new("TL", vec![18.into()]),
                ];
                for line in *lines {
                    operations.push(Operation::new(
                        "Tj",
                        vec![Object::string_literal(line.to_string())],
                    ));
                    operations.push(Operation::new("T*", vec![]));
                }
                operations.push(Operation::new("ET", vec![]));

                let content = Content { operations };
                let content_id =
                    doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

                doc.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "Contents" => content_id,
                })
                .into()
            })
            .collect();

        let count = page_ids.len() as u32;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids,
                "Count" => count,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[tokio::test]
    async fn text_is_extracted_and_carries_its_page_number() {
        let bytes = pdf(&[
            &["The library opens at seven in the morning."],
            &["Parking permits cost four hundred baht per semester."],
        ]);
        let doc = PdfParser.parse(input(bytes)).await.unwrap();

        let paragraphs = paragraphs(&doc);
        assert!(
            paragraphs.iter().any(|(text, page)| {
                text.contains("library opens at seven") && *page == Some(1)
            }),
            "got {paragraphs:?}"
        );
        assert!(
            paragraphs
                .iter()
                .any(|(text, page)| text.contains("Parking permits") && *page == Some(2)),
            "a citation that cannot say which page is much less useful: {paragraphs:?}"
        );
    }

    #[tokio::test]
    async fn a_pdf_with_no_selectable_text_is_refused_and_says_why() {
        // What a scan looks like from here: valid pages, no text operators.
        let bytes = pdf(&[&[], &[]]);
        let err = PdfParser.parse(input(bytes)).await.unwrap_err();

        let message = err.to_string();
        assert!(
            message.contains(error_codes::NO_EXTRACTABLE_TEXT),
            "{message}"
        );
        assert!(
            message.contains("OCR"),
            "the customer needs to know what to do about it: {message}"
        );
    }

    #[tokio::test]
    async fn a_watermark_alone_is_not_a_document() {
        // A handful of stray glyphs is the usual output of a scan, and it must
        // not pass for content.
        let bytes = pdf(&[&["DRAFT"]]);
        let err = PdfParser.parse(input(bytes)).await.unwrap_err();
        assert!(err.to_string().contains(error_codes::NO_EXTRACTABLE_TEXT));
    }

    #[tokio::test]
    async fn something_that_is_not_a_pdf_is_refused_rather_than_crashing() {
        let err = PdfParser
            .parse(input(b"%PDF-1.7\nnot really".to_vec()))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains(error_codes::UNSUPPORTED_FILE_TYPE)
                || err.to_string().contains(error_codes::NO_EXTRACTABLE_TEXT),
            "got {err}"
        );
    }

    #[test]
    fn pages_become_paragraphs_split_on_blank_lines() {
        let blocks = blocks_from(&[
            "First paragraph.\n\nSecond paragraph.".to_owned(),
            "On the next page.".to_owned(),
        ]);

        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[1], Block::Paragraph { page: Some(1), .. }));
        assert!(matches!(&blocks[2], Block::Paragraph { page: Some(2), .. }));
    }

    #[test]
    fn a_blank_page_contributes_nothing() {
        let blocks = blocks_from(&["".to_owned(), "  \n \n ".to_owned(), "Real text".to_owned()]);
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], Block::Paragraph { page: Some(3), .. }));
    }

    /// The question the generated fixtures above cannot answer.
    ///
    /// Thai PDFs are usually produced with subset-embedded fonts, and whether
    /// their glyphs map back to characters depends on whether the producer
    /// wrote a `ToUnicode` table. When it did not, `pdf-extract` returns
    /// plausible-looking mojibake rather than an error — which is the worst
    /// possible failure, because it embeds and retrieves for months.
    ///
    /// Drop a real customer PDF at the path below and run:
    ///
    /// ```text
    /// cargo test -p anthovai-ingestion --lib thai -- --ignored --nocapture
    /// ```
    ///
    /// If this fails, the decision to make is the sidecar `pdftotext`
    /// (poppler) in a separate no-network container, per the Phase E plan.
    #[tokio::test]
    #[ignore = "needs a real Thai PDF at tests/fixtures/thai-handbook.pdf"]
    async fn a_real_thai_pdf_comes_back_as_readable_thai() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/thai-handbook.pdf"
        );
        let Ok(bytes) = std::fs::read(path) else {
            println!("no fixture at {path}; nothing to check");
            return;
        };

        let doc = PdfParser.parse(input(bytes)).await.expect("parse the PDF");
        let text: String = paragraphs(&doc)
            .iter()
            .map(|(t, _)| *t)
            .collect::<Vec<_>>()
            .join("\n");

        println!("--- first 600 characters ---\n{}", truncate(&text, 600));
        println!("language: {:?}", doc.language);

        let thai = text
            .chars()
            .filter(|c| ('\u{0E00}'..='\u{0E7F}').contains(c));
        let ratio = thai.count() as f32 / text.chars().count().max(1) as f32;
        println!("Thai characters: {:.0}%", ratio * 100.0);

        // Mojibake from a subset font without a ToUnicode table lands well
        // under this; genuine Thai prose lands well over it.
        assert!(
            ratio > 0.3,
            "only {:.0}% of the extracted text is Thai — the glyphs are most \
             likely not mapping back to characters",
            ratio * 100.0
        );
        assert_eq!(doc.language.as_deref(), Some("tha"));
    }

    fn truncate(text: &str, chars: usize) -> String {
        text.chars().take(chars).collect()
    }

    #[test]
    fn the_parser_only_claims_pdfs() {
        assert!(PdfParser.supports(SourceType::Pdf));
        assert!(!PdfParser.supports(SourceType::Docx));
    }
}
