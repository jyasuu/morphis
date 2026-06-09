#!/usr/bin/env bash
set -euo pipefail

# Full integration check from zero: backend hurl tests + frontend Playwright tests.
# Usage:
#   ./scripts/run-all.sh              # full run (build → backend → frontend)
#   ./scripts/run-all.sh --no-playwright  # skip Playwright (CI without browser)
#   ./scripts/run-all.sh --skip-build     # reuse existing images

cd "$(dirname "$0")/.."

RUN_PLAYWRIGHT=true
BUILD_FLAG=""
for arg in "$@"; do
  [ "$arg" = "--no-playwright" ] && RUN_PLAYWRIGHT=false
  [ "$arg" = "--skip-build" ]    && BUILD_FLAG="--skip-build"
done

STEP=0

# ── 1. Build ──────────────────────────────────────────────
STEP=$((STEP + 1)); echo ""
echo "╔═══════════════════════════════════════════════╗"
echo "║  STEP $STEP: Build images                          ║"
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

# ── 2. Frontend Playwright ────────────────────────────────
STEP=$((STEP + 1)); echo ""
echo "╔═══════════════════════════════════════════════╗"
echo "║  STEP $STEP: Frontend Playwright tests                ║"
echo "╚═══════════════════════════════════════════════╝"
bash scripts/run-frontend-tests.sh
RC_FRONTEND=$?

echo ""
if [ "$RC_FRONTEND" -eq 0 ]; then
  echo "=== All checks passed ==="
else
  echo "=== Frontend tests failed ===" >&2
fi
exit "$RC_FRONTEND"
