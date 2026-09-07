//! Uploading knowledge, end to end over HTTP.
//!
//! The interesting failures here are all at the seams: a plan limit checked
//! after the bytes were already read, a document row left behind by a transfer
//! that broke off, a knowledge base id from another tenant that the foreign key
//! happily accepts. None of those appear below the HTTP layer.

use std::collections::HashMap;
use std::sync::Arc;

use anthovai_agent::AgentService;
use anthovai_api::{AppState, Services};
use anthovai_auth::{password::PasswordHasherConfig, AuthConfig, AuthService};
use anthovai_core::config::EmbeddingSettings;
use anthovai_core::{Clock, OrgId, Plan, TenantCtx};
use anthovai_db::Db;
use anthovai_jobs::{Handlers, Job, JobError, JobHandler, JobPayload, JobQueue};
use anthovai_knowledge::{repo as knowledge_repo, KnowledgeService};
use anthovai_storage::{InMemoryStorage, Storage};
use anthovai_testkit::db_test;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;
use common::chat_services;

const DASHBOARD_ORIGIN: &str = "https://app.anthovai.com";
const BOUNDARY: &str = "anthovaitestboundary";

struct Harness {
    app: Router,
    auth: Arc<AuthService>,
    storage: Arc<InMemoryStorage>,
    queue: JobQueue,
    db: Db,
    cookie: Option<String>,
    org_id: Option<String>,
    api_key: Option<String>,
}

impl Harness {
    fn new(db: &Db) -> Self {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .try_init();
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
                        dimension: 1536,
                        batch_size: 64,
                        concurrency: 4,
                    },
                ),
                chat,
                conversations,
                diagnostics: common::diagnostics(db, Arc::clone(&storage) as Storage),
            },
            clock,
            vec![DASHBOARD_ORIGIN.to_owned()],
            // Nothing here sends mail; the logging mailer records what it would
            // have sent and reports that it did not.
            std::sync::Arc::new(anthovai_auth::mail::LoggingMailer),
            "http://localhost:3000".to_owned(),
        );

        let auth = Arc::clone(&state.auth);
        Self {
            app: anthovai_api::app(state),
            auth,
            storage,
            queue: JobQueue::new(db.clone()),
            db: db.clone(),
            cookie: None,
            org_id: None,
            api_key: None,
        }
    }

    async fn send(&self, request: Request<Body>) -> Reply {
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

        Reply {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            set_cookie,
        }
    }

    fn builder(&self, method: &str, uri: &str) -> axum::http::request::Builder {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::ORIGIN, DASHBOARD_ORIGIN);

        if let Some(cookie) = &self.cookie {
            request = request.header(header::COOKIE, cookie);
        }
        if let Some(org_id) = &self.org_id {
            request = request.header("x-org-id", org_id);
        }
        if let Some(key) = &self.api_key {
            request = request.header(header::AUTHORIZATION, format!("Bearer {key}"));
        }
        request
    }

    async fn get(&self, uri: &str) -> Reply {
        self.send(self.builder("GET", uri).body(Body::empty()).unwrap())
            .await
    }

    async fn json(&self, method: &str, uri: &str, body: Value) -> Reply {
        self.send(
            self.builder(method, uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
    }

    async fn delete(&self, uri: &str) -> Reply {
        self.send(self.builder("DELETE", uri).body(Body::empty()).unwrap())
            .await
    }

    /// A multipart upload, built by hand so the part order — which the upload
    /// code depends on — is exactly what the test intends.
    async fn upload(&self, uri: &str, parts: &[Part<'_>]) -> Reply {
        let body = multipart_body(parts);
        self.send(
            self.builder("POST", uri)
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .header(header::CONTENT_LENGTH, body.len())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    }

    /// The same, but claiming a size in the header without sending one.
    async fn upload_claiming(&self, uri: &str, parts: &[Part<'_>], claimed: usize) -> Reply {
        let body = multipart_body(parts);
        self.send(
            self.builder("POST", uri)
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .header(header::CONTENT_LENGTH, claimed)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    }

    /// An upload with no `Content-Length` at all, as a chunked transfer has.
    /// There is nothing to check up front, so the limit has to be enforced
    /// while the bytes are arriving.
    async fn upload_chunked(&self, uri: &str, parts: &[Part<'_>]) -> Reply {
        let body = multipart_body(parts);
        self.send(
            self.builder("POST", uri)
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    }

    async fn onboard(&mut self) -> String {
        let email = format!(
            "owner-{}@abc.ac.th",
            anthovai_core::UserId::new().to_db().to_lowercase()
        );

        let signed_up = self
            .json(
                "POST",
                "/dashboard/v1/auth/signup",
                json!({"email": email, "password": "correct horse battery"}),
            )
            .await;
        assert_eq!(signed_up.status, StatusCode::CREATED);

        let user_id = signed_up.body["user_id"].as_str().unwrap().parse().unwrap();
        self.auth.mark_email_verified(user_id).await.unwrap();

        let signed_in = self
            .json(
                "POST",
                "/dashboard/v1/auth/login",
                json!({"email": email, "password": "correct horse battery"}),
            )
            .await;
        self.cookie = Some(
            signed_in
                .set_cookie
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned(),
        );

        let created = self
            .json(
                "POST",
                "/dashboard/v1/organizations",
                json!({
                    "name": "ABC School",
                    "slug": format!("abc-{}", OrgId::new().to_db().to_lowercase())
                }),
            )
            .await;
        assert_eq!(created.status, StatusCode::CREATED);
        self.org_id = Some(
            created.body["organization"]["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );

        created.body["default_workspace"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    async fn create_knowledge_base(&self, workspace_id: &str) -> String {
        let created = self
            .json(
                "POST",
                "/dashboard/v1/knowledge_bases",
                json!({"workspace_id": workspace_id, "name": "Student Handbook"}),
            )
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
        created.body["id"].as_str().unwrap().to_owned()
    }

    async fn set_plan(&self, plan: &str) {
        let org_id: OrgId = self.org_id.as_ref().unwrap().parse().unwrap();
        let mut db = self.db.system().await.unwrap();
        anthovai_db::sqlx::query("UPDATE organizations SET plan = $2 WHERE id = $1")
            .bind(org_id.to_db())
            .bind(plan)
            .execute(db.conn())
            .await
            .unwrap();
        db.commit().await.unwrap();
    }

    /// Run the queued work, the way the worker will.
    async fn run_worker(&self) -> usize {
        let mut handlers = Handlers::new();
        handlers.insert(
            "ingest_document",
            Arc::new(ReadyingHandler {
                db: self.db.clone(),
            }),
        );
        handlers.insert("delete_document_chunks", Arc::new(NoopHandler));

        anthovai_jobs::drain(&self.queue, &handlers, "test-worker")
            .await
            .expect("drain the queue")
    }

    /// What is queued for this tenant, by kind.
    async fn queued_kinds(&self) -> HashMap<String, i64> {
        let org_id: OrgId = self.org_id.as_ref().unwrap().parse().unwrap();
        let mut db = self.db.system().await.unwrap();

        let rows: Vec<(String, i64)> = anthovai_db::sqlx::query_as(
            "SELECT kind, count(*) FROM jobs WHERE tenant_id = $1 GROUP BY kind",
        )
        .bind(org_id.to_db())
        .fetch_all(db.conn())
        .await
        .unwrap();
        db.commit().await.unwrap();

        rows.into_iter().collect()
    }

    /// Documents in this tenant, whatever their status — including the ones the
    /// API hides, which is the point when checking for leftovers.
    async fn document_statuses(&self) -> Vec<String> {
        let org_id: OrgId = self.org_id.as_ref().unwrap().parse().unwrap();
        let ctx = TenantCtx::system(org_id, Plan::Enterprise);
        let mut db = self.db.tenant(&ctx).await.unwrap();

        let rows: Vec<(String,)> = anthovai_db::sqlx::query_as(
            "SELECT status FROM documents WHERE tenant_id = $1 ORDER BY created_at",
        )
        .bind(org_id.to_db())
        .fetch_all(db.conn())
        .await
        .unwrap();
        db.commit().await.unwrap();

        rows.into_iter().map(|(s,)| s).collect()
    }
}

struct Reply {
    status: StatusCode,
    body: Value,
    set_cookie: Option<String>,
}

impl Reply {
    fn error_code(&self) -> &str {
        self.body["error"]["code"].as_str().unwrap_or_default()
    }
}

enum Part<'a> {
    Field(&'a str, &'a str),
    File {
        name: &'a str,
        filename: &'a str,
        content: Vec<u8>,
    },
}

fn multipart_body(parts: &[Part<'_>]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match part {
            Part::Field(name, value) => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(value.as_bytes());
            }
            Part::File {
                name,
                filename,
                content,
            } => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n\
                         Content-Type: application/octet-stream\r\n\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(content);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

/// Stands in for the real ingestion, which arrives in Phase B. It does what the
/// pipeline will do to the document row, so the queue, the status machine and
/// the dashboard can be exercised now.
struct ReadyingHandler {
    db: Db,
}

#[async_trait]
impl JobHandler for ReadyingHandler {
    fn kind(&self) -> &'static str {
        "ingest_document"
    }

    async fn handle(&self, job: Job) -> Result<(), JobError> {
        let JobPayload::IngestDocument {
            document_id,
            version,
        } = job.payload
        else {
            return Err(JobError::permanent("wrong_payload", "not an ingest job"));
        };

        // Scoped to the job's tenant, exactly as the real handler is.
        let ctx = TenantCtx::system(job.org_id, Plan::Enterprise);
        let mut tenant = self
            .db
            .tenant(&ctx)
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;

        knowledge_repo::set_ready(&mut tenant, document_id, version, 3, 120, Some("th"))
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;
        tenant
            .commit()
            .await
            .map_err(|e| JobError::Transient(e.to_string()))?;
        Ok(())
    }
}

struct NoopHandler;

#[async_trait]
impl JobHandler for NoopHandler {
    fn kind(&self) -> &'static str {
        "delete_document_chunks"
    }

    async fn handle(&self, _job: Job) -> Result<(), JobError> {
        Ok(())
    }
}

// ---- knowledge bases ------------------------------------------------------

db_test!(async fn a_knowledge_base_records_the_model_it_was_built_with(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;

    let created = harness
        .json(
            "POST",
            "/dashboard/v1/knowledge_bases",
            json!({"workspace_id": workspace_id, "name": "Student Handbook"}),
        )
        .await;

    assert_eq!(created.status, StatusCode::CREATED);
    // Pinned at creation: a query has to be embedded by the same model as the
    // chunks it searches, so this cannot be changed later without re-embedding.
    assert_eq!(created.body["embedding_model"], "fake:hash-1536");
    assert_eq!(created.body["document_count"], 0);
    assert_eq!(created.body["storage_bytes"], 0);
});

db_test!(async fn one_tenant_cannot_see_anothers_knowledge_bases(db) {
    let mut alice = Harness::new(&db);
    let alice_workspace = alice.onboard().await;
    let alice_kb = alice.create_knowledge_base(&alice_workspace).await;

    let mut bob = Harness::new(&db);
    bob.onboard().await;

    let listed = bob.get("/dashboard/v1/knowledge_bases").await;
    assert_eq!(listed.body["data"].as_array().unwrap().len(), 0);

    let reply = bob
        .get(&format!("/dashboard/v1/knowledge_bases/{alice_kb}"))
        .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.error_code(), "knowledge_base_not_found");
});

// ---- uploading ------------------------------------------------------------

db_test!(async fn pasted_text_is_stored_and_queued(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let uploaded = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("title", "Admissions FAQ"),
                Part::Field("text", "หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์"),
            ],
        )
        .await;

    // Accepted, not done: ingestion happens in the worker.
    assert_eq!(uploaded.status, StatusCode::ACCEPTED, "{:?}", uploaded.body);
    assert_eq!(uploaded.body["status"], "queued");
    assert_eq!(uploaded.body["source_type"], "text");
    assert!(uploaded.body["size_bytes"].as_i64().unwrap() > 0);

    assert_eq!(harness.queued_kinds().await.get("ingest_document"), Some(&1));
    assert_eq!(harness.storage.len(), 1, "the bytes should be in storage");
});

db_test!(async fn a_file_is_typed_from_its_name_and_stored(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let uploaded = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::File {
                    name: "file",
                    filename: "handbook.md",
                    content: b"# Programs\n\nRust runs for 12 weeks.".to_vec(),
                },
            ],
        )
        .await;

    assert_eq!(uploaded.status, StatusCode::ACCEPTED, "{:?}", uploaded.body);
    assert_eq!(uploaded.body["source_type"], "md");
    assert_eq!(uploaded.body["title"], "handbook.md");
});

db_test!(async fn the_knowledge_base_must_come_before_the_file(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    // The other way round, the plan cannot be checked until after the bytes
    // have been read, so this is refused rather than quietly accepted.
    let uploaded = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::File {
                    name: "file",
                    filename: "handbook.md",
                    content: b"content".to_vec(),
                },
                Part::Field("knowledge_base_id", &kb_id),
            ],
        )
        .await;

    assert_eq!(uploaded.status, StatusCode::BAD_REQUEST);
    assert!(harness.storage.is_empty(), "nothing should have been stored");
});

db_test!(async fn every_format_the_endpoint_accepts_has_a_parser_behind_it(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    // Accepting a format with no parser behind it means a document sitting at
    // "failed" until one lands. Every extension here is one the worker can
    // read; `parsers::ParserRegistry` has the matching test on its side.
    for filename in [
        "handbook.pdf",
        "handbook.docx",
        "courses.json",
        "courses.csv",
        "page.html",
        "notes.txt",
        "notes.md",
    ] {
        let uploaded = harness
            .upload(
                "/dashboard/v1/documents",
                &[
                    Part::Field("knowledge_base_id", &kb_id),
                    Part::File {
                        name: "file",
                        filename,
                        // Whether these bytes parse is the worker's problem;
                        // whether the endpoint accepts the format is this one.
                        content: b"placeholder".to_vec(),
                    },
                ],
            )
            .await;

        assert_eq!(
            uploaded.status,
            StatusCode::ACCEPTED,
            "{filename} was refused: {:?}",
            uploaded.body
        );
    }
});

db_test!(async fn an_unknown_extension_is_refused(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let uploaded = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::File {
                    name: "file",
                    filename: "mystery",
                    content: b"content".to_vec(),
                },
            ],
        )
        .await;

    assert_eq!(uploaded.status, StatusCode::BAD_REQUEST);
});

db_test!(async fn an_empty_upload_leaves_nothing_behind(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let uploaded = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::File {
                    name: "file",
                    filename: "empty.txt",
                    content: Vec::new(),
                },
            ],
        )
        .await;

    assert_eq!(uploaded.status, StatusCode::BAD_REQUEST);

    // A reservation was made and has to be cleaned up, or the dashboard shows a
    // document stuck at "uploading" for ever.
    let statuses = harness.document_statuses().await;
    assert!(
        statuses.iter().all(|s| s == "deleted"),
        "an abandoned upload should not be left as `uploading`: {statuses:?}"
    );
});

db_test!(async fn an_oversized_file_is_refused_before_it_is_read(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    // The free plan allows 10MB. This claims 50MB in its Content-Length, so it
    // is refused without the body being read at all.
    let uploaded = harness
        .upload_claiming(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::File {
                    name: "file",
                    filename: "big.txt",
                    content: b"small in reality".to_vec(),
                },
            ],
            50 * 1024 * 1024,
        )
        .await;

    assert_eq!(uploaded.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(uploaded.error_code(), "file_too_large");
    assert!(harness.storage.is_empty());
});

db_test!(async fn an_upload_with_no_declared_size_is_still_capped(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;
    harness.set_plan("free").await;

    // A chunked upload declares no size, so there is nothing to check before
    // reading. The stream is counted as it arrives for exactly this case.
    let uploaded = harness
        .upload_chunked(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::File {
                    name: "file",
                    filename: "big.txt",
                    // The free plan allows 10MB.
                    content: vec![b'x'; 11 * 1024 * 1024],
                },
            ],
        )
        .await;

    assert_eq!(uploaded.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(uploaded.error_code(), "file_too_large");

    // And the reservation it made was cleaned up.
    let statuses = harness.document_statuses().await;
    assert!(
        statuses.iter().all(|s| s == "deleted"),
        "an over-sized upload should not leave a document behind: {statuses:?}"
    );
});

db_test!(async fn uploading_into_another_tenants_knowledge_base_is_refused(db) {
    let mut alice = Harness::new(&db);
    let alice_workspace = alice.onboard().await;
    let alice_kb = alice.create_knowledge_base(&alice_workspace).await;

    let mut bob = Harness::new(&db);
    bob.onboard().await;

    // The foreign key would accept this: referential integrity runs with the
    // referenced table's owner privileges and sees rows RLS hides.
    let uploaded = bob
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &alice_kb),
                Part::Field("title", "Injected"),
                Part::Field("text", "this should not land in Alice's knowledge base"),
            ],
        )
        .await;

    assert_eq!(uploaded.status, StatusCode::NOT_FOUND);
    assert_eq!(uploaded.error_code(), "knowledge_base_not_found");

    let alice_documents = alice
        .get(&format!("/dashboard/v1/knowledge_bases/{alice_kb}/documents"))
        .await;
    assert_eq!(alice_documents.body["data"].as_array().unwrap().len(), 0);
});

// ---- the worker -----------------------------------------------------------

db_test!(async fn the_worker_takes_a_queued_document_to_ready(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let uploaded = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("title", "Handbook"),
                Part::Field("text", "หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์"),
            ],
        )
        .await;
    let document_id = uploaded.body["id"].as_str().unwrap().to_owned();

    assert!(harness.run_worker().await >= 1);

    let document = harness
        .get(&format!("/dashboard/v1/documents/{document_id}"))
        .await;

    assert_eq!(document.body["status"], "ready");
    assert_eq!(document.body["progress"], 100);
    assert_eq!(document.body["chunk_count"], 3);
    assert_eq!(document.body["language"], "th");
});

db_test!(async fn a_failed_document_can_be_retried_and_a_ready_one_cannot(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let uploaded = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("title", "Handbook"),
                Part::Field("text", "content"),
            ],
        )
        .await;
    let document_id: anthovai_core::DocumentId =
        uploaded.body["id"].as_str().unwrap().parse().unwrap();

    harness.run_worker().await;

    // Re-running a document that already worked would duplicate its chunks.
    let refused = harness
        .json(
            "POST",
            &format!("/dashboard/v1/documents/{document_id}/retry"),
            json!({}),
        )
        .await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert_eq!(refused.error_code(), "document_not_failed");

    // Now make it look like a parser gave up on it.
    let org_id: OrgId = harness.org_id.as_ref().unwrap().parse().unwrap();
    let ctx = TenantCtx::system(org_id, Plan::Enterprise);
    let mut tenant = db.tenant(&ctx).await.unwrap();
    knowledge_repo::set_failed(&mut tenant, document_id, "no_extractable_text", "a scan")
        .await
        .unwrap();
    tenant.commit().await.unwrap();

    let failed = harness
        .get(&format!("/dashboard/v1/documents/{document_id}"))
        .await;
    assert_eq!(failed.body["status"], "failed");
    assert_eq!(failed.body["error_code"], "no_extractable_text");

    let retried = harness
        .json(
            "POST",
            &format!("/dashboard/v1/documents/{document_id}/retry"),
            json!({}),
        )
        .await;
    assert_eq!(retried.status, StatusCode::ACCEPTED);
    assert_eq!(retried.body["status"], "queued");

    harness.run_worker().await;
    let recovered = harness
        .get(&format!("/dashboard/v1/documents/{document_id}"))
        .await;
    assert_eq!(recovered.body["status"], "ready");
    assert!(recovered.body["error_code"].is_null());
});

// ---- deletion and counters ------------------------------------------------

db_test!(async fn deleting_a_document_queues_its_chunks_for_removal(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let uploaded = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("title", "Handbook"),
                Part::Field("text", "content to be removed"),
            ],
        )
        .await;
    let document_id = uploaded.body["id"].as_str().unwrap().to_owned();
    harness.run_worker().await;

    let deleted = harness
        .delete(&format!("/dashboard/v1/documents/{document_id}"))
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);

    assert_eq!(
        harness.queued_kinds().await.get("delete_document_chunks"),
        Some(&1)
    );

    let gone = harness
        .get(&format!("/dashboard/v1/documents/{document_id}"))
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
});

db_test!(async fn the_knowledge_base_counts_what_it_holds(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let uploaded = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("title", "Handbook"),
                Part::Field("text", "some content here"),
            ],
        )
        .await;
    let document_id = uploaded.body["id"].as_str().unwrap().to_owned();

    let after_upload = harness
        .get(&format!("/dashboard/v1/knowledge_bases/{kb_id}"))
        .await;
    assert_eq!(after_upload.body["document_count"], 1);
    assert!(after_upload.body["storage_bytes"].as_i64().unwrap() > 0);

    harness
        .delete(&format!("/dashboard/v1/documents/{document_id}"))
        .await;

    let after_delete = harness
        .get(&format!("/dashboard/v1/knowledge_bases/{kb_id}"))
        .await;
    assert_eq!(after_delete.body["document_count"], 0);
    assert_eq!(
        after_delete.body["storage_bytes"], 0,
        "storage should be given back when a document goes"
    );
});

db_test!(async fn the_free_plan_caps_how_many_documents_a_base_holds(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    // Rather than upload fifty documents, move the tenant to a plan whose limit
    // is one. The check is the same one; only the number differs.
    let org_id: OrgId = harness.org_id.as_ref().unwrap().parse().unwrap();
    let mut db_conn = db.system().await.unwrap();
    anthovai_db::sqlx::query("UPDATE organizations SET plan = 'free' WHERE id = $1")
        .bind(org_id.to_db())
        .execute(db_conn.conn())
        .await
        .unwrap();
    db_conn.commit().await.unwrap();

    for i in 0..50 {
        let reply = harness
            .upload(
                "/dashboard/v1/documents",
                &[
                    Part::Field("knowledge_base_id", &kb_id),
                    Part::Field("title", "Doc"),
                    Part::Field("text", "content"),
                ],
            )
            .await;
        assert_eq!(reply.status, StatusCode::ACCEPTED, "upload {i} should fit");
    }

    let over = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("title", "One too many"),
                Part::Field("text", "content"),
            ],
        )
        .await;

    assert_eq!(over.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(over.error_code(), "document_limit_reached");
});

// ---- the public API -------------------------------------------------------

db_test!(async fn a_key_needs_the_write_scope_to_upload(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let issued = harness
        .json(
            "POST",
            "/dashboard/v1/api_keys",
            json!({
                "workspace_id": workspace_id,
                "name": "Read only",
                "scopes": ["knowledge:read"]
            }),
        )
        .await;
    harness.api_key = Some(issued.body["secret"].as_str().unwrap().to_owned());
    harness.cookie = None;
    harness.org_id = None;

    let refused = harness
        .upload(
            "/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("title", "Synced"),
                Part::Field("text", "from the customer's own system"),
            ],
        )
        .await;

    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.error_code(), "scope_missing");
});

db_test!(async fn a_key_with_the_write_scope_can_sync_documents(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    let issued = harness
        .json(
            "POST",
            "/dashboard/v1/api_keys",
            json!({
                "workspace_id": workspace_id,
                "name": "Nightly sync",
                "scopes": ["knowledge:read", "knowledge:write"]
            }),
        )
        .await;
    let key = issued.body["secret"].as_str().unwrap().to_owned();

    harness.api_key = Some(key);
    harness.cookie = None;
    harness.org_id = None;

    let uploaded = harness
        .upload(
            "/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("title", "Course catalogue"),
                Part::Field("text", "หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์"),
            ],
        )
        .await;

    assert_eq!(uploaded.status, StatusCode::ACCEPTED, "{:?}", uploaded.body);

    let listed = harness
        .get(&format!("/v1/documents?knowledge_base_id={kb_id}"))
        .await;
    assert_eq!(listed.body["data"].as_array().unwrap().len(), 1);
});

db_test!(async fn a_document_from_another_tenant_is_reported_missing(db) {
    let mut alice = Harness::new(&db);
    let alice_workspace = alice.onboard().await;
    let alice_kb = alice.create_knowledge_base(&alice_workspace).await;
    let alice_doc = alice
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &alice_kb),
                Part::Field("title", "Private"),
                Part::Field("text", "salary bands"),
            ],
        )
        .await;
    let document_id = alice_doc.body["id"].as_str().unwrap().to_owned();

    let mut bob = Harness::new(&db);
    bob.onboard().await;

    let reply = bob
        .get(&format!("/dashboard/v1/documents/{document_id}"))
        .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.error_code(), "document_not_found");

    let deleted = bob
        .delete(&format!("/dashboard/v1/documents/{document_id}"))
        .await;
    assert_eq!(deleted.status, StatusCode::NOT_FOUND);
});

// ---- URL ingestion --------------------------------------------------------
//
// The fetch itself is not exercised here — that would mean a network in the
// test suite. What is exercised is the guard in front of it, because that is
// the part where a mistake hands a customer our internal network.

db_test!(async fn a_url_pointing_at_the_cloud_metadata_service_is_refused(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    // The single most valuable target of an SSRF: this address hands out
    // credentials to whatever asks it, and it is reachable from our servers
    // and from nowhere the customer sits.
    let refused = harness
        .upload(
            "/dashboard/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("url", "http://169.254.169.254/latest/meta-data/"),
            ],
        )
        .await;

    assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{:?}", refused.body);
    assert_eq!(refused.error_code(), "url_not_allowed");

    // Nothing was written: the guard runs before a document row exists, so
    // there is no half-made document to clean up.
    assert!(harness.document_statuses().await.is_empty());
});

db_test!(async fn our_own_ports_are_not_reachable_through_the_upload_endpoint(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    for url in [
        "http://127.0.0.1:8080/internal/health",
        "http://10.0.0.5/admin",
        "http://192.168.1.1/",
        "http://[::1]:5432/",
        "file:///etc/passwd",
        "gopher://127.0.0.1:6379/_INFO",
    ] {
        let refused = harness
            .upload(
                "/dashboard/v1/documents",
                &[
                    Part::Field("knowledge_base_id", &kb_id),
                    Part::Field("url", url),
                ],
            )
            .await;

        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{url} was accepted: {:?}",
            refused.body
        );
        assert_eq!(refused.error_code(), "url_not_allowed", "{url}");
    }

    assert!(harness.document_statuses().await.is_empty());
});

db_test!(async fn a_url_upload_still_needs_the_write_scope(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.create_knowledge_base(&workspace_id).await;

    // A read-only key, sent over the public API.
    let issued = harness
        .json(
            "POST",
            "/dashboard/v1/api_keys",
            json!({
                "workspace_id": workspace_id,
                "name": "Read only",
                "scopes": ["knowledge:read"]
            }),
        )
        .await;
    harness.api_key = Some(issued.body["secret"].as_str().unwrap().to_owned());
    harness.cookie = None;
    harness.org_id = None;

    let refused = harness
        .upload(
            "/v1/documents",
            &[
                Part::Field("knowledge_base_id", &kb_id),
                Part::Field("url", "https://www.anthovai.com/pricing"),
            ],
        )
        .await;

    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{:?}", refused.body);
});
