#!/usr/bin/env bash
#
# What the platform costs per question, with the model taken out of the picture.
#
# Everything except the provider call is ours: authentication, the quota check,
# embedding the question, two index scans, fusion, prompt assembly, and one
# transaction to record the turn. That is the part a load test can hold us to,
# and the target is a p95 under 400ms at 50 requests per second.
#
# The model itself is deliberately the echo provider. Measuring a real provider
# would measure their capacity, not ours, and the number would move every day
# for reasons we cannot fix.
#
#   ANTHOVAI_API_KEY=av_live_… AGENT_ID=agt_… ./scripts/load-test.sh
#
# Needs `oha` (cargo install oha).

set -euo pipefail

API="${API:-http://127.0.0.1:8080}"
RATE="${RATE:-50}"
DURATION="${DURATION:-30s}"
CONCURRENCY="${CONCURRENCY:-50}"
QUESTION="${QUESTION:-When does the library open in the morning?}"

if ! command -v oha >/dev/null 2>&1; then
  echo "oha is not installed. cargo install oha" >&2
  exit 1
fi

: "${ANTHOVAI_API_KEY:?set ANTHOVAI_API_KEY to a key with the chat scope}"
: "${AGENT_ID:?set AGENT_ID to a published agent}"

# A single request first. A load test against a misconfigured server measures
# how fast it can return 403.
probe=$(curl -sS -o /dev/null -w '%{http_code}' \
  -H "Authorization: Bearer ${ANTHOVAI_API_KEY}" \
  -H 'Content-Type: application/json' \
  -X POST "${API}/v1/chat" \
  -d "$(printf '{"agent_id":"%s","message":"%s"}' "$AGENT_ID" "$QUESTION")")

if [ "$probe" != "200" ]; then
  echo "the warm-up request returned ${probe}; fix that before measuring" >&2
  exit 1
fi

echo "Which model is answering — if this is not the echo model, the numbers"
echo "below are the provider's, not ours:"
curl -sS "${API}/internal/ready" | grep -o '"providers":{[^}]*}' || true
echo

exec oha \
  --no-tui \
  -q "${RATE}" \
  -c "${CONCURRENCY}" \
  -z "${DURATION}" \
  -m POST \
  -H "Authorization: Bearer ${ANTHOVAI_API_KEY}" \
  -H 'Content-Type: application/json' \
  -d "$(printf '{"agent_id":"%s","message":"%s"}' "$AGENT_ID" "$QUESTION")" \
  "${API}/v1/chat"
