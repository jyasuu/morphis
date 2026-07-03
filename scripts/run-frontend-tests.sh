#!/usr/bin/env bash
set -euo pipefail

# Start frontend dev server and run Playwright tests.
# Prerequisites: Docker services up, Keycloak setup done, ES seeded.
# Usage:
#   ./scripts/run-frontend-tests.sh              # OIDC auth-chain tests (default)
#   ./scripts/run-frontend-tests.sh --credentials # credentials mode (crud + entities)
#   ./scripts/run-frontend-tests.sh --skip-env    # skip sourcing .env.frontend

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/common.sh"

MODE="${1:-oidc}"
if [ "$MODE" = "--skip-env" ]; then
  MODE=oidc
  SKIP_ENV=true
else
  SKIP_ENV=false
fi
if [ "$MODE" = "--credentials" ]; then
  MODE=credentials
fi

cd "$SCRIPT_DIR/../frontend"

# Source shared env unless --skip-env
if [ "$SKIP_ENV" = false ]; then
  if [ "$MODE" = "credentials" ]; then
    # Clear Turbopack cache to ensure fresh env var bake
    # Use timeout + retry: zombie children may hold open handles preventing deletion
    rm -rf "$SCRIPT_DIR/../frontend/.next" 2>/dev/null || { sleep 2; rm -rf "$SCRIPT_DIR/../frontend/.next" 2>/dev/null || true; }
    # Unset any OIDC vars that may leak from a prior OIDC run in the same shell
    unset AUTH_OIDC_ISSUER AUTH_OIDC_CLIENT_ID AUTH_OIDC_CLIENT_SECRET AUTH_OIDC_NAME
    unset NEXT_PUBLIC_AUTH_OIDC_NAME
    export AUTH_SECRET=dev-secret-do-not-use-in-prod
    export AUTH_ADMIN_USERNAME=admin
    export AUTH_ADMIN_PASSWORD=admin
    export AUTH_DISABLED=true
    export GRAPHQL_URL=http://localhost:9080
    export NEXT_PUBLIC_GRAPHQL_URL=http://localhost:3000/api/graphql
    export AUTH_PROXY_JWT_SECRET=test-secret-key-for-integration-tests
  else
    set -a; source "$SCRIPT_DIR/.env.frontend"; set +a
  fi
fi

# Kill any stale frontend dev server on port 3000
# Use ss (not lsof) for reliable port detection
OLD_PID=$(ss -tlnp 2>/dev/null | grep ':3000 ' | grep -o 'pid=[0-9]*' | grep -o '[0-9]*' | head -1 || true)
if [ -n "$OLD_PID" ]; then
  echo "Killing stale frontend on port 3000 (PID $OLD_PID) ..."
  kill -9 "$OLD_PID" 2>/dev/null || true
  sleep 2
fi

# Start frontend dev server in background (MUST use setsid to survive shell exit)
echo "=== Start frontend ==="
setsid npx next dev --port 3000 > /tmp/nextdev.log 2>&1 &
FRONTEND_PID=$!
echo "  Frontend PID: $FRONTEND_PID"

echo "=== Wait for frontend ==="
if wait_for_http "Frontend" "http://localhost:3000/login" 200 30; then
  echo "  OK"
else
  echo "  FAILED" >&2
  kill "$FRONTEND_PID" 2>/dev/null || true
  exit 1
fi

echo "=== Run Playwright tests ==="
if [ "$MODE" = "credentials" ]; then
  npx playwright test tests/crud.spec.ts tests/entities.spec.ts --workers=1
else
  npx playwright test tests/auth-chain.spec.ts --workers=1
fi
RC=$?

# Cleanup
kill "$FRONTEND_PID" 2>/dev/null || true
echo "=== Frontend tests done ==="
exit $RC
