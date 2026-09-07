-- A policy `TO anthovai_system` applies to anything that is a *member* of that
-- role, not only to a session that has switched into it.
--
-- That distinction is the whole security of the deployment. The application
-- connects as one login role which is a member of both `anthovai_app` and
-- `anthovai_system`, because `SET ROLE` requires membership; `Db::tenant()`
-- switches per transaction. But PostgreSQL matches a policy's role list by
-- membership, so every one of these `USING (true)` policies was already in
-- force on the plain connection — before any `SET ROLE`, and regardless of
-- `NOINHERIT`, which governs privileges and not policy matching.
--
-- Measured on a database with 74 knowledge bases across several tenants: a
-- query on the bare connection, with no tenant pinned, returned all 74. The
-- tenant policy alone would have returned none, because `app_tenant_id()` is
-- NULL and `tenant_id = NULL` matches nothing — these permissive policies are
-- OR'd with it and let everything through.
--
-- So the predicate now asks whether the session is *actually running as* the
-- system role. `SET ROLE` changes `current_user`; mere membership does not.
-- Every intended caller already does the SET ROLE, so nothing legitimate
-- changes — and a query that forgets it now sees one tenant's rows or none,
-- rather than the whole table.

CREATE OR REPLACE FUNCTION acting_as_system() RETURNS BOOLEAN
LANGUAGE sql STABLE AS $$
  SELECT current_user = 'anthovai_system'
$$;

COMMENT ON FUNCTION acting_as_system() IS
  'True only inside a transaction that has done SET LOCAL ROLE anthovai_system. '
  'Membership in the role is not enough, which is the point.';

DO $$
DECLARE
  policy RECORD;
BEGIN
  FOR policy IN
    SELECT schemaname, tablename, policyname, cmd
    FROM pg_policies
    WHERE schemaname = 'public'
      AND roles = '{anthovai_system}'
      AND qual = 'true'
  LOOP
    EXECUTE format(
      'DROP POLICY %I ON %I.%I',
      policy.policyname, policy.schemaname, policy.tablename
    );
    EXECUTE format(
      'CREATE POLICY %I ON %I.%I FOR %s TO anthovai_system USING (acting_as_system())',
      policy.policyname, policy.schemaname, policy.tablename,
      CASE policy.cmd WHEN 'ALL' THEN 'ALL' ELSE policy.cmd END
    );
  END LOOP;
END
$$;
