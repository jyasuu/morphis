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
check_step "Wait for Morphis" wait_for_http "Morphis" "http://localhost:4000/health" 200 30

check_step "Keycloak setup" python3 scripts/keycloak-setup.py
check_step "Seed ES" bash seed_es.sh
check_step "Run hurl tests" docker compose run --rm tests

echo ""
echo "=== All backend checks passed ==="
