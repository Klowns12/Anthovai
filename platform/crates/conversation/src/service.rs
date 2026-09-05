//! Reading and erasing conversations.
//!
//! Writing them is the chat service's job — a turn is only ever recorded
//! alongside the usage it incurred, in one transaction, so the two cannot
//! disagree. What is here is everything else a customer can do with a
//! conversation once it exists.

use anthovai_core::{AgentId, ConversationId, Permission, Result, TenantCtx};
use anthovai_db::Db;
use anthovai_usage::{repo as usage_repo, UsageCounters};

use crate::repo::{self, ConversationFilter, MessageDetail};
use crate::Conversation;

/// How much of a conversation is returned at once. Long enough for any real
/// thread, bounded so one request cannot ask for a year of history.
const MESSAGE_LIMIT: i64 = 200;

#[derive(Clone, Debug)]
pub struct ConversationService {
    db: Db,
    clock: anthovai_core::Clock,
}

impl ConversationService {
    pub fn new(db: Db, clock: anthovai_core::Clock) -> Self {
        Self { db, clock }
    }

    pub async fn list(
        &self,
        ctx: &TenantCtx,
        agent_id: Option<AgentId>,
        external_user_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Conversation>> {
        ctx.require(Permission::Chat)?;

        let mut db = self.db.tenant(ctx).await?;
        let conversations = repo::list(
            &mut db,
            &ConversationFilter {
                agent_id,
                external_user_id: external_user_id.map(str::to_owned),
            },
            limit,
        )
        .await?;
        db.commit().await?;

        Ok(conversations)
    }

    pub async fn detail(
        &self,
        ctx: &TenantCtx,
        conversation_id: ConversationId,
    ) -> Result<(Conversation, Vec<MessageDetail>)> {
        ctx.require(Permission::Chat)?;

        let mut db = self.db.tenant(ctx).await?;
        let conversation = repo::find(&mut db, conversation_id).await?;
        let messages = repo::all_messages(&mut db, conversation_id, MESSAGE_LIMIT).await?;
        db.commit().await?;

        Ok((conversation, messages))
    }

    pub async fn delete(&self, ctx: &TenantCtx, conversation_id: ConversationId) -> Result<()> {
        ctx.require(Permission::Chat)?;

        let mut db = self.db.tenant(ctx).await?;
        repo::delete(&mut db, conversation_id).await?;
        db.commit().await
    }

    /// Erase everything belonging to one end user of a customer's product.
    ///
    /// The customer holds the relationship with that person; when they are
    /// asked to erase them, this is what they call.
    pub async fn delete_for_user(&self, ctx: &TenantCtx, external_user_id: &str) -> Result<u64> {
        ctx.require(Permission::Chat)?;

        let mut db = self.db.tenant(ctx).await?;
        let removed = repo::delete_for_external_user(&mut db, external_user_id).await?;
        db.commit().await?;

        Ok(removed)
    }

    pub async fn usage(&self, ctx: &TenantCtx) -> Result<UsageCounters> {
        ctx.require(Permission::UsageRead)?;

        let mut db = self.db.tenant(ctx).await?;
        let counters = usage_repo::counters(&mut db, self.clock.now()).await?;
        db.commit().await?;

        Ok(counters)
    }

    pub async fn daily_usage(
        &self,
        ctx: &TenantCtx,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<anthovai_usage::DailyUsage>> {
        ctx.require(Permission::UsageRead)?;

        let mut db = self.db.tenant(ctx).await?;
        let rows = usage_repo::daily(&mut db, from, to).await?;
        db.commit().await?;

        Ok(rows)
    }
}
