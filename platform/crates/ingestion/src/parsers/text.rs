//! Plain text and Markdown.
//!
//! Markdown gets its own parser rather than being treated as text because its
//! headings are the document's structure, and that structure is what makes a
//! retrieved paragraph make sense on its own.

use anthovai_core::{DomainError, Result};
use anthovai_knowledge::SourceType;
use async_trait::async_trait;
use pulldown_cmark::{Event, HeadingLevel, Parser as MarkdownEvents, Tag, TagEnd};

use crate::chunker::{Block, ParsedDocument};
use crate::normalize::normalize;
use crate::{error_codes, ParseInput, Parser};

/// Paragraphs separated by blank lines, and nothing else claimed about them.
pub struct TextParser;

#[async_trait]
impl Parser for TextParser {
    fn supports(&self, source_type: SourceType) -> bool {
        matches!(source_type, SourceType::Txt | SourceType::Text)
    }

    async fn parse(&self, input: ParseInput) -> Result<ParsedDocument> {
        let text = decode(&input.bytes)?;
        let text = normalize(&text);

        if text.trim().is_empty() {
            return Err(empty());
        }

        let blocks = text
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| Block::Paragraph {
                text: p.to_owned(),
                page: None,
            })
            .collect();

        Ok(ParsedDocument {
            title: input.title(),
            language: detect_language(&text),
            blocks,
        })
    }
}

/// Markdown, keeping the heading hierarchy.
pub struct MarkdownParser;

#[async_trait]
impl Parser for MarkdownParser {
    fn supports(&self, source_type: SourceType) -> bool {
        matches!(source_type, SourceType::Md)
    }

    async fn parse(&self, input: ParseInput) -> Result<ParsedDocument> {
        let text = decode(&input.bytes)?;
        let blocks = markdown_blocks(&text);

        if blocks.is_empty() {
            return Err(empty());
        }

        Ok(ParsedDocument {
            title: input.title(),
            language: detect_language(&text),
            blocks,
        })
    }
}

/// Walk the Markdown events, collecting headings and the prose under them.
///
/// Inline formatting is dropped: what is being indexed is meaning, and `**bold**`
/// markers in a chunk only cost tokens and confuse a match.
fn markdown_blocks(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut buffer = String::new();
    let mut heading: Option<u8> = None;

    let flush_paragraph = |buffer: &mut String, blocks: &mut Vec<Block>| {
        let text = normalize(buffer);
        if !text.trim().is_empty() {
            blocks.push(Block::Paragraph { text, page: None });
        }
        buffer.clear();
    };

    for event in MarkdownEvents::new(markdown) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_paragraph(&mut buffer, &mut blocks);
                heading = Some(heading_depth(level));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = heading.take() {
                    let text = buffer.trim().to_owned();
                    if !text.is_empty() {
                        blocks.push(Block::Heading { level, text });
                    }
                }
                buffer.clear();
            }

            Event::Start(Tag::Paragraph | Tag::Item) => {
                if heading.is_none() {
                    flush_paragraph(&mut buffer, &mut blocks);
                }
            }
            Event::End(TagEnd::Paragraph | TagEnd::Item) => {
                if heading.is_none() {
                    flush_paragraph(&mut buffer, &mut blocks);
                }
            }

            // Code blocks are content: a configuration snippet in a handbook is
            // often exactly what someone is looking for.
            Event::Text(text) | Event::Code(text) => buffer.push_str(&text),
            Event::SoftBreak | Event::HardBreak => buffer.push(' '),

            _ => {}
        }
    }

    flush_paragraph(&mut buffer, &mut blocks);
    blocks
}

fn heading_depth(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Bytes to text.
///
/// A file that is not UTF-8 is refused rather than guessed at: a mis-decoded
/// document becomes chunks of nonsense that are embedded, stored, and quietly
/// retrieved for months.
pub fn decode(bytes: &[u8]) -> Result<String> {
    // A UTF-8 byte order mark is common from Windows editors and is not content.
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);

    String::from_utf8(bytes.to_vec()).map_err(|_| {
        DomainError::validation(format!(
            "{}: the file is not valid UTF-8 text",
            error_codes::NO_EXTRACTABLE_TEXT
        ))
    })
}

/// Best-effort language detection, used for display and future per-language
/// handling. Short documents are left unlabelled rather than guessed at.
pub fn detect_language(text: &str) -> Option<String> {
    if text.chars().count() < 20 {
        return None;
    }
    whatlang::detect(text)
        .filter(|info| info.is_reliable())
        .map(|info| info.lang().code().to_owned())
}

fn empty() -> DomainError {
    DomainError::validation(format!(
        "{}: the file contains no text",
        error_codes::NO_EXTRACTABLE_TEXT
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(bytes: &[u8], title: &str) -> ParseInput {
        ParseInput {
            bytes: bytes.to_vec(),
            source_type: SourceType::Md,
            filename: Some(title.to_owned()),
            source_url: None,
        }
    }

    #[tokio::test]
    async fn text_becomes_paragraphs() {
        let doc = TextParser
            .parse(input(b"First paragraph.\n\nSecond paragraph.", "notes.txt"))
            .await
            .unwrap();

        assert_eq!(doc.blocks.len(), 2);
        assert!(
            matches!(&doc.blocks[0], Block::Paragraph { text, .. } if text == "First paragraph.")
        );
    }

    #[tokio::test]
    async fn an_empty_file_is_refused_with_a_reason() {
        let err = TextParser
            .parse(input(b"   \n\n  ", "empty.txt"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains(error_codes::NO_EXTRACTABLE_TEXT));
    }

    #[tokio::test]
    async fn a_file_that_is_not_utf8_is_refused_rather_than_guessed_at() {
        // Latin-1 bytes. Decoding these as UTF-8 would produce nonsense that
        // gets embedded and retrieved for months before anyone notices.
        let err = TextParser
            .parse(input(&[0x48, 0xE9, 0x6C, 0x6C, 0x6F], "mystery.txt"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains(error_codes::NO_EXTRACTABLE_TEXT));
    }

    #[tokio::test]
    async fn a_byte_order_mark_is_not_content() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("Hello from Windows.".as_bytes());

        let doc = TextParser.parse(input(&bytes, "notes.txt")).await.unwrap();
        let Block::Paragraph { text, .. } = &doc.blocks[0] else {
            panic!("expected a paragraph");
        };
        assert!(text.starts_with("Hello"), "got {text:?}");
    }

    #[tokio::test]
    async fn markdown_keeps_its_heading_hierarchy() {
        let markdown = "# Programs\n\n## Rust Programming\n\nRuns for twelve weeks.\n";
        let doc = MarkdownParser
            .parse(input(markdown.as_bytes(), "handbook.md"))
            .await
            .unwrap();

        assert!(matches!(
            &doc.blocks[0],
            Block::Heading { level: 1, text } if text == "Programs"
        ));
        assert!(matches!(
            &doc.blocks[1],
            Block::Heading { level: 2, text } if text == "Rust Programming"
        ));
        assert!(matches!(&doc.blocks[2], Block::Paragraph { .. }));
    }

    #[tokio::test]
    async fn inline_formatting_is_dropped_but_the_words_survive() {
        let doc = MarkdownParser
            .parse(input(b"The **Rust** course is _twelve_ weeks.", "x.md"))
            .await
            .unwrap();

        let Block::Paragraph { text, .. } = &doc.blocks[0] else {
            panic!("expected a paragraph");
        };
        assert_eq!(text, "The Rust course is twelve weeks.");
    }

    #[tokio::test]
    async fn list_items_become_their_own_paragraphs() {
        let markdown = "## Requirements\n\n- A laptop\n- Some patience\n";
        let doc = MarkdownParser
            .parse(input(markdown.as_bytes(), "x.md"))
            .await
            .unwrap();

        let paragraphs: Vec<&String> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { text, .. } => Some(text),
                _ => None,
            })
            .collect();

        assert_eq!(paragraphs.len(), 2, "got {paragraphs:?}");
    }

    #[tokio::test]
    async fn code_blocks_are_kept_as_content() {
        let markdown = "# Setup\n\n```bash\ncargo run --bin anthovai-api\n```\n";
        let doc = MarkdownParser
            .parse(input(markdown.as_bytes(), "x.md"))
            .await
            .unwrap();

        let has_command = doc
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Paragraph { text, .. } if text.contains("cargo run")));
        assert!(has_command, "a command in a handbook is often the answer");
    }

    #[tokio::test]
    async fn thai_markdown_survives_intact() {
        let markdown = "# หลักสูตร\n\nหลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์\n";
        let doc = MarkdownParser
            .parse(input(markdown.as_bytes(), "th.md"))
            .await
            .unwrap();

        assert!(matches!(&doc.blocks[0], Block::Heading { text, .. } if text == "หลักสูตร"));
        assert_eq!(doc.language.as_deref(), Some("tha"));
    }

    #[test]
    fn a_short_string_is_not_labelled_with_a_language() {
        assert_eq!(detect_language("hi"), None);
    }

    #[test]
    fn parsers_only_claim_what_they_can_read() {
        assert!(TextParser.supports(SourceType::Txt));
        assert!(!TextParser.supports(SourceType::Md));
        assert!(MarkdownParser.supports(SourceType::Md));
        assert!(!MarkdownParser.supports(SourceType::Pdf));
    }
}
