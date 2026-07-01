---
name: test-suite
description: Run the full integration test suite — hurl (backend), Playwright (frontend), and opencode MCP. Use when asked to run tests, verify the stack works end-to-end, or debug test failures.
---

# Test Suite

This project has three test suites that run at different layers of the stack.

## Prerequisites

All three suites require Docker services running:
```bash
docker compose up -d db es keycloak app auth-proxy
```

Keycloak must be set up (first time or after volume wipe):
```bash
python3 scripts/keycloak-setup.py
```

ES must be seeded:
```bash
bash seed_es.sh
```

## 1. Backend integration tests (hurl)

Entrypoint: `tests/docker-entrypoint.sh` — orchestrates execution order with cleanup between mutation and query tests.

⚠ **Key detail:** `.hurl` files are baked into the Docker image at build time. After editing them, rebuild the image:
```bash
./build-images.sh tests
```

Then run:
```bash
docker compose run --rm tests
```

Test order: health → mutations → **cleanup** → queries → relations → search → row_filters → auth_proxy

### Test files

| File | What it tests |
|---|---|
| `tests/health.hurl` | App health endpoint |
| `tests/mutations.hurl` | CRUD (creates orphan `HL` size — cleaned up after) |
| `tests/queries.hurl` | List/single queries on seed data |
| `tests/relations.hurl` | has_many, belongs_to, deep nesting |
| `tests/search.hurl` | ES search queries |
| `tests/row_filters.hurl` | Column RLS + subquery RLS + RBAC |
| `tests/auth_proxy.hurl` | Keycloak JWT auth through auth-proxy |

### Common failure patterns

- **Seed data mismatch**: If the test expects a different count of rows than seed.sql produces, update the `.hurl` assertion, then rebuild the test image.
- **Port 4000 not exposed**: The app container's port 4000 is NOT published to the host. All external traffic goes through auth-proxy on port 9080. Tests running inside Docker (`docker compose run --rm tests`) use `http://app:4000` directly.
- **ES credentials**: All ES requests need `-u elastic:morphis_es_pass`.

## 2. Frontend integration tests (Playwright)

Runs Keycloak OIDC login → frontend proxy → auth-proxy → API chain.

**Script** (recommended — handles env vars and cleanup):
```bash
bash scripts/run-frontend-tests.sh
```

**Manual** (for debugging):
```bash
# Kill stale process first
lsof -ti:3000 | xargs kill -9 2>/dev/null

# Start frontend
set -a; source scripts/.env.frontend; set +a
npx next dev --port 3000 &
wait_for_http "Frontend" "http://localhost:3000/login" 200 30

# Run tests
npx playwright test tests/auth-chain.spec.ts --workers=1
```

### Key env vars (from `scripts/.env.frontend`)

| Var | Value |
|---|---|
| `AUTH_OIDC_ISSUER` | `http://localhost:8080/realms/morphis` |
| `AUTH_OIDC_CLIENT_ID` | `morphis-test` |
| `GRAPHQL_URL` | `http://localhost:9080` |
| `AUTH_PROXY_JWT_SECRET` | `test-secret-key-for-integration-tests` |

Note: `AUTH_DISABLED=true` disables Keycloak — the auth-chain tests expect Keycloak to be enabled.

## 3. opencode MCP connection test

Verify the MCP server is reachable through the auth-proxy with JWT authentication:
```bash
opencode mcp list
opencode run 'find materials with status active'
```

The MCP URL is `http://localhost:9080/mcp` (through auth-proxy, which skips JWT for `/mcp` paths). The morphis app itself validates JWT on the MCP endpoint via `mcp.auth.enabled: true` in config.

## Quick reference

| Command | What it does | When to use |
|---|---|---|
| `./build-images.sh tests && docker compose run --rm tests` | Backend hurl tests | After changing `.hurl` files or seed data |
| `bash scripts/run-frontend-tests.sh` | Frontend Playwright tests | After changing frontend or auth-proxy |
| `opencode run '...'` | MCP tool tests | After changing MCP config or auth |
