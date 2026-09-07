-- Run once, as an admin (doadmin), against the anthovai database.
--
--   psql "$ADMIN_URL" -f bootstrap.sql
--
-- Creates the extensions and the login role the application uses. Migrations
-- create the two NOLOGIN roles themselves and are idempotent, so running them
-- after this is safe and does the rest.

-- pgvector, plus the two the schema needs. doadmin may install these.
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS citext;

-- The role the application connects as.
--
-- The whole point is that it is NOT a superuser: PostgreSQL exempts superusers
-- from row-level security entirely, so an application connected as doadmin
-- would read across every tenant and no policy, test or log would say so.
--
-- Change the password before running this, and use the same one in
-- ANTHOVAI__DATABASE__URL.
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'anthovai_api') THEN
    CREATE ROLE anthovai_api LOGIN PASSWORD 'CHANGE_ME_BEFORE_RUNNING';
  END IF;
END
$$;

GRANT CONNECT ON DATABASE anthovai TO anthovai_api;

-- Membership in the two roles the policies are written against. The
-- application switches between them per transaction with SET LOCAL ROLE.
GRANT anthovai_app, anthovai_system TO anthovai_api;

-- Verification. Every line below should report what the comment says it should;
-- if any of them does not, stop and fix it rather than deploying.
\echo ''
\echo '-- anthovai_api must NOT be superuser and must NOT bypass RLS:'
SELECT rolname, rolsuper, rolbypassrls, rolcanlogin
FROM pg_roles WHERE rolname IN ('anthovai_api', 'anthovai_app', 'anthovai_system');

\echo ''
\echo '-- vector, pgcrypto and citext must all be present:'
SELECT extname FROM pg_extension WHERE extname IN ('vector', 'pgcrypto', 'citext');
