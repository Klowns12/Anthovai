# Anthovai AI Platform

> Lives in `platform/` of the [anthovai.com](https://www.anthovai.com/) repository,
> alongside the marketing site it will be offered through. Every command below
> is run from `platform/`, not from the repository root.

Multi-tenant RAG-as-a-Service. A customer creates an agent, uploads their
knowledge, and gets an API key to call from their own website, LMS or app. They
never build retrieval themselves.

The design is fixed in [`docs/spec-v0.1/`](docs/spec-v0.1/README.md) — read
[06 — Rust Workspace Architecture](docs/spec-v0.1/06-rust-workspace-architecture.md)
before changing anything structural, and
[08 — P1 Implementation Checklist](docs/spec-v0.1/08-p1-implementation-checklist.md)
for what to build next.

## Running it locally

You need Rust (stable) and Docker. Only PostgreSQL has to be running: uploads
go to local disk by default, so MinIO is optional.

```bash
cp .env.example .env
docker compose -f docker/docker-compose.yml up -d postgres
cargo test --workspace
```

Then, in two terminals:

```bash
ANTHOVAI__DATABASE__RUN_MIGRATIONS_ON_START=true cargo run --bin anthovai-api
```

```bash
cargo run --bin anthovai-worker
```

### Or as containers

The same thing built into an image, which is what staging runs. One command,
and `--build` is only needed the first time and after a code change:

```bash
docker compose -f docker/docker-compose.yml -f docker/docker-compose.stack.yml up --build
```

Both files use the same project name, so this shares the database and bucket
with the developer setup above rather than stranding them. `OPENAI_API_KEY` and
`ANTHROPIC_API_KEY` are passed through from your shell when set; without them
the stack still answers questions, from the retrieved passages, using the local
echo model.

The API answers on <http://localhost:8080/internal/health>. Three more endpoints
are worth knowing:

| | |
|---|---|
| `GET /internal/health` | Alive. What an orchestrator restarts on, so it touches nothing that could be slow. |
| `GET /internal/ready` | Able to work: database, storage, model providers, queue depth. What a load balancer takes out of rotation on. |
| `GET /internal/metrics` | Prometheus. Request rate and latency by route, provider calls and latency by model, retrieval time, queue gauges. |
| `GET /v1/openapi.json` | The published API contract, generated from the types the server actually sends. Needs no key. |

Uploaded files land
under `./data/storage`; set `ANTHOVAI__STORAGE__PROVIDER=s3` to use MinIO or S3
instead.

Local services use non-default ports so they do not collide with anything else
on the machine: PostgreSQL on **55432**, MinIO on **9010** (console **9011**).

Migrations are plain SQL under `migrations/`. The server applies them when
`run_migrations_on_start` is set; apply them by hand only against a database
that sqlx has never migrated, or its bookkeeping and the schema will disagree.

## Layout

```
apps/api        the HTTP server binary
apps/worker     the background worker binary
crates/         one crate per domain, see doc 06 for the dependency rules
config/         default.toml, models.toml (the model registry), plans
migrations/     PostgreSQL schema and row-level security
docs/spec-v0.1/ the specification this code implements
```

## Three rules that are not negotiable

1. **Every domain function takes `&TenantCtx` first, and every query filters on
   `tenant_id`.** Row-level security backs this up, but it is the second line,
   not the first. A cross-tenant leak is the one bug that ends the product.
2. **Nothing above `crates/inference` knows a vendor exists.** Customers choose
   a policy, not a model name, so we can change provider without breaking a
   single integration.
3. **Schema or API change means the spec changes first.** Update doc 04 or 05
   in the same commit, or the documents stop being true and stop being useful.

## Running the database tests

Some tests need a real PostgreSQL, because row-level security and `SET LOCAL
ROLE` do not exist in a mock and they are the things most worth testing. Point
them at a database and they run; leave the variable unset and they announce that
they skipped, rather than passing quietly.

```bash
docker exec -i anthovai-postgres-1 psql -U anthovai -d postgres -c "CREATE DATABASE anthovai_test"
```

Then set `ANTHOVAI_TEST_DATABASE_URL` to
`postgres://anthovai:anthovai@localhost:55432/anthovai_test` and run
`cargo test --workspace`. Migrations are applied automatically.

## Where things stand

Phases A through G of [the plan](docs/spec-v0.1/09-development-plan-m4-m9.md)
are done: 532 tests pass, and everything from tenant isolation to a question
coming back with citations is proved against a real PostgreSQL — including
through the HTTP layer, with real cookies, headers, multipart bodies and status
codes.

Working today, end to end: sign up, create an organization and an agent, publish
it, mint an API key, create a knowledge base, upload a PDF, Word document,
spreadsheet, JSON export, Markdown file or web page — or hand us a URL to fetch
— watch the worker parse, chunk, embed and index it, then ask a question and get
an answer from a real model, with citations back to the passages it was built
from. Ask something the documents do not cover and it says so, without calling a
model at all.

Underneath: the tenant-scoped database layer with row-level security actually
enforced, plan limits and permissions for both actor kinds, the full API key
lifecycle, agent drafts and published versions with rollback, object storage on
disk or S3, a PostgreSQL job queue with retries and recovery from a dead worker,
and an ingestion pipeline that versions its output — the previous version keeps
serving until the new one is complete, so a re-upload never leaves a gap and a
failed one leaves the old version in place.

Retrieval: a question is embedded, searched against pgvector and a keyword
index, fused, diversified, trimmed to a token budget and assembled into a
knowledge block with citations. Measured against a real model, five Thai
questions about a Thai handbook each found the answering paragraph first, and an
unrelated question correctly found nothing.

Everything except the model call costs a **p95 of around 100ms at 50 requests a
second** against a budget of 400ms, measured by `crates/api/tests/load.rs` — a
thousand requests, no failures, on one developer machine also running the
database.

### The two things still open

**Nobody has confirmed what the models cost.** Chat and embeddings are both
real — three Thai questions about a Thai handbook came back with correct short
Thai answers, each citing the right section — but the price fields in
`config/models.toml` are zero and marked unconfirmed, so every usage record
carries a cost of nothing.

That is deliberate rather than forgotten: a plausible-looking wrong price
becomes a wrong invoice that no other part of the system contradicts. Production
refuses to start while any enabled model has no `priced_on` date, and
development warns at every startup. Confirm the figures, set the date, and both
stop.

Anthropic is a key away: the Claude rows are enabled and their names are what
the current models are called. Without `ANTHROPIC_API_KEY` the factory says so
at startup and routes to OpenAI instead.

**Thai PDFs have not been tested against a real file.** The generated fixtures
prove the structure; what they cannot prove is whether a subset-embedded Thai
font maps its glyphs back to characters. `pdf-extract` returns plausible-looking
mojibake rather than an error when it does not, which would be embedded and
retrieved for months before anyone noticed. Drop a real customer PDF at
`crates/ingestion/tests/fixtures/thai-handbook.pdf` and run:

```bash
cargo test -p anthovai-ingestion --lib thai -- --ignored --nocapture
```

### Two things to know about the tests

Tests that need a database skip themselves loudly without
`ANTHOVAI_TEST_DATABASE_URL`. Tests about retrieval *quality* need a real
embedding model, so they are ignored by default:

```bash
cargo test -p anthovai-retrieval --test relevance -- --ignored --nocapture
```

Everything else runs against a deterministic local embedder, and is written to
be structural: it asks whether the right rows are reachable and assembled, never
whether they are ranked well. Ranking is a property of the model, and judging it
against a stand-in would only measure the stand-in.
