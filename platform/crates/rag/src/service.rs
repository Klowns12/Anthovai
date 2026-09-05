//! Turning a question into a grounded answer.
//!
//! The order of the steps is the design. Quota is checked before anything is
//! paid for; retrieval happens before the model is called, so an agent with
//! nothing to say can say so without spending a request; and the answer is
//! matched back to its sources before it reaches the customer, so a citation
//! always points at a passage that was actually offered.

use std::sync::Arc;

use anthovai_agent::{AgentService, PromptBuilder, ResolvedAgent};
use anthovai_conversation::{repo as conversation_repo, Exchange};
use anthovai_core::{AgentId, Clock, ConversationId, Feature, RequestId, Result, TenantCtx};
use anthovai_db::Db;
use anthovai_guardrails::Guardrails;
use anthovai_inference::{ChatMessage, ChatRequestTemplate, ModelRouter, RoutingHints, TokenUsage};
use anthovai_retrieval::{RetrievalConfig, Retriever, SearchFilters, Source};
use anthovai_usage::{repo as usage_repo, UsageKind, UsageRecord};
use chrono::Utc;
use tracing::{debug, warn};

use crate::{fallback_output, model_output, short_circuit, ChatOutput};

pub struct ChatService {
    db: Db,
    agents: Arc<AgentService>,
    retriever: Arc<Retriever>,
    router: Arc<ModelRouter>,
    clock: Clock,
}

/// What a caller asks for.
#[derive(Clone, Debug)]
pub struct ChatInput {
    pub agent_id: AgentId,
    pub message: String,
    pub conversation_id: Option<ConversationId>,
    pub external_user_id: Option<String>,
    pub document_ids: Vec<String>,
    /// Include the retrieved passages and their scores in the answer. The
    /// dashboard playground uses this; the public API does not offer it.
    pub debug: bool,
}

/// The answer, and everything worth knowing about how it was produced.
#[derive(Clone, Debug)]
pub struct ChatResult {
    pub output: ChatOutput,
    pub conversation_id: ConversationId,
    pub message_id: anthovai_core::MessageId,
    pub request_id: RequestId,
    pub usage: TokenUsage,
    /// Present only for plans allowed to see which model answered.
    pub model: Option<AnsweredBy>,
    pub latency_ms: i64,
    pub debug: Option<RetrievalDebug>,
}

#[derive(Clone, Debug)]
pub struct AnsweredBy {
    pub provider: String,
    pub model: String,
}

/// Why these passages, for the playground.
#[derive(Clone, Debug)]
pub struct RetrievalDebug {
    pub passages: Vec<DebugPassage>,
    pub embedding_tokens: u32,
}

#[derive(Clone, Debug)]
pub struct DebugPassage {
    pub chunk_id: String,
    pub document_id: String,
    pub score: f32,
    pub vector_score: Option<f32>,
    pub snippet: String,
}

/// Which version of the agent to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    /// What customers are served.
    Published,
    /// What the dashboard is editing. Never billed against the message quota.
    Draft,
}

impl ChatService {
    pub fn new(
        db: Db,
        agents: Arc<AgentService>,
        retriever: Arc<Retriever>,
        router: Arc<ModelRouter>,
        clock: Clock,
    ) -> Self {
        Self {
            db,
            agents,
            retriever,
            router,
            clock,
        }
    }

    pub async fn chat(&self, ctx: &TenantCtx, input: ChatInput) -> Result<ChatResult> {
        self.answer(ctx, input, Version::Published).await
    }

    /// The playground. Runs the draft, so an edit can be tried before anyone
    /// else sees it, and does not spend the customer's allowance.
    pub async fn test(&self, ctx: &TenantCtx, input: ChatInput) -> Result<ChatResult> {
        self.answer(ctx, input, Version::Draft).await
    }

    async fn answer(
        &self,
        ctx: &TenantCtx,
        input: ChatInput,
        version: Version,
    ) -> Result<ChatResult> {
        let started = std::time::Instant::now();
        let request_id = ctx.request_id;

        // 1. The agent. This is where the key's agent scope and the agent's
        //    status are enforced, before anything else is spent.
        let agent = match version {
            Version::Published => self.agents.load_published(ctx, input.agent_id).await?,
            Version::Draft => self.agents.load_draft(ctx, input.agent_id).await?,
        };

        // 2. Quota, before any paid call. A tenant over their allowance is told
        //    so rather than being served and billed.
        let kind = match version {
            Version::Published => UsageKind::Chat,
            Version::Draft => UsageKind::Test,
        };
        if kind.counts_towards_message_quota() {
            self.check_quota(ctx).await?;
        }

        // 3. The question itself.
        let guardrails = Guardrails {
            max_input_chars: agent.config.guardrails.max_input_chars,
        };
        let verdict = guardrails.check_input(&input.message)?;
        if verdict.injection_suspected {
            // Logged, not blocked: these patterns catch real attempts and
            // ordinary questions alike, and with no tools wired up an injected
            // instruction has nothing to act on.
            warn!(
                agent_id = %agent.id,
                pattern = ?verdict.matched_pattern,
                "the question looks like a prompt-injection attempt"
            );
        }

        // 4. The conversation this turn belongs to.
        let mut db = self.db.tenant(ctx).await?;
        let conversation = conversation_repo::get_or_create(
            &mut db,
            agent.id,
            input.conversation_id,
            ctx.api_key_id(),
            input.external_user_id.as_deref(),
        )
        .await?;
        let history = conversation_repo::recent_messages(
            &mut db,
            conversation.id,
            (agent.config.behavior.history_turns * 2) as i64,
        )
        .await?;
        db.commit().await?;

        // 5. What the agent knows about it.
        let retrieved = self.retrieve(ctx, &agent, &input).await?;

        // 6. A strict agent with nothing relevant has nothing to say, and
        //    asking a model to say it costs money and invites invention.
        if short_circuit(
            agent.config.behavior.strict_knowledge,
            retrieved.candidates.len(),
        )
        .is_some()
        {
            debug!(agent_id = %agent.id, "nothing relevant retrieved, answering from the fallback");

            let output = fallback_output(&agent.config.behavior.fallback_message);
            let result = self
                .persist(
                    ctx,
                    &agent,
                    conversation.id,
                    &input,
                    output,
                    TokenUsage::default(),
                    None,
                    kind,
                    retrieved.embedding_tokens,
                    started,
                    request_id,
                )
                .await?;

            return Ok(self.with_debug(result, &retrieved, input.debug));
        }

        // 7. The prompt, and the model.
        let organization_name = self.organization_name(ctx).await;
        let prompt = PromptBuilder::new(
            &agent.config,
            &organization_name,
            &self.clock.now().format("%-d %B %Y").to_string(),
        )
        .build(&retrieved.context.block);

        let messages = build_messages(&history, &input.message);
        let context_tokens = rough_tokens(&prompt)
            + messages
                .iter()
                .map(|m| rough_tokens(&m.content))
                .sum::<u32>();

        let template = ChatRequestTemplate {
            system: prompt,
            messages,
            max_tokens: agent.config.response.length.max_output_tokens(),
            reasoning: agent.config.reasoning(),
            stop: Vec::new(),
            // A hash, so a provider can group abuse by customer without us
            // handing over who the customer is.
            tenant_hash: tenant_hash(ctx),
            request_id: request_id.to_string(),
        };

        let routed = self
            .router
            .chat(
                &agent.config.model_policy,
                &RoutingHints::new(agent.config.reasoning(), context_tokens),
                template,
            )
            .await?;

        // 8. Match the answer back to what was offered.
        let mut output = model_output(
            routed.response.text.clone(),
            &retrieved.context.sources,
            agent.config.behavior.citations,
        );

        if guardrails
            .check_output(&output.answer, &agent.config.instructions)
            .leaked_instructions
        {
            warn!(agent_id = %agent.id, "the answer reproduced the agent's instructions");
            output = fallback_output(&agent.config.behavior.fallback_message);
        }

        let answered_by = AnsweredBy {
            provider: routed.provider.to_string(),
            model: routed.response.model.clone(),
        };

        let result = self
            .persist(
                ctx,
                &agent,
                conversation.id,
                &input,
                output,
                routed.response.usage,
                Some((answered_by, routed.model_id.clone())),
                kind,
                retrieved.embedding_tokens,
                started,
                request_id,
            )
            .await?;

        Ok(self.with_debug(result, &retrieved, input.debug))
    }

    async fn retrieve(
        &self,
        ctx: &TenantCtx,
        agent: &ResolvedAgent,
        input: &ChatInput,
    ) -> Result<anthovai_retrieval::Retrieved> {
        if agent.knowledge_base_ids.is_empty() {
            return Ok(anthovai_retrieval::Retrieved::default());
        }

        let config = RetrievalConfig {
            top_k: agent.config.retrieval.top_k,
            context_token_budget: agent.config.retrieval.context_token_budget,
            min_relevance: agent.config.retrieval.min_relevance,
            hybrid: agent.config.retrieval.hybrid,
            mmr_lambda: agent.config.retrieval.mmr_lambda,
            ..RetrievalConfig::default()
        };

        self.retriever
            .retrieve(
                ctx,
                &agent.knowledge_base_ids,
                &input.message,
                &SearchFilters {
                    document_ids: input.document_ids.clone(),
                },
                &config,
            )
            .await
    }

    /// Store the turn and what it cost, in one transaction.
    ///
    /// Not in the background: a turn that is answered but not recorded is one
    /// the customer is not billed for and cannot see in their history, and
    /// finding that out later is worse than the few milliseconds this costs.
    #[allow(clippy::too_many_arguments)]
    async fn persist(
        &self,
        ctx: &TenantCtx,
        agent: &ResolvedAgent,
        conversation_id: ConversationId,
        input: &ChatInput,
        output: ChatOutput,
        usage: TokenUsage,
        answered_by: Option<(AnsweredBy, String)>,
        kind: UsageKind,
        embedding_tokens: u32,
        started: std::time::Instant,
        request_id: RequestId,
    ) -> Result<ChatResult> {
        let latency_ms = started.elapsed().as_millis() as i64;
        let (answered, model_id) = match answered_by {
            Some((answered, model_id)) => (Some(answered), Some(model_id)),
            None => (None, None),
        };

        let cost_usd_micro = model_id
            .as_deref()
            .and_then(|id| self.router.registry().by_id(id))
            .map(|spec| spec.cost_micro(usage.input_tokens, usage.output_tokens) as i64)
            .unwrap_or(0);

        let mut db = self.db.tenant(ctx).await?;

        let message_id = conversation_repo::append_exchange(
            &mut db,
            conversation_id,
            &Exchange {
                question: input.message.clone(),
                answer: output.answer.clone(),
                request_id,
                sources: serde_json::to_value(&output.sources).unwrap_or(serde_json::Value::Null),
                model_used: answered
                    .as_ref()
                    .map(|a| format!("{}:{}", a.provider, a.model)),
                grounded: output.grounded,
                metadata: serde_json::json!({
                    "agent_version": agent.version,
                    "used_fallback": output.used_fallback,
                    "latency_ms": latency_ms,
                }),
            },
        )
        .await?;

        usage_repo::record(
            &mut db,
            &UsageRecord {
                org_id: ctx.org_id,
                agent_id: Some(agent.id),
                api_key_id: ctx.api_key_id(),
                request_id,
                kind,
                provider: answered.as_ref().map(|a| a.provider.clone()),
                model: answered.as_ref().map(|a| a.model.clone()),
                input_tokens: usage.input_tokens as i32,
                output_tokens: usage.output_tokens as i32,
                embedding_tokens: embedding_tokens as i32,
                latency_ms: Some(latency_ms as i32),
                cost_usd_micro,
                created_at: Utc::now(),
            },
        )
        .await?;

        db.commit().await?;

        // Which model answered is ours to know and the customer's to see only
        // on a plan that pays for the choice.
        let model = answered.filter(|_| ctx.plan.allows(Feature::RevealProviderInResponse));

        Ok(ChatResult {
            output,
            conversation_id,
            message_id,
            request_id,
            usage,
            model,
            latency_ms,
            debug: None,
        })
    }

    fn with_debug(
        &self,
        mut result: ChatResult,
        retrieved: &anthovai_retrieval::Retrieved,
        wanted: bool,
    ) -> ChatResult {
        if wanted {
            result.debug = Some(RetrievalDebug {
                passages: retrieved
                    .candidates
                    .iter()
                    .map(|c| DebugPassage {
                        chunk_id: c.chunk_id.clone(),
                        document_id: c.document_id.clone(),
                        score: c.score,
                        vector_score: c.vector_score,
                        snippet: c.content.chars().take(200).collect(),
                    })
                    .collect(),
                embedding_tokens: retrieved.embedding_tokens,
            });
        }
        result
    }

    async fn check_quota(&self, ctx: &TenantCtx) -> Result<()> {
        let mut db = self.db.tenant(ctx).await?;
        let counters = usage_repo::counters(&mut db, self.clock.now()).await?;
        db.commit().await?;

        anthovai_usage::check_message_quota(ctx.plan, &counters)
    }

    /// The organization's name, for the prompt. A failure here is not worth
    /// failing the request over.
    async fn organization_name(&self, ctx: &TenantCtx) -> String {
        let Ok(mut db) = self.db.tenant(ctx).await else {
            return "this organization".to_owned();
        };
        let name = anthovai_tenant::repo::get_organization(&mut db)
            .await
            .map(|org| org.name)
            .unwrap_or_else(|_| "this organization".to_owned());
        let _ = db.commit().await;
        name
    }
}

/// The history, then the new question.
fn build_messages(history: &[anthovai_conversation::Message], question: &str) -> Vec<ChatMessage> {
    use anthovai_conversation::MessageRole;

    let mut messages: Vec<ChatMessage> = history
        .iter()
        .filter_map(|m| match m.role {
            MessageRole::User => Some(ChatMessage::user(&m.content)),
            MessageRole::Assistant => Some(ChatMessage::assistant(&m.content)),
            // Ours, not part of the conversation.
            MessageRole::System => None,
        })
        .collect();

    messages.push(ChatMessage::user(question));
    messages
}

/// An opaque, stable identifier for a tenant, for provider-side abuse tracking.
fn tenant_hash(ctx: &TenantCtx) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(ctx.org_id.to_db().as_bytes()))
        .chars()
        .take(32)
        .collect()
}

/// Enough to choose a model tier by context size. The provider's own count is
/// what ends up on the usage record.
fn rough_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    words.max(chars / 4) as u32
}

/// Sources, for a caller that wants them separately.
pub fn sources_of(result: &ChatResult) -> &[Source] {
    &result.output.sources
}

impl std::fmt::Debug for ChatService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChatService")
    }
}

#[cfg(test)]
mod tests {
    use anthovai_core::{OrgId, Plan};

    use super::*;

    fn ctx() -> TenantCtx {
        TenantCtx::system(OrgId::new(), Plan::Free)
    }

    #[test]
    fn the_tenant_hash_hides_the_tenant() {
        let ctx = ctx();
        let hash = tenant_hash(&ctx);

        assert_eq!(hash.len(), 32);
        assert!(!hash.contains(&ctx.org_id.to_db()));
        assert_eq!(hash, tenant_hash(&ctx), "the same tenant hashes the same");
    }

    #[test]
    fn two_tenants_hash_differently() {
        assert_ne!(tenant_hash(&ctx()), tenant_hash(&ctx()));
    }

    #[test]
    fn the_question_comes_last() {
        use anthovai_conversation::{Message, MessageRole};
        use anthovai_core::{ConversationId, MessageId};

        let history = vec![
            Message {
                id: MessageId::new(),
                conversation_id: ConversationId::new(),
                role: MessageRole::User,
                content: "earlier question".into(),
                created_at: Utc::now(),
            },
            Message {
                id: MessageId::new(),
                conversation_id: ConversationId::new(),
                role: MessageRole::Assistant,
                content: "earlier answer".into(),
                created_at: Utc::now(),
            },
        ];

        let messages = build_messages(&history, "the new question");

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2].content, "the new question");
        assert_eq!(messages[0].content, "earlier question");
    }

    #[test]
    fn our_own_system_messages_are_not_replayed_as_history() {
        use anthovai_conversation::{Message, MessageRole};
        use anthovai_core::{ConversationId, MessageId};

        let history = vec![Message {
            id: MessageId::new(),
            conversation_id: ConversationId::new(),
            role: MessageRole::System,
            content: "an internal note".into(),
            created_at: Utc::now(),
        }];

        let messages = build_messages(&history, "question");
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn drafts_do_not_spend_the_message_allowance() {
        assert!(UsageKind::Chat.counts_towards_message_quota());
        assert!(!UsageKind::Test.counts_towards_message_quota());
    }
}
