# Anthovai

Two things live here, and they deploy separately.

| | | |
|---|---|---|
| **the site** | repository root | [anthovai.com](https://www.anthovai.com/) — Next.js, eight languages, deployed by Vercel on every push to `main` |
| **the platform** | [`platform/`](platform/README.md) | The multi-tenant RAG service the site will offer. Rust, PostgreSQL with pgvector, its own Docker image |

They share a repository because the platform is sold through the site, not
because they build together. Nothing at the root imports from `platform/`, and
nothing in `platform/` imports from the root.

## Working on the site

```bash
bun install
bun run dev
```

## Working on the platform

Everything it needs is in [`platform/README.md`](platform/README.md), and every
command there is run from `platform/`:

```bash
cd platform
docker compose -f docker/docker-compose.yml up -d postgres
cargo test --workspace
```

## Continuous integration

`.github/workflows/platform-ci.yml` runs only when something under `platform/`
changes, so editing a landing page does not spend ten minutes compiling Rust.
Vercel builds the site on every push to `main` — including a push that only
touched `platform/`, which is a redundant build of identical content rather
than a problem, and can be skipped later with an `ignoreCommand` in
`vercel.json` if it becomes one.
