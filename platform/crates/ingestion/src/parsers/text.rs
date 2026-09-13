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

/// Paragraphs separated by blank lines — unless the text has headings, in which
/// case they are kept.
///
/// Pasting into the dashboard produces `SourceType::Text`, because the type is
/// decided from the shape of the request before any bytes are read. That meant
/// a customer who pasted a structured document had its structure thrown away:
/// every `## heading` became an ordinary paragraph, the chunker never saw a
/// heading to split on, and a document covering three subjects became one
/// chunk whose embedding was near none of them.
///
/// Measured on a clinic's opening hours: asking "what time does it open on
/// Saturday" retrieved nothing at all, three times out of three, while the
/// answer sat verbatim in the text.
pub struct TextParser;

/// Whether this text is using Markdown headings.
///
/// An ATX heading and nothing looser: `#` through `######` at the start of a
/// line, followed by a space and something. `#1`, `#hashtag` and a `#` comment
/// do not qualify, which is the point — plain text that happens to contain a
/// hash should stay plain text.
fn has_headings(text: &str) -> bool {
    text.lines().any(|line| {
        let hashes = line.chars().take_while(|c| *c == '#').count();
        (1..=6).contains(&hashes)
            && line[hashes..].starts_with(' ')
            && !line[hashes + 1..].trim().is_empty()
    })
}

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

        let blocks = if has_headings(&text) {
            markdown_blocks(&text)
        } else {
            text.split("\n\n")
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| Block::Paragraph {
                    text: p.to_owned(),
                    page: None,
                })
                .collect()
        };

        if blocks.is_empty() {
            return Err(empty());
        }

        Ok(ParsedDocument {
            title: input.title(),
            language: detect_language(&text),
            blocks,
            // Text the file carried, not a transcription of an image.
            ocr: false,
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
            // Text the file carried, not a transcription of an image.
            ocr: false,
        })
    }
}

/// Walk the Markdown events, collecting headings and the prose under them.
///
/// Inline formatting is dropped: what is being indexed is meaning, and `**bold**`
/// markers in a chunk only cost tokens and confuse a match.
pub(crate) fn markdown_blocks(markdown: &str) -> Vec<Block> {
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

    #[test]
    fn a_hash_that_is_not_a_heading_leaves_the_text_alone() {
        // The reason `has_headings` is strict. None of these should turn a
        // plain document into a Markdown one.
        assert!(!has_headings("issue #42 is still open"));
        assert!(!has_headings("#hashtag"));
        assert!(!has_headings("#"));
        assert!(!has_headings("#   "));
        assert!(!has_headings("####### seven is not a heading"));
        assert!(!has_headings("no hashes at all"));
    }

    #[test]
    fn an_atx_heading_is_recognised() {
        assert!(has_headings("# Title"));
        assert!(has_headings("intro\n\n## Section\n\nbody"));
        assert!(has_headings("###### deep"));
    }

    #[tokio::test]
    async fn pasted_text_with_headings_keeps_them() {
        // The bug this exists for: a pasted document with three sections used
        // to arrive as one undifferentiated run of paragraphs, so the chunker
        // had nothing to split on.
        let pasted = "# นโยบายคลินิก\n\n                      ## เวลาทำการ\n\n                      จันทร์ถึงศุกร์ 10:00 ถึง 20:00 น.\n\n                      ## การนัดหมาย\n\n                      โทร 02-259-8800\n\n                      ## สิทธิ\n\n                      ใช้ประกันสังคมได้";

        let parsed = TextParser
            .parse(input(pasted.as_bytes(), "pasted.txt"))
            .await
            .expect("parse");

        let headings: Vec<&str> = parsed
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(
            headings,
            vec!["นโยบายคลินิก", "เวลาทำการ", "การนัดหมาย", "สิทธิ"],
            "every heading in pasted text must survive as a heading"
        );
    }

    #[tokio::test]
    async fn pasted_text_without_headings_is_still_paragraphs() {
        let parsed = TextParser
            .parse(input(
                "first thought\n\nsecond thought".as_bytes(),
                "pasted.txt",
            ))
            .await
            .expect("parse");

        assert_eq!(parsed.blocks.len(), 2);
        assert!(parsed
            .blocks
            .iter()
            .all(|b| matches!(b, Block::Paragraph { .. })));
    }

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
