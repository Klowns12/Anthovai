//! Word documents.
//!
//! A `.docx` is a zip of XML. Two things follow from that and both matter here:
//! the format has to be confirmed from the archive's layout rather than from
//! its first four bytes, which every zip shares; and an archive is an untrusted
//! input that can claim to expand to more than we are willing to hold.
//!
//! What is extracted is paragraphs and the heading styles above them, because
//! the heading path is what makes a retrieved paragraph make sense alone.

use std::io::{Cursor, Read};

use anthovai_core::{DomainError, Result};
use anthovai_knowledge::SourceType;
use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::chunker::{Block, ParsedDocument};
use crate::normalize::normalize;
use crate::parsers::text::detect_language;
use crate::{error_codes, ParseInput, Parser};

/// The entry every Word document has, and the one thing that tells a `.docx`
/// apart from any other zip.
const DOCUMENT_PART: &str = "word/document.xml";

/// The most XML we will decompress. Word documents are small; anything past
/// this is either a corpus or a zip bomb, and both should be refused rather
/// than held in memory.
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

pub struct DocxParser;

#[async_trait]
impl Parser for DocxParser {
    fn supports(&self, source_type: SourceType) -> bool {
        matches!(source_type, SourceType::Docx)
    }

    async fn parse(&self, input: ParseInput) -> Result<ParsedDocument> {
        let title = input.title();

        // Decompressing and walking XML is CPU work with no await in it, and a
        // large document would otherwise stall every other job on this thread.
        let xml = tokio::task::spawn_blocking(move || document_xml(&input.bytes))
            .await
            .map_err(|_| {
                DomainError::Internal(anyhow::anyhow!(
                    "the Word document could not be read: the task was lost"
                ))
            })??;

        let blocks = blocks_from(&xml)?;
        if blocks.is_empty() {
            return Err(empty());
        }

        let sample: String = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { text, .. } => Some(text.as_str()),
                Block::Heading { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .take(40)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ParsedDocument {
            title,
            language: detect_language(&sample),
            blocks,
        })
    }
}

/// Pull `word/document.xml` out of the archive.
///
/// A zip without that entry is not a Word document, whatever its extension
/// says — that is the check the magic bytes cannot make.
fn document_xml(bytes: &[u8]) -> Result<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(not_a_docx)?;

    let mut part = archive.by_name(DOCUMENT_PART).map_err(|_| {
        DomainError::validation(format!(
            "{}: this is a zip archive but not a Word document — it has no {DOCUMENT_PART}",
            error_codes::UNSUPPORTED_FILE_TYPE
        ))
    })?;

    // The declared size is the archive's own claim, so it is checked first and
    // the read is capped anyway in case the claim was a lie.
    if part.size() > MAX_DOCUMENT_BYTES {
        return Err(too_large(part.size()));
    }

    let mut xml = String::new();
    let read = part
        .by_ref()
        .take(MAX_DOCUMENT_BYTES + 1)
        .read_to_string(&mut xml)
        .map_err(|e| {
            DomainError::validation(format!(
                "{}: the Word document's text could not be read ({e})",
                error_codes::NO_EXTRACTABLE_TEXT
            ))
        })?;

    if read as u64 > MAX_DOCUMENT_BYTES {
        return Err(too_large(read as u64));
    }

    Ok(xml)
}

/// Walk the document body, one `w:p` at a time.
///
/// A paragraph carrying a `Heading1`–`Heading6` style becomes a heading; every
/// other paragraph, table cell included, becomes prose.
fn blocks_from(xml: &str) -> Result<Vec<Block>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut blocks = Vec::new();
    let mut buffer = String::new();
    let mut heading: Option<u8> = None;
    let mut in_paragraph = false;
    // Word stores deleted text in `w:delText` and instructions in `w:instrText`.
    // Neither is what the reader sees, so neither should be indexed.
    let mut in_text = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) => match local_name(tag.name().as_ref()) {
                "p" => {
                    in_paragraph = true;
                    heading = None;
                    buffer.clear();
                }
                "t" if in_paragraph => in_text = true,
                "pStyle" if in_paragraph => {
                    heading = heading_level(&attribute(&tag, "val").unwrap_or_default());
                }
                _ => {}
            },

            Ok(Event::Empty(tag)) => match local_name(tag.name().as_ref()) {
                "pStyle" if in_paragraph => {
                    heading = heading_level(&attribute(&tag, "val").unwrap_or_default());
                }
                // A tab or a line break inside a paragraph is a word boundary.
                // Without this, "Rust" and "12 weeks" run together.
                "tab" | "br" | "cr" if in_paragraph => buffer.push(' '),
                _ => {}
            },

            Ok(Event::Text(text)) if in_text => {
                buffer.push_str(&text.xml10_content());
            }

            // Entities arrive as their own event rather than inside the text
            // around them, so a parser that only reads `Text` silently drops
            // them: "Fees &amp; charges" becomes "Fees  charges". Word writes
            // `&amp;` for every ampersand in a document, so this is the common
            // case rather than an exotic one.
            Ok(Event::GeneralRef(entity)) if in_text => {
                if let Some(character) = entity.resolve_char_ref().map_err(malformed)? {
                    buffer.push(character);
                } else if let Some(text) =
                    quick_xml::escape::resolve_predefined_entity(&entity.xml10_content())
                {
                    buffer.push_str(text);
                }
                // An entity we cannot resolve is one a DTD would have defined,
                // and Word does not write those. Dropping it is better than
                // putting `&custom;` into the index.
            }

            Ok(Event::End(tag)) => match local_name(tag.name().as_ref()) {
                "t" => in_text = false,
                "p" if in_paragraph => {
                    in_paragraph = false;
                    push(&mut blocks, heading.take(), &buffer);
                    buffer.clear();
                }
                // A table cell ends a line even mid-row, so two cells do not
                // read as one sentence.
                "tc" => buffer.push(' '),
                _ => {}
            },

            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(malformed(e)),
        }
    }

    Ok(blocks)
}

fn push(blocks: &mut Vec<Block>, heading: Option<u8>, buffer: &str) {
    let text = normalize(buffer);
    let text = text.trim();
    if text.is_empty() {
        return;
    }

    match heading {
        Some(level) => blocks.push(Block::Heading {
            level,
            text: text.to_owned(),
        }),
        None => blocks.push(Block::Paragraph {
            text: text.to_owned(),
            page: None,
        }),
    }
}

/// `w:p` and `p` are the same element; the prefix depends on the writer.
fn local_name(name: &str) -> &str {
    match name.find(':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

fn attribute(tag: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    tag.attributes()
        .flatten()
        .find_map(|attr| (local_name(attr.key.as_ref()) == name).then(|| attr.value.into_owned()))
}

/// `Heading1` through `Heading6`, however the writer spelled it.
///
/// Word itself writes `Heading1`; other writers produce `heading 1` or
/// `Heading-1`, and a document whose structure is dropped chunks as one flat
/// wall of prose.
fn heading_level(style: &str) -> Option<u8> {
    let style = style.to_lowercase();
    let digits = style.strip_prefix("heading")?;
    digits
        .trim_start_matches([' ', '-', '_'])
        .parse::<u8>()
        .ok()
        .filter(|level| (1..=6).contains(level))
}

fn not_a_docx(error: zip::result::ZipError) -> DomainError {
    DomainError::validation(format!(
        "{}: this is not a readable Word document ({error})",
        error_codes::UNSUPPORTED_FILE_TYPE
    ))
}

fn malformed(error: impl std::fmt::Display) -> DomainError {
    DomainError::validation(format!(
        "{}: the Word document's XML is malformed ({error})",
        error_codes::NO_EXTRACTABLE_TEXT
    ))
}

fn empty() -> DomainError {
    DomainError::validation(format!(
        "{}: the Word document contains no text",
        error_codes::NO_EXTRACTABLE_TEXT
    ))
}

fn too_large(bytes: u64) -> DomainError {
    DomainError::validation(format!(
        "{}: the document's text is {bytes} bytes, past the limit of {MAX_DOCUMENT_BYTES}",
        error_codes::FILE_TOO_LARGE
    ))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// A `.docx` holding exactly `body`, built here so the tests do not depend
    /// on a binary fixture nobody can read in a diff.
    fn docx(body: &str) -> Vec<u8> {
        archive(&[(
            DOCUMENT_PART,
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                  <w:body>{body}</w:body>
                </w:document>"#
            ),
        )])
    }

    fn archive(entries: &[(&str, String)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            zip.start_file::<_, ()>(*name, Default::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn paragraph(text: &str) -> String {
        format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
    }

    fn styled(style: &str, text: &str) -> String {
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr>\
             <w:r><w:t>{text}</w:t></w:r></w:p>"
        )
    }

    fn input(bytes: Vec<u8>) -> ParseInput {
        ParseInput {
            bytes,
            source_type: SourceType::Docx,
            filename: Some("handbook.docx".to_owned()),
            source_url: None,
        }
    }

    #[tokio::test]
    async fn headings_and_paragraphs_keep_their_shape() {
        let body = format!(
            "{}{}{}",
            styled("Heading1", "Programs"),
            styled("Heading2", "Rust Programming"),
            paragraph("Runs for twelve weeks.")
        );
        let doc = DocxParser.parse(input(docx(&body))).await.unwrap();

        assert!(matches!(
            &doc.blocks[0],
            Block::Heading { level: 1, text } if text == "Programs"
        ));
        assert!(matches!(
            &doc.blocks[1],
            Block::Heading { level: 2, text } if text == "Rust Programming"
        ));
        assert!(matches!(
            &doc.blocks[2],
            Block::Paragraph { text, .. } if text == "Runs for twelve weeks."
        ));
    }

    #[tokio::test]
    async fn a_paragraph_split_across_runs_is_put_back_together() {
        // Word splits a paragraph at every formatting change, so this is the
        // normal case rather than an odd one.
        let body = "<w:p><w:r><w:t>The Rust course </w:t></w:r>\
                    <w:r><w:t>runs for twelve weeks.</w:t></w:r></w:p>";
        let doc = DocxParser.parse(input(docx(body))).await.unwrap();

        assert!(matches!(
            &doc.blocks[0],
            Block::Paragraph { text, .. } if text == "The Rust course runs for twelve weeks."
        ));
    }

    #[tokio::test]
    async fn a_tab_separates_words_rather_than_joining_them() {
        let body = "<w:p><w:r><w:t>Rust</w:t><w:tab/><w:t>12 weeks</w:t></w:r></w:p>";
        let doc = DocxParser.parse(input(docx(body))).await.unwrap();

        assert!(matches!(
            &doc.blocks[0],
            Block::Paragraph { text, .. } if text == "Rust 12 weeks"
        ));
    }

    #[tokio::test]
    async fn deleted_text_and_field_instructions_are_not_indexed() {
        // `w:delText` is a tracked deletion and `w:instrText` is a field code.
        // Neither is on the page, so neither belongs in an answer.
        let body = "<w:p><w:r><w:delText>4900 baht</w:delText>\
                    <w:instrText>PAGE \\* MERGEFORMAT</w:instrText>\
                    <w:t>5900 baht</w:t></w:r></w:p>";
        let doc = DocxParser.parse(input(docx(body))).await.unwrap();

        let Block::Paragraph { text, .. } = &doc.blocks[0] else {
            panic!("expected a paragraph");
        };
        assert_eq!(text, "5900 baht", "a struck-out price must not be quoted");
    }

    #[tokio::test]
    async fn table_cells_do_not_run_into_one_another() {
        let body = format!(
            "<w:tbl><w:tr><w:tc>{}</w:tc><w:tc>{}</w:tc></w:tr></w:tbl>",
            paragraph("Rust"),
            paragraph("12 weeks")
        );
        let doc = DocxParser.parse(input(docx(&body))).await.unwrap();

        let texts: Vec<&str> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["Rust", "12 weeks"]);
    }

    #[tokio::test]
    async fn xml_entities_are_decoded() {
        // `xml10_content` replaced an explicit decode step when the crate was
        // upgraded past RUSTSEC-2026-0194. A silent change here would put
        // `&amp;` into a customer's index instead of `&`.
        let body = paragraph("Fees &amp; charges &lt; 5,000 THB");
        let doc = DocxParser.parse(input(docx(&body))).await.unwrap();

        let Block::Paragraph { text, .. } = &doc.blocks[0] else {
            panic!("expected a paragraph");
        };
        assert_eq!(text, "Fees & charges < 5,000 THB");
    }

    #[tokio::test]
    async fn thai_text_survives_and_is_detected() {
        let body = format!(
            "{}{}",
            styled("Heading1", "หลักสูตร"),
            paragraph(
                "หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์ เรียนช่วงเย็นวันธรรมดา \
                 ตั้งแต่หกโมงเย็นถึงสามทุ่ม"
            )
        );
        let doc = DocxParser.parse(input(docx(&body))).await.unwrap();

        assert!(matches!(&doc.blocks[0], Block::Heading { text, .. } if text == "หลักสูตร"));
        assert_eq!(doc.language.as_deref(), Some("tha"));
    }

    #[tokio::test]
    async fn a_heading_style_is_recognised_however_it_is_spelled() {
        assert_eq!(heading_level("Heading1"), Some(1));
        assert_eq!(heading_level("heading 3"), Some(3));
        assert_eq!(heading_level("Heading-6"), Some(6));
        assert_eq!(heading_level("Heading7"), None, "Word stops at six");
        assert_eq!(heading_level("Normal"), None);
        assert_eq!(heading_level("HeadingChar"), None);
    }

    #[tokio::test]
    async fn a_zip_that_is_not_a_word_document_is_refused_for_that_reason() {
        // Every zip starts with the same four bytes, so this is exactly the
        // case magic bytes cannot catch.
        let not_word = archive(&[("hello.txt", "not a document".to_owned())]);
        let err = DocxParser.parse(input(not_word)).await.unwrap_err();

        assert!(
            err.to_string().contains(error_codes::UNSUPPORTED_FILE_TYPE),
            "got {err}"
        );
        assert!(err.to_string().contains(DOCUMENT_PART), "got {err}");
    }

    #[tokio::test]
    async fn something_that_is_not_a_zip_at_all_is_refused() {
        let err = DocxParser
            .parse(input(b"just some text".to_vec()))
            .await
            .unwrap_err();
        assert!(err.to_string().contains(error_codes::UNSUPPORTED_FILE_TYPE));
    }

    #[tokio::test]
    async fn an_empty_document_is_refused_with_a_reason() {
        let err = DocxParser.parse(input(docx(""))).await.unwrap_err();
        assert!(err.to_string().contains(error_codes::NO_EXTRACTABLE_TEXT));
    }

    #[test]
    fn the_parser_only_claims_word_documents() {
        assert!(DocxParser.supports(SourceType::Docx));
        assert!(!DocxParser.supports(SourceType::Pdf));
    }
}
