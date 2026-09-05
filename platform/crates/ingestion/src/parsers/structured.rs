//! JSON and CSV: data, not prose.
//!
//! A course catalogue or a price list is a table, and treating it as prose
//! produces chunks that straddle two products and answer for neither. So each
//! record becomes its own chunk, flattened to `field: value` lines, keyed by
//! whatever the data itself calls it. That key is what a citation shows and
//! what a metadata filter will match on later.

use std::collections::BTreeMap;

use anthovai_core::{DomainError, Result};
use anthovai_knowledge::SourceType;
use async_trait::async_trait;
use serde_json::Value;

use crate::chunker::{Block, ParsedDocument};
use crate::normalize::normalize;
use crate::parsers::text::{decode, detect_language};
use crate::{error_codes, ParseInput, Parser};

/// How many records one file may hold.
///
/// A file past this is a database export rather than a knowledge base, and
/// embedding it would cost more than the customer expects to be charged.
const MAX_RECORDS: usize = 100_000;

/// How deep a nested object is flattened. Beyond this the path is longer than
/// the value and stops being worth embedding.
const MAX_DEPTH: usize = 6;

/// Field names that name a record, in the order they are tried.
const KEY_FIELDS: &[&str] = &["id", "key", "slug", "code", "sku", "name", "title"];

// ---- JSON -----------------------------------------------------------------

pub struct JsonParser;

#[async_trait]
impl Parser for JsonParser {
    fn supports(&self, source_type: SourceType) -> bool {
        matches!(source_type, SourceType::Json)
    }

    async fn parse(&self, input: ParseInput) -> Result<ParsedDocument> {
        let text = decode(&input.bytes)?;
        let value: Value = serde_json::from_str(&text).map_err(|e| {
            DomainError::validation(format!(
                "{}: the file is not valid JSON ({e})",
                error_codes::NO_EXTRACTABLE_TEXT
            ))
        })?;

        let records = records_from(&value);
        if records.is_empty() {
            return Err(empty());
        }
        if records.len() > MAX_RECORDS {
            return Err(too_many(records.len()));
        }

        let blocks: Vec<Block> = records
            .into_iter()
            .filter_map(|(key, value)| {
                let text = flatten(&value);
                (!text.trim().is_empty()).then_some(Block::Record { key, text })
            })
            .collect();

        if blocks.is_empty() {
            return Err(empty());
        }

        Ok(ParsedDocument {
            title: input.title(),
            language: detect_language(&sample_of(&blocks)),
            blocks,
        })
    }
}

/// Split a JSON document into records, following §A.6 of the RAG flow.
///
/// The shape decides: an array is a list of records, an object whose values are
/// all objects is a keyed list of records, and anything else is one record.
fn records_from(value: &Value) -> Vec<(String, Value)> {
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, item)| (record_key(item, i), item.clone()))
            .collect(),

        Value::Object(map) if map.values().all(|v| v.is_object()) && !map.is_empty() => map
            .iter()
            .map(|(key, item)| (key.clone(), item.clone()))
            .collect(),

        Value::Object(map) if map.is_empty() => Vec::new(),

        // A single object, or a bare scalar. One record either way.
        other => vec![(record_key(other, 0), other.clone())],
    }
}

/// What to call a record.
///
/// A name the data already carries is worth far more in a citation than
/// "record 41", so those fields are tried first.
fn record_key(value: &Value, index: usize) -> String {
    if let Value::Object(map) = value {
        for field in KEY_FIELDS {
            match map.get(*field) {
                Some(Value::String(s)) if !s.trim().is_empty() => return s.trim().to_owned(),
                Some(Value::Number(n)) => return n.to_string(),
                _ => {}
            }
        }
    }
    format!("record {}", index + 1)
}

/// One `field: value` line per leaf, nested paths joined with dots.
fn flatten(value: &Value) -> String {
    let mut lines = Vec::new();
    walk(value, &mut Vec::new(), 0, &mut lines);
    normalize(&lines.join("\n"))
}

fn walk(value: &Value, path: &mut Vec<String>, depth: usize, out: &mut Vec<String>) {
    // A leaf is written out; a container past the depth limit is written as the
    // text it would render to, rather than dropped — a truncated answer beats a
    // silently missing one.
    if depth >= MAX_DEPTH {
        push_leaf(
            path,
            &scalar(value).unwrap_or_else(|| value.to_string()),
            out,
        );
        return;
    }

    match value {
        Value::Object(map) => {
            for (key, child) in map {
                path.push(key.clone());
                walk(child, path, depth + 1, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            // A list of scalars reads better on one line than as `tags.0`,
            // `tags.1`, and it is what a question about "tags" will match.
            if let Some(joined) = scalars_joined(items) {
                push_leaf(path, &joined, out);
                return;
            }
            for (i, child) in items.iter().enumerate() {
                path.push((i + 1).to_string());
                walk(child, path, depth + 1, out);
                path.pop();
            }
        }
        other => {
            if let Some(text) = scalar(other) {
                push_leaf(path, &text, out);
            }
        }
    }
}

fn push_leaf(path: &[String], text: &str, out: &mut Vec<String>) {
    if text.trim().is_empty() {
        return;
    }
    let name = if path.is_empty() {
        "value".to_owned()
    } else {
        path.join(".")
    };
    out.push(format!("{name}: {text}"));
}

/// A scalar as the text a reader would expect. `null` is nothing to say, so it
/// produces no line at all rather than the word "null".
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        _ => None,
    }
}

fn scalars_joined(items: &[Value]) -> Option<String> {
    if items.is_empty() {
        return Some(String::new());
    }
    let parts: Option<Vec<String>> = items.iter().map(scalar).collect();
    Some(parts?.join(", "))
}

// ---- CSV ------------------------------------------------------------------

pub struct CsvParser;

#[async_trait]
impl Parser for CsvParser {
    fn supports(&self, source_type: SourceType) -> bool {
        matches!(source_type, SourceType::Csv)
    }

    async fn parse(&self, input: ParseInput) -> Result<ParsedDocument> {
        let text = decode(&input.bytes)?;

        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .delimiter(delimiter_of(&text))
            .from_reader(text.as_bytes());

        let headers: Vec<String> = reader
            .headers()
            .map_err(malformed)?
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let name = name.trim();
                if name.is_empty() {
                    format!("column {}", i + 1)
                } else {
                    name.to_owned()
                }
            })
            .collect();

        if headers.is_empty() {
            return Err(empty());
        }

        let mut blocks = Vec::new();
        for (index, record) in reader.records().enumerate() {
            if blocks.len() >= MAX_RECORDS {
                return Err(too_many(MAX_RECORDS + 1));
            }

            // One malformed row in a ten-thousand-row export should not cost
            // the customer the other 9,999.
            let Ok(record) = record else { continue };

            let fields: Vec<String> = headers
                .iter()
                .zip(record.iter())
                .filter(|(_, value)| !value.trim().is_empty())
                .map(|(name, value)| format!("{name}: {}", value.trim()))
                .collect();

            if fields.is_empty() {
                continue;
            }

            blocks.push(Block::Record {
                key: row_key(&headers, &record, index),
                text: normalize(&fields.join("\n")),
            });
        }

        if blocks.is_empty() {
            return Err(empty());
        }

        Ok(ParsedDocument {
            title: input.title(),
            language: detect_language(&sample_of(&blocks)),
            blocks,
        })
    }
}

/// Which character separates the columns.
///
/// Exports from Thai Excel are semicolon-separated often enough that guessing
/// wrong would turn every row into one unsearchable field. Decided from the
/// header line, where the right separator appears the most.
fn delimiter_of(text: &str) -> u8 {
    let header = text.lines().next().unwrap_or_default();
    b",;\t"
        .iter()
        .copied()
        .max_by_key(|d| header.bytes().filter(|b| b == d).count())
        .filter(|d| header.bytes().any(|b| b == *d))
        .unwrap_or(b',')
}

fn row_key(headers: &[String], record: &csv::StringRecord, index: usize) -> String {
    for field in KEY_FIELDS {
        let position = headers.iter().position(|h| h.eq_ignore_ascii_case(field));
        if let Some(value) = position.and_then(|i| record.get(i)) {
            if !value.trim().is_empty() {
                return value.trim().to_owned();
            }
        }
    }
    format!("row {}", index + 1)
}

fn malformed(error: csv::Error) -> DomainError {
    DomainError::validation(format!(
        "{}: the file is not readable as CSV ({error})",
        error_codes::NO_EXTRACTABLE_TEXT
    ))
}

// ---- shared ---------------------------------------------------------------

/// Enough text for language detection without walking a hundred thousand rows.
fn sample_of(blocks: &[Block]) -> String {
    blocks
        .iter()
        .take(20)
        .filter_map(|b| match b {
            Block::Record { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn empty() -> DomainError {
    DomainError::validation(format!(
        "{}: the file holds no records",
        error_codes::NO_EXTRACTABLE_TEXT
    ))
}

fn too_many(found: usize) -> DomainError {
    DomainError::validation(format!(
        "{}: {found} records is past the limit of {MAX_RECORDS}. Split the file, \
         or load it through the API a page at a time.",
        error_codes::FILE_TOO_LARGE
    ))
}

/// Field names present across records, for the metadata filters that will use
/// them. Cheap to compute here, and impossible to recover once chunked.
pub fn field_names(blocks: &[Block]) -> Vec<String> {
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    for block in blocks.iter().take(200) {
        if let Block::Record { text, .. } = block {
            for line in text.lines() {
                if let Some((name, _)) = line.split_once(": ") {
                    seen.insert(name.to_owned(), ());
                }
            }
        }
    }
    seen.into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(bytes: &[u8], source_type: SourceType, title: &str) -> ParseInput {
        ParseInput {
            bytes: bytes.to_vec(),
            source_type,
            filename: Some(title.to_owned()),
            source_url: None,
        }
    }

    fn records(doc: &ParsedDocument) -> Vec<(&str, &str)> {
        doc.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Record { key, text } => Some((key.as_str(), text.as_str())),
                _ => None,
            })
            .collect()
    }

    // ---- JSON ----

    #[tokio::test]
    async fn an_array_becomes_one_record_per_item() {
        let json = br#"[
            {"id": "rust-101", "course": "Rust Programming", "duration": "12 weeks"},
            {"id": "go-101", "course": "Go Programming", "duration": "8 weeks"}
        ]"#;
        let doc = JsonParser
            .parse(input(json, SourceType::Json, "courses.json"))
            .await
            .unwrap();

        let records = records(&doc);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, "rust-101", "the id should name the record");
        assert!(records[0].1.contains("course: Rust Programming"));
        assert!(
            !records[0].1.contains("Go Programming"),
            "one record must never carry another's fields"
        );
    }

    #[tokio::test]
    async fn an_object_of_objects_is_keyed_by_its_own_keys() {
        let json = br#"{"rust-101": {"course": "Rust"}, "go-101": {"course": "Go"}}"#;
        let doc = JsonParser
            .parse(input(json, SourceType::Json, "courses.json"))
            .await
            .unwrap();

        let keys: Vec<&str> = records(&doc).iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec!["go-101", "rust-101"], "sorted by key");
    }

    #[tokio::test]
    async fn a_single_object_is_one_record() {
        let json = br#"{"name": "ABC School", "founded": 1997}"#;
        let doc = JsonParser
            .parse(input(json, SourceType::Json, "school.json"))
            .await
            .unwrap();

        let records = records(&doc);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "ABC School");
        assert!(records[0].1.contains("founded: 1997"));
    }

    #[tokio::test]
    async fn nested_objects_are_flattened_to_dotted_paths() {
        let json = br#"{"id": "x", "price": {"amount": 4900, "currency": "THB"}}"#;
        let doc = JsonParser
            .parse(input(json, SourceType::Json, "x.json"))
            .await
            .unwrap();

        let text = records(&doc)[0].1;
        assert!(text.contains("price.amount: 4900"), "got {text:?}");
        assert!(text.contains("price.currency: THB"), "got {text:?}");
    }

    #[tokio::test]
    async fn a_list_of_tags_stays_on_one_line() {
        // `tags.1: async` would not match a question about tags; the joined
        // line does.
        let json = br#"{"id": "x", "tags": ["ownership", "borrowing", "async"]}"#;
        let doc = JsonParser
            .parse(input(json, SourceType::Json, "x.json"))
            .await
            .unwrap();

        assert!(
            records(&doc)[0]
                .1
                .contains("tags: ownership, borrowing, async"),
            "got {:?}",
            records(&doc)[0].1
        );
    }

    #[tokio::test]
    async fn a_null_field_says_nothing_rather_than_the_word_null() {
        let json = br#"{"id": "x", "teacher": null, "course": "Rust"}"#;
        let doc = JsonParser
            .parse(input(json, SourceType::Json, "x.json"))
            .await
            .unwrap();

        let text = records(&doc)[0].1;
        assert!(!text.contains("null"), "got {text:?}");
        assert!(text.contains("course: Rust"));
    }

    #[tokio::test]
    async fn thai_values_survive_and_are_detected() {
        let json = "[{\"id\":\"rust-101\",\"รายละเอียด\":\"หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์ \
                    เรียนช่วงเย็นวันธรรมดา ตั้งแต่หกโมงเย็นถึงสามทุ่ม\"}]";
        let doc = JsonParser
            .parse(input(json.as_bytes(), SourceType::Json, "th.json"))
            .await
            .unwrap();

        assert!(records(&doc)[0].1.contains("สัปดาห์"));
        assert_eq!(doc.language.as_deref(), Some("tha"));
    }

    #[tokio::test]
    async fn broken_json_is_refused_with_a_reason() {
        let err = JsonParser
            .parse(input(b"{not json", SourceType::Json, "x.json"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains(error_codes::NO_EXTRACTABLE_TEXT));
    }

    #[tokio::test]
    async fn an_empty_array_is_refused() {
        let err = JsonParser
            .parse(input(b"[]", SourceType::Json, "x.json"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains(error_codes::NO_EXTRACTABLE_TEXT));
    }

    // ---- CSV ----

    #[tokio::test]
    async fn a_csv_row_becomes_a_record_named_by_its_id() {
        let csv = b"id,course,duration\nrust-101,Rust Programming,12 weeks\ngo-101,Go,8 weeks\n";
        let doc = CsvParser
            .parse(input(csv, SourceType::Csv, "courses.csv"))
            .await
            .unwrap();

        let records = records(&doc);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, "rust-101");
        assert!(records[0].1.contains("course: Rust Programming"));
        assert!(records[0].1.contains("duration: 12 weeks"));
    }

    #[tokio::test]
    async fn a_semicolon_export_is_read_as_columns_not_as_one_field() {
        // What Excel writes on a Thai locale. Guessing comma here would make
        // every row a single unsearchable blob.
        let csv = "ชื่อ;ราคา\nหลักสูตร Rust;4900\n";
        let doc = CsvParser
            .parse(input(csv.as_bytes(), SourceType::Csv, "th.csv"))
            .await
            .unwrap();

        let text = records(&doc)[0].1;
        assert!(text.contains("ราคา: 4900"), "got {text:?}");
    }

    #[tokio::test]
    async fn an_empty_cell_produces_no_line() {
        let csv = b"id,course,teacher\nrust-101,Rust,\n";
        let doc = CsvParser
            .parse(input(csv, SourceType::Csv, "x.csv"))
            .await
            .unwrap();

        let text = records(&doc)[0].1;
        assert!(!text.contains("teacher"), "got {text:?}");
    }

    #[tokio::test]
    async fn a_row_with_no_id_column_is_numbered() {
        let csv = b"course,duration\nRust,12 weeks\n";
        let doc = CsvParser
            .parse(input(csv, SourceType::Csv, "x.csv"))
            .await
            .unwrap();
        assert_eq!(records(&doc)[0].0, "row 1");
    }

    #[tokio::test]
    async fn a_header_only_file_is_refused() {
        let err = CsvParser
            .parse(input(b"id,course\n", SourceType::Csv, "x.csv"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains(error_codes::NO_EXTRACTABLE_TEXT));
    }

    #[tokio::test]
    async fn a_quoted_comma_stays_inside_its_field() {
        let csv = b"id,description\nx,\"ownership, borrowing, async\"\n";
        let doc = CsvParser
            .parse(input(csv, SourceType::Csv, "x.csv"))
            .await
            .unwrap();

        assert!(records(&doc)[0]
            .1
            .contains("description: ownership, borrowing, async"));
    }

    #[test]
    fn field_names_are_collected_for_later_filtering() {
        let blocks = vec![
            Block::Record {
                key: "a".into(),
                text: "course: Rust\nprice: 4900".into(),
            },
            Block::Record {
                key: "b".into(),
                text: "course: Go\nteacher: Somchai".into(),
            },
        ];
        assert_eq!(field_names(&blocks), vec!["course", "price", "teacher"]);
    }

    #[test]
    fn parsers_only_claim_what_they_can_read() {
        assert!(JsonParser.supports(SourceType::Json));
        assert!(!JsonParser.supports(SourceType::Csv));
        assert!(CsvParser.supports(SourceType::Csv));
        assert!(!CsvParser.supports(SourceType::Json));
    }
}
