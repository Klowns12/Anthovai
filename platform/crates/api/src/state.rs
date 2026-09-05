//! What every handler is given.

use std::sync::Arc;

use anthovai_agent::AgentService;
use anthovai_auth::AuthService;
use anthovai_conversation::ConversationService;
use anthovai_core::Plan;
use anthovai_knowledge::KnowledgeService;
use anthovai_rag::ChatService;
use anthovai_tenant::TenantService;
use chrono::{DateTime, Utc};

use crate::rate_limit::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<AuthService>,
    pub tenants: Arc<TenantService>,
    pub agents: Arc<AgentService>,
    pub knowledge: Arc<KnowledgeService>,
    pub chat: Arc<ChatService>,
    pub conversations: Arc<ConversationService>,
    pub limits: Arc<RateLimiter>,
    /// The HTTP client used to fetch customer-supplied URLs. One per process:
    /// it holds the connection pool, and it is the only client configured to
    /// refuse redirects so the SSRF guard sees every hop.
    pub fetcher: Arc<reqwest::Client>,
    pub diagnostics: Arc<Diagnostics>,
    pub clock: anthovai_core::Clock,
    /// Origins allowed to make state-changing dashboard requests.
    pub dashboard_origins: Arc<Vec<String>>,
    pub started_at: DateTime<Utc>,
    pub version: &'static str,
}

/// The largest upload any plan permits. The route-level body limit uses this;
/// the service then enforces the caller's own plan, which is usually smaller.
pub fn max_upload_bytes() -> usize {
    Plan::Enterprise.limits().max_file_bytes as usize
}

/// What the readiness endpoint needs in order to answer honestly.
///
/// These are handles the request path already holds indirectly, through the
/// services. Health checks reach them directly because "can this process reach
/// the database?" is not a question any service exposes, and threading it
/// through every one of them to ask would be worse than this.
#[derive(Clone)]
pub struct Diagnostics {
    pub db: anthovai_db::Db,
    pub storage: anthovai_storage::Storage,
    pub router: Arc<anthovai_inference::ModelRouter>,
    pub jobs: Arc<anthovai_jobs::JobQueue>,
    /// `None` when no Prometheus recorder is installed, which is every test.
    pub metrics: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

pub struct Services {
    pub auth: AuthService,
    pub tenants: TenantService,
    /// Shared with the chat service, which loads agents on every question.
    pub agents: Arc<AgentService>,
    pub knowledge: KnowledgeService,
    pub chat: ChatService,
    pub conversations: ConversationService,
    pub diagnostics: Diagnostics,
}

impl AppState {
    pub fn new(
        services: Services,
        clock: anthovai_core::Clock,
        dashboard_origins: Vec<String>,
    ) -> Self {
        Self {
            auth: Arc::new(services.auth),
            tenants: Arc::new(services.tenants),
            agents: services.agents,
            knowledge: Arc::new(services.knowledge),
            chat: Arc::new(services.chat),
            conversations: Arc::new(services.conversations),
            diagnostics: Arc::new(services.diagnostics),
            limits: Arc::new(RateLimiter::new(clock.clone())),
            // A failure here means the TLS backend did not initialise, which
            // is fatal at startup and must not be quietly downgraded to a
            // default client — that one follows redirects, which would walk
            // straight past the guard.
            fetcher: Arc::new(crate::fetch::client().expect("the HTTP client could not be built")),
            started_at: clock.now(),
            clock,
            dashboard_origins: Arc::new(dashboard_origins),
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("version", &self.version)
            .field("started_at", &self.started_at)
            .finish()
    }
}
