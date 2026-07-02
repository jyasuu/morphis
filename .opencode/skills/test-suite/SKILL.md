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

Test order: health → mutations → **cleanup** → queries → relations → search → row_filters → auth_proxy → **re-seed**

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

### Re-seed after tests

After all hurl tests finish, `docker-entrypoint.sh` re-inserts the `admin` user into `user_permissions`. This is needed because `row_filters.hurl` cleans up ALL `user_permissions` rows during its own cleanup, which would break downstream consumers (e.g. frontend tests) that depend on the `admin` user for RLS subquery resolution.

### Common failure patterns

- **Seed data mismatch**: If the test expects a different count of rows than seed.sql produces, update the `.hurl` assertion, then rebuild the test image.
- **Port 4000 not exposed**: The app container's port 4000 is NOT published to the host (removed in security audit). All external traffic goes through auth-proxy on port 9080. Tests running inside Docker (`docker compose run --rm tests`) use `http://app:4000` directly.
- **Health check fails on host**: `scripts/integration-check.sh` checks Morphis health from the host, but port 4000 is unreachable. The script now uses `docker run --rm --network <network> curlimages/curl` to curl `http://app:4000/health` from inside the Docker network. If the `curlimages/curl` image is not pulled, it will pull automatically but may timeout on first run; retry the script.
- **Frontend dev server not starting via `npx`**: `npx next dev` can prompt to install `next` from the global cache, which may fail or hang in background processes. Use `node_modules/.bin/next dev` directly or `npx --no-install next dev` for reliable background startup.
- **ES credentials**: All ES requests need `-u elastic:morphis_es_pass`.
- **RLS blocking all data**: The materials table has row_filters on `tenant_id`. If `user_permissions` is missing an entry for the current user, the subquery RLS returns no rows and every list query shows empty. Re-insert with: `INSERT INTO user_permissions (user_id, tenant_id, region) VALUES ('admin', 'default', 'main') ON CONFLICT DO NOTHING;`

## 2. Frontend integration tests (Playwright)

Three test files in `frontend/tests/`:

| File | Auth mode | What it tests |
|---|---|---|
| `crud.spec.ts` | Credentials (`AUTH_DISABLED=true`) | Create → view → edit → delete a material |
| `entities.spec.ts` | Any | Home page entity picker, materials list navigation |
| `auth-chain.spec.ts` | Keycloak OIDC | Full Keycloak login → frontend → auth-proxy → API |

### Credentials mode (crud + entities)

Start the frontend with credentials auth and run the tests:

```bash
# Kill stale process
kill -9 $(ss -tlnp | grep 3000 | grep -oP 'pid=\K\d+') 2>/dev/null; sleep 1

# Start frontend (use setsid to survive shell exit)
cd frontend && \
  AUTH_SECRET=dev-secret-do-not-use-in-prod \
  AUTH_ADMIN_USERNAME=admin \
  AUTH_ADMIN_PASSWORD=admin \
  AUTH_DISABLED=true \
  GRAPHQL_URL=http://localhost:9080 \
  NEXT_PUBLIC_GRAPHQL_URL=http://localhost:3000/api/graphql \
  AUTH_PROXY_JWT_SECRET=test-secret-key-for-integration-tests \
  setsid node_modules/.bin/next dev --port 3000 > /tmp/nextdev.log 2>&1 &

# Wait for it
for i in $(seq 1 30); do
  code=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:3000/login 2>/dev/null || echo "000")
  if [ "$code" = "200" ]; then break; fi
  sleep 2
done

# Run tests
cd frontend && npx playwright test tests/crud.spec.ts tests/entities.spec.ts --workers=1
```

The CRUD test logs in with `admin`/`admin` via the credentials form, then creates a material, verifies it appears in the list, edits it, and deletes it. Delete goes through `/api/graphql` proxy (not direct `localhost:4000`).

### OIDC mode (auth-chain)

Uses Keycloak OIDC login → frontend proxy → auth-proxy → API chain.

**Script** (recommended — handles env vars and cleanup):
```bash
bash scripts/run-frontend-tests.sh
```

⚠ **Caveat:** the script can time out in some environments (background process + wait interaction). If it hangs, use the manual approach below.

**Manual** (for debugging — critical to run as a single command):
```bash
# Kill stale process first
kill -9 $(ss -tlnp | grep 3000 | grep -oP 'pid=\K\d+') 2>/dev/null; sleep 2

# Start frontend with nohup (so it survives shell exit)
cd frontend && source ../scripts/.env.frontend && \
  nohup node_modules/.bin/next dev --port 3000 > /tmp/nextdev.log 2>&1 &
for i in $(seq 1 30); do
  code=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:3000/login 2>/dev/null || echo "000")
  if [ "$code" = "200" ]; then break; fi
  sleep 1
done

# Run tests — MUST source env vars in same shell as playwright
source ../scripts/.env.frontend && \
  cd frontend && npx playwright test tests/auth-chain.spec.ts --workers=1
```

### Common failure patterns

- **Env vars not propagating**: `source scripts/.env.frontend` must be done in the SAME shell that runs `npx playwright test`. Setting them in a parent shell or separate command will NOT work.
- **Frontend dies before tests run**: Always use `setsid` (preferred) or `nohup` when starting `npx next dev` in the background. Without it, the process is killed when the parent shell exits.
- **Stale process on port 3000**: The dev server or a previous test run can leave a zombie bound to port 3000. Kill with `kill -9` and verify with `ss -tlnp | grep 3000` before retrying.
- **`scripts/run-frontend-tests.sh` hangs**: Kill any stale processes on port 3000 first, then retry the script.
- **RLS returns empty list**: After hurl tests run, `row_filters.hurl` cleans up ALL `user_permissions` rows. The entrypoint re-seeds `admin` after tests finish, but if running frontend tests independently, ensure `admin` exists: `INSERT INTO user_permissions ... VALUES ('admin', 'default', 'main')`.

### Key env vars

| Var | OIDC mode | Credentials mode |
|---|---|---|
| `AUTH_SECRET` | from `.env.frontend` | `dev-secret-do-not-use-in-prod` |
| `AUTH_ADMIN_USERNAME` | — | `admin` |
| `AUTH_ADMIN_PASSWORD` | — | `admin` |
| `AUTH_OIDC_ISSUER` | `http://localhost:8080/realms/morphis` | — |
| `AUTH_OIDC_CLIENT_ID` | `morphis-test` | — |
| `AUTH_OIDC_CLIENT_SECRET` | `morphis-test-secret` | — |
| `AUTH_DISABLED` | — | `true` |
| `GRAPHQL_URL` | `http://localhost:9080` | `http://localhost:9080` |
| `AUTH_PROXY_JWT_SECRET` | `test-secret-key-for-integration-tests` | `test-secret-key-for-integration-tests` |

## 3. opencode MCP connection test

Verify the MCP server is reachable through the auth-proxy with JWT authentication:
```bash
opencode mcp list
opencode run 'find materials with status active'
```

The MCP URL is `http://localhost:9080/mcp` (through auth-proxy, which skips JWT for `/mcp` paths). The morphis app itself validates JWT on the MCP endpoint via `mcp.auth.enabled: true` in config.

## Auth-proxy

The auth-proxy (`auth-proxy/src/main.rs`) validates JWTs in two modes:

1. **JWKS RS256** — validates against Keycloak's JWKS endpoint (`jwt_jwks_url`). Used for OIDC logins.
2. **HS256 fallback** — validates against shared secret (`jwt_secret`). Used for credentials-based logins (AUTH_DISABLED=true).

The proxy tries RS256 (JWKS) first; if RS256 fails, it falls back to HS256. The shared secret is `test-secret-key-for-integration-tests` configured in `auth-proxy/config.docker.yaml`.

When neither validation succeeds, the proxy returns 401. On success, it maps JWT claims (`sub`, `tenant_id`, `role`) to upstream request headers (`X-User-ID`, `X-Tenant-ID`, `X-Role`).

Rebuild after changing auth-proxy source or config:
```bash
cargo build --release -p auth-proxy
./build-images.sh auth
docker compose up -d auth-proxy
```

## Quick reference

| Command | What it does | When to use |
|---|---|---|
| `./scripts/run-all.sh` | Full suite from zero (build → backend → frontend) | After any changes |
| `./scripts/run-all.sh --no-playwright` | Backend only, skip frontend | After backend-only changes |
| `./scripts/run-all.sh --skip-build` | Reuse existing images | Quick iteration without rebuilding |
| `./scripts/integration-check.sh` | Backend only (build → start → seed → hurl) | Debugging backend failures |
| `./scripts/integration-check.sh --skip-build` | Backend only, reuse images | After hurl/seed changes only |
| `./build-images.sh tests && docker compose run --rm tests` | Backend hurl tests (manual) | After changing `.hurl` files or seed data |
| `bash scripts/run-frontend-tests.sh` | Frontend Playwright OIDC tests | After changing frontend or auth-proxy |
| `cd frontend && npx playwright test tests/crud.spec.ts tests/entities.spec.ts --workers=1` | Frontend credentials-mode tests | After changing CRUD workflow or entities |
| `cargo build --release -p auth-proxy && ./build-images.sh auth && docker compose up -d auth-proxy` | Rebuild + restart auth-proxy | After changing auth-proxy source or config |
| `opencode run '...'` | MCP tool tests | After changing MCP config or auth |
