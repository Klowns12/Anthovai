-- Let the system role read the tables it has to sweep across tenants.
--
-- `FORCE ROW LEVEL SECURITY` applies to every role without an exemption, and
-- the policies from `0002_rls.sql` are all written against `app_tenant_id()` —
-- which is never set on a system connection. The result is not an error. It is
-- an empty result set, which is the worst possible outcome: a cross-tenant
-- query that should have found 1,327 rows returns none, reports success, and
-- the work it was supposed to queue simply never happens.
--
-- That is exactly what the re-embedding sweep did on its first run. It is the
-- same shape as the `api_keys_system_lookup` policy already here, and for the
-- same reason: authentication and background sweeps both run before a tenant
-- has been chosen.
--
-- SELECT only. Writing still goes through a tenant-scoped transaction, so a
-- background job can find work anywhere but can only change one tenant's rows
-- at a time.

DROP POLICY IF EXISTS knowledge_bases_system_read ON knowledge_bases;
CREATE POLICY knowledge_bases_system_read ON knowledge_bases
  FOR SELECT
  TO anthovai_system
  USING (true);

-- The re-embedding sweep reads bases; the handler then reads that base's
-- documents inside a tenant transaction. This is here for the same class of
-- job that needs to find work before it knows whose it is — a scheduled
-- integrity check, say, which would otherwise report a clean bill of health on
-- a database it could not see.
DROP POLICY IF EXISTS documents_system_read ON documents;
CREATE POLICY documents_system_read ON documents
  FOR SELECT
  TO anthovai_system
  USING (true);
