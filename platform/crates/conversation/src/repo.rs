//! Repositories for conversations and messages.

use anthovai_core::{AgentId, ApiKeyId, ConversationId, DomainError, MessageId, RequestId, Result};
use anthovai_db::repo::id;
use anthovai_db::{on_missing_reference, TenantDb};
use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::{Conversation, Message};

/// Find a conversation, or start one.
///
/// The agent is checked as well as the tenant: a conversation belongs to one
/// agent, and continuing it against a different one would mix two agents'
/// histories into a single thread.
pub async fn get_or_create(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    conversation_id: Option<ConversationId>,
    api_key_id: Option<ApiKeyId>,
    external_user_id: Option<&str>,
) -> Result<Conversation> {
    if let Some(id) = conversation_id {
        let existing = find(db, id).await?;
        if existing.agent_id != agent_id {
            // Reported as missing rather than as a mismatch: a caller must not
            // be able to discover which conversations exist for other agents.
            return Err(DomainError::NotFound("conversation"));
        }
        return Ok(existing);
    }

    create(db, agent_id, api_key_id, external_user_id).await
}

pub async fn create(
    db: &mut TenantDb<'_>,
    agent_id: AgentId,
    api_key_id: Option<ApiKeyId>,
    external_user_id: Option<&str>,
) -> Result<Conversation> {
    let conversation = Conversation {
        id: ConversationId::new(),
        agent_id,
        external_user_id: external_user_id.map(str::to_owned),
        message_count: 0,
        last_message_at: None,
    };
    let tenant = db.tenant_key();

    sqlx::query(
        "INSERT INTO conversations (id, tenant_id, agent_id, api_key_id, external_user_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(conversation.id.to_db())
    .bind(&tenant)
    .bind(agent_id.to_db())
    .bind(api_key_id.map(|k| k.to_db()))
    .bind(external_user_id)
    .execute(db.conn())
    .await
    .map_err(|e| on_missing_reference(e, "agent"))?;

    Ok(conversation)
}

pub async fn find(db: &mut TenantDb<'_>, conversation_id: ConversationId) -> Result<Conversation> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        "SELECT id, agent_id, external_user_id, message_count, last_message_at
         FROM conversations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(conversation_id.to_db())
    .bind(&tenant)
    .fetch_optional(db.conn())
    .await?
    .ok_or(DomainError::NotFound("conversation"))?;

    conversation_row(&row)
}

#[derive(Clone, Debug, Default)]
pub struct ConversationFilter {
    pub agent_id: Option<AgentId>,
    pub external_user_id: Option<String>,
}

pub async fn list(
    db: &mut TenantDb<'_>,
    filter: &ConversationFilter,
    limit: i64,
) -> Result<Vec<Conversation>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT id, agent_id, external_user_id, message_count, last_message_at
         FROM conversations
         WHERE tenant_id = $1
           AND ($2::text IS NULL OR agent_id = $2)
           AND ($3::text IS NULL OR external_user_id = $3)
         ORDER BY coalesce(last_message_at, created_at) DESC
         LIMIT $4",
    )
    .bind(&tenant)
    .bind(filter.agent_id.map(|a| a.to_db()))
    .bind(filter.external_user_id.as_deref())
    .bind(limit)
    .fetch_all(db.conn())
    .await?;

    rows.iter().map(conversation_row).collect()
}

/// What a customer said and what the agent answered, as one exchange.
#[derive(Clone, Debug)]
pub struct Exchange {
    pub question: String,
    pub answer: String,
    pub request_id: RequestId,
    pub sources: serde_json::Value,
    pub model_used: Option<String>,
    pub grounded: bool,
    pub metadata: serde_json::Value,
}

/// Record both halves of a turn together.
///
/// One statement each, in the caller's transaction: a question stored without
/// its answer would come back as history and confuse the next turn.
pub async fn append_exchange(
    db: &mut TenantDb<'_>,
    conversation_id: ConversationId,
    exchange: &Exchange,
) -> Result<MessageId> {
    let tenant = db.tenant_key();
    let question_id = MessageId::new();
    let answer_id = MessageId::new();

    sqlx::query(
        "INSERT INTO messages (id, tenant_id, conversation_id, request_id, role, content)
         VALUES ($1, $2, $3, $4, 'user', $5)",
    )
    .bind(question_id.to_db())
    .bind(&tenant)
    .bind(conversation_id.to_db())
    .bind(exchange.request_id.to_db())
    .bind(&exchange.question)
    .execute(db.conn())
    .await?;

    sqlx::query(
        "INSERT INTO messages
           (id, tenant_id, conversation_id, request_id, role, content, sources,
            model_used, grounded, metadata)
         VALUES ($1, $2, $3, $4, 'assistant', $5, $6, $7, $8, $9)",
    )
    .bind(answer_id.to_db())
    .bind(&tenant)
    .bind(conversation_id.to_db())
    .bind(exchange.request_id.to_db())
    .bind(&exchange.answer)
    .bind(&exchange.sources)
    .bind(&exchange.model_used)
    .bind(exchange.grounded)
    .bind(&exchange.metadata)
    .execute(db.conn())
    .await?;

    sqlx::query(
        "UPDATE conversations
         SET message_count = message_count + 2, last_message_at = now(), updated_at = now()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(conversation_id.to_db())
    .bind(&tenant)
    .execute(db.conn())
    .await?;

    Ok(answer_id)
}

/// The most recent messages, oldest first.
///
/// Read newest-first with a limit and then reversed, so the query uses the
/// index rather than sorting a conversation that may run to thousands of turns.
pub async fn recent_messages(
    db: &mut TenantDb<'_>,
    conversation_id: ConversationId,
    limit: i64,
) -> Result<Vec<Message>> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let tenant = db.tenant_key();

    let rows = sqlx::query(
        "SELECT id, conversation_id, role, content, created_at
         FROM messages
         WHERE conversation_id = $1 AND tenant_id = $2
         ORDER BY seq DESC
         LIMIT $3",
    )
    .bind(conversation_id.to_db())
    .bind(&tenant)
    .bind(limit)
    .fetch_all(db.conn())
    .await?;

    let mut messages = rows.iter().map(message_row).collect::<Result<Vec<_>>>()?;
    messages.reverse();
    Ok(messages)
}

/// Everything in a conversation, for the dashboard and for a data request.
pub async fn all_messages(
    db: &mut TenantDb<'_>,
    conversation_id: ConversationId,
    limit: i64,
) -> Result<Vec<MessageDetail>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT id, conversation_id, role, content, sources, grounded, model_used,
                metadata, created_at
         FROM messages
         WHERE conversation_id = $1 AND tenant_id = $2
         ORDER BY seq
         LIMIT $3",
    )
    .bind(conversation_id.to_db())
    .bind(&tenant)
    .bind(limit)
    .fetch_all(db.conn())
    .await?;

    rows.iter()
        .map(|row| {
            Ok(MessageDetail {
                message: message_row(row)?,
                sources: row.try_get("sources").map_err(sql)?,
                grounded: row.try_get("grounded").map_err(sql)?,
                model_used: row.try_get("model_used").map_err(sql)?,
                metadata: row.try_get("metadata").map_err(sql)?,
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct MessageDetail {
    pub message: Message,
    pub sources: Option<serde_json::Value>,
    pub grounded: Option<bool>,
    pub model_used: Option<String>,
    pub metadata: serde_json::Value,
}

/// Delete a conversation and its messages outright.
///
/// A real deletion, not a flag: this is what answers a request to erase
/// someone's data, and a soft-deleted row would not.
pub async fn delete(db: &mut TenantDb<'_>, conversation_id: ConversationId) -> Result<()> {
    let tenant = db.tenant_key();
    let affected = sqlx::query("DELETE FROM conversations WHERE id = $1 AND tenant_id = $2")
        .bind(conversation_id.to_db())
        .bind(&tenant)
        .execute(db.conn())
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(DomainError::NotFound("conversation"));
    }
    Ok(())
}

/// Erase everything belonging to one end user, across every agent.
pub async fn delete_for_external_user(
    db: &mut TenantDb<'_>,
    external_user_id: &str,
) -> Result<u64> {
    let tenant = db.tenant_key();
    let affected =
        sqlx::query("DELETE FROM conversations WHERE tenant_id = $1 AND external_user_id = $2")
            .bind(&tenant)
            .bind(external_user_id)
            .execute(db.conn())
            .await?
            .rows_affected();

    Ok(affected)
}

fn conversation_row(row: &sqlx::postgres::PgRow) -> Result<Conversation> {
    Ok(Conversation {
        id: id(row, "id")?,
        agent_id: id(row, "agent_id")?,
        external_user_id: row.try_get("external_user_id").map_err(sql)?,
        message_count: row.try_get("message_count").map_err(sql)?,
        last_message_at: row
            .try_get::<Option<DateTime<Utc>>, _>("last_message_at")
            .map_err(sql)?,
    })
}

fn message_row(row: &sqlx::postgres::PgRow) -> Result<Message> {
    let role: String = row.try_get("role").map_err(sql)?;

    Ok(Message {
        id: id(row, "id")?,
        conversation_id: id(row, "conversation_id")?,
        role: role.parse()?,
        content: row.try_get("content").map_err(sql)?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at").map_err(sql)?,
    })
}

fn sql(err: sqlx::Error) -> DomainError {
    DomainError::Database(err)
}
