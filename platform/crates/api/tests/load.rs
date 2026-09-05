//! What a question costs us, with the model taken out of the picture.
//!
//! Everything except the provider call is ours: authentication, the quota
//! check, embedding the question, two index scans, fusion, prompt assembly, and
//! one transaction to record the turn. That is the part a load test can hold us
//! to, and the target from the Phase G plan is a **p95 under 400ms at 50
//! requests per second**.
//!
//! The model is deliberately the echo provider. Measuring a real one would
//! measure their capacity rather than ours, and the number would move every day
//! for reasons we could not fix.
//!
//! Ignored by default — it takes half a minute and wants a database to itself:
//!
//! ```text
//! ANTHOVAI_TEST_DATABASE_URL=... \
//!   cargo test --release -p anthovai-api --test load -- --ignored --nocapture
//! ```
//!
//! Run it in release. A debug build measures `rustc -O0`, not the platform.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anthovai_agent::AgentService;
use anthovai_api::{AppState, Services};
use anthovai_auth::{password::PasswordHasherConfig, AuthConfig, AuthService};
use anthovai_core::config::EmbeddingSettings;
use anthovai_core::{Clock, DocumentId, OrgId};
use anthovai_db::Db;
use anthovai_embeddings::{EmbeddingRunner, HashEmbedder, RunnerConfig};
use anthovai_ingestion::{pipeline, IngestPipeline};
use anthovai_knowledge::KnowledgeService;
use anthovai_storage::{InMemoryStorage, Storage};
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;
use common::{chat_services, multipart_body, Part, BOUNDARY, TEST_DIMENSION};

/// Requests per second the test aims for.
const TARGET_RPS: u64 = 50;
const DURATION: Duration = Duration::from_secs(20);

/// The bar. Everything but the model call, at the target rate.
const P95_BUDGET: Duration = Duration::from_millis(400);

/// How many requests may be in flight at once.
///
/// A closed-loop test with unbounded concurrency measures how much work can be
/// piled onto a machine, not how quickly it answers. This is the queue depth a
/// load balancer would allow.
const CONCURRENCY: usize = 32;

const HANDBOOK: &str = "# ABC School Handbook\n\n\
## Library\n\n\
The library opens at seven in the morning and closes at nine in the evening. \
Students may borrow up to six books at a time, and renewals are done at the \
desk or through the student portal.\n\n\
## Cafeteria\n\n\
The cafeteria serves hot lunch between eleven and two. A vegetarian option is \
available every day, and the salad bar stays open until the last class ends.\n\n\
## Parking\n\n\
Parking permits cost four hundred baht per semester and are issued by the \
registrar office. Motorcycle spaces are free but must still be registered.\n\n\
## Enrolment\n\n\
Applications open in March and close at the end of April. There is no entrance \
examination, but places are limited and applicants describe what they have \
built before.\n";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "takes half a minute and needs a database to itself"]
async fn a_question_costs_less_than_the_budget_at_fifty_a_second() {
    let Some(db) = database().await else {
        println!("ANTHOVAI_TEST_DATABASE_URL is not set; nothing to measure");
        return;
    };

    let (app, key, agent_id, org_id) = ready(&db).await;

    // Warm up: the first request pays for a connection from the pool, a
    // prepared statement, and the tokenizer's one-off initialisation. Counting
    // those would measure startup rather than steady state.
    for _ in 0..20 {
        let status = ask(&app, &key, &agent_id).await;
        assert_eq!(status, StatusCode::OK, "the warm-up request failed");
    }

    let total = TARGET_RPS * DURATION.as_secs();
    let interval = Duration::from_secs_f64(1.0 / TARGET_RPS as f64);
    let permits = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let started = Instant::now();

    let mut running = tokio::task::JoinSet::new();
    for i in 0..total {
        // Open loop: requests are issued on a schedule rather than as fast as
        // the last one finishes. A closed loop hides queueing — when the server
        // slows down, so does the load, and the graph looks fine.
        let due = started + interval * i as u32;
        let now = Instant::now();
        if due > now {
            tokio::time::sleep(due - now).await;
        }

        let permit = Arc::clone(&permits).acquire_owned().await.unwrap();
        let app = app.clone();
        let key = key.clone();
        let agent_id = agent_id.clone();

        running.spawn(async move {
            let at = Instant::now();
            let status = ask(&app, &key, &agent_id).await;
            drop(permit);
            (at.elapsed(), status)
        });
    }

    let mut latencies: Vec<Duration> = Vec::with_capacity(total as usize);
    let mut failures = 0;
    while let Some(result) = running.join_next().await {
        let (elapsed, status) = result.expect("a request task panicked");
        if status == StatusCode::OK {
            latencies.push(elapsed);
        } else {
            failures += 1;
        }
    }

    let wall = started.elapsed();
    latencies.sort_unstable();

    let achieved = latencies.len() as f64 / wall.as_secs_f64();
    println!(
        "requests      {} in {:.1}s",
        latencies.len(),
        wall.as_secs_f64()
    );
    println!("rate          {achieved:.1}/s (target {TARGET_RPS})");
    println!("failures      {failures}");
    println!("p50           {:?}", percentile(&latencies, 50.0));
    println!("p95           {:?}", percentile(&latencies, 95.0));
    println!("p99           {:?}", percentile(&latencies, 99.0));
    println!("max           {:?}", latencies.last().copied().unwrap());

    cleanup(&db, org_id).await;

    // A failed request is fast, so a run that mostly errored would post an
    // excellent p95. Check that before the latency.
    assert_eq!(failures, 0, "{failures} requests did not return 200");
    assert!(
        achieved >= TARGET_RPS as f64 * 0.9,
        "only {achieved:.1} requests a second were issued: the harness could \
         not keep up, so the latency below measures the harness"
    );

    let p95 = percentile(&latencies, 95.0);
    assert!(
        p95 < P95_BUDGET,
        "p95 is {p95:?}, past the budget of {P95_BUDGET:?}"
    );
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[index.saturating_sub(1).min(sorted.len() - 1)]
}

async fn ask(app: &Router, key: &str, agent_id: &str) -> StatusCode {
    let body = json!({
        "agent_id": agent_id,
        "message": "When does the library open in the morning?",
        "options": {"include_sources": true, "include_usage": true}
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat")
        .header(header::AUTHORIZATION, format!("Bearer {key}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();

    // Drained so the timing includes serialising the answer, which for a
    // response carrying passages is not free.
    let _ = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await;
    status
}

// ---- setting the scene ----------------------------------------------------

async fn database() -> Option<Db> {
    let url = std::env::var("ANTHOVAI_TEST_DATABASE_URL").ok()?;
    // A pool large enough that the test measures the server rather than its own
    // contention for connections.
    let db = Db::connect(&url, 32).await.expect("connect");
    db.run_migrations().await.expect("migrate");
    Some(db)
}

/// A tenant with an indexed handbook, a published agent, and a key for it.
async fn ready(db: &Db) -> (Router, String, String, OrgId) {
    let clock = Clock::system();
    let storage = Arc::new(InMemoryStorage::new());
    let agents = Arc::new(AgentService::new(db.clone()));
    let (chat, conversations) = chat_services(db, Arc::clone(&agents), &clock);

    let state = AppState::new(
        Services {
            auth: AuthService::new(
                db.clone(),
                clock.clone(),
                AuthConfig {
                    password: PasswordHasherConfig::fast_for_tests(),
                    ..AuthConfig::default()
                },
            ),
            tenants: anthovai_tenant::TenantService::new(db.clone()),
            agents,
            knowledge: KnowledgeService::new(
                db.clone(),
                Arc::clone(&storage) as Storage,
                EmbeddingSettings {
                    default_model: "fake:hash-1536".to_owned(),
                    dimension: TEST_DIMENSION,
                    batch_size: 64,
                    concurrency: 4,
                },
            ),
            chat,
            conversations,
            diagnostics: common::diagnostics(db, Arc::clone(&storage) as Storage),
        },
        clock,
        vec!["https://app.anthovai.com".to_owned()],
    );

    let auth = Arc::clone(&state.auth);
    let app = anthovai_api::app(state);

    let mut client = Client {
        app: app.clone(),
        auth,
        cookie: None,
        org_id: None,
    };

    let workspace_id = client.onboard().await;
    let org_id: OrgId = client.org_id.as_ref().unwrap().parse().unwrap();

    // Enterprise, so the month's message quota is not what ends the run.
    let mut system = db.system().await.unwrap();
    anthovai_db::sqlx::query("UPDATE organizations SET plan = 'enterprise' WHERE id = $1")
        .bind(org_id.to_db())
        .execute(system.conn())
        .await
        .unwrap();
    system.commit().await.unwrap();

    let kb_id = client.knowledge_base(&workspace_id).await;
    let document_id = client.upload(&kb_id).await;
    ingest(db, Arc::clone(&storage) as Storage, org_id, &document_id).await;

    let agent_id = client.publish_agent(&workspace_id, &kb_id).await;
    let key = client.api_key(&workspace_id).await;

    (app, key, agent_id, org_id)
}

async fn ingest(db: &Db, storage: Storage, org_id: OrgId, document_id: &str) {
    let pipeline = IngestPipeline::new(
        db.clone(),
        storage,
        Arc::new(EmbeddingRunner::new(
            Arc::new(HashEmbedder::new(TEST_DIMENSION)),
            RunnerConfig::default(),
        )),
        pipeline::chunk_config_from(500, 80),
    );

    let document_id: DocumentId = document_id.parse().unwrap();
    let outcome = pipeline.run(org_id, document_id, 1).await.expect("ingest");
    assert!(outcome.chunks > 0);
}

/// A load test that left its rows behind would make the next run slower than
/// the last, and the trend would look like a regression.
async fn cleanup(db: &Db, org_id: OrgId) {
    let mut system = db.system().await.unwrap();
    let _ = anthovai_db::sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org_id.to_db())
        .execute(system.conn())
        .await;
    let _ = system.commit().await;
}

struct Client {
    app: Router,
    auth: Arc<AuthService>,
    cookie: Option<String>,
    org_id: Option<String>,
}

impl Client {
    async fn send(&self, request: Request<Body>) -> (StatusCode, Value, Option<String>) {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            set_cookie,
        )
    }

    fn builder(&self, method: &str, uri: &str) -> axum::http::request::Builder {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::ORIGIN, "https://app.anthovai.com");
        if let Some(cookie) = &self.cookie {
            request = request.header(header::COOKIE, cookie);
        }
        if let Some(org_id) = &self.org_id {
            request = request.header("x-org-id", org_id);
        }
        request
    }

    async fn json(&self, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
        let (status, value, _) = self
            .send(
                self.builder(method, uri)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await;
        (status, value)
    }

    async fn onboard(&mut self) -> String {
        let email = format!(
            "load-{}@abc.ac.th",
            anthovai_core::UserId::new().to_db().to_lowercase()
        );

        let (_, signed_up) = self
            .json(
                "POST",
                "/dashboard/v1/auth/signup",
                json!({"email": email, "password": "correct horse battery"}),
            )
            .await;
        let user_id = signed_up["user_id"].as_str().unwrap().parse().unwrap();
        self.auth.mark_email_verified(user_id).await.unwrap();

        let (_, _, set_cookie) = self
            .send(
                self.builder("POST", "/dashboard/v1/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(
                            &json!({"email": email, "password": "correct horse battery"}),
                        )
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await;
        self.cookie = Some(set_cookie.unwrap().split(';').next().unwrap().to_owned());

        let (_, created) = self
            .json(
                "POST",
                "/dashboard/v1/organizations",
                json!({
                    "name": "Load test",
                    "slug": format!("load-{}", OrgId::new().to_db().to_lowercase())
                }),
            )
            .await;
        self.org_id = Some(created["organization"]["id"].as_str().unwrap().to_owned());
        created["default_workspace"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    async fn knowledge_base(&self, workspace_id: &str) -> String {
        let (_, created) = self
            .json(
                "POST",
                "/dashboard/v1/knowledge_bases",
                json!({"workspace_id": workspace_id, "name": "Handbook"}),
            )
            .await;
        created["id"].as_str().unwrap().to_owned()
    }

    async fn upload(&self, kb_id: &str) -> String {
        let body = multipart_body(&[
            Part::Field("knowledge_base_id", kb_id),
            Part::File {
                name: "file",
                filename: "handbook.md",
                content: HANDBOOK.as_bytes().to_vec(),
            },
        ]);

        let (status, uploaded, _) = self
            .send(
                self.builder("POST", "/dashboard/v1/documents")
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={BOUNDARY}"),
                    )
                    .header(header::CONTENT_LENGTH, body.len())
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{uploaded:?}");
        uploaded["id"].as_str().unwrap().to_owned()
    }

    async fn publish_agent(&self, workspace_id: &str, kb_id: &str) -> String {
        let (_, created) = self
            .json(
                "POST",
                "/dashboard/v1/agents",
                json!({
                    "workspace_id": workspace_id,
                    "name": "Load",
                    "config": {"instructions": "Answer from the handbook."}
                }),
            )
            .await;
        let agent_id = created["id"].as_str().unwrap().to_owned();

        self.json(
            "PUT",
            &format!("/dashboard/v1/agents/{agent_id}/knowledge_bases"),
            json!({"knowledge_base_ids": [kb_id]}),
        )
        .await;
        self.json(
            "POST",
            &format!("/dashboard/v1/agents/{agent_id}/publish"),
            json!({}),
        )
        .await;

        agent_id
    }

    async fn api_key(&self, workspace_id: &str) -> String {
        let (_, issued) = self
            .json(
                "POST",
                "/dashboard/v1/api_keys",
                json!({"workspace_id": workspace_id, "name": "Load"}),
            )
            .await;
        issued["secret"].as_str().unwrap().to_owned()
    }
}
