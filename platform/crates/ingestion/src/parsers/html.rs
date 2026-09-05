//! Web pages.
//!
//! Most of a page is not the page. Navigation, footers, cookie banners and
//! sidebars repeat on every URL of a site, and indexing them means every
//! question retrieves the menu. So the furniture is stripped first, and what is
//! left is read for headings and paragraphs the same way Markdown is.

use anthovai_core::{DomainError, Result};
use anthovai_knowledge::SourceType;
use async_trait::async_trait;
use scraper::{Html, Node, Selector};

use crate::chunker::{Block, ParsedDocument};
use crate::normalize::normalize;
use crate::parsers::text::{decode, detect_language};
use crate::{error_codes, ParseInput, Parser};

/// Elements that are never content, whatever page they are on.
///
/// `script` and `style` matter for a second reason: their contents are text
/// nodes, so leaving them in fills the index with minified JavaScript.
const FURNITURE: &str = "script, style, noscript, template, svg, nav, header, footer, aside, \
                         form, iframe, button, [role=navigation], [role=banner], \
                         [role=contentinfo], [aria-hidden=true], .nav, .navbar, .menu, \
                         .sidebar, .footer, .cookie, .cookie-banner, .breadcrumb, \
                         .advertisement, .ads";

/// Where a page's real content usually is, in the order worth trying.
///
/// Falling straight back to `body` on a page with no such element is right:
/// better a noisy document than none.
const CONTENT: &[&str] = &["main", "article", "[role=main]", "#content", ".content"];

pub struct HtmlParser;

#[async_trait]
impl Parser for HtmlParser {
    fn supports(&self, source_type: SourceType) -> bool {
        matches!(source_type, SourceType::Html | SourceType::Url)
    }

    async fn parse(&self, input: ParseInput) -> Result<ParsedDocument> {
        let html = decode(&input.bytes)?;
        let parsed = Extracted::from(&html);

        if parsed.blocks.is_empty() {
            return Err(DomainError::validation(format!(
                "{}: this page has no readable text. If its content is drawn by \
                 JavaScript, the HTML we receive is empty.",
                error_codes::NO_EXTRACTABLE_TEXT
            )));
        }

        // The page's own `<title>` beats a filename or a URL: it is what the
        // author called it, and it is what a citation should show.
        let title = parsed
            .title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| input.title());

        let sample: String = parsed
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { text, .. } | Block::Heading { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .take(40)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ParsedDocument {
            title,
            language: detect_language(&sample),
            blocks: parsed.blocks,
        })
    }
}

struct Extracted {
    title: Option<String>,
    blocks: Vec<Block>,
}

impl Extracted {
    fn from(html: &str) -> Self {
        let document = Html::parse_document(html);

        let title = select("title")
            .and_then(|s| document.select(&s).next())
            .map(|el| normalize(&el.text().collect::<String>()).trim().to_owned());

        // Everything to skip, resolved once: the furniture itself and every
        // node inside it.
        let mut skip = std::collections::HashSet::new();
        if let Some(selector) = select(FURNITURE) {
            for element in document.select(&selector) {
                for node in element.descendants() {
                    skip.insert(node.id());
                }
            }
        }

        let root = CONTENT
            .iter()
            .filter_map(|s| select(s))
            .find_map(|s| document.select(&s).next())
            .or_else(|| select("body").and_then(|s| document.select(&s).next()));

        let mut blocks = Vec::new();
        if let Some(root) = root {
            let mut buffer = String::new();
            // The level of the heading currently being read. Its text arrives
            // from child nodes, so what is open has to be remembered until the
            // next block boundary closes it.
            let mut heading: Option<u8> = None;

            for node in root.descendants() {
                if skip.contains(&node.id()) {
                    continue;
                }

                match node.value() {
                    Node::Element(element) => {
                        let name = element.name();
                        if let Some(level) = heading_level(name) {
                            flush(&mut buffer, &mut heading, &mut blocks);
                            heading = Some(level);
                        } else if breaks_a_block(name) {
                            flush(&mut buffer, &mut heading, &mut blocks);
                        } else if name == "br" {
                            buffer.push(' ');
                        }
                    }
                    Node::Text(text) => {
                        let text = text.trim();
                        if text.is_empty() {
                            continue;
                        }
                        if !buffer.is_empty() && !buffer.ends_with(' ') {
                            buffer.push(' ');
                        }
                        buffer.push_str(text);
                    }
                    _ => {}
                }
            }

            flush(&mut buffer, &mut heading, &mut blocks);
        }

        Self { title, blocks }
    }
}

/// Close whatever block was open.
///
/// A heading with no text — an empty `<h2>`, or one holding only an image — is
/// dropped rather than left open, so the paragraph after it does not end up
/// filling in a heading it has nothing to do with.
fn flush(buffer: &mut String, heading: &mut Option<u8>, blocks: &mut Vec<Block>) {
    let text = normalize(buffer);
    let text = text.trim().to_owned();
    buffer.clear();
    let level = heading.take();

    if text.is_empty() {
        return;
    }

    blocks.push(match level {
        Some(level) => Block::Heading { level, text },
        None => Block::Paragraph { text, page: None },
    });
}

fn heading_level(name: &str) -> Option<u8> {
    match name {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

/// Elements that end whatever text was being collected.
///
/// Without this a list of five bullets becomes one run-on sentence, and a
/// question matching one bullet retrieves all five.
fn breaks_a_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "li"
            | "tr"
            | "td"
            | "th"
            | "section"
            | "blockquote"
            | "pre"
            | "dd"
            | "dt"
            | "figcaption"
    )
}

/// `Selector::parse` fails only on a selector we wrote, so a mistake here means
/// that filter silently does nothing rather than taking the page down.
fn select(selector: &str) -> Option<Selector> {
    match Selector::parse(selector) {
        Ok(selector) => Some(selector),
        Err(e) => {
            tracing::error!(%selector, error = %e, "a built-in CSS selector is invalid");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(html: &str) -> ParseInput {
        ParseInput {
            bytes: html.as_bytes().to_vec(),
            source_type: SourceType::Html,
            filename: None,
            source_url: Some("https://abc.ac.th/courses".to_owned()),
        }
    }

    fn texts(doc: &ParsedDocument) -> Vec<String> {
        doc.blocks
            .iter()
            .map(|b| match b {
                Block::Heading { level, text } => format!("h{level}: {text}"),
                Block::Paragraph { text, .. } => text.clone(),
                Block::Record { text, .. } => text.clone(),
            })
            .collect()
    }

    #[tokio::test]
    async fn headings_and_paragraphs_keep_their_shape() {
        let html = r#"<html><head><title>Courses</title></head><body><main>
            <h1>Programs</h1>
            <h2>Rust Programming</h2>
            <p>Runs for twelve weeks.</p>
        </main></body></html>"#;
        let doc = HtmlParser.parse(input(html)).await.unwrap();

        assert_eq!(doc.title, "Courses", "the page's own title should win");
        assert_eq!(
            texts(&doc),
            vec![
                "h1: Programs",
                "h2: Rust Programming",
                "Runs for twelve weeks."
            ]
        );
    }

    #[tokio::test]
    async fn the_furniture_is_not_indexed() {
        // Every page of a site carries these. Indexing them means every
        // question retrieves the menu.
        let html = r#"<html><body>
            <nav><a href="/">Home</a><a href="/courses">Courses</a></nav>
            <div class="cookie-banner">We use cookies</div>
            <main><p>The library opens at seven.</p></main>
            <footer>Copyright ABC School</footer>
        </body></html>"#;
        let doc = HtmlParser.parse(input(html)).await.unwrap();

        let all = texts(&doc).join("\n");
        assert!(all.contains("library opens at seven"));
        for furniture in ["Home", "Courses", "cookies", "Copyright"] {
            assert!(!all.contains(furniture), "`{furniture}` survived: {all}");
        }
    }

    #[tokio::test]
    async fn script_and_style_contents_never_reach_the_index() {
        let html = r#"<html><body><main>
            <script>window.config={apiKey:"sk-not-a-real-key"}</script>
            <style>.x{color:red}</style>
            <p>Real content.</p>
        </main></body></html>"#;
        let doc = HtmlParser.parse(input(html)).await.unwrap();

        let all = texts(&doc).join("\n");
        assert_eq!(all, "Real content.", "got {all:?}");
    }

    #[tokio::test]
    async fn list_items_do_not_run_together() {
        let html = r#"<html><body><main><ul>
            <li>A laptop</li><li>Some patience</li>
        </ul></main></body></html>"#;
        let doc = HtmlParser.parse(input(html)).await.unwrap();

        assert_eq!(texts(&doc), vec!["A laptop", "Some patience"]);
    }

    #[tokio::test]
    async fn inline_markup_does_not_split_a_sentence() {
        let html = "<html><body><main><p>The <strong>Rust</strong> course \
                    runs for <em>twelve</em> weeks.</p></main></body></html>";
        let doc = HtmlParser.parse(input(html)).await.unwrap();

        assert_eq!(
            texts(&doc),
            vec!["The Rust course runs for twelve weeks."],
            "inline tags are formatting, not structure"
        );
    }

    #[tokio::test]
    async fn a_page_with_no_main_element_falls_back_to_the_body() {
        let html = "<html><body><p>Just a paragraph.</p></body></html>";
        let doc = HtmlParser.parse(input(html)).await.unwrap();
        assert_eq!(texts(&doc), vec!["Just a paragraph."]);
    }

    #[tokio::test]
    async fn an_empty_heading_is_not_kept_as_structure() {
        let html = r#"<html><body><main>
            <h2><img src="logo.png"></h2>
            <p>Content.</p>
        </main></body></html>"#;
        let doc = HtmlParser.parse(input(html)).await.unwrap();
        assert_eq!(texts(&doc), vec!["Content."]);
    }

    #[tokio::test]
    async fn thai_pages_survive_and_are_detected() {
        let html = "<html><head><title>หลักสูตร</title></head><body><main>\
                    <h1>หลักสูตร</h1>\
                    <p>หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์ เรียนช่วงเย็นวันธรรมดา \
                    ตั้งแต่หกโมงเย็นถึงสามทุ่ม</p></main></body></html>";
        let doc = HtmlParser.parse(input(html)).await.unwrap();

        assert_eq!(doc.title, "หลักสูตร");
        assert!(texts(&doc).join("\n").contains("12 สัปดาห์"));
        assert_eq!(doc.language.as_deref(), Some("tha"));
    }

    #[tokio::test]
    async fn html_entities_are_decoded() {
        let html = "<html><body><main><p>Fees &amp; charges &mdash; 4,900 THB</p>\
                    </main></body></html>";
        let doc = HtmlParser.parse(input(html)).await.unwrap();
        assert_eq!(texts(&doc), vec!["Fees & charges — 4,900 THB"]);
    }

    #[tokio::test]
    async fn a_page_drawn_entirely_by_javascript_is_refused_with_a_reason() {
        let html = r#"<html><body><div id="root"></div>
            <script src="/app.js"></script></body></html>"#;
        let err = HtmlParser.parse(input(html)).await.unwrap_err();

        let message = err.to_string();
        assert!(
            message.contains(error_codes::NO_EXTRACTABLE_TEXT),
            "{message}"
        );
        assert!(message.contains("JavaScript"), "{message}");
    }

    #[tokio::test]
    async fn a_url_document_uses_the_same_parser() {
        assert!(HtmlParser.supports(SourceType::Url));
        assert!(HtmlParser.supports(SourceType::Html));
        assert!(!HtmlParser.supports(SourceType::Md));
    }
}
