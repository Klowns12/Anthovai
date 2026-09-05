//! The published API contract.
//!
//! `/v1` only. The dashboard API is ours and changes with the frontend; the
//! moment it appears in a published document, someone builds against it and it
//! stops being ours.
//!
//! Every schema here is derived from the type that actually goes on the wire,
//! never restated. A hand-written mirror is wrong the first time a field is
//! added, and nothing catches it — the document keeps describing a shape the
//! server stopped sending.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Anthovai API",
        version = "1.0.0",
        description = "\
Ask an agent a question and get an answer built from your own documents, with \
citations back to the passages it used.

**Authentication.** Every request carries `Authorization: Bearer av_live_…`. A \
key belongs to one workspace, carries a set of scopes, and may be restricted to \
particular agents. Keys are shown once when created and cannot be read back.

**Ingestion is asynchronous.** Uploading a document returns `202 Accepted` with \
a status of `queued`. A worker parses, chunks and embeds it; poll the document \
until its status is `ready`.

**Errors** all share one shape, and `error.code` is the stable string to branch \
on. `error.request_id` is what to quote when asking us about a request.",
        contact(name = "Anthovai", url = "https://www.anthovai.com/"),
    ),
    servers((url = "https://api.anthovai.com", description = "Production")),
    modifiers(&BearerKey),
    tags(
        (name = "Chat", description = "Asking an agent a question"),
        (name = "Conversations", description = "Reading and erasing what was asked"),
        (name = "Agents", description = "What this key may ask"),
        (name = "Knowledge", description = "The documents an agent answers from"),
        (name = "Usage", description = "What has been spent this month"),
    ),
    paths(
        crate::public::chat::chat,
        crate::public::chat::list_conversations,
        crate::public::chat::get_conversation,
        crate::public::chat::delete_conversation,
        crate::public::chat::usage,
        crate::public::agents::list,
        crate::public::agents::get_agent,
        crate::public::knowledge::create,
        crate::public::knowledge::list_knowledge_bases,
        crate::public::knowledge::get_knowledge_base,
        crate::public::knowledge::delete_knowledge_base,
        crate::public::knowledge::list_documents,
        crate::public::knowledge::upload,
        crate::public::knowledge::get_document,
        crate::public::knowledge::delete_document,
    ),
)]
pub struct ApiDoc;

struct BearerKey;

impl Modify for BearerKey {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .as_mut()
            .expect("the derive always produces a components section");

        components.add_security_scheme(
            "api_key",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("av_live_… or av_test_…")
                    .description(Some(
                        "The key is sent in this header and nowhere else. A key \
                         in a query string is refused outright, because it would \
                         end up in access logs, browser history and referrer \
                         headers — and would then have to be treated as leaked.",
                    ))
                    .build(),
            ),
        );

        // Named so the header cannot be mistaken for an alternative way to
        // authenticate. It is a correlation id, and it is echoed back.
        components.add_security_scheme(
            "request_id",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-Request-Id",
                "Optional. Echoed in the response and in our logs so your trace \
                 and ours line up. Alphanumeric, dashes and underscores, at most \
                 64 characters; anything else is replaced with an id of ours.",
            ))),
        );
    }
}

/// The document as JSON, pretty-printed.
pub fn document() -> String {
    ApiDoc::openapi()
        .to_pretty_json()
        .expect("the OpenAPI document is built from types that always serialise")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> serde_json::Value {
        serde_json::from_str(&document()).expect("valid JSON")
    }

    #[test]
    fn every_public_endpoint_is_documented() {
        // The list the router actually serves. When a route is added to `/v1`
        // and not to the document, this is what says so — otherwise the first
        // person to notice is a customer reading documentation that does not
        // mention the endpoint they were told to call.
        let expected = [
            ("/v1/chat", "post"),
            ("/v1/conversations", "get"),
            ("/v1/conversations/{conversation_id}", "get"),
            ("/v1/conversations/{conversation_id}", "delete"),
            ("/v1/usage", "get"),
            ("/v1/agents", "get"),
            ("/v1/agents/{agent_id}", "get"),
            ("/v1/knowledge_bases", "get"),
            ("/v1/knowledge_bases", "post"),
            ("/v1/knowledge_bases/{kb_id}", "get"),
            ("/v1/knowledge_bases/{kb_id}", "delete"),
            ("/v1/documents", "get"),
            ("/v1/documents", "post"),
            ("/v1/documents/{document_id}", "get"),
            ("/v1/documents/{document_id}", "delete"),
        ];

        let doc = doc();
        let paths = &doc["paths"];

        for (path, method) in expected {
            assert!(
                paths[path][method].is_object(),
                "{method} {path} is not in the document"
            );
        }

        let documented: usize = paths
            .as_object()
            .unwrap()
            .values()
            .map(|methods| methods.as_object().unwrap().len())
            .sum();
        assert_eq!(
            documented,
            expected.len(),
            "the document describes operations this test does not list"
        );
    }

    #[test]
    fn the_dashboard_api_is_not_published() {
        // It is ours, and it changes with the frontend. Publishing it would
        // mean someone builds against it and it stops being ours.
        let doc = doc();
        for path in doc["paths"].as_object().unwrap().keys() {
            assert!(path.starts_with("/v1/"), "{path} should not be published");
        }
    }

    #[test]
    fn every_operation_says_how_to_authenticate() {
        let doc = doc();
        for (path, methods) in doc["paths"].as_object().unwrap() {
            for (method, operation) in methods.as_object().unwrap() {
                assert!(
                    operation["security"].is_array(),
                    "{method} {path} does not name a security scheme"
                );
            }
        }
    }

    #[test]
    fn an_answers_sources_are_described_by_the_type_that_produces_them() {
        // `Source` is derived from the retrieval crate's own struct. If that
        // type gains a field, this document gains it too — which is the whole
        // reason it is not restated at the HTTP layer.
        let doc = doc();
        let source = &doc["components"]["schemas"]["Source"]["properties"];

        for field in [
            "index",
            "document_id",
            "chunk_id",
            "title",
            "snippet",
            "score",
        ] {
            assert!(source[field].is_object(), "Source.{field} is missing");
        }
    }

    #[test]
    fn the_error_shape_is_documented_because_callers_branch_on_it() {
        let doc = doc();
        let error = &doc["components"]["schemas"]["ErrorDetail"]["properties"];

        for field in ["type", "code", "message", "request_id", "doc_url"] {
            assert!(error[field].is_object(), "ErrorDetail.{field} is missing");
        }
    }
}
