#!/usr/bin/env bash
set -euo pipefail

# Start frontend dev server and run Playwright auth-chain tests.
# Prerequisites: Docker services up, Keycloak setup done, ES seeded.

cd "$(dirname "$0")/../frontend"

FRONTEND_ENV="AUTH_SECRET=dev-secret-do-not-use-in-prod \
AUTH_OIDC_ISSUER=http://localhost:8080/realms/morphis \
AUTH_OIDC_CLIENT_ID=morphis-test \
AUTH_OIDC_CLIENT_SECRET=morphis-test-secret \
AUTH_OIDC_NAME=Keycloak \
NEXT_PUBLIC_AUTH_OIDC_NAME=Keycloak \
GRAPHQL_URL=http://localhost:9080 \
AUTH_PROXY_JWT_SECRET=test-secret-key-for-integration-tests"

echo "=== Starting frontend dev server ==="
eval "$FRONTEND_ENV npx next dev --port 3000 &"
FRONTEND_PID=$!
echo "Frontend PID: $FRONTEND_PID"

# Wait for frontend
for i in $(seq 1 30); do
    if curl -s -o /dev/null -w "%{http_code}" http://localhost:3000/login 2>/dev/null | grep -q 200; then
        echo "Frontend ready"
        break
    fi
    sleep 2
done

echo "=== Running Playwright auth-chain tests ==="
eval "$FRONTEND_ENV npx playwright test tests/auth-chain.spec.ts --workers=1"

# Cleanup
kill $FRONTEND_PID 2>/dev/null || true
echo "=== Done ==="
