//! The background worker: ingestion and cleanup.
//!
//! Everything slow happens here rather than in a request. An upload endpoint
//! stores the bytes and queues a job; this is what turns those bytes into
//! something an agent can search.

use std::sync::Arc;
use std::time::Duration;

use anthovai_core::config::{load_dotenv, Settings};
use anthovai_db::Db;
use anthovai_embeddings::EmbeddingRunner;
use anthovai_ingestion::{pipeline, IngestPipeline};
use anthovai_jobs::{WorkerConfig, WorkerRuntime};
use anyhow::Context;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod handlers;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    // Before anything reads the environment: `Settings::load` consults it, and
    // so does every provider that needs an API key.
    load_dotenv();

    let settings = Settings::load().context("could not load configuration from config/")?;

    let db = Db::connect(&settings.database.url, settings.database.max_connections)
        .await
        .context("could not connect to the database")?;
    db.ping().await.context("the database is not answering")?;

    let storage = anthovai_storage::from_settings(&settings.storage)
        .context("could not open object storage")?;

    // Fails here rather than on the first document if production has no
    // provider key: fake vectors in a customer's knowledge base look like
    // working software and retrieve nothing useful.
    let embedder = anthovai_providers::embedding_provider(
        &settings.providers,
        &settings.embeddings,
        anthovai_providers::Environment::from_env(),
    )
    .context("could not configure embeddings")?;

    let embedder_model_id = embedder.model_id().to_owned();

    let pipeline = Arc::new(IngestPipeline::new(
        db.clone(),
        Arc::clone(&storage),
        Arc::new(EmbeddingRunner::new(
            embedder,
            pipeline::runner_config(
                settings.embeddings.batch_size,
                settings.embeddings.concurrency,
            ),
        )),
        pipeline::chunk_config_from(500, 80),
    ));

    let config = WorkerConfig {
        concurrency: settings.worker.concurrency,
        poll_interval: Duration::from_millis(settings.worker.poll_interval_ms),
        ..WorkerConfig::default()
    };

    let runtime = WorkerRuntime::new(db.clone(), config)
        .register(Arc::new(handlers::IngestDocumentHandler::new(pipeline)))
        .register(Arc::new(handlers::DeleteDocumentChunksHandler::new(
            db.clone(),
        )))
        .register(Arc::new(handlers::PurgeDeletedChunksHandler::new(
            db.clone(),
        )))
        .register(Arc::new(handlers::ReembedKnowledgeBaseHandler::new(
            db.clone(),
            embedder_model_id.clone(),
        )));

    // A knowledge base built by the local stand-in answers questions and means
    // nothing by them, so the moment a real embedder is configured, every such
    // base is queued to be rebuilt. Only when the embedder is real — otherwise
    // this would queue work to replace fake vectors with fake vectors.
    if !anthovai_embeddings::is_fake_model(&embedder_model_id) {
        match queue_reembedding(&db).await {
            Ok(0) => {}
            Ok(queued) => info!(
                model = %embedder_model_id,
                knowledge_bases = queued,
                "queued knowledge bases built with a stand-in for re-embedding"
            ),
            // Not fatal. The worker's job is to process the queue; failing to
            // start because of a housekeeping sweep would stop every customer's
            // uploads over a problem that affects only development data.
            Err(e) => warn!(error = %e, "could not queue knowledge bases for re-embedding"),
        }
    }

    info!(
        concurrency = settings.worker.concurrency,
        poll_interval_ms = settings.worker.poll_interval_ms,
        "anthovai-worker starting"
    );

    runtime.run(&settings.database.url, shutdown_signal()).await;

    info!("anthovai-worker stopped");
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

/// Stop taking new jobs, then let the ones in hand finish. A half-ingested
/// document would otherwise have to be processed again from the start.
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

    info!("shutdown signal received, finishing in-flight jobs");
}

/// Queue every knowledge base whose vectors were built by a stand-in.
///
/// Runs across tenants, which is why it is here rather than in a service: no
/// request has chosen an organization, and this touches all of them.
///
/// Enqueuing the same base twice is harmless — the re-embed handler re-queues
/// each document as a fresh version, and the pipeline discards a half-written
/// version before writing one — so no bookkeeping is kept about which bases
/// have already been swept.
async fn queue_reembedding(db: &Db) -> anyhow::Result<usize> {
    let mut system = db.system().await?;
    let bases = anthovai_knowledge::repo::knowledge_bases_needing_reembedding(&mut system).await?;

    for (org_id, knowledge_base_id) in &bases {
        anthovai_jobs::JobQueue::enqueue_in(
            &mut system,
            *org_id,
            &anthovai_jobs::JobPayload::ReembedKnowledgeBase {
                knowledge_base_id: *knowledge_base_id,
            },
        )
        .await?;
    }

    system.commit().await?;
    Ok(bases.len())
}
