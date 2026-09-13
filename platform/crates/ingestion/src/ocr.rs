//! Text recovered from scans.
//!
//! `pdf-extract` reads the text a PDF carries; a scan carries none. For those
//! the pages go to the OCR sidecar — a small HTTP service on the same machine
//! running Typhoon OCR, a model trained on Thai documents — and what comes back
//! is Markdown, one string per page. It runs beside the worker rather than in
//! it because the model is a Python runtime on a GPU, and because keeping it
//! out of process means a crash in the model costs one document, not the queue.
//!
//! Nothing leaves the machine. The sidecar is reached over the loopback or the
//! compose network, never the internet. That is the promise the product is sold
//! on, and this module is where it is kept.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Deserialize;
use tracing::debug;

use crate::chunker::Block;
use crate::parsers::text::markdown_blocks;

/// A connection to the OCR sidecar.
pub struct OcrClient {
    http: reqwest::Client,
    base_url: String,
}

/// Why OCR did not produce text, and whether trying again could help.
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    /// The sidecar did not answer. The same file will very likely go through
    /// once it is back, so this is retried.
    #[error("the OCR service at {url} is not answering: {reason}")]
    Unavailable { url: String, reason: String },

    /// The sidecar looked at the file and refused it — pages too small to
    /// read, not a PDF, too many pages. Retrying changes nothing.
    #[error("the OCR service refused the file ({status}): {detail}")]
    Rejected { status: u16, detail: String },

    /// An answer that could not be read. A version mismatch between the
    /// worker and the sidecar, most likely — an operator's problem, not the
    /// customer's, so it is retried rather than pinned on the document.
    #[error("the OCR service answered with something unexpected: {0}")]
    Malformed(String),
}

impl OcrError {
    pub fn is_retryable(&self) -> bool {
        !matches!(self, Self::Rejected { .. })
    }
}

#[derive(Deserialize)]
struct OcrResponse {
    pages: Vec<OcrPage>,
}

#[derive(Deserialize)]
struct OcrPage {
    page: u32,
    markdown: String,
}

/// FastAPI wraps its reason in `{"detail": ...}`; that is the part worth
/// showing the customer.
#[derive(Deserialize)]
struct Rejection {
    detail: serde_json::Value,
}

impl OcrClient {
    /// `timeout` covers one whole document, every page of it. Typhoon OCR on a
    /// small GPU takes about a minute a page.
    pub fn new(base_url: impl Into<String>, timeout: Duration) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder().timeout(timeout).build()?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Whether the sidecar is up with its model loaded.
    pub async fn health(&self) -> Result<(), OcrError> {
        let response = self
            .http
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| self.unavailable(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(self.unavailable(format!("health returned {}", response.status())))
        }
    }

    /// The Markdown of every page of a PDF, in page order.
    pub async fn ocr_pdf(&self, pdf: &[u8]) -> Result<Vec<String>, OcrError> {
        self.post_ocr(serde_json::json!({ "pdf_b64": BASE64.encode(pdf) }))
            .await
    }

    /// The Markdown of one photograph or scan.
    ///
    /// The sidecar refuses a picture too small to read reliably — a phone
    /// photo taken from across the room — and that refusal is final: the
    /// remedy is a better picture, not a retry.
    pub async fn ocr_image(&self, image: &[u8]) -> Result<String, OcrError> {
        let mut pages = self
            .post_ocr(serde_json::json!({ "image_b64": BASE64.encode(image) }))
            .await?;
        match pages.len() {
            1 => Ok(pages.remove(0)),
            n => Err(OcrError::Malformed(format!(
                "expected one page for an image, got {n}"
            ))),
        }
    }

    async fn post_ocr(&self, body: serde_json::Value) -> Result<Vec<String>, OcrError> {
        let response = self
            .http
            .post(format!("{}/ocr", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| self.unavailable(e.to_string()))?;

        let status = response.status();
        if status.is_server_error() {
            return Err(self.unavailable(format!("returned {status}")));
        }
        if !status.is_success() {
            let raw = response.text().await.unwrap_or_default();
            let detail = serde_json::from_str::<Rejection>(&raw)
                .map(|r| match r.detail {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                })
                .unwrap_or(raw);
            return Err(OcrError::Rejected {
                status: status.as_u16(),
                detail,
            });
        }

        let parsed: OcrResponse = response
            .json()
            .await
            .map_err(|e| OcrError::Malformed(e.to_string()))?;

        let mut pages = parsed.pages;
        pages.sort_by_key(|p| p.page);
        debug!(pages = pages.len(), "pages recovered by OCR");

        Ok(pages.into_iter().map(|p| p.markdown).collect())
    }

    fn unavailable(&self, reason: String) -> OcrError {
        OcrError::Unavailable {
            url: self.base_url.clone(),
            reason,
        }
    }
}

/// What an OCR failure means for the document.
///
/// A refusal is about the file and is final. Everything else is about the
/// service and carries `ocr_unavailable`, which the pipeline retries — a scan
/// queued while the sidecar restarts must not fail for good.
pub(crate) fn to_domain_error(error: OcrError) -> anthovai_core::DomainError {
    use crate::error_codes;
    use anthovai_core::DomainError;

    match error {
        OcrError::Rejected { detail, .. } => DomainError::validation(format!(
            "{}: the OCR service refused this file: {detail}",
            error_codes::NO_EXTRACTABLE_TEXT
        )),
        other => DomainError::rejected(error_codes::OCR_UNAVAILABLE, other.to_string()),
    }
}

impl std::fmt::Debug for OcrClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OcrClient")
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// Blocks from OCR'd pages, every paragraph carrying its page number.
///
/// The model writes its Markdown with tables as HTML, which the Markdown
/// parser would drop whole — and on an invoice the table *is* the document.
/// So the tables are flattened to text first: one row per paragraph, cells
/// separated by ` | `. Not pretty, but every number stays retrievable.
pub fn blocks_from_pages(pages: &[String]) -> Vec<Block> {
    let mut blocks = Vec::new();

    for (index, page) in pages.iter().enumerate() {
        let page_number = (index + 1) as u32;
        let text = tables_to_text(page);

        for block in markdown_blocks(&text) {
            blocks.push(match block {
                Block::Paragraph { text, .. } => Block::Paragraph {
                    text,
                    page: Some(page_number),
                },
                other => other,
            });
        }
    }

    blocks
}

/// Flatten the HTML the model emits into text the Markdown parser keeps.
///
/// Rows become paragraphs, cells are joined with ` | `, `<page_number>` is
/// dropped with its content (it is layout, not text), and every other tag is
/// removed leaving what it wrapped.
fn tables_to_text(markdown: &str) -> String {
    if !markdown.contains('<') {
        return markdown.to_owned();
    }

    let mut out = String::with_capacity(markdown.len());
    let mut rest = markdown;

    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];

        // A lone `<` — a comparison in prose, say — is text, not a tag.
        let Some(end) = after.find('>') else {
            out.push_str(after);
            rest = "";
            break;
        };

        let tag = after[1..end].trim().to_ascii_lowercase();
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        rest = &after[end + 1..];

        match (name, closing) {
            ("table", _) | ("p", true) => out.push_str("\n\n"),
            ("tr", true) => {
                while out.ends_with(" | ") {
                    out.truncate(out.len() - 3);
                }
                out.push_str("\n\n");
            }
            ("td" | "th", true) => out.push_str(" | "),
            ("br", _) => out.push(' '),
            ("page_number", false) => {
                if let Some(close) = rest.find("</page_number>") {
                    rest = &rest[close + "</page_number>".len()..];
                }
            }
            _ => {}
        }
    }

    out.push_str(rest);
    unescape(&out)
}

/// The handful of entities an HTML table can carry.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// Stand-in sidecars for tests, here so the PDF parser's tests can use them too.
#[cfg(test)]
pub(crate) mod testing {
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::{Json, Router};

    /// Serve a router on a free loopback port and return its base URL.
    pub(crate) async fn serve(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind a loopback port");
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("serve the stand-in sidecar");
        });
        url
    }

    /// A URL nothing listens on.
    pub(crate) async fn dead_url() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        url
    }

    /// A sidecar that answers every request with the given pages, in the
    /// order given — which need not be page order.
    pub(crate) fn sidecar_answering(pages: Vec<(u32, &'static str)>) -> Router {
        let body = serde_json::json!({
            "model": "stand-in",
            "total_ms": 1,
            "pages": pages
                .iter()
                .map(|(n, md)| serde_json::json!({ "page": n, "markdown": md, "ms": 1 }))
                .collect::<Vec<_>>(),
        });
        Router::new()
            .route(
                "/health",
                get(|| async { Json(serde_json::json!({ "status": "ok" })) }),
            )
            .route(
                "/ocr",
                post(move || {
                    let body = body.clone();
                    async move { Json(body) }
                }),
            )
    }

    /// A sidecar that refuses everything the way the real one refuses a page
    /// too small to read.
    pub(crate) fn sidecar_refusing() -> Router {
        Router::new().route(
            "/ocr",
            post(|| async {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({
                        "detail": "image too small (768x1087); longest side must be >= 1200px"
                    })),
                )
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraphs(blocks: &[Block]) -> Vec<(&str, Option<u32>)> {
        blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { text, page } => Some((text.as_str(), *page)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_html_table_becomes_one_row_per_paragraph() {
        let text = tables_to_text(
            "<table><tr><td>ลำดับ</td><td>รายการ</td><td>จำนวน</td></tr>\
             <tr><td>1</td><td>อิฐมอญ</td><td>4,000</td></tr></table>",
        );
        let blocks = markdown_blocks(&text);
        let rows = paragraphs(&blocks);

        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].0, "ลำดับ | รายการ | จำนวน");
        assert_eq!(rows[1].0, "1 | อิฐมอญ | 4,000");
    }

    #[test]
    fn a_page_number_is_layout_not_content() {
        let text = tables_to_text("ข้อความจริง\n\n<page_number>3</page_number>");
        let blocks = markdown_blocks(&text);
        let rows = paragraphs(&blocks);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].0, "ข้อความจริง");
    }

    #[test]
    fn entities_and_a_lone_angle_bracket_survive() {
        assert_eq!(
            tables_to_text("<td>a &amp; b</td> ราคา < 100"),
            "a & b |  ราคา < 100"
        );
    }

    #[test]
    fn markdown_without_html_is_left_alone() {
        let text = "# หัวข้อ\n\nย่อหน้า";
        assert_eq!(tables_to_text(text), text);
    }

    #[test]
    fn every_paragraph_knows_which_page_it_came_from() {
        let pages = vec![
            "# ใบส่งของ\n\nส่งที่หน่วยงาน โครงการรามอินทรา".to_owned(),
            "<table><tr><td>1</td><td>อิฐมอญ</td></tr></table>".to_owned(),
        ];
        let blocks = blocks_from_pages(&pages);

        assert!(matches!(&blocks[0], Block::Heading { level: 1, .. }));
        let paragraphs = paragraphs(&blocks);
        assert_eq!(
            paragraphs,
            vec![
                ("ส่งที่หน่วยงาน โครงการรามอินทรา", Some(1)),
                ("1 | อิฐมอญ", Some(2)),
            ]
        );
    }

    #[tokio::test]
    async fn pages_come_back_in_page_order_whatever_order_they_arrive() {
        let url = testing::serve(testing::sidecar_answering(vec![
            (2, "หน้าสอง"),
            (1, "# หน้าหนึ่ง"),
        ]))
        .await;
        let client = OcrClient::new(url, Duration::from_secs(5)).unwrap();

        let pages = client.ocr_pdf(b"%PDF-1.4 not really").await.unwrap();
        assert_eq!(pages, vec!["# หน้าหนึ่ง".to_owned(), "หน้าสอง".to_owned()]);
        client.health().await.expect("the stand-in reports healthy");
    }

    #[tokio::test]
    async fn a_refusal_is_final_and_says_why() {
        let url = testing::serve(testing::sidecar_refusing()).await;
        let client = OcrClient::new(url, Duration::from_secs(5)).unwrap();

        let err = client.ocr_pdf(b"tiny").await.unwrap_err();
        assert!(!err.is_retryable(), "{err}");
        assert!(
            matches!(&err, OcrError::Rejected { status: 422, detail } if detail.contains("too small")),
            "{err}"
        );
    }

    #[tokio::test]
    async fn a_service_that_is_not_there_is_worth_retrying() {
        let client = OcrClient::new(testing::dead_url().await, Duration::from_secs(5)).unwrap();

        let err = client.ocr_pdf(b"%PDF-1.4").await.unwrap_err();
        assert!(err.is_retryable(), "{err}");
        assert!(matches!(err, OcrError::Unavailable { .. }));
        assert!(client.health().await.is_err());
    }

    #[test]
    fn a_trailing_slash_on_the_url_is_not_a_second_path_segment() {
        let client = OcrClient::new("http://127.0.0.1:9090/", Duration::from_secs(1)).unwrap();
        assert_eq!(client.base_url(), "http://127.0.0.1:9090");
    }
}
