-- Proving an address belongs to the person who typed it.
--
-- Until now `mark_email_verified` existed as a service method with nothing
-- calling it, so the only way to verify anybody was an UPDATE by hand. A live
-- API key requires a verified address, which meant no real customer could ever
-- obtain one.
--
-- Shaped like `sessions`, and for the same reasons: only the hash is stored, so
-- a leaked backup cannot be replayed into somebody's account, and the row is
-- keyed by that hash because it is the only thing a request arrives holding.

CREATE TABLE email_verifications (
  token_hash  TEXT PRIMARY KEY,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  -- The address this token proves, captured when it was issued. If the account
  -- changes address before the link is followed, the token proves something
  -- that is no longer true and must not be accepted.
  email       CITEXT NOT NULL,
  expires_at  TIMESTAMPTZ NOT NULL,
  -- Set when it is used. Kept rather than deleted so a second click on the same
  -- link can say "already confirmed" instead of "invalid", which is what a mail
  -- client that prefetches links will otherwise produce for everyone.
  consumed_at TIMESTAMPTZ,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX email_verifications_user_idx ON email_verifications (user_id);
CREATE INDEX email_verifications_expiry_idx ON email_verifications (expires_at);

-- No `tenant_id`: verification happens before an organization is chosen, and
-- often before one exists. `users` and `sessions` are outside the tenant model
-- for the same reason, so RLS is not enabled here — the table is only ever
-- reached through the system role, which is not a tenant-scoped path.
--
-- The blanket grant in `0002_rls.sql` ran once, against the tables that existed
-- then. A new table needs its own.
GRANT SELECT, INSERT, UPDATE, DELETE ON email_verifications TO anthovai_system;

COMMENT ON TABLE email_verifications IS
  'Outstanding email-verification tokens, stored as SHA-256 hashes.';
