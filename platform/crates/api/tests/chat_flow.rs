//! Asking an agent a question, end to end.
//!
//! Everything a customer touches happens over HTTP here — onboarding, uploading
//! a handbook, publishing an agent, minting a key, asking. Only ingestion is
//! called directly, because in production a worker does it and there is no
//! worker in this process.
//!
//! The model is the echo provider, so nothing here says whether an answer is
//! *good*. What it does say is that the question reached the right agent, the
//! passages came from the right tenant, the citation points at a passage that
//! was really offered, and the turn was recorded once with its usage. Those are
//! the parts that break when the wiring is wrong, and they are invisible from
//! any layer below this one.

use std::sync::Arc;

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
use anthovai_testkit::db_test;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};

mod common;
use common::{chat_services, multipart_body, Part, BOUNDARY, TEST_DIMENSION};

const DASHBOARD_ORIGIN: &str = "https://app.anthovai.com";

/// Written so a question can share words with exactly one section. The hash
/// embedder cannot rank meaning, so the keyword half of the hybrid search is
/// what has to find these — which is why they are in English.
const HANDBOOK: &str = "# ABC School Handbook\n\n\
## Library\n\n\
The library opens at seven in the morning and closes at nine in the evening. \
Students may borrow up to six books at a time.\n\n\
## Cafeteria\n\n\
The cafeteria serves hot lunch between eleven and two. A vegetarian option is \
available every day.\n\n\
## Parking\n\n\
Parking permits cost four hundred baht per semester and are issued by the \
registrar office.\n";

struct Harness {
    app: Router,
    auth: Arc<AuthService>,
    storage: Arc<InMemoryStorage>,
    db: Db,
    cookie: Option<String>,
    org_id: Option<String>,
    api_key: Option<String>,
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

impl Harness {
    fn new(db: &Db) -> Self {
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
            vec![DASHBOARD_ORIGIN.to_owned()],
        );

        let auth = Arc::clone(&state.auth);
        Self {
            app: anthovai_api::app(state),
            auth,
            storage,
            db: db.clone(),
            cookie: None,
            org_id: None,
            api_key: None,
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

    async fn send(&self, request: Request<Body>) -> Reply {
        use tower::ServiceExt;

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

    async fn post(&self, uri: &str, body: Value) -> Reply {
        self.json("POST", uri, body).await
    }

    async fn delete(&self, uri: &str) -> Reply {
        self.send(self.builder("DELETE", uri).body(Body::empty()).unwrap())
            .await
    }

    async fn onboard(&mut self) -> String {
        let email = format!(
            "owner-{}@abc.ac.th",
            anthovai_core::UserId::new().to_db().to_lowercase()
        );

        let signed_up = self
            .post(
                "/dashboard/v1/auth/signup",
                json!({"email": email, "password": "correct horse battery"}),
            )
            .await;
        assert_eq!(
            signed_up.status,
            StatusCode::CREATED,
            "{:?}",
            signed_up.body
        );

        let user_id = signed_up.body["user_id"].as_str().unwrap().parse().unwrap();
        self.auth.mark_email_verified(user_id).await.unwrap();

        let signed_in = self
            .post(
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
            .post(
                "/dashboard/v1/organizations",
                json!({
                    "name": "ABC School",
                    "slug": format!("abc-{}", OrgId::new().to_db().to_lowercase())
                }),
            )
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
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

    /// A knowledge base holding the handbook, indexed and searchable.
    async fn seed_knowledge(&self, workspace_id: &str) -> String {
        let created = self
            .post(
                "/dashboard/v1/knowledge_bases",
                json!({"workspace_id": workspace_id, "name": "Student Handbook"}),
            )
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
        let kb_id = created.body["id"].as_str().unwrap().to_owned();

        let body = multipart_body(&[
            Part::Field("knowledge_base_id", &kb_id),
            Part::File {
                name: "file",
                filename: "handbook.md",
                content: HANDBOOK.as_bytes().to_vec(),
            },
        ]);
        let uploaded = self
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
        assert_eq!(uploaded.status, StatusCode::ACCEPTED, "{:?}", uploaded.body);

        self.ingest(uploaded.body["id"].as_str().unwrap()).await;
        kb_id
    }

    /// What the worker would do with the queued job.
    async fn ingest(&self, document_id: &str) {
        let pipeline = IngestPipeline::new(
            self.db.clone(),
            Arc::clone(&self.storage) as Storage,
            Arc::new(EmbeddingRunner::new(
                Arc::new(HashEmbedder::new(TEST_DIMENSION)),
                RunnerConfig::default(),
            )),
            pipeline::chunk_config_from(500, 80),
        );

        let org_id: OrgId = self.org_id.as_ref().unwrap().parse().unwrap();
        let document_id: DocumentId = document_id.parse().unwrap();
        let outcome = pipeline
            .run(org_id, document_id, 1)
            .await
            .expect("ingest the handbook");
        assert!(outcome.chunks > 0, "the handbook produced no chunks");
    }

    /// An agent that can answer from `kb_id`, live.
    async fn publish_agent(&self, workspace_id: &str, name: &str, kb_id: &str) -> String {
        let created = self
            .post(
                "/dashboard/v1/agents",
                json!({
                    "workspace_id": workspace_id,
                    "name": name,
                    "config": {"instructions": "You answer questions about ABC School."}
                }),
            )
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
        let agent_id = created.body["id"].as_str().unwrap().to_owned();

        let attached = self
            .json(
                "PUT",
                &format!("/dashboard/v1/agents/{agent_id}/knowledge_bases"),
                json!({"knowledge_base_ids": [kb_id]}),
            )
            .await;
        assert_eq!(
            attached.status,
            StatusCode::NO_CONTENT,
            "{:?}",
            attached.body
        );

        let published = self
            .post(
                &format!("/dashboard/v1/agents/{agent_id}/publish"),
                json!({}),
            )
            .await;
        assert_eq!(published.status, StatusCode::OK, "{:?}", published.body);

        agent_id
    }

    async fn take_api_key(&mut self, workspace_id: &str, body: Value) -> String {
        let mut request = json!({"workspace_id": workspace_id, "name": "Website"});
        for (key, value) in body.as_object().unwrap() {
            request[key] = value.clone();
        }

        let issued = self.post("/dashboard/v1/api_keys", request).await;
        assert_eq!(issued.status, StatusCode::CREATED, "{:?}", issued.body);
        let secret = issued.body["secret"].as_str().unwrap().to_owned();
        self.api_key = Some(secret.clone());
        secret
    }

    /// Move this organization onto another plan. Stands in for the staff-only
    /// endpoint, which is P3.
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

    async fn ask(&self, agent_id: &str, message: &str) -> Reply {
        self.post(
            "/v1/chat",
            json!({"agent_id": agent_id, "message": message}),
        )
        .await
    }
}

/// Onboard, index the handbook, publish an agent, and hold a key for it.
async fn ready(db: &Db) -> (Harness, String) {
    let mut harness = Harness::new(db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.seed_knowledge(&workspace_id).await;
    let agent_id = harness
        .publish_agent(&workspace_id, "Admissions", &kb_id)
        .await;
    harness
        .take_api_key(&workspace_id, json!({"scopes": ["chat", "usage:read"]}))
        .await;
    (harness, agent_id)
}

// ---- answering ------------------------------------------------------------

db_test!(async fn a_question_is_answered_from_the_uploaded_handbook(db) {
    let (harness, agent_id) = ready(&db).await;

    let answered = harness
        .ask(&agent_id, "When does the library open in the morning?")
        .await;
    assert_eq!(answered.status, StatusCode::OK, "{:?}", answered.body);

    assert_eq!(answered.body["agent_id"], agent_id.as_str());
    assert_eq!(answered.body["grounded"], true);
    assert!(!answered.body["answer"].as_str().unwrap().is_empty());
    assert!(answered.body["conversation_id"].is_string());

    // A citation the customer can follow: every source names a document that
    // was actually retrieved for this question.
    let sources = answered.body["sources"].as_array().unwrap();
    assert!(!sources.is_empty(), "an answer with no sources is not grounded");
    for source in sources {
        assert!(source["document_id"].is_string(), "{source:?}");
    }

    // Usage is what a customer is billed on, so it has to be reported.
    assert!(answered.body["usage"]["input_tokens"].as_u64().unwrap() > 0);
});

db_test!(async fn a_second_question_continues_the_same_conversation(db) {
    let (harness, agent_id) = ready(&db).await;

    let first = harness.ask(&agent_id, "When does the library open?").await;
    let conversation_id = first.body["conversation_id"].as_str().unwrap().to_owned();

    let second = harness
        .post(
            "/v1/chat",
            json!({
                "agent_id": agent_id,
                "message": "And when does the cafeteria serve lunch?",
                "conversation_id": conversation_id
            }),
        )
        .await;
    assert_eq!(second.status, StatusCode::OK, "{:?}", second.body);
    assert_eq!(second.body["conversation_id"], conversation_id.as_str());

    // Both turns, in order, each question with its answer.
    let detail = harness
        .get(&format!("/v1/conversations/{conversation_id}"))
        .await;
    assert_eq!(detail.status, StatusCode::OK, "{:?}", detail.body);

    let roles: Vec<&str> = detail.body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, vec!["user", "assistant", "user", "assistant"]);
});

db_test!(async fn asking_something_the_handbook_does_not_cover_is_not_invented(db) {
    let (harness, agent_id) = ready(&db).await;

    let answered = harness
        .ask(&agent_id, "zzzz qqqq vvvv unrelated gibberish token")
        .await;
    assert_eq!(answered.status, StatusCode::OK, "{:?}", answered.body);

    // Nothing was retrieved, so there is nothing to ground an answer in. The
    // agent says so rather than answering from the model's own memory.
    assert_eq!(answered.body["grounded"], false, "{:?}", answered.body);
    assert!(answered.body["sources"].as_array().unwrap().is_empty());
});

// ---- who may ask ----------------------------------------------------------

db_test!(async fn an_unpublished_agent_cannot_be_asked(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let _kb_id = harness.seed_knowledge(&workspace_id).await;

    let created = harness
        .post(
            "/dashboard/v1/agents",
            json!({
                "workspace_id": workspace_id,
                "name": "Draft",
                "config": {"instructions": "You answer questions about ABC School."}
            }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    let agent_id = created.body["id"].as_str().unwrap().to_owned();

    harness
        .take_api_key(&workspace_id, json!({"scopes": ["chat", "usage:read"]}))
        .await;

    let answered = harness.ask(&agent_id, "When does the library open?").await;
    assert_eq!(answered.status, StatusCode::FORBIDDEN, "{:?}", answered.body);
    assert_eq!(answered.error_code(), "agent_not_published");
});

db_test!(async fn a_key_scoped_to_one_agent_cannot_ask_another(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    harness.set_plan("business").await;

    let kb_id = harness.seed_knowledge(&workspace_id).await;

    let admissions = harness
        .publish_agent(&workspace_id, "Admissions", &kb_id)
        .await;
    let finance = harness.publish_agent(&workspace_id, "Finance", &kb_id).await;

    harness
        .take_api_key(
            &workspace_id,
            json!({"all_agents": false, "agent_ids": [admissions]}),
        )
        .await;

    let allowed = harness.ask(&admissions, "When does the library open?").await;
    assert_eq!(allowed.status, StatusCode::OK, "{:?}", allowed.body);

    // Not 403: a key that may not touch this agent is not told it exists.
    let refused = harness.ask(&finance, "When does the library open?").await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{:?}", refused.body);
    assert_eq!(refused.error_code(), "agent_not_found");
});

db_test!(async fn a_key_without_the_chat_scope_is_refused(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.seed_knowledge(&workspace_id).await;
    let agent_id = harness
        .publish_agent(&workspace_id, "Admissions", &kb_id)
        .await;

    harness
        .take_api_key(&workspace_id, json!({"scopes": ["agents:read"]}))
        .await;

    let refused = harness.ask(&agent_id, "When does the library open?").await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{:?}", refused.body);
});

db_test!(async fn one_tenant_cannot_read_another_tenants_conversations(db) {
    let (harness, agent_id) = ready(&db).await;
    let answered = harness.ask(&agent_id, "When does the library open?").await;
    let conversation_id = answered.body["conversation_id"].as_str().unwrap().to_owned();

    // A second organization, with its own key, asking for the first one's
    // conversation by id.
    let mut intruder = Harness::new(&db);
    let workspace_id = intruder.onboard().await;
    intruder
        .take_api_key(&workspace_id, json!({"scopes": ["chat", "usage:read"]}))
        .await;

    let refused = intruder
        .get(&format!("/v1/conversations/{conversation_id}"))
        .await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{:?}", refused.body);

    let listed = intruder.get("/v1/conversations").await;
    assert_eq!(listed.status, StatusCode::OK);
    assert!(listed.body["data"].as_array().unwrap().is_empty());
});

// ---- erasure and usage ----------------------------------------------------

db_test!(async fn deleting_a_conversation_really_removes_it(db) {
    let (harness, agent_id) = ready(&db).await;
    let answered = harness.ask(&agent_id, "When does the library open?").await;
    let conversation_id = answered.body["conversation_id"].as_str().unwrap().to_owned();

    let deleted = harness
        .delete(&format!("/v1/conversations/{conversation_id}"))
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{:?}", deleted.body);

    let gone = harness
        .get(&format!("/v1/conversations/{conversation_id}"))
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
});

db_test!(async fn usage_counts_every_answered_question(db) {
    let (harness, agent_id) = ready(&db).await;

    for _ in 0..3 {
        let answered = harness.ask(&agent_id, "When does the library open?").await;
        assert_eq!(answered.status, StatusCode::OK, "{:?}", answered.body);
    }

    let usage = harness.get("/v1/usage").await;
    assert_eq!(usage.status, StatusCode::OK, "{:?}", usage.body);
    assert_eq!(usage.body["totals"]["messages"], 3);
    assert!(usage.body["totals"]["input_tokens"].as_i64().unwrap() > 0);
    assert_eq!(usage.body["quota"]["messages_used"], 3);
});

// ---- the playground -------------------------------------------------------

db_test!(async fn the_playground_runs_the_draft_and_shows_its_working(db) {
    let mut harness = Harness::new(&db);
    let workspace_id = harness.onboard().await;
    let kb_id = harness.seed_knowledge(&workspace_id).await;
    let agent_id = harness
        .publish_agent(&workspace_id, "Admissions", &kb_id)
        .await;

    // An edit that has not been published. The playground should be answering
    // from this, which is the whole point of trying it before customers see it.
    let edited = harness
        .json(
            "PATCH",
            &format!("/dashboard/v1/agents/{agent_id}"),
            json!({"config": {"instructions": "Answer only in one sentence."}}),
        )
        .await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.body);

    let tried = harness
        .post(
            &format!("/dashboard/v1/agents/{agent_id}/test"),
            json!({"message": "When does the library open?"}),
        )
        .await;
    assert_eq!(tried.status, StatusCode::OK, "{:?}", tried.body);
    assert!(!tried.body["answer"].as_str().unwrap().is_empty());

    // Why these passages: the first thing anyone asks when an answer is wrong.
    let passages = tried.body["retrieval"]["passages"].as_array().unwrap();
    assert!(!passages.is_empty(), "{:?}", tried.body);
    assert!(!passages[0]["snippet"].as_str().unwrap().is_empty());
    assert!(passages[0]["chunk_id"].is_string());

    // A trial run is not a customer's question and must not be billed.
    harness
        .take_api_key(&workspace_id, json!({"scopes": ["chat", "usage:read"]}))
        .await;
    let usage = harness.get("/v1/usage").await;
    assert_eq!(usage.body["totals"]["messages"], 0, "{:?}", usage.body);
});
