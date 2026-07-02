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

check_step "Start services" docker compose up -d db es keycloak app auth-proxy

check_step "Wait for Keycloak" wait_for_http "Keycloak" "http://localhost:8080/realms/master" 200 60
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

check_step "Keycloak setup" python3 scripts/keycloak-setup.py
check_step "Seed ES" bash seed_es.sh
check_step "Run hurl tests" docker compose run --rm tests

echo ""
echo "=== All backend checks passed ==="
