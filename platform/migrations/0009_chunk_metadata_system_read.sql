-- Let the system role read the chunk metadata it sweeps on.
--
-- `0006` made every system policy require that the session is actually running
-- as `anthovai_system` rather than merely a member of it. `document_chunks`
-- never had a system policy at all, because until now nothing cross-tenant
-- needed to read it.
--
-- The re-embedding sweep now does: it asks which knowledge bases hold chunks
-- built by an older chunker, and that question is answered from
-- `document_chunks.metadata`. Without a policy the subquery returns nothing —
-- not an error, nothing — so the sweep found no stale bases, reported success,
-- and every knowledge base built by an older chunker stayed as it was.
--
-- That is the third time this shape has bitten this schema, and it always
-- looks like success: `knowledge_bases` in 0005, `usage_counters` in 0004, and
-- now this. A `FORCE ROW LEVEL SECURITY` table with no policy for a role does
-- not refuse that role. It returns an empty set.
--
-- SELECT only, and only while actually acting as the role. Writing chunks stays
-- inside a tenant-scoped transaction.
DROP POLICY IF EXISTS document_chunks_system_read ON document_chunks;
CREATE POLICY document_chunks_system_read ON document_chunks
  FOR SELECT
  TO anthovai_system
  USING (acting_as_system());
