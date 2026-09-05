//! Splitting a parsed document into chunks.
//!
//! Chunks carry a contextual header naming the document and the heading path
//! they came from. That header is what makes a bare paragraph retrievable and
//! what makes a citation readable.

use serde::{Deserialize, Serialize};

/// One unit of parsed content, before chunking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
        page: Option<u32>,
    },
    /// A JSON object or CSV row, already flattened to `field: value` lines.
    Record {
        key: String,
        text: String,
    },
}

#[derive(Clone, Debug, Default)]
pub struct ParsedDocument {
    pub title: String,
    pub language: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Copy, Debug)]
pub struct ChunkConfig {
    pub target_tokens: usize,
    pub overlap_tokens: usize,
    pub max_tokens: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            target_tokens: 500,
            overlap_tokens: 80,
            max_tokens: 600,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChunkDraft {
    pub index: usize,
    /// Header plus body: exactly what gets embedded.
    pub content: String,
    pub token_count: usize,
    pub heading_path: Vec<String>,
    pub page: Option<u32>,
    pub record_key: Option<String>,
}

/// A rough token count: good enough for sizing chunks, and it costs nothing.
/// Real accounting uses the provider's reported usage.
pub fn estimate_tokens(text: &str) -> usize {
    let words = text.split_whitespace().count();
    // Thai and other scripts without spaces need the character-based floor.
    let chars = text.chars().count();
    words.max(chars / 4).max(1)
}

/// Split a parsed document. Headings set the context for the paragraphs that
/// follow; records become one chunk each.
pub fn chunk(doc: &ParsedDocument, config: &ChunkConfig) -> Vec<ChunkDraft> {
    let mut out = Vec::new();
    let mut heading_path: Vec<String> = Vec::new();
    let mut pending: Vec<(String, Option<u32>)> = Vec::new();

    let flush = |pending: &mut Vec<(String, Option<u32>)>,
                 heading_path: &[String],
                 out: &mut Vec<ChunkDraft>| {
        if pending.is_empty() {
            return;
        }
        let page = pending.first().and_then(|(_, p)| *p);
        let body = pending
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        pending.clear();

        for piece in split_to_size(&body, config) {
            push_chunk(out, &doc.title, heading_path, &piece, page, None);
        }
    };

    for block in &doc.blocks {
        match block {
            Block::Heading { level, text } => {
                flush(&mut pending, &heading_path, &mut out);
                heading_path.truncate((*level as usize).saturating_sub(1));
                heading_path.push(text.clone());
            }
            Block::Paragraph { text, page } => {
                pending.push((text.clone(), *page));
                let so_far: usize = pending.iter().map(|(t, _)| estimate_tokens(t)).sum();
                if so_far >= config.target_tokens {
                    flush(&mut pending, &heading_path, &mut out);
                }
            }
            Block::Record { key, text } => {
                flush(&mut pending, &heading_path, &mut out);
                for piece in split_to_size(text, config) {
                    push_chunk(
                        &mut out,
                        &doc.title,
                        &heading_path,
                        &piece,
                        None,
                        Some(key.clone()),
                    );
                }
            }
        }
    }
    flush(&mut pending, &heading_path, &mut out);
    out
}

fn push_chunk(
    out: &mut Vec<ChunkDraft>,
    title: &str,
    heading_path: &[String],
    body: &str,
    page: Option<u32>,
    record_key: Option<String>,
) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    let content = format!("{}\n{}", header(title, heading_path, &record_key), body);
    out.push(ChunkDraft {
        index: out.len(),
        token_count: estimate_tokens(&content),
        content,
        heading_path: heading_path.to_vec(),
        page,
        record_key,
    });
}

fn header(title: &str, heading_path: &[String], record_key: &Option<String>) -> String {
    let mut parts = vec![format!("Document: {title}")];
    if !heading_path.is_empty() {
        parts.push(format!("Section: {}", heading_path.join(" > ")));
    }
    if let Some(key) = record_key {
        parts.push(format!("Record: {key}"));
    }
    format!("[{}]", parts.join(" > "))
}

/// Split text that is over the size limit, keeping an overlap so a sentence
/// that straddles a boundary is still findable from either side.
fn split_to_size(text: &str, config: &ChunkConfig) -> Vec<String> {
    if estimate_tokens(text) <= config.max_tokens {
        return vec![text.to_owned()];
    }

    let sentences = split_sentences(text);
    let mut pieces = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_tokens = 0;

    for sentence in sentences {
        let tokens = estimate_tokens(&sentence);
        if current_tokens + tokens > config.target_tokens && !current.is_empty() {
            pieces.push(current.join(" "));
            // Carry the tail of this piece into the next one.
            let mut carried = Vec::new();
            let mut carried_tokens = 0;
            for previous in current.iter().rev() {
                let t = estimate_tokens(previous);
                if carried_tokens + t > config.overlap_tokens {
                    break;
                }
                carried_tokens += t;
                carried.insert(0, previous.clone());
            }
            current = carried;
            current_tokens = carried_tokens;
        }
        current_tokens += tokens;
        current.push(sentence);
    }
    if !current.is_empty() {
        pieces.push(current.join(" "));
    }
    pieces
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '\n' | '。' | '๚') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_owned());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph(text: &str) -> Block {
        Block::Paragraph {
            text: text.to_owned(),
            page: Some(1),
        }
    }

    #[test]
    fn every_chunk_carries_the_document_title() {
        let doc = ParsedDocument {
            title: "Course Catalog 2026".into(),
            blocks: vec![paragraph("Rust runs for 12 weeks.")],
            ..Default::default()
        };
        let chunks = chunk(&doc, &ChunkConfig::default());
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("Document: Course Catalog 2026"));
        assert!(chunks[0].content.contains("12 weeks"));
    }

    #[test]
    fn headings_become_the_section_path() {
        let doc = ParsedDocument {
            title: "Handbook".into(),
            blocks: vec![
                Block::Heading {
                    level: 1,
                    text: "Programs".into(),
                },
                Block::Heading {
                    level: 2,
                    text: "Rust Programming".into(),
                },
                paragraph("Twelve weeks, evenings."),
            ],
            ..Default::default()
        };
        let chunks = chunk(&doc, &ChunkConfig::default());
        assert_eq!(chunks[0].heading_path, vec!["Programs", "Rust Programming"]);
        assert!(chunks[0]
            .content
            .contains("Section: Programs > Rust Programming"));
    }

    #[test]
    fn a_sibling_heading_replaces_the_previous_one() {
        let doc = ParsedDocument {
            title: "Handbook".into(),
            blocks: vec![
                Block::Heading {
                    level: 1,
                    text: "Admissions".into(),
                },
                paragraph("Apply online."),
                Block::Heading {
                    level: 1,
                    text: "Fees".into(),
                },
                paragraph("Pay by term."),
            ],
            ..Default::default()
        };
        let chunks = chunk(&doc, &ChunkConfig::default());
        assert_eq!(chunks[0].heading_path, vec!["Admissions"]);
        assert_eq!(chunks[1].heading_path, vec!["Fees"]);
    }

    #[test]
    fn each_record_becomes_its_own_chunk() {
        let doc = ParsedDocument {
            title: "courses.json".into(),
            blocks: vec![
                Block::Record {
                    key: "rust-101".into(),
                    text: "course: Rust\nduration: 12 weeks".into(),
                },
                Block::Record {
                    key: "go-101".into(),
                    text: "course: Go\nduration: 8 weeks".into(),
                },
            ],
            ..Default::default()
        };
        let chunks = chunk(&doc, &ChunkConfig::default());
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].record_key.as_deref(), Some("rust-101"));
        assert!(chunks[0].content.contains("Record: rust-101"));
    }

    #[test]
    fn long_text_is_split_and_stays_under_the_limit() {
        let long = "This sentence is here to make the document long. ".repeat(200);
        let doc = ParsedDocument {
            title: "Long".into(),
            blocks: vec![paragraph(&long)],
            ..Default::default()
        };
        let config = ChunkConfig::default();
        let chunks = chunk(&doc, &config);

        assert!(chunks.len() > 1, "a long document must be split");
        for c in &chunks {
            assert!(
                c.token_count <= config.max_tokens * 2,
                "chunk of {} tokens is too large",
                c.token_count
            );
        }
    }

    #[test]
    fn chunk_indices_are_contiguous() {
        let long = "Sentence number one. ".repeat(400);
        let doc = ParsedDocument {
            title: "Long".into(),
            blocks: vec![paragraph(&long)],
            ..Default::default()
        };
        let chunks = chunk(&doc, &ChunkConfig::default());
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.index, i);
        }
    }

    #[test]
    fn empty_paragraphs_produce_no_chunks() {
        let doc = ParsedDocument {
            title: "Empty".into(),
            blocks: vec![paragraph("   "), paragraph("")],
            ..Default::default()
        };
        assert!(chunk(&doc, &ChunkConfig::default()).is_empty());
    }

    #[test]
    fn thai_text_is_not_counted_as_a_single_token() {
        let thai = "หลักสูตรนี้ใช้เวลาเรียนสิบสองสัปดาห์";
        assert!(
            estimate_tokens(thai) > 5,
            "spaceless scripts need the character floor"
        );
    }

    #[test]
    fn page_numbers_survive_into_the_chunk() {
        let doc = ParsedDocument {
            title: "Catalog".into(),
            blocks: vec![Block::Paragraph {
                text: "Rust runs for 12 weeks.".into(),
                page: Some(4),
            }],
            ..Default::default()
        };
        assert_eq!(chunk(&doc, &ChunkConfig::default())[0].page, Some(4));
    }
}
