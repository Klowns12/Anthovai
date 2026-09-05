//! The HTTP layer end to end, against a real PostgreSQL.
//!
//! These drive the router the way a client does — headers, cookies, status
//! codes, JSON bodies — because that is the layer where an authorisation check
//! gets forgotten or a secret leaks into a listing. The services underneath
//! have their own tests; these are about the wiring.

use anthovai_agent::AgentService;
use anthovai_api::{AppState, Services};
use anthovai_auth::{password::PasswordHasherConfig, AuthConfig, AuthService};
use anthovai_core::config::EmbeddingSettings;
use anthovai_core::Clock;
use anthovai_db::Db;
use anthovai_knowledge::KnowledgeService;
use anthovai_storage::InMemoryStorage;
use anthovai_tenant::TenantService;
use anthovai_testkit::db_test;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

mod common;
use common::chat_services;

const DASHBOARD_ORIGIN: &str = "https://app.anthovai.com";

fn app(db: &Db) -> (Router, Arc<AuthService>) {
    let clock = Clock::system();
    let storage: anthovai_storage::Storage = Arc::new(InMemoryStorage::new());
    let agents = Arc::new(AgentService::new(db.clone()));
    let (chat, conversations) = chat_services(db, Arc::clone(&agents), &clock);

    let state = AppState::new(
        Services {
            auth: AuthService::new(
                db.clone(),
                clock.clone(),
                AuthConfig {
                    // Production-cost hashing would make this suite slow for no
                    // extra confidence; the parameters have their own tests.
                    password: PasswordHasherConfig::fast_for_tests(),
                    ..AuthConfig::default()
                },
            ),
            tenants: TenantService::new(db.clone()),
            agents,
            knowledge: KnowledgeService::new(
                db.clone(),
                Arc::clone(&storage),
                test_embedding_settings(),
            ),
            chat,
            conversations,
            diagnostics: common::diagnostics(db, storage),
        },
        clock,
        vec![DASHBOARD_ORIGIN.to_owned()],
    );

    let auth = Arc::clone(&state.auth);
    (anthovai_api::app(state), auth)
}

fn test_embedding_settings() -> EmbeddingSettings {
    EmbeddingSettings {
        // Named `fake:` so a knowledge base created here can never be mistaken
        // for one whose chunks were embedded by a real model.
        default_model: "fake:hash-1536".to_owned(),
        dimension: 1536,
        batch_size: 64,
        concurrency: 4,
    }
}

/// One request, one response, parsed.
struct Reply {
    status: StatusCode,
    body: Value,
    set_cookie: Option<String>,
    cache_control: Option<String>,
}

impl Reply {
    fn error_code(&self) -> &str {
        self.body["error"]["code"].as_str().unwrap_or_default()
    }
}

struct Client {
    app: Router,
    auth: Arc<AuthService>,
    db: Db,
    cookie: Option<String>,
    org_id: Option<String>,
    api_key: Option<String>,
}

impl Client {
    fn new(db: &Db) -> Self {
        let (app, auth) = app(db);
        Self {
            app,
            auth,
            db: db.clone(),
            cookie: None,
            org_id: None,
            api_key: None,
        }
    }

    /// Move this organization onto another plan. Stands in for the staff-only
    /// endpoint in `docs/spec-v0.1/05-api-specification.md` §9.7, which is P3.
    async fn set_plan(&self, plan: &str) {
        let org_id: anthovai_core::OrgId = self.org_id.as_ref().unwrap().parse().unwrap();
        let mut db = self.db.system().await.unwrap();
        anthovai_db::sqlx::query("UPDATE organizations SET plan = $2 WHERE id = $1")
            .bind(org_id.to_db())
            .bind(plan)
            .execute(db.conn())
            .await
            .unwrap();
        db.commit().await.unwrap();
    }

    async fn send(&self, method: &str, uri: &str, body: Option<Value>) -> Reply {
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

        let request = match body {
            Some(json) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&json).unwrap()))
                .unwrap(),
            None => request.body(Body::empty()).unwrap(),
        };

        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let cache_control = response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

        Reply {
            status,
            body,
            set_cookie,
            cache_control,
        }
    }

    async fn get(&self, uri: &str) -> Reply {
        self.send("GET", uri, None).await
    }

    async fn post(&self, uri: &str, body: Value) -> Reply {
        self.send("POST", uri, Some(body)).await
    }

    async fn patch(&self, uri: &str, body: Value) -> Reply {
        self.send("PATCH", uri, Some(body)).await
    }

    async fn put(&self, uri: &str, body: Value) -> Reply {
        self.send("PUT", uri, Some(body)).await
    }

    async fn delete(&self, uri: &str) -> Reply {
        self.send("DELETE", uri, None).await
    }

    /// Sign up, sign in, create an organization: the state every other test
    /// starts from.
    async fn onboard(&mut self) -> String {
        let email = format!(
            "owner-{}@abc.ac.th",
            anthovai_core::UserId::new().to_db().to_lowercase()
        );

        let signed_up = self
            .post(
                "/dashboard/v1/auth/signup",
                json!({"email": email, "password": "correct horse battery", "name": "Owner"}),
            )
            .await;
        assert_eq!(
            signed_up.status,
            StatusCode::CREATED,
            "{:?}",
            signed_up.body
        );

        // Stands in for clicking the link in the verification mail, which is
        // what a live API key requires. The mailer itself is P3.
        let user_id = signed_up.body["user_id"].as_str().unwrap().parse().unwrap();
        self.auth.mark_email_verified(user_id).await.unwrap();

        let signed_in = self
            .post(
                "/dashboard/v1/auth/login",
                json!({"email": email, "password": "correct horse battery"}),
            )
            .await;
        assert_eq!(signed_in.status, StatusCode::OK, "{:?}", signed_in.body);
        self.cookie = Some(cookie_pair(&signed_in.set_cookie.unwrap()));

        let slug = format!("abc-{}", anthovai_core::OrgId::new().to_db().to_lowercase());
        let created = self
            .post(
                "/dashboard/v1/organizations",
                json!({"name": "ABC School", "slug": slug}),
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

    /// Mint a key and start sending it.
    async fn take_api_key(&mut self, workspace_id: &str) -> String {
        let issued = self
            .post(
                "/dashboard/v1/api_keys",
                json!({"workspace_id": workspace_id, "name": "Production website"}),
            )
            .await;
        assert_eq!(issued.status, StatusCode::CREATED, "{:?}", issued.body);

        let secret = issued.body["secret"].as_str().unwrap().to_owned();
        self.api_key = Some(secret.clone());
        secret
    }

    async fn create_agent(&self, workspace_id: &str, name: &str) -> Value {
        let created = self
            .post(
                "/dashboard/v1/agents",
                json!({
                    "workspace_id": workspace_id,
                    "name": name,
                    "config": {"instructions": "You help students of ABC School."}
                }),
            )
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
        created.body
    }
}

/// `Set-Cookie` carries attributes; a `Cookie` header carries only the pair.
fn cookie_pair(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_owned()
}

// ---- health and plumbing --------------------------------------------------

db_test!(async fn health_answers(db) {
    let client = Client::new(&db);
    let reply = client.get("/internal/health").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["status"], "ok");
});

db_test!(async fn errors_carry_the_documented_shape(db) {
    let client = Client::new(&db);
    let reply = client.get("/v1/agents").await;

    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.body["error"]["type"], "authentication_error");
    assert_eq!(reply.error_code(), "missing_bearer_token");
    assert!(reply.body["error"]["request_id"].as_str().is_some());
    assert!(reply.body["error"]["doc_url"].as_str().is_some());
});

// ---- sign-in --------------------------------------------------------------

db_test!(async fn the_session_cookie_is_locked_down(db) {
    let mut client = Client::new(&db);
    client.onboard().await;

    let cookie = client.cookie.as_ref().unwrap();
    assert!(cookie.starts_with("__Host-av_session="));
});

db_test!(async fn signing_in_sets_a_no_store_response(db) {
    let client = Client::new(&db);
    let email = format!(
        "owner-{}@abc.ac.th",
        anthovai_core::UserId::new().to_db().to_lowercase()
    );
    client
        .post(
            "/dashboard/v1/auth/signup",
            json!({"email": email, "password": "correct horse battery"}),
        )
        .await;

    let reply = client
        .post(
            "/dashboard/v1/auth/login",
            json!({"email": email, "password": "correct horse battery"}),
        )
        .await;

    assert_eq!(reply.cache_control.as_deref(), Some("no-store"));
});

db_test!(async fn a_wrong_password_is_a_401_not_a_500(db) {
    let client = Client::new(&db);
    let reply = client
        .post(
            "/dashboard/v1/auth/login",
            json!({"email": "nobody@example.com", "password": "whatever it is"}),
        )
        .await;

    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.error_code(), "invalid_credentials");
});

db_test!(async fn repeated_failures_are_rate_limited(db) {
    let client = Client::new(&db);
    let email = format!(
        "target-{}@abc.ac.th",
        anthovai_core::UserId::new().to_db().to_lowercase()
    );
    let attempt = json!({"email": email, "password": "wrong password here"});

    for _ in 0..5 {
        let reply = client.post("/dashboard/v1/auth/login", attempt.clone()).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }

    let blocked = client.post("/dashboard/v1/auth/login", attempt).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(blocked.error_code(), "rate_limited");
});

db_test!(async fn a_cross_site_post_is_refused(db) {
    let mut client = Client::new(&db);
    client.onboard().await;

    // Same session, forged Origin.
    let request = Request::builder()
        .method("POST")
        .uri("/dashboard/v1/workspaces")
        .header(header::ORIGIN, "https://evil.example.com")
        .header(header::COOKIE, client.cookie.as_ref().unwrap())
        .header("x-org-id", client.org_id.as_ref().unwrap())
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"name": "Injected", "slug": "injected"})).unwrap(),
        ))
        .unwrap();

    let response = client.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
});

db_test!(async fn me_lists_the_organizations_the_user_belongs_to(db) {
    let mut client = Client::new(&db);
    client.onboard().await;

    let reply = client.get("/dashboard/v1/me").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["organizations"].as_array().unwrap().len(), 1);
    assert_eq!(reply.body["organizations"][0]["role"], "owner");
    // The password hash must never reach a response.
    assert!(reply.body["user"].get("password_hash").is_none());
});

db_test!(async fn signing_out_clears_the_cookie_and_the_session(db) {
    let mut client = Client::new(&db);
    client.onboard().await;

    let reply = client.post("/dashboard/v1/auth/logout", json!({})).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert!(reply.set_cookie.unwrap().contains("Max-Age=0"));

    let after = client.get("/dashboard/v1/me").await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
});

// ---- organization scoping -------------------------------------------------

db_test!(async fn a_missing_org_header_is_a_clear_error(db) {
    let mut client = Client::new(&db);
    client.onboard().await;
    client.org_id = None;

    let reply = client.get("/dashboard/v1/workspaces").await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
});

db_test!(async fn naming_another_users_organization_reports_it_missing(db) {
    let mut alice = Client::new(&db);
    alice.onboard().await;

    let mut bob = Client::new(&db);
    bob.onboard().await;

    // Alice's session, Bob's organization id.
    alice.org_id = bob.org_id.clone();
    let reply = alice.get("/dashboard/v1/workspaces").await;

    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.error_code(), "organization_not_found");
});

db_test!(async fn workspaces_can_be_created_listed_and_deleted(db) {
    let mut client = Client::new(&db);
    client.onboard().await;

    let created = client
        .post(
            "/dashboard/v1/workspaces",
            json!({"name": "Customer Support", "slug": "support"}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let workspace_id = created.body["id"].as_str().unwrap().to_owned();

    let listed = client.get("/dashboard/v1/workspaces").await;
    assert_eq!(listed.body["data"].as_array().unwrap().len(), 2);

    let deleted = client
        .delete(&format!("/dashboard/v1/workspaces/{workspace_id}"))
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);

    let after = client.get("/dashboard/v1/workspaces").await;
    assert_eq!(after.body["data"].as_array().unwrap().len(), 1);
});

db_test!(async fn the_organization_can_be_renamed(db) {
    let mut client = Client::new(&db);
    client.onboard().await;

    let renamed = client
        .patch("/dashboard/v1/organizations/current", json!({"name": "ABC International"}))
        .await;

    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(renamed.body["name"], "ABC International");
});

// ---- API keys -------------------------------------------------------------

db_test!(async fn a_key_is_shown_once_and_then_only_by_prefix(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;

    let issued = client
        .post(
            "/dashboard/v1/api_keys",
            json!({"workspace_id": workspace_id, "name": "Production website"}),
        )
        .await;

    assert_eq!(issued.status, StatusCode::CREATED);
    let secret = issued.body["secret"].as_str().unwrap();
    assert!(secret.starts_with("av_live_"));
    assert_eq!(issued.cache_control.as_deref(), Some("no-store"));

    let listed = client.get("/dashboard/v1/api_keys").await;
    let key = &listed.body["data"][0];
    assert!(key.get("secret").is_none(), "the listing must not carry the secret");
    assert!(key.get("key_hash").is_none());
    assert!(secret.starts_with(key["prefix"].as_str().unwrap()));
});

db_test!(async fn a_key_authenticates_against_the_public_api(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    client.take_api_key(&workspace_id).await;

    // The default scope is chat only, so reading agents is refused.
    let refused = client.get("/v1/agents").await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.error_code(), "scope_missing");
});

db_test!(async fn a_scoped_key_can_read_agents(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;

    let issued = client
        .post(
            "/dashboard/v1/api_keys",
            json!({
                "workspace_id": workspace_id,
                "name": "Reader",
                "scopes": ["chat", "agents:read"]
            }),
        )
        .await;
    client.api_key = Some(issued.body["secret"].as_str().unwrap().to_owned());

    let listed = client.get("/v1/agents").await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(listed.body["data"].as_array().unwrap().len(), 0);
});

db_test!(async fn a_revoked_key_is_refused_by_the_public_api(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;

    let issued = client
        .post(
            "/dashboard/v1/api_keys",
            json!({"workspace_id": workspace_id, "name": "Doomed", "scopes": ["agents:read"]}),
        )
        .await;
    let key_id = issued.body["id"].as_str().unwrap().to_owned();
    client.api_key = Some(issued.body["secret"].as_str().unwrap().to_owned());

    assert_eq!(client.get("/v1/agents").await.status, StatusCode::OK);

    let revoked = client
        .post(&format!("/dashboard/v1/api_keys/{key_id}/revoke"), json!({}))
        .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);

    let refused = client.get("/v1/agents").await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    assert_eq!(refused.error_code(), "revoked_api_key");
});

db_test!(async fn a_key_in_the_query_string_is_refused(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    let secret = client.take_api_key(&workspace_id).await;

    // A key in a URL ends up in logs and referrers. Refuse it outright rather
    // than accept a credential that should now be considered leaked.
    let request = Request::builder()
        .method("GET")
        .uri(format!("/v1/agents?api_key={secret}"))
        .header(header::AUTHORIZATION, format!("Bearer {secret}"))
        .body(Body::empty())
        .unwrap();

    let response = client.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
});

// ---- agents ---------------------------------------------------------------

db_test!(async fn an_agent_starts_as_a_draft_and_is_invisible_publicly(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;

    let agent = client.create_agent(&workspace_id, "ABC School Assistant").await;
    assert_eq!(agent["status"], "draft");
    assert_eq!(agent["draft_version"], 1);
    assert!(agent["published_version"].is_null());

    let agent_id = agent["id"].as_str().unwrap().to_owned();

    let issued = client
        .post(
            "/dashboard/v1/api_keys",
            json!({"workspace_id": workspace_id, "name": "Reader", "scopes": ["agents:read"]}),
        )
        .await;
    client.api_key = Some(issued.body["secret"].as_str().unwrap().to_owned());

    let publicly = client.get(&format!("/v1/agents/{agent_id}")).await;
    assert_eq!(publicly.status, StatusCode::FORBIDDEN);
    assert_eq!(publicly.error_code(), "agent_not_published");
});

db_test!(async fn publishing_makes_an_agent_live(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    let agent_id = client
        .create_agent(&workspace_id, "ABC School Assistant")
        .await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let published = client
        .post(&format!("/dashboard/v1/agents/{agent_id}/publish"), json!({}))
        .await;
    assert_eq!(published.status, StatusCode::OK);
    assert_eq!(published.body["status"], "active");
    assert_eq!(published.body["published_version"], 1);

    let issued = client
        .post(
            "/dashboard/v1/api_keys",
            json!({"workspace_id": workspace_id, "name": "Reader", "scopes": ["agents:read"]}),
        )
        .await;
    client.api_key = Some(issued.body["secret"].as_str().unwrap().to_owned());

    let publicly = client.get(&format!("/v1/agents/{agent_id}")).await;
    assert_eq!(publicly.status, StatusCode::OK);
    assert_eq!(publicly.body["status"], "active");
    assert_eq!(publicly.body["published_version"], 1);
});

db_test!(async fn the_public_view_never_reveals_the_configuration(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    let agent_id = client.create_agent(&workspace_id, "Assistant").await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    client
        .post(&format!("/dashboard/v1/agents/{agent_id}/publish"), json!({}))
        .await;

    let issued = client
        .post(
            "/dashboard/v1/api_keys",
            json!({"workspace_id": workspace_id, "name": "Reader", "scopes": ["agents:read"]}),
        )
        .await;
    client.api_key = Some(issued.body["secret"].as_str().unwrap().to_owned());

    let publicly = client.get(&format!("/v1/agents/{agent_id}")).await;
    let body = publicly.body.to_string();

    for secret in ["instructions", "model_policy", "config", "You help students"] {
        assert!(
            !body.contains(secret),
            "the public agent view leaked `{secret}`: {body}"
        );
    }
});

db_test!(async fn editing_creates_a_new_draft_and_leaves_the_live_version_alone(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    let agent_id = client.create_agent(&workspace_id, "Assistant").await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    client
        .post(&format!("/dashboard/v1/agents/{agent_id}/publish"), json!({}))
        .await;

    let edited = client
        .patch(
            &format!("/dashboard/v1/agents/{agent_id}"),
            json!({"config": {"instructions": "A completely different brief."}}),
        )
        .await;

    assert_eq!(edited.status, StatusCode::OK);
    assert_eq!(edited.body["draft_version"], 2);
    assert_eq!(
        edited.body["published_version"], 1,
        "editing must not change what customers are being served"
    );
    assert_eq!(
        edited.body["published_config"]["instructions"],
        "You help students of ABC School."
    );
});

db_test!(async fn a_bad_configuration_is_refused_before_it_becomes_a_version(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    let agent_id = client.create_agent(&workspace_id, "Assistant").await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let refused = client
        .patch(
            &format!("/dashboard/v1/agents/{agent_id}"),
            json!({"config": {
                "instructions": "fine",
                "retrieval": {
                    "top_k": 500,
                    "context_token_budget": 6000,
                    "min_relevance": 0.25,
                    "hybrid": true,
                    "mmr_lambda": 0.7
                }
            }}),
        )
        .await;

    assert_eq!(refused.status, StatusCode::BAD_REQUEST);

    let after = client.get(&format!("/dashboard/v1/agents/{agent_id}")).await;
    assert_eq!(after.body["draft_version"], 1, "no version should have been written");
});

db_test!(async fn choosing_a_provider_needs_a_higher_plan(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;

    let refused = client
        .post(
            "/dashboard/v1/agents",
            json!({
                "workspace_id": workspace_id,
                "name": "Picky",
                "config": {
                    "instructions": "hello",
                    "model_policy": {"type": "provider_only", "provider": "anthropic"}
                }
            }),
        )
        .await;

    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert!(refused.error_code().starts_with("plan_required"));
});

db_test!(async fn rolling_back_republishes_an_earlier_version(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    let agent_id = client.create_agent(&workspace_id, "Assistant").await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    client
        .post(&format!("/dashboard/v1/agents/{agent_id}/publish"), json!({}))
        .await;
    client
        .patch(
            &format!("/dashboard/v1/agents/{agent_id}"),
            json!({"config": {"instructions": "A regrettable rewrite."}}),
        )
        .await;
    client
        .post(&format!("/dashboard/v1/agents/{agent_id}/publish"), json!({}))
        .await;

    let rolled_back = client
        .post(
            &format!("/dashboard/v1/agents/{agent_id}/rollback"),
            json!({"version": 1}),
        )
        .await;

    assert_eq!(rolled_back.status, StatusCode::OK);
    assert_eq!(rolled_back.body["published_version"], 1);
    assert_eq!(
        rolled_back.body["draft_version"], 2,
        "a rollback must not throw away work in progress"
    );
});

db_test!(async fn a_paused_agent_refuses_public_traffic_but_stays_editable(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    let agent_id = client.create_agent(&workspace_id, "Assistant").await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    client
        .post(&format!("/dashboard/v1/agents/{agent_id}/publish"), json!({}))
        .await;

    let issued = client
        .post(
            "/dashboard/v1/api_keys",
            json!({"workspace_id": workspace_id, "name": "Reader", "scopes": ["agents:read"]}),
        )
        .await;
    let key = issued.body["secret"].as_str().unwrap().to_owned();

    let paused = client
        .post(&format!("/dashboard/v1/agents/{agent_id}/pause"), json!({}))
        .await;
    assert_eq!(paused.status, StatusCode::NO_CONTENT);

    client.api_key = Some(key);
    let publicly = client.get(&format!("/v1/agents/{agent_id}")).await;
    assert_eq!(publicly.status, StatusCode::FORBIDDEN);
    assert_eq!(publicly.error_code(), "agent_paused");

    // The dashboard can still see and fix it.
    client.api_key = None;
    let internally = client.get(&format!("/dashboard/v1/agents/{agent_id}")).await;
    assert_eq!(internally.status, StatusCode::OK);
    assert_eq!(internally.body["status"], "paused");
});

db_test!(async fn an_archived_agent_is_gone_publicly(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    let agent_id = client.create_agent(&workspace_id, "Assistant").await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    client
        .post(&format!("/dashboard/v1/agents/{agent_id}/publish"), json!({}))
        .await;

    let issued = client
        .post(
            "/dashboard/v1/api_keys",
            json!({"workspace_id": workspace_id, "name": "Reader", "scopes": ["agents:read"]}),
        )
        .await;
    let key = issued.body["secret"].as_str().unwrap().to_owned();

    client
        .post(&format!("/dashboard/v1/agents/{agent_id}/archive"), json!({}))
        .await;

    client.api_key = Some(key);
    let publicly = client.get(&format!("/v1/agents/{agent_id}")).await;
    assert_eq!(publicly.status, StatusCode::GONE);
    assert_eq!(publicly.error_code(), "agent_archived");
});

db_test!(async fn an_agent_from_another_tenant_is_reported_missing(db) {
    let mut alice = Client::new(&db);
    let alice_workspace = alice.onboard().await;
    let alice_agent = alice.create_agent(&alice_workspace, "Alice's assistant").await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut bob = Client::new(&db);
    bob.onboard().await;

    // Bob's session, Alice's agent id.
    let reply = bob.get(&format!("/dashboard/v1/agents/{alice_agent}")).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.error_code(), "agent_not_found");
});

db_test!(async fn a_key_scoped_to_one_agent_cannot_see_another(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    // Two agents, so the free plan's limit of one is not what this tests.
    client.set_plan("business").await;

    let allowed = client.create_agent(&workspace_id, "Allowed").await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let hidden = client.create_agent(&workspace_id, "Hidden").await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    for agent_id in [&allowed, &hidden] {
        client
            .post(&format!("/dashboard/v1/agents/{agent_id}/publish"), json!({}))
            .await;
    }

    let issued = client
        .post(
            "/dashboard/v1/api_keys",
            json!({
                "workspace_id": workspace_id,
                "name": "Narrow",
                "scopes": ["agents:read"],
                "all_agents": false,
                "agent_ids": [allowed]
            }),
        )
        .await;
    assert_eq!(issued.status, StatusCode::CREATED, "{:?}", issued.body);
    client.api_key = Some(issued.body["secret"].as_str().unwrap().to_owned());

    assert_eq!(
        client.get(&format!("/v1/agents/{allowed}")).await.status,
        StatusCode::OK
    );

    let refused = client.get(&format!("/v1/agents/{hidden}")).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    assert_eq!(refused.error_code(), "agent_not_found");

    // And the listing hides it too.
    let listed = client.get("/v1/agents").await;
    assert_eq!(listed.body["data"].as_array().unwrap().len(), 1);
});

db_test!(async fn the_free_plan_allows_one_agent(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;

    client.create_agent(&workspace_id, "First").await;

    let refused = client
        .post(
            "/dashboard/v1/agents",
            json!({"workspace_id": workspace_id, "name": "Second"}),
        )
        .await;

    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(refused.error_code(), "agent_limit_reached");
});

db_test!(async fn an_agent_cannot_read_another_tenants_knowledge_base(db) {
    let mut client = Client::new(&db);
    let workspace_id = client.onboard().await;
    let agent_id = client.create_agent(&workspace_id, "Assistant").await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // A knowledge base id that does not belong to this tenant — which is
    // indistinguishable from one that does not exist, and must stay that way.
    let stranger = anthovai_core::KnowledgeBaseId::new();
    let refused = client
        .put(
            &format!("/dashboard/v1/agents/{agent_id}/knowledge_bases"),
            json!({"knowledge_base_ids": [stranger.to_string()]}),
        )
        .await;

    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    assert_eq!(refused.error_code(), "knowledge_base_not_found");
});

// ---- the published contract ------------------------------------------------

db_test!(async fn the_openapi_document_is_served_without_a_key(db) {
    // A customer writing an integration should not need a key to read how to
    // get one. The document describes no particular organization.
    let client = Client::new(&db);
    let reply = client.get("/v1/openapi.json").await;

    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["openapi"].as_str().unwrap_or_default(), "3.1.0");
    assert!(reply.body["paths"]["/v1/chat"]["post"].is_object());

    // Every documented path is one this server actually serves.
    for path in reply.body["paths"].as_object().unwrap().keys() {
        assert!(path.starts_with("/v1/"), "{path} should not be published");
    }
});
