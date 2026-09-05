//! Agent services.
//!
//! An agent is a draft and a published version living side by side. The
//! dashboard playground always runs the draft; public traffic always runs the
//! published one. Editing therefore never disturbs what customers are serving.

use anthovai_core::{
    AgentId, DomainError, KnowledgeBaseId, Permission, Result, TenantCtx, WorkspaceId,
};
use anthovai_db::Db;

use crate::repo::{self, AgentRow, AgentVersionRow};
use crate::{AgentConfig, AgentStatus, ResolvedAgent};

#[derive(Clone, Debug)]
pub struct AgentService {
    db: Db,
}

/// An agent as the dashboard sees it: both versions, and the history.
#[derive(Clone, Debug)]
pub struct AgentDetail {
    pub agent: AgentRow,
    pub draft: Option<AgentVersionRow>,
    pub published: Option<AgentVersionRow>,
    pub versions: Vec<AgentVersionRow>,
    pub knowledge_base_ids: Vec<KnowledgeBaseId>,
}

#[derive(Clone, Debug)]
pub struct CreateAgent {
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub description: Option<String>,
    pub config: AgentConfig,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateAgent {
    pub name: Option<String>,
    pub description: Option<String>,
    /// A new configuration becomes a new draft version. The published one is
    /// untouched until someone publishes.
    pub config: Option<AgentConfig>,
}

/// How many versions the dashboard shows. Enough to roll back through a bad
/// afternoon, not the entire history of the agent.
const VERSION_HISTORY_LIMIT: i64 = 50;

impl AgentService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn create(&self, ctx: &TenantCtx, request: CreateAgent) -> Result<AgentDetail> {
        ctx.require(Permission::AgentWrite)?;
        validate_name(&request.name)?;
        request.config.validate(ctx.plan)?;

        let agent_id = AgentId::new();
        let version_id = anthovai_core::AgentVersionId::new();

        let mut db = self.db.tenant(ctx).await?;

        let existing = repo::count_agents(&mut db).await?;
        if existing >= ctx.plan.limits().max_agents {
            return Err(DomainError::QuotaExceeded("agent_limit_reached"));
        }

        repo::insert_agent(
            &mut db,
            agent_id,
            request.workspace_id,
            request.name.trim(),
            request.description.as_deref(),
            ctx.user_id(),
        )
        .await?;
        repo::insert_version(
            &mut db,
            version_id,
            agent_id,
            1,
            &request.config,
            ctx.user_id(),
        )
        .await?;
        repo::set_draft_version(&mut db, agent_id, version_id).await?;

        let detail = self.load_detail(&mut db, agent_id).await?;
        db.commit().await?;
        Ok(detail)
    }

    pub async fn get(&self, ctx: &TenantCtx, agent_id: AgentId) -> Result<AgentDetail> {
        ctx.require(Permission::AgentRead)?;
        let mut db = self.db.tenant(ctx).await?;
        let detail = self.load_detail(&mut db, agent_id).await?;
        db.commit().await?;
        Ok(detail)
    }

    pub async fn list(
        &self,
        ctx: &TenantCtx,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Vec<AgentRow>> {
        ctx.require(Permission::AgentRead)?;
        let mut db = self.db.tenant(ctx).await?;
        let agents = repo::list_agents(&mut db, workspace_id).await?;
        db.commit().await?;
        Ok(agents)
    }

    /// Editing produces a new draft version rather than mutating one, so a
    /// rollback always has something to go back to.
    pub async fn update(
        &self,
        ctx: &TenantCtx,
        agent_id: AgentId,
        request: UpdateAgent,
    ) -> Result<AgentDetail> {
        ctx.require(Permission::AgentWrite)?;
        if let Some(config) = &request.config {
            config.validate(ctx.plan)?;
        }

        let mut db = self.db.tenant(ctx).await?;
        let agent = repo::find_agent(&mut db, agent_id).await?;
        if agent.status == AgentStatus::Archived {
            return Err(DomainError::Gone("agent_archived"));
        }

        if request.name.is_some() || request.description.is_some() {
            let name = match &request.name {
                Some(name) => {
                    validate_name(name)?;
                    name.trim().to_owned()
                }
                None => agent.name.clone(),
            };
            let description = request.description.or(agent.description.clone());
            repo::update_agent_details(&mut db, agent_id, &name, description.as_deref()).await?;
        }

        if let Some(config) = request.config {
            let version = repo::next_version_number(&mut db, agent_id).await?;
            let version_id = anthovai_core::AgentVersionId::new();
            repo::insert_version(
                &mut db,
                version_id,
                agent_id,
                version,
                &config,
                ctx.user_id(),
            )
            .await?;
            repo::set_draft_version(&mut db, agent_id, version_id).await?;
        }

        let detail = self.load_detail(&mut db, agent_id).await?;
        db.commit().await?;
        Ok(detail)
    }

    /// Make the draft live.
    pub async fn publish(&self, ctx: &TenantCtx, agent_id: AgentId) -> Result<AgentDetail> {
        ctx.require(Permission::AgentPublish)?;

        let mut db = self.db.tenant(ctx).await?;
        let agent = repo::find_agent(&mut db, agent_id).await?;
        if agent.status == AgentStatus::Archived {
            return Err(DomainError::Gone("agent_archived"));
        }

        let draft = agent
            .draft_version_id
            .ok_or(DomainError::Conflict("nothing_to_publish"))?;
        repo::set_published_version(&mut db, agent_id, draft).await?;

        let detail = self.load_detail(&mut db, agent_id).await?;
        db.commit().await?;
        Ok(detail)
    }

    /// Publish an older version again. The draft is left alone: rolling back a
    /// live mistake should not also throw away the work in progress.
    pub async fn rollback(
        &self,
        ctx: &TenantCtx,
        agent_id: AgentId,
        version: i32,
    ) -> Result<AgentDetail> {
        ctx.require(Permission::AgentPublish)?;

        let mut db = self.db.tenant(ctx).await?;
        let agent = repo::find_agent(&mut db, agent_id).await?;
        if agent.status == AgentStatus::Archived {
            return Err(DomainError::Gone("agent_archived"));
        }

        let target = repo::find_version_by_number(&mut db, agent_id, version).await?;
        repo::set_published_version(&mut db, agent_id, target.id).await?;

        let detail = self.load_detail(&mut db, agent_id).await?;
        db.commit().await?;
        Ok(detail)
    }

    /// Take an agent offline without losing it. Public traffic gets a clear
    /// error; the dashboard playground still works, so it can be fixed.
    pub async fn pause(&self, ctx: &TenantCtx, agent_id: AgentId) -> Result<()> {
        self.transition(ctx, agent_id, AgentStatus::Paused).await
    }

    pub async fn resume(&self, ctx: &TenantCtx, agent_id: AgentId) -> Result<()> {
        ctx.require(Permission::AgentPublish)?;
        let mut db = self.db.tenant(ctx).await?;
        let agent = repo::find_agent(&mut db, agent_id).await?;

        match agent.status {
            AgentStatus::Archived => return Err(DomainError::Gone("agent_archived")),
            // Resuming an agent that was never published would put an agent
            // with no configuration in front of customers.
            _ if agent.published_version_id.is_none() => {
                return Err(DomainError::Conflict("nothing_to_publish"))
            }
            _ => {}
        }

        repo::set_status(&mut db, agent_id, AgentStatus::Active).await?;
        db.commit().await
    }

    pub async fn archive(&self, ctx: &TenantCtx, agent_id: AgentId) -> Result<()> {
        ctx.require(Permission::AgentWrite)?;
        let mut db = self.db.tenant(ctx).await?;
        repo::archive_agent(&mut db, agent_id).await?;
        db.commit().await
    }

    pub async fn set_knowledge_bases(
        &self,
        ctx: &TenantCtx,
        agent_id: AgentId,
        knowledge_base_ids: &[KnowledgeBaseId],
    ) -> Result<()> {
        ctx.require(Permission::AgentWrite)?;
        let mut db = self.db.tenant(ctx).await?;
        repo::find_agent(&mut db, agent_id).await?;
        repo::set_knowledge_bases(&mut db, agent_id, knowledge_base_ids).await?;
        db.commit().await
    }

    /// What public traffic runs. Three gates, in order: the key's agent scope,
    /// the agent's own status, and whether anything has been published at all.
    pub async fn load_published(
        &self,
        ctx: &TenantCtx,
        agent_id: AgentId,
    ) -> Result<ResolvedAgent> {
        // Checked first, and reported as missing rather than forbidden, so a
        // key cannot probe for agent ids outside its scope.
        ctx.require_agent(agent_id)?;

        let mut db = self.db.tenant(ctx).await?;
        let agent = repo::find_agent(&mut db, agent_id).await?;

        match agent.status {
            AgentStatus::Active => {}
            AgentStatus::Paused => return Err(DomainError::Forbidden("agent_paused")),
            AgentStatus::Archived => return Err(DomainError::Gone("agent_archived")),
            AgentStatus::Draft => return Err(DomainError::Forbidden("agent_not_published")),
        }

        let version_id = agent
            .published_version_id
            .ok_or(DomainError::Forbidden("agent_not_published"))?;
        let version = repo::find_version(&mut db, version_id).await?;
        let knowledge_base_ids = repo::list_knowledge_base_ids(&mut db, agent_id).await?;
        db.commit().await?;

        Ok(ResolvedAgent {
            id: agent.id,
            org_id: ctx.org_id,
            workspace_id: agent.workspace_id,
            name: agent.name,
            status: agent.status,
            version_id: version.id,
            version: version.version,
            config: version.config,
            knowledge_base_ids,
            updated_at: agent.updated_at,
        })
    }

    /// What the playground runs: the draft, so an edit can be tried before
    /// anyone else sees it.
    pub async fn load_draft(&self, ctx: &TenantCtx, agent_id: AgentId) -> Result<ResolvedAgent> {
        ctx.require(Permission::AgentRead)?;

        let mut db = self.db.tenant(ctx).await?;
        let agent = repo::find_agent(&mut db, agent_id).await?;
        if agent.status == AgentStatus::Archived {
            return Err(DomainError::Gone("agent_archived"));
        }

        let version_id = agent
            .draft_version_id
            .or(agent.published_version_id)
            .ok_or(DomainError::NotFound("agent_version"))?;
        let version = repo::find_version(&mut db, version_id).await?;
        let knowledge_base_ids = repo::list_knowledge_base_ids(&mut db, agent_id).await?;
        db.commit().await?;

        Ok(ResolvedAgent {
            id: agent.id,
            org_id: ctx.org_id,
            workspace_id: agent.workspace_id,
            name: agent.name,
            status: agent.status,
            version_id: version.id,
            version: version.version,
            config: version.config,
            knowledge_base_ids,
            updated_at: agent.updated_at,
        })
    }

    async fn transition(
        &self,
        ctx: &TenantCtx,
        agent_id: AgentId,
        status: AgentStatus,
    ) -> Result<()> {
        ctx.require(Permission::AgentPublish)?;
        let mut db = self.db.tenant(ctx).await?;
        let agent = repo::find_agent(&mut db, agent_id).await?;
        if agent.status == AgentStatus::Archived {
            return Err(DomainError::Gone("agent_archived"));
        }
        repo::set_status(&mut db, agent_id, status).await?;
        db.commit().await
    }

    async fn load_detail(
        &self,
        db: &mut anthovai_db::TenantDb<'_>,
        agent_id: AgentId,
    ) -> Result<AgentDetail> {
        let agent = repo::find_agent(db, agent_id).await?;
        let versions = repo::list_versions(db, agent_id, VERSION_HISTORY_LIMIT).await?;
        let knowledge_base_ids = repo::list_knowledge_base_ids(db, agent_id).await?;

        let find = |wanted: Option<anthovai_core::AgentVersionId>| {
            wanted.and_then(|id| versions.iter().find(|v| v.id == id).cloned())
        };

        Ok(AgentDetail {
            draft: find(agent.draft_version_id),
            published: find(agent.published_version_id),
            versions,
            knowledge_base_ids,
            agent,
        })
    }
}

fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DomainError::validation("agent name is required"));
    }
    if trimmed.chars().count() > 120 {
        return Err(DomainError::validation(
            "agent name must be at most 120 characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_must_say_something() {
        assert!(validate_name("   ").is_err());
        assert!(validate_name("ABC School Assistant").is_ok());
    }

    #[test]
    fn absurdly_long_names_are_refused() {
        assert!(validate_name(&"a".repeat(121)).is_err());
    }

    #[test]
    fn name_length_is_counted_in_characters() {
        // 120 Thai characters is a valid name, even though it is far more bytes.
        assert!(validate_name(&"ก".repeat(120)).is_ok());
    }
}
