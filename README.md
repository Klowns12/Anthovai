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

Both halves are filtered, so neither waits on the other.

`.github/workflows/platform-ci.yml` runs only when something under `platform/`
changes — editing a landing page does not spend ten minutes compiling Rust.

`ignoreCommand` in `vercel.json` does the mirror image: a push that touched
nothing outside `platform/` does not rebuild the site.

```
git diff --quiet HEAD^ HEAD -- . ':(exclude)platform'
```

Vercel reads the exit code, and the sense is inverted from what you might
expect: **0 skips the build, anything else builds.** So `--quiet` returning 0
(nothing changed outside `platform/`) skips, 1 (something did) builds, and 128
(no `HEAD^` — a shallow clone, or the first commit) also builds. Every failure
mode falls on the side of building, which is the harmless one.
