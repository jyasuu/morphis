#!/usr/bin/env bash
set -euo pipefail

# Start frontend dev server and run Playwright auth-chain tests.
# Prerequisites: Docker services up, Keycloak setup done, ES seeded.
# Usage: ./scripts/run-frontend-tests.sh [--skip-env]

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/common.sh"

cd "$SCRIPT_DIR/../frontend"

# Source shared env unless --skip-env (for when already set in parent shell)
if [ "${1:-}" != "--skip-env" ]; then
  set -a; source "$SCRIPT_DIR/.env.frontend"; set +a
fi

# Kill any stale frontend dev server on port 3000
if lsof -ti:3000 &>/dev/null; then
  echo "Killing stale frontend on port 3000 ..."
  lsof -ti:3000 | xargs kill
  sleep 1
fi

check_step "Start frontend" npx next dev --port 3000 &
FRONTEND_PID=$!
echo "  Frontend PID: $FRONTEND_PID"

check_step "Wait for frontend" wait_for_http "Frontend" "http://localhost:3000/login" 200 30

check_step "Run Playwright auth-chain tests" \
  npx playwright test tests/auth-chain.spec.ts --workers=1

# Cleanup
kill "$FRONTEND_PID" 2>/dev/null || true
echo "=== Frontend tests done ==="
