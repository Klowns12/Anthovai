-- Messages need an order that does not depend on the clock.
--
-- A question and its answer are written in one transaction, and `created_at`
-- defaults to `now()`, which in PostgreSQL is the transaction's start time —
-- identical for both rows. Ordering by it alone left the pair in whatever order
-- the planner returned them, so a conversation could come back with the answer
-- before the question, and the history handed to the model with it.
--
-- A sequence settles it: monotonic, assigned at insert, independent of both the
-- clock and the row's id.

ALTER TABLE messages ADD COLUMN IF NOT EXISTS seq BIGSERIAL;

-- Rows written before this migration keep whatever order the sequence assigns
-- them, which follows their physical order — the same order they were inserted.
CREATE INDEX IF NOT EXISTS messages_conversation_seq_idx
  ON messages (conversation_id, seq);

-- The old index ordered by a column that cannot order these rows.
DROP INDEX IF EXISTS messages_conv_idx;

-- A BIGSERIAL is backed by a sequence, and inserting a row now draws from it.
-- Without this the application role can write every column of `messages` and
-- still fail on the one it never names.
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO anthovai_app, anthovai_system;
