#![allow(dead_code)]

//! What every HTTP suite needs to stand a server up.

use std::collections::HashMap;
use std::sync::Arc;

use anthovai_agent::AgentService;
use anthovai_conversation::ConversationService;
use anthovai_core::Clock;
use anthovai_db::Db;
use anthovai_embeddings::HashEmbedder;
use anthovai_inference::{
    ChatProvider, EchoProvider, HealthTracker, ModelRegistry, ModelRouter, ProviderId,
};
use anthovai_rag::ChatService;
use anthovai_retrieval::Retriever;

/// The dimension the fake embedder produces, matching `fake:hash-1536`.
pub const TEST_DIMENSION: usize = 1536;

/// A chat stack that answers locally.
///
/// These suites are about the HTTP layer, so the model is the echo provider and
/// the embedder is the hash one: no network, and a wrong answer means wrong
/// wiring rather than a bad model.
pub fn chat_services(
    db: &Db,
    agents: Arc<AgentService>,
    clock: &Clock,
) -> (ChatService, ConversationService) {
    let retriever = Arc::new(Retriever::new(
        db.clone(),
        vec![Arc::new(HashEmbedder::new(TEST_DIMENSION))],
    ));

    let router = Arc::new(ModelRouter::new(
        ModelRegistry::echo_only(),
        HashMap::from([(
            ProviderId::Anthropic,
            Arc::new(EchoProvider::new()) as Arc<dyn ChatProvider>,
        )]),
        HealthTracker::new(clock.clone()),
    ));

    (
        ChatService::new(db.clone(), agents, retriever, router, clock.clone()),
        ConversationService::new(db.clone(), clock.clone()),
    )
}

// ---- multipart ------------------------------------------------------------

pub const BOUNDARY: &str = "anthovaitestboundary";

pub enum Part<'a> {
    Field(&'a str, &'a str),
    File {
        name: &'a str,
        filename: &'a str,
        content: Vec<u8>,
    },
}

/// Built by hand so the part order — which the upload code depends on — is
/// exactly what the test intends.
pub fn multipart_body(parts: &[Part<'_>]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match part {
            Part::Field(name, value) => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(value.as_bytes());
            }
            Part::File {
                name,
                filename,
                content,
            } => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n\
                         Content-Type: application/octet-stream\r\n\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(content);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

// ---- diagnostics ----------------------------------------------------------

/// What the readiness endpoint reads, pointed at the test's own database and
/// storage. No metrics handle: installing a Prometheus recorder is a
/// process-wide side effect, and test binaries run many servers at once.
pub fn diagnostics(
    db: &Db,
    storage: anthovai_storage::Storage,
) -> anthovai_api::state::Diagnostics {
    anthovai_api::state::Diagnostics {
        db: db.clone(),
        storage,
        router: Arc::new(ModelRouter::new(
            ModelRegistry::echo_only(),
            HashMap::from([(
                ProviderId::Anthropic,
                Arc::new(EchoProvider::new()) as Arc<dyn ChatProvider>,
            )]),
            HealthTracker::new(Clock::system()),
        )),
        jobs: Arc::new(anthovai_jobs::JobQueue::new(db.clone())),
        metrics: None,
    }
}
