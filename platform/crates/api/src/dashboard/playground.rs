//! Trying an agent before anyone else sees it.
//!
//! Runs the draft rather than the published version, and reports which
//! passages were retrieved and how they scored — because when an answer is
//! wrong, the useful question is almost always "what did it read?"

use anthovai_core::{AgentId, ConversationId};
use anthovai_rag::ChatInput;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::dashboard::organizations::parse_id;
use crate::error::ApiError;
use crate::extract::SessionAuth;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/agents/{agent_id}/test", post(test))
}

#[derive(Deserialize)]
struct TestRequest {
    message: String,
    #[serde(default)]
    conversation_id: Option<String>,
    /// Include the retrieved passages and their scores.
    #[serde(default = "yes")]
    debug: bool,
}

fn yes() -> bool {
    true
}

#[derive(Serialize)]
struct TestResponse {
    id: String,
    conversation_id: String,
    answer: String,
    grounded: bool,
    used_fallback: bool,
    sources: Vec<anthovai_retrieval::Source>,
    usage: Usage,
    /// Always shown here: this is our own dashboard, and knowing which model
    /// answered is the first thing anyone asks when tuning an agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<Model>,
    latency_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    retrieval: Option<Retrieval>,
}

#[derive(Serialize)]
struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Serialize)]
struct Model {
    provider: String,
    name: String,
}

#[derive(Serialize)]
struct Retrieval {
    embedding_tokens: u32,
    passages: Vec<Passage>,
}

#[derive(Serialize)]
struct Passage {
    chunk_id: String,
    document_id: String,
    score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    similarity: Option<f32>,
    snippet: String,
}

async fn test(
    State(state): State<AppState>,
    session: SessionAuth,
    Path(agent_id): Path<String>,
    Json(body): Json<TestRequest>,
) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, session.request_id.clone());

    let agent_id: AgentId = parse_id(&agent_id, &session.request_id)?;
    let conversation_id = body
        .conversation_id
        .as_deref()
        .map(|raw| parse_id::<ConversationId>(raw, &session.request_id))
        .transpose()?;

    let result = state
        .chat
        .test(
            &session.ctx,
            ChatInput {
                agent_id,
                message: body.message,
                conversation_id,
                external_user_id: None,
                document_ids: Vec::new(),
                debug: body.debug,
            },
        )
        .await
        .map_err(reject)?;

    Ok(Json(TestResponse {
        id: result.message_id.to_string(),
        conversation_id: result.conversation_id.to_string(),
        answer: result.output.answer.clone(),
        grounded: result.output.grounded,
        used_fallback: result.output.used_fallback,
        sources: result.output.sources.clone(),
        usage: Usage {
            input_tokens: result.usage.input_tokens,
            output_tokens: result.usage.output_tokens,
        },
        model: result.model.as_ref().map(|m| Model {
            provider: m.provider.clone(),
            name: m.model.clone(),
        }),
        latency_ms: result.latency_ms,
        retrieval: result.debug.as_ref().map(|debug| Retrieval {
            embedding_tokens: debug.embedding_tokens,
            passages: debug
                .passages
                .iter()
                .map(|p| Passage {
                    chunk_id: p.chunk_id.clone(),
                    document_id: p.document_id.clone(),
                    score: p.score,
                    similarity: p.vector_score,
                    snippet: p.snippet.clone(),
                })
                .collect(),
        }),
    })
    .into_response())
}
