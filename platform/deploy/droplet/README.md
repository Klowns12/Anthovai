# The platform on `k-garden-server`

Not the App Platform deployment in `../digitalocean/` — that one is still
written but unused. This is where the platform actually runs today: a plain
Docker Compose stack on an existing droplet, at `/opt/anthovai/`.

```
droplet   k-garden-server, SGP1, 4 GB / 80 GB, 152.42.177.130
stack     /opt/anthovai/{docker-compose.yml,.env}
API       host port 8090   (8080 belongs to something else here)
```

The compose file in this directory is byte-identical to the one on the droplet.
Keeping it here is the point: it was edited over SSH for a while, which meant
the only copy of how the thing runs lived on the thing itself.

```bash
scp docker-compose.yml root@152.42.177.130:/opt/anthovai/
ssh root@152.42.177.130 'cd /opt/anthovai && docker compose --env-file .env up -d'
```

`.env` stays on the droplet only — this repository is public. `.env.example`
lists what it must contain.

## This droplet is not ours alone

It runs eighteen containers of other people's production — finance, property,
devicehub, vikunja — on 3.8 GB of RAM with swap already in use. Two
consequences, both deliberate:

- **Every service declares `mem_limit`.** Not for our benefit. Without one, the
  kernel's OOM killer chooses its own victim when memory runs short, and it is
  as likely to pick the finance database as anything of ours. With one, the
  process that overruns its share is ours, and it is ours that dies.
- **Only the API is published on `0.0.0.0`.** PostgreSQL and MinIO listen on
  `127.0.0.1` alone; they have no business being reachable from the internet.

Measured after the first deployment: api 23 MB, worker 14 MB, postgres 56 MB,
minio 154 MB — 247 MB total, host available RAM unchanged, all 27 other
containers still running.

## TLS is not done, and the website must not point here until it is

The API answers on plain HTTP at `http://152.42.177.130:8090`. The website
proxies a customer's session cookie through it and their application sends an
API key with every question, so both would cross the internet readable. Do not
set `ANTHOVAI_API_URL` in Vercel until this section says otherwise.

The intended fix needs no DNS change: the host already runs Caddy
(container `proxy`) on 80/443, and its existing blocks use
`*.152.42.177.130.sslip.io`, which Let's Encrypt will certify as-is.
`Caddyfile.fragment` in this directory is the block to add.

**It is blocked on something that is not ours to decide.** The `proxy`
container bind-mounts `/opt/ot-worker/Caddyfile` — a single file, so Docker
binds the *inode*. Someone edited that file with an editor that writes a new
file and renames it over the old one, so since 2026-08-29 the container has
held an inode that no longer has a path on the host:

```
host      inode=525085   mtime=2026-09-13
container inode=608763   mtime=2026-08-29   <- unlinked; no path on the host
```

Two things follow. `caddy reload` reads the stale inode, so a block appended to
the host file takes effect silently never — and `caddy validate` cheerfully
reports "Valid configuration" about the old file, which is exactly the kind of
green light worth distrusting. Worse, the running config contains a
`daybook.152.42.177.130.sslip.io` block that the host file does **not**: that
site survives only as long as that inode does. Restarting the proxy as things
stand would take it down.

The only way to update the config is to recreate the container, which drops
80/443 for every site on the box for a few seconds. That is the owner's call,
not ours. When it is made, the order is: write the host file as
*running config + daybook + our block* (a superset, so nothing in service is
lost), validate it in a throwaway container rather than the running one,
recreate, then check every existing hostname before checking ours.

A backup of the host file as it was before our block was appended is at
`/opt/ot-worker/Caddyfile.bak.20260913-094054`.
