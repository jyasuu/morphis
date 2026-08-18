#!/usr/bin/env bash
set -euo pipefail

# Full backend integration check from zero: builds, starts services,
# sets up Keycloak, seeds ES, runs hurl test suite.
# Usage: ./scripts/integration-check.sh [--skip-build]

cd "$(dirname "$0")/.."
source scripts/common.sh

SKIP_BUILD=false
for arg in "$@"; do
  [ "$arg" = "--skip-build" ] && SKIP_BUILD=true
done

if [ "$SKIP_BUILD" = false ]; then
  check_step "Build images" ./build-images.sh
  check_step "Build test image" docker compose build --no-cache tests
else
  echo "=== Skipping build ==="
fi

check_step "Start services" docker compose up -d db es keycloak app auth-proxy rabbitmq pgx-listen-rabbitmq pgx-consume

check_step "Wait for Keycloak" wait_for_http "Keycloak" "http://localhost:8180/realms/master" 200 60
check_step "Wait for Morphis" bash -c '
  for i in $(seq 1 60); do
    code=$(docker run --rm --network "$(docker compose ls -q 2>/dev/null || echo workspace)_default" curlimages/curl:latest -s -o /dev/null -w "%{http_code}" http://app:4000/health 2>/dev/null || echo "000")
    if [ "$code" = "200" ]; then
      echo "  Morphis ready (HTTP $code)"
      exit 0
    fi
    sleep 1
  done
  echo "  ERROR: Morphis not ready after 60s (last HTTP $code)" >&2
  exit 1
'

check_step "Wait for indexing pipeline" bash -c '
  for i in $(seq 1 30); do
    consumers=$(curl -s -u guest:guest http://localhost:15672/api/queues/%2F/pgx-events | python3 -c "import json,sys; print(json.load(sys.stdin).get(\"consumer_count\", 0))" 2>/dev/null || echo 0)
    if [ "$consumers" -ge 1 ]; then
      echo "  pipeline consuming ($consumers consumer on pgx-events)"
      exit 0
    fi
    sleep 2
  done
  echo "  ERROR: indexing pipeline did not attach a consumer to pgx-events" >&2
  exit 1
'

check_step "Keycloak setup" python3 scripts/keycloak-setup.py
check_step "Seed ES" bash seed_es.sh
check_step "Run hurl tests" docker compose run --rm tests

check_step "Verify N+1 is gone (relation_batch log)" bash -c '
  LOGS=$(docker compose logs app 2>/dev/null | grep "relation_batch" || true)
  if [ -z "$LOGS" ]; then
    echo "  ERROR: no relation_batch log lines found in app output" >&2
    exit 1
  fi
  echo "$LOGS" | grep -q "queries=1" || { echo "  ERROR: expected queries=1 but found: $LOGS" >&2; exit 1; }
  echo "$LOGS"
  echo "  OK: relation resolvers batched (queries=1, not N)"
'

echo ""
echo "=== All backend checks passed ==="
