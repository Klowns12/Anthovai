-- Row-level security: the second line of tenant isolation.
--
-- Repositories already bind `WHERE tenant_id = $1` from the request's
-- TenantCtx. This layer means that if one of them ever forgets, the database
-- returns no rows instead of another customer's data.
--
-- Every request runs inside a transaction that has done
--   SET LOCAL ROLE anthovai_app;
--   SELECT set_config('app.tenant_id', $1, true);
-- which `Db::tenant()` does for you. The SET ROLE matters as much as the GUC:
-- a superuser or the table owner would otherwise bypass every policy below,
-- which is exactly what happens when a developer connects as the database
-- owner. Outside such a transaction the setting is empty and nothing matches.

-- Roles.
--   anthovai_app    — request handling. Subject to every policy here.
--   anthovai_system — the handful of genuinely cross-tenant operations.
-- Both are NOLOGIN: a deployment creates a login role and grants it membership
--   CREATE ROLE anthovai_api LOGIN PASSWORD '...';
--   GRANT anthovai_app, anthovai_system TO anthovai_api;
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'anthovai_app') THEN
    CREATE ROLE anthovai_app NOLOGIN;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'anthovai_system') THEN
    CREATE ROLE anthovai_system NOLOGIN;
  END IF;
END
$$;

-- Helper: the tenant this transaction is pinned to, or NULL when unset.
CREATE OR REPLACE FUNCTION app_tenant_id() RETURNS TEXT
LANGUAGE sql STABLE AS $$
  SELECT NULLIF(current_setting('app.tenant_id', true), '')
$$;

-- Apply the same policy to every table holding customer data.
DO $$
DECLARE
  target TEXT;
  tables TEXT[] := ARRAY[
    'workspaces', 'agents', 'agent_versions', 'knowledge_bases',
    'agent_knowledge_bases', 'documents', 'document_chunks', 'api_keys',
    'conversations', 'messages', 'usage_records', 'audit_logs'
  ];
BEGIN
  FOREACH target IN ARRAY tables LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', target);
    EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', target);
    EXECUTE format('DROP POLICY IF EXISTS %I ON %I', target || '_tenant_isolation', target);
    EXECUTE format(
      'CREATE POLICY %I ON %I USING (tenant_id = app_tenant_id()) WITH CHECK (tenant_id = app_tenant_id())',
      target || '_tenant_isolation', target
    );
    EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE ON %I TO anthovai_app', target);
  END LOOP;
END
$$;

-- The organization row itself is readable only by its own tenant.
ALTER TABLE organizations ENABLE ROW LEVEL SECURITY;
ALTER TABLE organizations FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS organizations_tenant_isolation ON organizations;
CREATE POLICY organizations_tenant_isolation ON organizations
  USING (id = app_tenant_id())
  WITH CHECK (id = app_tenant_id());
GRANT SELECT, INSERT, UPDATE, DELETE ON organizations TO anthovai_app;

-- Creating an organization is the one moment there is no tenant yet: the row
-- being inserted is what establishes it. That runs as anthovai_system.
DROP POLICY IF EXISTS organizations_system_bootstrap ON organizations;
CREATE POLICY organizations_system_bootstrap ON organizations
  TO anthovai_system
  USING (true)
  WITH CHECK (true);

-- Authenticating an API key means finding it by hash, and the hash is all we
-- have — the tenant is what the lookup returns. Read-only, and only for the
-- system role: everything the key then does runs as anthovai_app under the
-- tenant it resolved to.
DROP POLICY IF EXISTS api_keys_system_lookup ON api_keys;
CREATE POLICY api_keys_system_lookup ON api_keys
  FOR SELECT
  TO anthovai_system
  USING (true);

-- The first workspace is created in the same breath as the organization.
DROP POLICY IF EXISTS workspaces_system_bootstrap ON workspaces;
CREATE POLICY workspaces_system_bootstrap ON workspaces
  TO anthovai_system
  USING (true)
  WITH CHECK (true);

-- Tables with no tenant column, and so no policy to write:
--   users, memberships, sessions  — identity, before an org is chosen
--   jobs                          — the worker takes whatever is queued, then
--                                   scopes itself to the job's tenant
--   usage_counters, subscriptions — billing rollups
--   api_key_agents                — reached only through its parent key
GRANT USAGE ON SCHEMA public TO anthovai_app, anthovai_system;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO anthovai_system;
GRANT SELECT, INSERT, UPDATE, DELETE ON
  users, memberships, sessions, api_key_agents, usage_counters
  TO anthovai_app;
