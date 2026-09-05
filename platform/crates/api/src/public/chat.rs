//! The endpoint the whole platform exists to serve.

use anthovai_core::{AgentId, ConversationId, DomainError, Scope};
use anthovai_rag::{ChatInput, ChatResult};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::dashboard::organizations::parse_id;
use crate::error::ApiError;
use crate::extract::ApiKeyAuth;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chat", post(chat))
        .route("/conversations", get(list_conversations))
        .route(
            "/conversations/{conversation_id}",
            get(get_conversation).delete(delete_conversation),
        )
        .route("/usage", get(usage))
}

#[derive(Deserialize, ToSchema)]
pub struct ChatRequest {
    /// The agent to ask, `agt_…`. It must be published.
    pub agent_id: String,
    /// The question, as the end user typed it.
    pub message: String,
    /// Continue an existing conversation. Omit to start a new one; the id of
    /// the new conversation comes back in the response.
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub user: Option<UserRef>,
    #[serde(default)]
    pub filters: Option<Filters>,
    #[serde(default)]
    pub options: Options,
}

/// Who is asking, in the customer's own system.
///
/// Only ever an identifier of their choosing — we do not want a name or an
/// email. It is what makes "erase everything for this person" answerable.
#[derive(Deserialize, ToSchema)]
pub struct UserRef {
    pub id: String,
}

#[derive(Deserialize, Default, ToSchema)]
pub struct Filters {
    /// Search only these documents. Empty means the whole knowledge base.
    #[serde(default)]
    pub document_ids: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct Options {
    #[serde(default = "yes")]
    pub include_sources: bool,
    #[serde(default = "yes")]
    pub include_usage: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            include_sources: true,
            include_usage: true,
        }
    }
}

fn yes() -> bool {
    true
}

#[derive(Serialize, ToSchema)]
pub struct ChatResponse {
    /// The assistant message, `msg_…`.
    pub id: String,
    pub request_id: String,
    pub conversation_id: String,
    pub agent_id: String,
    pub answer: String,
    /// Whether the answer was built from retrieved passages. `false` means
    /// nothing relevant was found and the agent said so; there are no sources
    /// to show and the answer should not be presented as fact.
    pub grounded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<anthovai_retrieval::Source>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelView>,
    pub latency_ms: i64,
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct UsageView {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

/// Which model answered.
///
/// Only on a plan that pays for choosing one — on every other plan the answer
/// is Anthovai's, and naming the model behind it would tie the API to a vendor
/// we may swap.
#[derive(Serialize, ToSchema)]
pub struct ModelView {
    pub provider: String,
    pub name: String,
}

#[utoipa::path(
    post,
    path = "/v1/chat",
    tag = "Chat",
    request_body = ChatRequest,
    responses(
        (status = 200, description = "The answer, with the passages it was built from", body = ChatResponse),
        (status = 400, description = "The request could not be read", body = crate::error::ErrorBody),
        (status = 403, description = "The key lacks the `chat` scope, is scoped to other agents, or the agent is not published", body = crate::error::ErrorBody),
        (status = 404, description = "No such agent in this organization", body = crate::error::ErrorBody),
        (status = 429, description = "Rate limited, or the monthly message quota is spent", body = crate::error::ErrorBody),
        (status = 503, description = "No model provider is answering", body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn chat(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Json(body): Json<ChatRequest>,
) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, auth.request_id.clone());
    auth.ctx.require_scope(Scope::Chat).map_err(reject)?;

    let agent_id: AgentId = parse_id(&body.agent_id, &auth.request_id)?;
    let conversation_id = body
        .conversation_id
        .as_deref()
        .map(|raw| parse_id::<ConversationId>(raw, &auth.request_id))
        .transpose()?;

    let result = state
        .chat
        .chat(
            &auth.ctx,
            ChatInput {
                agent_id,
                message: body.message,
                conversation_id,
                external_user_id: body.user.map(|u| u.id),
                document_ids: body.filters.unwrap_or_default().document_ids,
                // The public API does not expose retrieval internals: which
                // passages scored what is ours and the dashboard's to see.
                debug: false,
            },
        )
        .await
        .map_err(reject)?;

    Ok(Json(response_for(&result, agent_id, &body.options)).into_response())
}

fn response_for(result: &ChatResult, agent_id: AgentId, options: &Options) -> ChatResponse {
    ChatResponse {
        id: result.message_id.to_string(),
        request_id: result.request_id.to_string(),
        conversation_id: result.conversation_id.to_string(),
        agent_id: agent_id.to_string(),
        answer: result.output.answer.clone(),
        grounded: result.output.grounded,
        sources: options
            .include_sources
            .then(|| result.output.sources.clone()),
        usage: options.include_usage.then(|| UsageView {
            input_tokens: result.usage.input_tokens,
            output_tokens: result.usage.output_tokens,
            total_tokens: result.usage.total(),
        }),
        model: result.model.as_ref().map(|m| ModelView {
            provider: m.provider.clone(),
            name: m.model.clone(),
        }),
        latency_ms: result.latency_ms,
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

// ---- conversations --------------------------------------------------------

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ConversationQuery {
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    /// 1–200, default 50.
    pub limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ConversationView {
    pub id: String,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub message_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ConversationListResponse {
    pub data: Vec<ConversationView>,
}

#[utoipa::path(
    get,
    path = "/v1/conversations",
    tag = "Conversations",
    params(ConversationQuery),
    responses(
        (status = 200, description = "Conversations this key may see", body = ConversationListResponse),
        (status = 403, description = "The key lacks the `chat` scope", body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn list_conversations(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Query(query): Query<ConversationQuery>,
) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, auth.request_id.clone());
    auth.ctx.require_scope(Scope::Chat).map_err(reject)?;

    let agent_id = query
        .agent_id
        .as_deref()
        .map(|raw| parse_id::<AgentId>(raw, &auth.request_id))
        .transpose()?;

    // A key scoped to particular agents must not list conversations belonging
    // to the others.
    if let Some(agent_id) = agent_id {
        auth.ctx.require_agent(agent_id).map_err(reject)?;
    }

    let conversations = state
        .conversations
        .list(
            &auth.ctx,
            agent_id,
            query.user_id.as_deref(),
            query.limit.unwrap_or(50).clamp(1, 200),
        )
        .await
        .map_err(reject)?;

    Ok(Json(ConversationListResponse {
        data: conversations
            .iter()
            .filter(|c| auth.ctx.require_agent(c.agent_id).is_ok())
            .map(view_of)
            .collect::<Vec<_>>(),
    })
    .into_response())
}

#[derive(Serialize, ToSchema)]
pub struct ConversationDetail {
    #[serde(flatten)]
    pub conversation: ConversationView,
    pub messages: Vec<MessageView>,
}

#[derive(Serialize, ToSchema)]
pub struct MessageView {
    pub id: String,
    /// `user` or `assistant`.
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<anthovai_retrieval::Source>>)]
    pub sources: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounded: Option<bool>,
    pub created_at: String,
}

#[utoipa::path(
    get,
    path = "/v1/conversations/{conversation_id}",
    tag = "Conversations",
    params(("conversation_id" = String, Path, description = "The conversation id, `conv_…`")),
    responses(
        (status = 200, description = "The conversation and its messages, oldest first", body = ConversationDetail),
        (status = 404, description = "No such conversation in this organization", body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn get_conversation(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Path(conversation_id): Path<String>,
) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, auth.request_id.clone());
    auth.ctx.require_scope(Scope::Chat).map_err(reject)?;

    let conversation_id: ConversationId = parse_id(&conversation_id, &auth.request_id)?;
    let (conversation, messages) = state
        .conversations
        .detail(&auth.ctx, conversation_id)
        .await
        .map_err(reject)?;

    auth.ctx
        .require_agent(conversation.agent_id)
        .map_err(reject)?;

    Ok(Json(ConversationDetail {
        conversation: view_of(&conversation),
        messages: messages
            .iter()
            .map(|detail| MessageView {
                id: detail.message.id.to_string(),
                role: detail.message.role.as_str().to_owned(),
                content: detail.message.content.clone(),
                sources: detail.sources.clone(),
                grounded: detail.grounded,
                created_at: detail.message.created_at.to_rfc3339(),
            })
            .collect(),
    })
    .into_response())
}

/// A real deletion. This is what answers a request to erase someone's data.
#[utoipa::path(
    delete,
    path = "/v1/conversations/{conversation_id}",
    tag = "Conversations",
    params(("conversation_id" = String, Path, description = "The conversation id, `conv_…`")),
    responses(
        (status = 204, description = "Deleted. The messages are gone, not hidden."),
        (status = 404, description = "No such conversation in this organization", body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn delete_conversation(
    State(state): State<AppState>,
    auth: ApiKeyAuth,
    Path(conversation_id): Path<String>,
) -> Result<Response, ApiError> {
    let reject = |e| ApiError::from_domain(e, auth.request_id.clone());
    auth.ctx.require_scope(Scope::Chat).map_err(reject)?;

    let conversation_id: ConversationId = parse_id(&conversation_id, &auth.request_id)?;

    // Proves it exists in this tenant and is within the key's agent scope
    // before anything is destroyed.
    let (conversation, _) = state
        .conversations
        .detail(&auth.ctx, conversation_id)
        .await
        .map_err(reject)?;
    auth.ctx
        .require_agent(conversation.agent_id)
        .map_err(reject)?;

    state
        .conversations
        .delete(&auth.ctx, conversation_id)
        .await
        .map_err(reject)?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

fn view_of(conversation: &anthovai_conversation::Conversation) -> ConversationView {
    ConversationView {
        id: conversation.id.to_string(),
        agent_id: conversation.agent_id.to_string(),
        user_id: conversation.external_user_id.clone(),
        message_count: conversation.message_count,
        last_message_at: conversation.last_message_at.map(|at| at.to_rfc3339()),
    }
}

// ---- usage ----------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct UsageResponse {
    pub totals: Totals,
    pub quota: Quota,
}

#[derive(Serialize, ToSchema)]
pub struct Totals {
    pub messages: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Serialize, ToSchema)]
pub struct Quota {
    pub messages_limit: i64,
    pub messages_used: i64,
    pub resets_at: String,
}

#[utoipa::path(
    get,
    path = "/v1/usage",
    tag = "Usage",
    responses(
        (status = 200, description = "This month, so far", body = UsageResponse),
        (status = 403, description = "The key lacks the `usage:read` scope", body = crate::error::ErrorBody),
    ),
    security(("api_key" = []))
)]
async fn usage(State(state): State<AppState>, auth: ApiKeyAuth) -> Result<Response, ApiError> {
    let reject = |e: DomainError| ApiError::from_domain(e, auth.request_id.clone());
    auth.ctx.require_scope(Scope::UsageRead).map_err(reject)?;

    let counters = state.conversations.usage(&auth.ctx).await.map_err(reject)?;
    let limits = auth.ctx.plan.limits();

    Ok(Json(UsageResponse {
        totals: Totals {
            messages: counters.messages,
            input_tokens: counters.input_tokens,
            output_tokens: counters.output_tokens,
        },
        quota: Quota {
            messages_limit: limits.messages_per_month,
            messages_used: counters.messages,
            resets_at: next_period_start(state.clock.now()).to_rfc3339(),
        },
    })
    .into_response())
}

/// The first moment of next month, when the allowance starts again.
fn next_period_start(now: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    use chrono::{Datelike, NaiveDate, TimeZone, Utc};

    let (year, month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };

    NaiveDate::from_ymd_opt(year, month, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| Utc.from_utc_datetime(&dt))
        .unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;

    #[test]
    fn the_allowance_resets_at_the_start_of_next_month() {
        let now = DateTime::parse_from_rfc3339("2026-09-17T13:45:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            next_period_start(now).to_rfc3339(),
            "2026-10-01T00:00:00+00:00"
        );
    }

    #[test]
    fn december_rolls_into_the_new_year() {
        let now = DateTime::parse_from_rfc3339("2026-12-31T23:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            next_period_start(now).to_rfc3339(),
            "2027-01-01T00:00:00+00:00"
        );
    }
}
