//! The API server: public API, dashboard API, and internal endpoints.

use std::net::SocketAddr;
use std::sync::Arc;

use anthovai_agent::AgentService;
use anthovai_api::{AppState, Services};
use anthovai_auth::{AuthConfig, AuthService};
use anthovai_conversation::ConversationService;
use anthovai_core::config::{load_dotenv, Settings};
use anthovai_core::Clock;
use anthovai_db::Db;
use anthovai_knowledge::KnowledgeService;
use anthovai_rag::ChatService;
use anthovai_retrieval::Retriever;
use anthovai_tenant::TenantService;
use anyhow::Context;

use tracing::info;
use tracing_subscriber::EnvFilter;

/// Where the model registry lives. Overridable so a deployment can mount its
/// own list without rebuilding.
fn models_path() -> String {
    std::env::var("ANTHOVAI_MODELS_PATH").unwrap_or_else(|_| "config/models.toml".to_owned())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    // Before anything reads the environment: `Settings::load` consults it, and
    // so does every provider that needs an API key.
    load_dotenv();

    let settings = Settings::load().context("could not load configuration from config/")?;

    // Installed before anything else measures: a metric recorded before the
    // recorder exists is silently dropped, so startup would be the one part of
    // the process we could never see.
    let metrics = match anthovai_api::metrics::install() {
        Ok(handle) => Some(handle),
        Err(e) => {
            // Not fatal. A server that refuses to start because its metrics
            // endpoint could not be set up trades a visibility problem for an
            // outage, which is the worse of the two.
            tracing::warn!(error = %e, "metrics are not being recorded");
            None
        }
    };

    let db = Db::connect(&settings.database.url, settings.database.max_connections)
        .await
        .context("could not connect to the database")?;

    if settings.database.run_migrations_on_start {
        db.run_migrations()
            .await
            .context("could not apply migrations")?;
        info!("migrations applied");
    }
    db.ping().await.context("the database is not answering")?;

    let environment = anthovai_providers::Environment::from_env();

    // Local disk is the right default for a developer and a data-loss bug in a
    // container: the filesystem goes away on the next deploy, taking every
    // document a customer uploaded with it. Nothing reports that. The documents
    // rows survive, so the knowledge base still lists them, and the failure
    // surfaces later as `file_missing` on a re-embed — long after the file that
    // could have been re-uploaded is the only copy anyone had.
    //
    // Refused rather than warned about, for the same reason as an unpriced
    // model: by the time the symptom appears the damage cannot be undone.
    if environment.is_production() && settings.storage.provider == "local" {
        anyhow::bail!(
            concat!(
                "storage.provider is \"local\", which stores customer documents on the ",
                "container's own filesystem and loses them on the next deploy. Set ",
                "ANTHOVAI__STORAGE__PROVIDER=s3 with the endpoint, bucket and credentials."
            )
        );
    }

    // Opened at startup so a misconfigured bucket stops the deployment rather
    // than surfacing as a failed upload an hour later.
    let storage = anthovai_storage::from_settings(&settings.storage)
        .context("could not open object storage")?;
    info!(provider = %settings.storage.provider, "object storage ready");
    // The knowledge service takes ownership of the storage handle; readiness
    // needs to reach the same bucket, so it keeps its own clone.
    let diagnostic_storage = std::sync::Arc::clone(&storage);

    let clock = Clock::system();

    // The same embedder that built the index has to answer questions against
    // it — a knowledge base embedded by one model and searched by another
    // returns nonsense, so both binaries choose it the same way.
    let embedder = anthovai_providers::embedding_provider(
        &settings.providers,
        &settings.embeddings,
        environment,
    )
    .context("could not configure embeddings")?;
    let retriever = Arc::new(Retriever::new(db.clone(), vec![embedder]));

    let registry = anthovai_providers::model_registry(&models_path())
        .context("could not read the model registry")?;
    let router = Arc::new(
        anthovai_providers::chat_router(&settings.providers, registry, clock.clone(), environment)
            .context("could not configure the chat providers")?,
    );

    // One agent service, shared: the chat path loads an agent on every
    // question, and the dashboard edits the same rows.
    let agents = Arc::new(AgentService::new(db.clone()));

    let auth = AuthService::new(
        db.clone(),
        clock.clone(),
        AuthConfig {
            session_ttl_hours: settings.auth.session_ttl_hours as i64,
            api_key_cache_secs: settings.auth.api_key_cache_secs,
            password: anthovai_auth::password::PasswordHasherConfig {
                memory_kib: settings.auth.argon2_memory_kib,
                ..Default::default()
            },
            ..Default::default()
        },
    );

    // Built before the state so a bad From address or an unreachable relay is a
    // startup failure, not a surprise on the first customer's first signup.
    let mailer = anthovai_auth::mail::from_settings(&anthovai_auth::MailSettings {
        smtp_url: settings.mail.smtp_url.clone(),
        username: settings.mail.username.clone(),
        password: settings.mail.password.clone(),
        from: settings.mail.from.clone(),
    })
    .context("could not configure outgoing mail")?;

    let state = AppState::new(
        Services {
            auth,
            tenants: TenantService::new(db.clone()),
            agents: Arc::clone(&agents),
            knowledge: KnowledgeService::new(db.clone(), storage, settings.embeddings.clone()),
            chat: ChatService::new(
                db.clone(),
                agents,
                retriever,
                Arc::clone(&router),
                clock.clone(),
            ),
            conversations: ConversationService::new(db.clone(), clock.clone()),
            diagnostics: anthovai_api::state::Diagnostics {
                db: db.clone(),
                storage: diagnostic_storage,
                router,
                jobs: Arc::new(anthovai_jobs::JobQueue::new(db.clone())),
                metrics,
            },
        },
        clock,
        settings.server.dashboard_origins.clone(),
        mailer,
        settings.mail.site_url.clone(),
    );

    let addr: SocketAddr = format!("{}:{}", settings.server.host, settings.server.port)
        .parse()
        .context("server.host and server.port do not form a valid address")?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("could not bind {addr}"))?;

    info!(%addr, version = env!("CARGO_PKG_VERSION"), "anthovai-api listening");

    axum::serve(listener, anthovai_api::app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("the server stopped unexpectedly")?;

    info!("anthovai-api stopped");
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,anthovai=debug,sqlx=warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

/// Stop accepting new connections on Ctrl-C or SIGTERM, then let in-flight
/// requests finish.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("could not install the Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("could not install the SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    info!("shutdown signal received");
}
