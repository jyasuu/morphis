#!/usr/bin/env bash
set -euo pipefail

# Full integration check from zero: backend hurl tests + frontend Playwright tests.
# Usage:
#   ./scripts/run-all.sh                    # full run (build → backend → frontend both modes)
#   ./scripts/run-all.sh --no-playwright    # skip Playwright (CI without browser)
#   ./scripts/run-all.sh --skip-build       # reuse existing images
#   ./scripts/run-all.sh --playwright-oidc  # OIDC auth-chain only (skip credentials mode)

cd "$(dirname "$0")/.."

RUN_PLAYWRIGHT=true
PLAYWRIGHT_CREDENTIALS=true
BUILD_FLAG=""
for arg in "$@"; do
  [ "$arg" = "--no-playwright" ]    && RUN_PLAYWRIGHT=false
  [ "$arg" = "--skip-build" ]       && BUILD_FLAG="--skip-build"
  [ "$arg" = "--playwright-oidc" ]  && PLAYWRIGHT_CREDENTIALS=false
done

STEP=0

# ── 1. Build + Backend tests ──────────────────────────────
STEP=$((STEP + 1)); echo ""
echo "╔═══════════════════════════════════════════════╗"
echo "║  STEP $STEP: Build + Backend integration tests        ║"
echo "╚═══════════════════════════════════════════════╝"
bash scripts/integration-check.sh $BUILD_FLAG
RC_BACKEND=$?

if [ "$RC_BACKEND" -ne 0 ]; then
  echo "Backend checks failed — aborting"
  exit "$RC_BACKEND"
fi

if [ "$RUN_PLAYWRIGHT" = false ]; then
  echo ""
  echo "=== All done (Playwright skipped) ==="
  exit 0
fi

RC_FRONTEND=0

# ── 2a. Frontend Playwright — OIDC auth-chain ─────────────
STEP=$((STEP + 1)); echo ""
echo "╔═══════════════════════════════════════════════╗"
echo "║  STEP $STEP: Frontend OIDC (Keycloak) tests            ║"
echo "╚═══════════════════════════════════════════════╝"
bash scripts/run-frontend-tests.sh || RC_FRONTEND=$?

if [ "$PLAYWRIGHT_CREDENTIALS" = true ]; then
  # ── 2b. Frontend Playwright — Credentials mode ──────────
  STEP=$((STEP + 1)); echo ""
  echo "╔═══════════════════════════════════════════════╗"
  echo "║  STEP $STEP: Frontend Credentials (CRUD + Entities)   ║"
  echo "╚═══════════════════════════════════════════════╝"
  bash scripts/run-frontend-tests.sh --credentials || RC_FRONTEND=$?
fi

echo ""
if [ "$RC_FRONTEND" -eq 0 ]; then
  echo "=== All checks passed ==="
else
  echo "=== Some frontend tests failed ===" >&2
fi
exit "$RC_FRONTEND"
