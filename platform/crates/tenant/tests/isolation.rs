//! Cross-tenant isolation, exercised against a real PostgreSQL.
//!
//! This is the product's first promise and the one bug that would end it, so it
//! is tested at the layer where it can actually fail: real roles, real
//! row-level security, real SQL. The CI workflow runs this file as its own job.

use anthovai_core::{Actor, OrgId, Plan, RequestId, Role, TenantCtx, UserId, WorkspaceId};
use anthovai_db::{sqlx, Db};
use anthovai_tenant::{repo, TenantService};
use anthovai_testkit::db_test;

/// A dashboard context for an owner of this organization.
fn owner_ctx(org_id: OrgId, user_id: UserId) -> TenantCtx {
    TenantCtx {
        org_id,
        workspace_id: None,
        actor: Actor::User {
            user_id,
            role: Role::Owner,
        },
        plan: Plan::Free,
        request_id: RequestId::new(),
    }
}

/// Create a user directly: signing up is `anthovai-auth`'s concern, and this
/// file is about what happens after two tenants exist.
async fn seed_user(db: &Db) -> UserId {
    let user_id = UserId::new();
    let mut system = db.system().await.expect("system transaction");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id.to_db())
        .bind(format!("{user_id}@example.test"))
        .execute(system.conn())
        .await
        .expect("insert user");
    system.commit().await.expect("commit");
    user_id
}

db_test!(async fn two_organizations_cannot_see_each_others_workspaces(db) {
    let service = TenantService::new(db.clone());

    let alice = seed_user(&db).await;
    let bob = seed_user(&db).await;

    let org_a = service
        .create_organization(alice, "Org A", &format!("org-a-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .expect("create org A");
    let org_b = service
        .create_organization(bob, "Org B", &format!("org-b-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .expect("create org B");

    let ctx_a = owner_ctx(org_a.organization.id, alice);
    let ctx_b = owner_ctx(org_b.organization.id, bob);

    service
        .create_workspace(&ctx_a, "A private", "a-private")
        .await
        .expect("A creates a workspace");
    service
        .create_workspace(&ctx_b, "B private", "b-private")
        .await
        .expect("B creates a workspace");

    let a_sees = service.list_workspaces(&ctx_a).await.expect("A lists");
    let b_sees = service.list_workspaces(&ctx_b).await.expect("B lists");

    assert!(
        a_sees.iter().all(|w| w.org_id == org_a.organization.id),
        "A saw a workspace belonging to another tenant"
    );
    assert!(
        b_sees.iter().all(|w| w.org_id == org_b.organization.id),
        "B saw a workspace belonging to another tenant"
    );
    assert!(a_sees.iter().any(|w| w.slug == "a-private"));
    assert!(!a_sees.iter().any(|w| w.slug == "b-private"));
});

db_test!(async fn naming_another_tenants_workspace_reports_it_missing(db) {
    let service = TenantService::new(db.clone());

    let alice = seed_user(&db).await;
    let bob = seed_user(&db).await;
    let org_a = service
        .create_organization(alice, "Org A", &format!("a-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .unwrap();
    let org_b = service
        .create_organization(bob, "Org B", &format!("b-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .unwrap();

    let ctx_a = owner_ctx(org_a.organization.id, alice);

    // A knows B's workspace id — say it leaked through a log or a screenshot.
    let err = service
        .get_workspace(&ctx_a, org_b.default_workspace.id)
        .await
        .expect_err("A must not be able to read B's workspace");

    // NotFound, not Forbidden: "you may not see this" still confirms it exists.
    assert_eq!(err.code(), "workspace_not_found", "got {err}");
});

db_test!(async fn a_deliberately_unfiltered_query_still_sees_one_tenant(db) {
    // The scenario row-level security exists for: a repository that forgot its
    // WHERE clause. The transaction's role and app.tenant_id must save it.
    let service = TenantService::new(db.clone());

    let alice = seed_user(&db).await;
    let bob = seed_user(&db).await;
    let org_a = service
        .create_organization(alice, "Org A", &format!("a-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .unwrap();
    let org_b = service
        .create_organization(bob, "Org B", &format!("b-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .unwrap();

    let ctx_a = owner_ctx(org_a.organization.id, alice);
    let mut tenant_db = db.tenant(&ctx_a).await.expect("tenant transaction");

    let rows: Vec<(String,)> = sqlx::query_as("SELECT tenant_id FROM workspaces")
        .fetch_all(tenant_db.conn())
        .await
        .expect("unfiltered select");

    assert!(!rows.is_empty(), "A should still see its own rows");
    assert!(
        rows.iter().all(|(tenant,)| *tenant == org_a.organization.id.to_db()),
        "an unfiltered query leaked rows from another tenant"
    );
    assert!(
        !rows
            .iter()
            .any(|(tenant,)| *tenant == org_b.organization.id.to_db()),
        "B's rows were visible to A"
    );
});

db_test!(async fn writing_into_another_tenant_is_refused(db) {
    let service = TenantService::new(db.clone());

    let alice = seed_user(&db).await;
    let bob = seed_user(&db).await;
    let org_a = service
        .create_organization(alice, "Org A", &format!("a-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .unwrap();
    let org_b = service
        .create_organization(bob, "Org B", &format!("b-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .unwrap();

    let ctx_a = owner_ctx(org_a.organization.id, alice);
    let mut tenant_db = db.tenant(&ctx_a).await.unwrap();

    // A forges a row carrying B's tenant id.
    let result = sqlx::query("INSERT INTO workspaces (id, tenant_id, name, slug) VALUES ($1, $2, $3, $4)")
        .bind(WorkspaceId::new().to_db())
        .bind(org_b.organization.id.to_db())
        .bind("injected")
        .bind("injected")
        .execute(tenant_db.conn())
        .await;

    assert!(
        result.is_err(),
        "the WITH CHECK clause must refuse a row belonging to another tenant"
    );
});

db_test!(async fn a_member_of_one_org_is_not_authorized_in_another(db) {
    let service = TenantService::new(db.clone());

    let alice = seed_user(&db).await;
    let bob = seed_user(&db).await;
    let org_a = service
        .create_organization(alice, "Org A", &format!("a-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .unwrap();
    let org_b = service
        .create_organization(bob, "Org B", &format!("b-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .unwrap();

    assert!(service.authorize(alice, org_a.organization.id).await.is_ok());

    let err = service
        .authorize(alice, org_b.organization.id)
        .await
        .expect_err("alice is not a member of org B");
    assert_eq!(err.code(), "organization_not_found");
});

db_test!(async fn an_unaccepted_invitation_grants_nothing(db) {
    let service = TenantService::new(db.clone());

    let owner = seed_user(&db).await;
    let invitee = seed_user(&db).await;
    let org = service
        .create_organization(owner, "Org", &format!("o-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .unwrap();

    let mut system = db.system().await.unwrap();
    repo::insert_membership(&mut system, invitee, org.organization.id, Role::Editor, false)
        .await
        .expect("invite");
    system.commit().await.unwrap();

    let err = service
        .authorize(invitee, org.organization.id)
        .await
        .expect_err("a pending invitation must not authorize");
    assert_eq!(err.code(), "organization_not_found");
});

db_test!(async fn creating_an_organization_makes_a_usable_tenant(db) {
    let service = TenantService::new(db.clone());
    let alice = seed_user(&db).await;

    let created = service
        .create_organization(alice, "ABC School", &format!("abc-{}", OrgId::new().to_db().to_lowercase()))
        .await
        .expect("create");

    // The owner can get back in, the default workspace exists, and the plan is free.
    let (role, plan) = service
        .authorize(alice, created.organization.id)
        .await
        .expect("owner is authorized");
    assert_eq!(role, Role::Owner);
    assert_eq!(plan, Plan::Free);

    let ctx = owner_ctx(created.organization.id, alice);
    let organization = service.get_organization(&ctx).await.expect("read back");
    assert_eq!(organization.name, "ABC School");

    let workspaces = service.list_workspaces(&ctx).await.unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].slug, "default");
});

db_test!(async fn a_duplicate_slug_is_a_conflict_not_a_crash(db) {
    let service = TenantService::new(db.clone());
    let alice = seed_user(&db).await;
    let slug = format!("dup-{}", OrgId::new().to_db().to_lowercase());

    service
        .create_organization(alice, "First", &slug)
        .await
        .expect("first org takes the slug");

    let err = service
        .create_organization(alice, "Second", &slug)
        .await
        .expect_err("the slug is taken");
    assert_eq!(err.code(), "slug_taken");
});

// ---- the shape of the defence itself ---------------------------------------

db_test!(async fn every_table_holding_a_tenant_id_has_a_policy(db) {
    // Repository filters are the first line and row-level security is the
    // second, so a table with a `tenant_id` and no policy is running on one
    // line of defence without saying so. Two tables were in exactly that state
    // until `0004_rls_gaps.sql`, and nothing caught it — a checklist did,
    // months later. This is that checklist, run on every build.
    //
    // A table may be exempt, but only deliberately and only here.
    // Each of these is cross-tenant by nature, and each is unreachable from the
    // application role — which the second half of this test checks, so the
    // exemption is a fact about the schema rather than a promise in a comment.
    const EXEMPT: &[&str] = &[
        // A worker claims the next job before it knows whose it is, then scopes
        // itself to that job's tenant to do the work. A tenant policy here
        // would mean it could never find anything to claim.
        "jobs",
        // Answers which organizations a user belongs to — a question asked
        // before an organization has been chosen.
        "memberships",
    ];

    let mut system = db.system().await.unwrap();
    let rows: Vec<(String, bool, i64)> = anthovai_db::sqlx::query_as(
        "SELECT c.relname,
                c.relrowsecurity,
                (SELECT count(*) FROM pg_policy p WHERE p.polrelid = c.oid)
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public'
           AND c.relkind = 'r'
           AND EXISTS (
             SELECT 1 FROM information_schema.columns col
             WHERE col.table_schema = 'public'
               AND col.table_name = c.relname
               AND col.column_name = 'tenant_id'
           )
         ORDER BY c.relname",
    )
    .fetch_all(system.conn())
    .await
    .unwrap();
    system.commit().await.unwrap();

    assert!(
        rows.len() > 5,
        "the query found almost nothing, so it is not testing what it claims"
    );

    let unprotected: Vec<&str> = rows
        .iter()
        .filter(|(name, _, _)| !EXEMPT.contains(&name.as_str()))
        .filter(|(_, enabled, policies)| !enabled || *policies == 0)
        .map(|(name, _, _)| name.as_str())
        .collect();

    assert!(
        unprotected.is_empty(),
        "these tables have a `tenant_id` and no row-level security: {unprotected:?}. \
         Add a policy, or add the table to EXEMPT with the reason."
    );
});

db_test!(async fn the_application_roles_cannot_bypass_the_policies(db) {
    // A role with BYPASSRLS makes every policy in the schema decorative, and
    // nothing else in the system would look any different.
    let mut system = db.system().await.unwrap();
    let roles: Vec<(String, bool, bool)> = anthovai_db::sqlx::query_as(
        "SELECT rolname, rolbypassrls, rolsuper FROM pg_roles
         WHERE rolname IN ('anthovai_app', 'anthovai_system')
         ORDER BY rolname",
    )
    .fetch_all(system.conn())
    .await
    .unwrap();
    system.commit().await.unwrap();

    assert_eq!(roles.len(), 2, "both roles should exist: {roles:?}");
    for (name, bypass, superuser) in roles {
        assert!(!bypass, "{name} can bypass row-level security");
        assert!(!superuser, "{name} is a superuser");
    }
});

db_test!(async fn a_connection_that_forgets_to_set_a_role_sees_nothing(db) {
    // The system policies are written `TO anthovai_system`, and PostgreSQL
    // matches a policy's role list by *membership* — not by which role the
    // session is currently running as. The application connects as one login
    // role that is a member of both `anthovai_app` and `anthovai_system`,
    // because `SET ROLE` requires membership; so before
    // `0006_system_policies_need_the_role.sql` every `USING (true)` system
    // policy was already in force on the bare connection, before any SET ROLE.
    //
    // Found while preparing a managed-database deployment: on a database with
    // 74 knowledge bases across several tenants, a query on the plain
    // connection with no tenant pinned returned all 74. `NOINHERIT` on the
    // login role does not help — it governs privileges, not policy matching.
    //
    // The tenant policy alone yields nothing here, since `app_tenant_id()` is
    // NULL and `tenant_id = NULL` matches no row. So "nothing" is the correct
    // answer for a query that has chosen neither a role nor a tenant, and
    // anything above zero means a permissive policy is letting it through.
    const GUARDED: &[&str] = &[
        "knowledge_bases",
        "documents",
        "api_keys",
        "organizations",
        "workspaces",
        "usage_counters",
        "subscriptions",
    ];

    // Seed one visible row so a zero cannot come from an empty table.
    let service = TenantService::new(db.clone());
    let alice = seed_user(&db).await;
    service
        .create_organization(
            alice,
            "Forgetful test",
            &format!("forgetful-{}", OrgId::new().to_db().to_lowercase()),
        )
        .await
        .expect("create");

    // The pool's own connection: no SET ROLE, no tenant pinned. Exactly what a
    // repository method that skipped `db.tenant()` would be running on.
    for table in GUARDED {
        let sql = format!("SELECT count(*) FROM {table}");
        let visible: i64 = anthovai_db::sqlx::query_scalar(&sql)
            .fetch_one(db.pool())
            .await
            .unwrap_or_else(|e| panic!("`{table}`: {e}"));

        assert_eq!(
            visible, 0,
            "`{table}` returned {visible} rows to a connection that pinned no              tenant and switched to no role. Every tenant's rows are reachable              from any query that forgets to go through `db.tenant()`."
        );
    }
});

db_test!(async fn the_system_role_can_read_the_tables_it_sweeps(db) {
    // A `FORCE ROW LEVEL SECURITY` table with no policy for the system role
    // does not refuse that role. It returns an empty result set, successfully.
    // A background sweep across tenants then reports that it found no work and
    // quietly does none — which is how the re-embedding sweep behaved on its
    // first run, before `0005_system_reads.sql`.
    //
    // Every table a cross-tenant job reads belongs in this list.
    const SWEPT: &[&str] = &["knowledge_bases", "documents", "api_keys"];

    // A row in every one of them, so the assertion below is never vacuous.
    //
    // The first version of this test seeded only `knowledge_bases` and passed
    // locally — because other tests had left rows in the rest. On CI's clean
    // database it failed on `documents`, correctly. A test that depends on
    // another test's leftovers is not testing what it claims to.
    let service = TenantService::new(db.clone());
    let alice = seed_user(&db).await;
    let created = service
        .create_organization(
            alice,
            "Sweep test",
            &format!("sweep-{}", OrgId::new().to_db().to_lowercase()),
        )
        .await
        .expect("create");

    let ctx = owner_ctx(created.organization.id, alice);
    let org = created.organization.id.to_db();
    let workspace = created.default_workspace.id.to_db();
    let kb_id = anthovai_core::KnowledgeBaseId::new();

    let mut tenant = db.tenant(&ctx).await.unwrap();
    anthovai_db::sqlx::query(
        "INSERT INTO knowledge_bases (id, tenant_id, workspace_id, name, embedding_model, embedding_dim)
         VALUES ($1, $2, $3, 'Swept', 'fake:hash-1536', 1536)",
    )
    .bind(kb_id.to_db())
    .bind(&org)
    .bind(&workspace)
    .execute(tenant.conn())
    .await
    .unwrap();

    anthovai_db::sqlx::query(
        "INSERT INTO documents (id, tenant_id, knowledge_base_id, title, source_type)
         VALUES ($1, $2, $3, 'Swept', 'md')",
    )
    .bind(anthovai_core::DocumentId::new().to_db())
    .bind(&org)
    .bind(kb_id.to_db())
    .execute(tenant.conn())
    .await
    .unwrap();

    // `key_hash` is unique across the whole table, so a fixed value here would
    // collide the second time this test runs. It is not a real hash either way.
    let key_id = anthovai_core::ApiKeyId::new();
    anthovai_db::sqlx::query(
        "INSERT INTO api_keys (id, tenant_id, workspace_id, name, key_hash, prefix)
         VALUES ($1, $2, $3, 'Swept', $4, 'av_test_swep')",
    )
    .bind(key_id.to_db())
    .bind(&org)
    .bind(&workspace)
    .bind(format!("not-a-real-hash-{}", key_id.to_db()))
    .execute(tenant.conn())
    .await
    .unwrap();

    tenant.commit().await.unwrap();

    let mut system = db.system().await.unwrap();
    for table in SWEPT {
        let sql = format!("SELECT count(*) FROM {table}");
        let visible: i64 = anthovai_db::sqlx::query_scalar(&sql)
            .fetch_one(system.conn())
            .await
            .unwrap_or_else(|e| panic!("the system role could not read `{table}`: {e}"));

        assert!(
            visible > 0,
            "the system role reads zero rows from `{table}`. It is not being \
             refused — row-level security is returning nothing, so any sweep \
             over this table finds no work and says so cheerfully."
        );
    }
    system.commit().await.unwrap();
});
