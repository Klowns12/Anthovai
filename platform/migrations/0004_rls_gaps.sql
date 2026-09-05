-- Row-level security on the two tables that were missed.
--
-- `0002_rls.sql` lists tables "with no tenant column, and so no policy to
-- write". Two of them turned out to have one: `usage_counters` and
-- `subscriptions` are both keyed by `tenant_id`. Nothing leaked — every query
-- against them filters on the tenant from the connection, never from a caller —
-- but that is the first line of defence doing all the work alone, and the whole
-- design says it should not have to.
--
-- Found by walking the checklist in `docs/spec-v0.1/07-security-multitenancy.md`
-- §12 rather than by a test, which is why `crates/tenant/tests/isolation.rs` now
-- has one that fails when a new table with a `tenant_id` arrives without a
-- policy.

DO $$
DECLARE
  target TEXT;
  tables TEXT[] := ARRAY['usage_counters', 'subscriptions'];
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

-- The quota is checked while a request is still being authenticated, before a
-- tenant context exists to pin the connection to. That read runs as the system
-- role and is read-only; writing a counter still goes through the tenant
-- policy above.
DROP POLICY IF EXISTS usage_counters_system_read ON usage_counters;
CREATE POLICY usage_counters_system_read ON usage_counters
  FOR SELECT
  TO anthovai_system
  USING (true);

-- Billing is read the same way, before an organization has been chosen.
DROP POLICY IF EXISTS subscriptions_system_read ON subscriptions;
CREATE POLICY subscriptions_system_read ON subscriptions
  FOR SELECT
  TO anthovai_system
  USING (true);

-- `jobs` keeps its exemption, and this is the reason: a worker asks for the
-- next job of any tenant, because it does not know whose work is waiting until
-- it has claimed some. It then opens a tenant-scoped transaction for that job's
-- tenant and does the actual work under the policies above. A tenant policy on
-- `jobs` would mean a worker could never find anything to do.
COMMENT ON TABLE jobs IS
  'Deliberately without row-level security: the worker claims work before it '
  'knows whose it is, then scopes itself to the claimed job''s tenant. Reached '
  'only through the system role, never from a request.';

-- `memberships` also carries a `tenant_id`, and it is genuinely cross-tenant:
-- "which organizations does this user belong to?" is the question asked before
-- an organization has been chosen, so a tenant policy would make it
-- unanswerable. Every query against it already runs as the system role.
--
-- So the exemption is made structural rather than left as a convention: the
-- application role loses its grant. A request-path query that reached for this
-- table without a tenant filter now fails outright instead of returning every
-- organization's membership list.
REVOKE ALL ON memberships FROM anthovai_app;

COMMENT ON TABLE memberships IS
  'Deliberately without row-level security: it answers which organizations a '
  'user belongs to, which is a question asked before one is chosen. Reachable '
  'only by the system role — the application role has no grant.';
