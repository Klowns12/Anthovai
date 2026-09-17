#!/bin/bash
# Merge the *running* Caddyfile (stale inode, includes daybook) with the
# Anthovai TLS blocks, validate against the live proxy image, then recreate
# the proxy so the new file is what Caddy actually reads.
set -euo pipefail

STAMP=$(date +%Y%m%d-%H%M%S)
HOST_FILE=/opt/ot-worker/Caddyfile
FRAGMENT=/opt/anthovai/Caddyfile.fragment
MERGED=/tmp/Caddyfile.anthovai-merge-$STAMP
BACKUP_HOST=/opt/ot-worker/Caddyfile.bak.$STAMP-host
BACKUP_RUNNING=/opt/ot-worker/Caddyfile.bak.$STAMP-running

cp -a "$HOST_FILE" "$BACKUP_HOST"
docker exec proxy cat /etc/caddy/Caddyfile > "$BACKUP_RUNNING"
echo "backed up host -> $BACKUP_HOST"
echo "backed up running -> $BACKUP_RUNNING"

cp "$BACKUP_RUNNING" "$MERGED"
if grep -q 'anthovai.152.42.177.130.sslip.io' "$MERGED"; then
  echo "running config already has the anthovai block"
else
  printf '\n' >> "$MERGED"
  awk '/^anthovai\.152\.42\.177\.130\.sslip\.io \{/,/^}/' "$FRAGMENT" >> "$MERGED"
  echo "appended anthovai block to running config"
fi

if grep -q 'demo.152.42.177.130.sslip.io' "$MERGED"; then
  echo "running config already has the demo block"
else
  printf '\n' >> "$MERGED"
  awk '/^demo\.152\.42\.177\.130\.sslip\.io \{/,/^}/' "$FRAGMENT" >> "$MERGED"
  echo "appended demo block to running config"
fi

if ! grep -q 'daybook.152.42.177.130.sslip.io' "$MERGED"; then
  echo "ERROR: merged file is missing daybook; aborting" >&2
  exit 1
fi
if ! grep -q 'anthovai.152.42.177.130.sslip.io' "$MERGED"; then
  echo "ERROR: merged file is missing anthovai; aborting" >&2
  exit 1
fi
if ! grep -q 'demo.152.42.177.130.sslip.io' "$MERGED"; then
  echo "ERROR: merged file is missing the OCR demo host; aborting" >&2
  exit 1
fi
if ! grep -q 'vikunja.152.42.177.130.sslip.io' "$MERGED"; then
  echo "ERROR: merged file is missing vikunja; aborting" >&2
  exit 1
fi

ENV_FILE=/tmp/proxy-env-$STAMP
docker inspect proxy --format '{{range .Config.Env}}{{println .}}{{end}}' \
  | grep -E '^(FILE_SIZE_LIMIT|BUCKET_NAME|SITE_ADDRESS|CERT_EMAIL|CERT_ACME_CA|CERT_ACME_DNS|TRUSTED_PROXIES)=' \
  > "$ENV_FILE"

echo "validating merged Caddyfile with image work-plane-proxy ..."
docker run --rm --network none \
  --env-file "$ENV_FILE" \
  -v "$MERGED:/etc/caddy/Caddyfile:ro" \
  work-plane-proxy \
  caddy validate --config /etc/caddy/Caddyfile

install -m 644 "$MERGED" "$HOST_FILE"
echo "wrote merged file inode=$(ls -i "$HOST_FILE" | awk '{print $1}')"

cd /root/work-plane
docker compose up -d --force-recreate --no-deps proxy

echo "waiting for proxy to listen ..."
for i in $(seq 1 30); do
  if docker exec proxy wget -qO- http://127.0.0.1:80 >/dev/null 2>&1 \
     || docker exec proxy caddy version >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

echo "==== container now reads ===="
docker exec proxy ls -li /etc/caddy/Caddyfile
echo "==== anthovai present? ===="
docker exec proxy grep -n 'anthovai.152.42.177.130.sslip.io' /etc/caddy/Caddyfile
echo "==== demo present? ===="
docker exec proxy grep -n 'demo.152.42.177.130.sslip.io' /etc/caddy/Caddyfile
echo "==== daybook present? ===="
docker exec proxy grep -n 'daybook.152.42.177.130.sslip.io' /etc/caddy/Caddyfile
