# Deploying the Anthovai platform to DigitalOcean

The marketing site stays on Vercel. This is the part Vercel cannot run: a
long-lived Rust process, a worker that polls every 500 ms, PostgreSQL with
pgvector, and durable object storage.

Everything here has been rehearsed against the built image on a local stack
running as `ANTHOVAI_ENV=production`, including the SQL. What has not been
rehearsed is DigitalOcean itself, so the steps that touch their console are
written to be checked rather than trusted.

---

## Before you start

| | |
|---|---|
| DigitalOcean account | with billing set up |
| `doctl` | then `doctl auth init` |
| `psql` | to run `bootstrap.sql` |
| An OpenAI key | the one in `platform/.env` will do |

Pick **Singapore (`sgp` / `sgp1`)** for all three resources. It is the closest
region to customers in Thailand, and keeping the app, the database and the
bucket together avoids paying for an ocean crossing on every retrieval.

---

## 1. Database

Create a **Managed PostgreSQL 16** cluster in `sgp`. The smallest node is
enough to start.

From the cluster's *Connection details*, take the **admin** URI (user
`doadmin`) and run the bootstrap:

```bash
psql "$ADMIN_URL" -f bootstrap.sql
```

Edit the password in `bootstrap.sql` first. It prints two tables at the end,
and both must be right before you continue:

- `anthovai_api` shows `rolsuper = f`, `rolbypassrls = f`, `rolcanlogin = t`
- `vector`, `pgcrypto` and `citext` are all listed

Now apply the migrations, still as the admin:

```bash
cd platform
sqlx migrate run --database-url "$ADMIN_URL"
```

> **Migrations are a deliberate manual step.**
> `ANTHOVAI__DATABASE__RUN_MIGRATIONS_ON_START` stays `false` because the
> application connects as a role that cannot create roles or extensions — and
> giving it those rights would undo the isolation the next section is about.
> Run them as the admin, once per release, before the new image goes live.

### Why not just use doadmin for everything

Because `doadmin` is a superuser, and PostgreSQL exempts superusers from
row-level security entirely.

Measured on the rehearsal database, which held 74 knowledge bases across
several tenants:

| connection | rows returned by a plain count on knowledge_bases |
|---|---|
| superuser, no role switch | **74** — every tenant |
| `anthovai_api`, no role switch | **0** |
| `anthovai_api` + `SET LOCAL ROLE anthovai_app` + tenant pinned | **1** |
| `anthovai_api` + `SET LOCAL ROLE anthovai_system` | 74 — as a sweep should |

There is no error in the first row. It succeeds, and returns everyone's data.

The second row reads 0 only because of
`0006_system_policies_need_the_role.sql`. Before that migration it was 74 as
well: the system policies are written `TO anthovai_system`, and PostgreSQL
matches a policy's role list by *membership* rather than by which role the
session is running as — so they applied to the login role before any
`SET ROLE`. `NOINHERIT` does not help; it governs privileges, not policy
matching.

---

## 2. Spaces

Create a **Space** in `sgp1`. Keep it **private** — every object in it is a
customer's document.

Then *API -> Spaces Keys -> Generate New Key*. You get an access key and a
secret shown once.

The application reads these as `AWS_ACCESS_KEY_ID` and
`AWS_SECRET_ACCESS_KEY`, which is the convention every S3-compatible host
uses.

---

## 3. The app

```bash
cd platform/deploy/digitalocean
doctl apps spec validate --spec app.yaml
```

That catches a bad region or instance slug before it costs anything.

Fill in every `CHANGE_ME_` value. **Do not commit the filled-in file** — set
the secrets in the App Platform UI instead (*Settings -> App-Level Environment
Variables*), or keep a local copy outside the repository.

```bash
doctl apps create --spec app.yaml
doctl apps list
```

`SESSION_SECRET` is generated once and then left alone, since changing it signs
every customer out at the same moment:

```bash
openssl rand -base64 48
```

### If the build cannot find the Dockerfile

The repository root is the Next.js site and the platform is a subdirectory, so
`source_dir: platform` with `dockerfile_path: platform/docker/Dockerfile` is
doing something slightly unusual. If App Platform disagrees about which path is
relative to what, build elsewhere and deploy the image instead:

```bash
doctl registry create anthovai
docker build -f platform/docker/Dockerfile -t registry.digitalocean.com/anthovai/platform:v1 platform
docker push registry.digitalocean.com/anthovai/platform:v1
```

Then replace the `github:`, `source_dir` and `dockerfile_path` lines in both
components with:

```yaml
    image:
      registry_type: DOCR
      repository: platform
      tag: v1
```

---

## 4. Check it before pointing the site at it

```bash
curl -s https://YOUR-APP.ondigitalocean.app/internal/health
curl -s https://YOUR-APP.ondigitalocean.app/internal/ready
```

`ready` is the one that matters. If it says
`"storage":{"status":"failing","detail":"object storage is not answering"}`
with a time around three seconds, the credentials are not reaching the client
and it is timing out against a metadata service that does not exist on this
host. Check the two `AWS_*` variables.

`queue` may say `degraded` with a count of dead jobs. That is history, not a
fault.

---

## 5. Point the website at it

In **Vercel -> Settings -> Environment Variables**:

```
ANTHOVAI_API_URL = https://YOUR-APP.ondigitalocean.app
```

Redeploy the site. Until this is set, every dashboard call returns
`platform_unreachable` with a 503, which is what anthovai.com does today.

Then confirm the round trip **in a browser, not with curl** — the session
cookie is `__Host-` prefixed and `Secure`, which curl will not store over plain
HTTP even though browsers treat `localhost` as a secure origin:

1. Sign up at `https://www.anthovai.com/th/signup`
2. Create an organization and a knowledge base, upload a document
3. Wait for it to reach `ready` — this is what proves the worker is running
4. Create an agent, attach the knowledge base, publish
5. Issue a **test** key and call `/v1/chat` with it

If step 3 never completes, the worker is not running or cannot reach the
database. If step 2 or 4 fails with `origin_not_allowed`, the site's origin is
missing from `ANTHOVAI__SERVER__DASHBOARD_ORIGINS`.

---

## Known, and not blocking

- **Confirmation emails need an SMTP server.** Without
  `ANTHOVAI__MAIL__SMTP_URL` the platform logs the link instead of sending it
  and says so — at startup, and in its answer to the dashboard, which then
  tells the customer rather than sending them to an empty inbox. Live keys
  require a confirmed address, so this is the last thing between a real
  customer and a live key. Four variables: `SMTP_URL`, `USERNAME`,
  `PASSWORD`, `FROM`, all under `ANTHOVAI__MAIL__`. `ANTHOVAI__MAIL__SITE_URL`
  must be the public site, since that is where the link points.
- **Prices came from third-party trackers**, accepted on 2026-09-07, not from
  OpenAI's own page. Reconcile against the first real invoice.
- **`gpt-5.5` is legacy** now that OpenAI's pricing page has moved to
  `gpt-5.6`. It still answers; the large tier rests on a model the vendor has
  moved past.
- **Embedding tokens are counted but never priced.** Ingestion and every
  question cost real money that appears on no invoice.
- **Reading scans is off by default.** `ANTHOVAI__OCR__ENABLED=false` means a
  scanned PDF or a photograph is refused with a reason rather than queued
  against a service that is not there. Turning it on means deploying
  `ocr-sidecar/` alongside the API and worker.
- **One database test is intermittent.** `the_worker_takes_a_queued_document_to_ready`
  passes alone and sometimes fails in a full run: the test harness drains the
  whole `jobs` table rather than its own tenant's, so concurrent tests claim
  each other's work. It says nothing about the deployment.


## Fixed since this was written

Kept here because a checklist that only ever grows is one nobody trusts.

- Documents used to become **one chunk each** at typical Thai sizes, because the
  chunker sized them with `chars / 4` while the real tokenizer sat unused in the
  same crate — a 3.7x undercount for Thai. Pasted text also lost its Markdown
  headings, so there was nothing to split on. Both fixed; a question set went
  from 18 answers out of 24 to 24 out of 24, and existing knowledge bases are
  found and rebuilt automatically by the worker's startup sweep.
- A strict agent used to treat **a refusal as an absence** — asked whether a
  discounted book could be returned, with the policy saying plainly that it
  could not, it replied "I have no information about this" seven times out of
  eight. Fixed in the prompt; 48/48 after, with questions the documents do not
  cover still declined 24 times out of 24.
- **Live keys could not be issued at all** until email confirmation existed.
  They can now, given SMTP.

---

## Rollback

```bash
doctl apps list-deployments APP_ID
```

App Platform keeps previous deployments and can roll back from the console.
Migrations do not: they are forward-only, so rolling the image back must be to
a version whose schema expectations the database still satisfies.
