//! Sign-up, sign-in, sessions and the API key lifecycle, against a real
//! PostgreSQL.
//!
//! The interesting failures here are all at the boundary: a revoked key that
//! keeps working because of a cache, an expired key the status column still
//! calls active, a key from one tenant resolving into another. None of those
//! show up without the database.

use anthovai_auth::{
    password::PasswordHasherConfig, AuthConfig, AuthService, CreateApiKey, Environment,
};
use anthovai_core::{
    Actor, AgentId, AgentScope, ApiKeyId, Clock, OrgId, Plan, RequestId, Role, Scope, TenantCtx,
    UserId,
};
use anthovai_db::Db;
use anthovai_tenant::TenantService;
use anthovai_testkit::db_test;
use chrono::{Duration, Utc};

fn config() -> AuthConfig {
    AuthConfig {
        // Production-cost hashing would make this suite slow for no extra
        // confidence: the parameters are covered by the unit tests.
        password: PasswordHasherConfig::fast_for_tests(),
        ..AuthConfig::default()
    }
}

fn unique_email() -> String {
    format!("owner-{}@abc.ac.th", UserId::new().to_db().to_lowercase())
}

fn unique_slug(prefix: &str) -> String {
    format!("{prefix}-{}", OrgId::new().to_db().to_lowercase())
}

fn owner_ctx(org_id: OrgId, user_id: UserId, plan: Plan) -> TenantCtx {
    TenantCtx {
        org_id,
        workspace_id: None,
        actor: Actor::User {
            user_id,
            role: Role::Owner,
        },
        plan,
        request_id: RequestId::new(),
    }
}

/// A signed-up user with an organization, ready to mint keys.
async fn tenant_with_owner(db: &Db, auth: &AuthService) -> (TenantCtx, anthovai_core::WorkspaceId) {
    let email = unique_email();
    let user_id = auth
        .sign_up(&email, "correct horse battery", Some("Owner"))
        .await
        .expect("sign up");

    let tenants = TenantService::new(db.clone());
    let created = tenants
        .create_organization(user_id, "ABC School", &unique_slug("abc"))
        .await
        .expect("create organization");

    (
        owner_ctx(created.organization.id, user_id, Plan::Free),
        created.default_workspace.id,
    )
}

/// Insert a minimal agent row. Agent management is Milestone 3's job; this is
/// only enough for an API key to be scoped to something real.
async fn seed_agent(
    db: &Db,
    ctx: &TenantCtx,
    workspace_id: anthovai_core::WorkspaceId,
    name: &str,
) -> AgentId {
    let agent_id = AgentId::new();
    let mut tenant_db = db.tenant(ctx).await.expect("tenant transaction");
    anthovai_db::sqlx::query(
        "INSERT INTO agents (id, tenant_id, workspace_id, name, status)
         VALUES ($1, $2, $3, $4, 'active')",
    )
    .bind(agent_id.to_db())
    .bind(ctx.org_id.to_db())
    .bind(workspace_id.to_db())
    .bind(name)
    .execute(tenant_db.conn())
    .await
    .expect("insert agent");
    tenant_db.commit().await.expect("commit");
    agent_id
}

fn key_request(workspace_id: anthovai_core::WorkspaceId) -> CreateApiKey {
    CreateApiKey {
        workspace_id,
        name: "Production website".into(),
        environment: Environment::Live,
        scopes: vec![Scope::Chat],
        agents: AgentScope::All,
        expires_in_days: None,
    }
}

// ---- sign-up and sign-in --------------------------------------------------

db_test!(async fn a_signed_up_user_can_sign_in(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let email = unique_email();

    auth.sign_up(&email, "correct horse battery", Some("Owner"))
        .await
        .expect("sign up");

    let session = auth
        .sign_in(&email, "correct horse battery", None, None)
        .await
        .expect("sign in");

    let user = auth.verify_session(&session.token).await.expect("verify");
    assert_eq!(user.email, email);
});

db_test!(async fn a_wrong_password_is_indistinguishable_from_a_missing_account(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let email = unique_email();
    auth.sign_up(&email, "correct horse battery", None)
        .await
        .unwrap();

    let wrong_password = auth
        .sign_in(&email, "wrong password entirely", None, None)
        .await
        .expect_err("wrong password");
    let no_account = auth
        .sign_in(&unique_email(), "correct horse battery", None, None)
        .await
        .expect_err("no such account");

    // Same code both ways: otherwise the endpoint tells an attacker which
    // addresses are registered.
    assert_eq!(wrong_password.code(), "invalid_credentials");
    assert_eq!(no_account.code(), wrong_password.code());
});

db_test!(async fn the_same_address_cannot_sign_up_twice(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let email = unique_email();
    auth.sign_up(&email, "correct horse battery", None)
        .await
        .unwrap();

    let err = auth
        .sign_up(&email.to_uppercase(), "another password here", None)
        .await
        .expect_err("addresses are unique regardless of case");
    assert_eq!(err.code(), "email_taken");
});

db_test!(async fn signing_out_ends_the_session(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let email = unique_email();
    auth.sign_up(&email, "correct horse battery", None)
        .await
        .unwrap();
    let session = auth
        .sign_in(&email, "correct horse battery", None, None)
        .await
        .unwrap();

    auth.sign_out(&session.token).await.expect("sign out");

    let err = auth
        .verify_session(&session.token)
        .await
        .expect_err("the session is gone");
    assert_eq!(err.code(), "session_expired");
});

db_test!(async fn an_expired_session_is_rejected_and_cleaned_up(db) {
    let (clock, hands) = Clock::fixed(Utc::now());
    let auth = AuthService::new(
        db.clone(),
        clock,
        AuthConfig {
            session_ttl_hours: 1,
            ..config()
        },
    );
    let email = unique_email();
    auth.sign_up(&email, "correct horse battery", None)
        .await
        .unwrap();
    let session = auth
        .sign_in(&email, "correct horse battery", None, None)
        .await
        .unwrap();

    hands.advance(Duration::hours(2));

    let err = auth
        .verify_session(&session.token)
        .await
        .expect_err("the session has expired");
    assert_eq!(err.code(), "session_expired");
});

db_test!(async fn a_fabricated_session_token_is_rejected(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let err = auth
        .verify_session("deadbeef".repeat(8).as_str())
        .await
        .expect_err("no such session");
    assert_eq!(err.code(), "session_expired");
});

// ---- API keys -------------------------------------------------------------

db_test!(async fn a_new_key_authenticates_into_its_own_tenant(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx, workspace_id) = tenant_with_owner(&db, &auth).await;

    let issued = auth
        .create_api_key(&ctx, key_request(workspace_id))
        .await
        .expect("create key");
    assert!(issued.secret.starts_with("av_live_"));

    let authenticated = auth
        .authenticate_api_key(&format!("Bearer {}", issued.secret), RequestId::new())
        .await
        .expect("authenticate");

    assert_eq!(authenticated.org_id, ctx.org_id);
    assert_eq!(authenticated.workspace_id, Some(workspace_id));
    assert!(authenticated.require_scope(Scope::Chat).is_ok());
    assert!(authenticated.require_scope(Scope::KnowledgeWrite).is_err());
});

db_test!(async fn the_secret_is_never_stored_and_never_listed(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx, workspace_id) = tenant_with_owner(&db, &auth).await;

    let issued = auth
        .create_api_key(&ctx, key_request(workspace_id))
        .await
        .unwrap();

    let listed = auth.list_api_keys(&ctx, None).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].prefix, issued.prefix);
    assert!(issued.secret.starts_with(&listed[0].prefix));

    // The prefix is enough to recognise a key in the dashboard and far too
    // little to use one.
    assert!(listed[0].prefix.len() < issued.secret.len());
});

db_test!(async fn a_revoked_key_stops_working_immediately(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx, workspace_id) = tenant_with_owner(&db, &auth).await;

    let issued = auth
        .create_api_key(&ctx, key_request(workspace_id))
        .await
        .unwrap();
    let bearer = format!("Bearer {}", issued.secret);

    // Authenticate once so the key is sitting in the cache.
    auth.authenticate_api_key(&bearer, RequestId::new())
        .await
        .expect("works before revocation");

    auth.revoke_api_key(&ctx, issued.id).await.expect("revoke");

    let err = auth
        .authenticate_api_key(&bearer, RequestId::new())
        .await
        .expect_err("a revoked key must not authenticate, cached or not");
    assert_eq!(err.code(), "revoked_api_key");
});

db_test!(async fn an_expired_key_stops_working_even_while_cached(db) {
    let (clock, hands) = Clock::fixed(Utc::now());
    let auth = AuthService::new(db.clone(), clock, config());
    let (ctx, workspace_id) = tenant_with_owner(&db, &auth).await;

    let issued = auth
        .create_api_key(
            &ctx,
            CreateApiKey {
                expires_in_days: Some(1),
                ..key_request(workspace_id)
            },
        )
        .await
        .unwrap();
    let bearer = format!("Bearer {}", issued.secret);

    auth.authenticate_api_key(&bearer, RequestId::new())
        .await
        .expect("valid today");

    // Past the deadline but well inside the cache TTL: expiry has to be checked
    // on every request, not just on a cache miss.
    hands.advance(Duration::days(2));

    let err = auth
        .authenticate_api_key(&bearer, RequestId::new())
        .await
        .expect_err("the key is past its deadline");
    assert_eq!(err.code(), "expired_api_key");
});

db_test!(async fn rotation_leaves_the_old_key_working_for_a_grace_period(db) {
    let (clock, hands) = Clock::fixed(Utc::now());
    let auth = AuthService::new(
        db.clone(),
        clock,
        AuthConfig {
            rotation_grace_hours: 24,
            ..config()
        },
    );
    let (ctx, workspace_id) = tenant_with_owner(&db, &auth).await;

    let old = auth
        .create_api_key(&ctx, key_request(workspace_id))
        .await
        .unwrap();
    let new = auth
        .rotate_api_key(&ctx, old.id, key_request(workspace_id))
        .await
        .expect("rotate");

    assert_ne!(old.secret, new.secret);

    let old_bearer = format!("Bearer {}", old.secret);
    let new_bearer = format!("Bearer {}", new.secret);

    // Both work during the grace period, so a running deployment can be updated.
    auth.authenticate_api_key(&old_bearer, RequestId::new())
        .await
        .expect("old key still works during the grace period");
    auth.authenticate_api_key(&new_bearer, RequestId::new())
        .await
        .expect("new key works");

    hands.advance(Duration::hours(25));

    assert_eq!(
        auth.authenticate_api_key(&old_bearer, RequestId::new())
            .await
            .expect_err("the grace period is over")
            .code(),
        "expired_api_key"
    );
    auth.authenticate_api_key(&new_bearer, RequestId::new())
        .await
        .expect("the new key is unaffected");
});

db_test!(async fn a_scoped_key_can_only_address_its_own_agents(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx, workspace_id) = tenant_with_owner(&db, &auth).await;

    let allowed = seed_agent(&db, &ctx, workspace_id, "Allowed").await;
    let other = AgentId::new();

    let issued = auth
        .create_api_key(
            &ctx,
            CreateApiKey {
                agents: AgentScope::Only(vec![allowed]),
                ..key_request(workspace_id)
            },
        )
        .await
        .unwrap();

    let authenticated = auth
        .authenticate_api_key(&format!("Bearer {}", issued.secret), RequestId::new())
        .await
        .unwrap();

    assert!(authenticated.require_agent(allowed).is_ok());
    // NotFound, not Forbidden: a key must not be able to probe for agent ids.
    assert_eq!(
        authenticated
            .require_agent(other)
            .expect_err("out of scope")
            .code(),
        "agent_not_found"
    );
});

db_test!(async fn a_key_from_one_tenant_never_resolves_into_another(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx_a, workspace_a) = tenant_with_owner(&db, &auth).await;
    let (ctx_b, _workspace_b) = tenant_with_owner(&db, &auth).await;

    let key_a = auth
        .create_api_key(&ctx_a, key_request(workspace_a))
        .await
        .unwrap();

    let resolved = auth
        .authenticate_api_key(&format!("Bearer {}", key_a.secret), RequestId::new())
        .await
        .unwrap();

    assert_eq!(resolved.org_id, ctx_a.org_id);
    assert_ne!(resolved.org_id, ctx_b.org_id);
});

db_test!(async fn one_tenant_cannot_revoke_anothers_key(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx_a, workspace_a) = tenant_with_owner(&db, &auth).await;
    let (ctx_b, _) = tenant_with_owner(&db, &auth).await;

    let key_a = auth
        .create_api_key(&ctx_a, key_request(workspace_a))
        .await
        .unwrap();

    let err = auth
        .revoke_api_key(&ctx_b, key_a.id)
        .await
        .expect_err("B must not touch A's key");
    assert_eq!(err.code(), "api_key_not_found");

    // And A's key still works.
    auth.authenticate_api_key(&format!("Bearer {}", key_a.secret), RequestId::new())
        .await
        .expect("unaffected");
});

db_test!(async fn one_tenant_cannot_list_anothers_keys(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx_a, workspace_a) = tenant_with_owner(&db, &auth).await;
    let (ctx_b, workspace_b) = tenant_with_owner(&db, &auth).await;

    auth.create_api_key(&ctx_a, key_request(workspace_a))
        .await
        .unwrap();
    auth.create_api_key(&ctx_b, key_request(workspace_b))
        .await
        .unwrap();

    let a_sees = auth.list_api_keys(&ctx_a, None).await.unwrap();
    let b_sees = auth.list_api_keys(&ctx_b, None).await.unwrap();

    assert_eq!(a_sees.len(), 1);
    assert_eq!(b_sees.len(), 1);
    assert_ne!(a_sees[0].id, b_sees[0].id);
});

db_test!(async fn an_unknown_key_is_rejected_without_saying_why(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let stranger = anthovai_auth::generate(Environment::Live);

    let err = auth
        .authenticate_api_key(&format!("Bearer {}", stranger.plaintext), RequestId::new())
        .await
        .expect_err("this key was never issued");
    assert_eq!(err.code(), "invalid_api_key");
});

db_test!(async fn a_malformed_key_never_reaches_the_database(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());

    for bad in [
        "Bearer sk-someone-elses-key",
        "Bearer av_live_tooshort",
        "Basic dXNlcjpwYXNz",
        "Bearer ",
    ] {
        let err = auth
            .authenticate_api_key(bad, RequestId::new())
            .await
            .expect_err("should be rejected on shape alone");
        assert!(
            err.code() == "invalid_api_key" || err.code() == "missing_bearer_token",
            "unexpected code {} for {bad:?}",
            err.code()
        );
    }
});

db_test!(async fn an_api_key_cannot_manage_api_keys(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx, workspace_id) = tenant_with_owner(&db, &auth).await;

    let issued = auth
        .create_api_key(&ctx, key_request(workspace_id))
        .await
        .unwrap();
    let key_ctx = auth
        .authenticate_api_key(&format!("Bearer {}", issued.secret), RequestId::new())
        .await
        .unwrap();

    // A leaked key must not be able to mint more keys for itself.
    let err = auth
        .create_api_key(&key_ctx, key_request(workspace_id))
        .await
        .expect_err("keys cannot beget keys");
    assert_eq!(err.code(), "api_key_cannot_perform_action");

    assert!(auth.revoke_api_key(&key_ctx, ApiKeyId::new()).await.is_err());
});

db_test!(async fn a_key_cannot_be_scoped_to_another_tenants_agent(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx_a, workspace_a) = tenant_with_owner(&db, &auth).await;
    let (ctx_b, workspace_b) = tenant_with_owner(&db, &auth).await;

    let b_agent = seed_agent(&db, &ctx_b, workspace_b, "B's agent").await;

    // A tries to mint a key pointing at an agent it does not own. Row-level
    // security hides the row, so the foreign key has nothing to match.
    let err = auth
        .create_api_key(
            &ctx_a,
            CreateApiKey {
                agents: AgentScope::Only(vec![b_agent]),
                ..key_request(workspace_a)
            },
        )
        .await
        .expect_err("A must not scope a key to B's agent");

    assert_eq!(err.code(), "agent_not_found");

    // And nothing was left behind by the failed attempt.
    assert!(auth.list_api_keys(&ctx_a, None).await.unwrap().is_empty());
});

db_test!(async fn a_key_with_no_scopes_is_refused_at_creation(db) {
    let auth = AuthService::new(db.clone(), Clock::system(), config());
    let (ctx, workspace_id) = tenant_with_owner(&db, &auth).await;

    let err = auth
        .create_api_key(
            &ctx,
            CreateApiKey {
                scopes: vec![],
                ..key_request(workspace_id)
            },
        )
        .await
        .expect_err("a key that can do nothing is a mistake, not a feature");
    assert_eq!(err.code(), "invalid_request");
});
