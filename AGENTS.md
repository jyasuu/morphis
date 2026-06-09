# CLAUDE.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.

---

## Project-Specific: morphis (Rust GraphQL + Pingora auth-proxy)

### Quick commands
```bash
# Build + run all integration tests (from zero: DB init, ES seed, cleanup, hurl tests)
./build-images.sh && docker compose run --rm tests

# Rust builds
cargo build --release -p morphis        # binary → target/release/morphis
cargo build --release -p auth-proxy     # binary → target/release/auth-proxy
cargo clippy --all                       # lint

# Fresh start (nuke DB+ES volumes, restart services)
docker compose down -v && docker compose up -d db es

# Rebuild a single image (after binary rebuild)
./build-images.sh app   # morphis only
./build-images.sh auth  # auth-proxy only
./build-images.sh tests # test runner only
```

### Run integration check from zero
```bash
# Full bootstrap: build → backend hurl tests → frontend Playwright (all from zero)
./scripts/run-all.sh

# Backend hurl tests only (skip Playwright for CI without browser)
./scripts/run-all.sh --no-playwright

# Reuse existing images (skip docker build)
./scripts/run-all.sh --skip-build
```

Individual scripts (fine-grained):
```bash
# Backend only (hurl tests)
./scripts/integration-check.sh
./scripts/integration-check.sh --skip-build  # reuse existing images

# Frontend only (Playwright — needs Docker services up + Keycloak setup)
./scripts/run-frontend-tests.sh
```

Manual step-by-step:
```bash
./build-images.sh
docker compose build --no-cache tests
docker compose up -d db es keycloak app auth-proxy
python3 scripts/keycloak-setup.py
bash seed_es.sh
docker compose run --rm tests
```

### Frontend Playwright integration tests
| File | What it tests |
|---|---|
| `frontend/tests/auth-chain.spec.ts` | Full Keycloak OIDC login → frontend → auth-proxy → API |
| `frontend/tests/entities.spec.ts` | Entity picker, navigation (uses AUTH_DISABLED=true) |
| `frontend/tests/crud.spec.ts` | Material CRUD via UI (uses AUTH_DISABLED=true) |

The frontend API route (`app/api/graphql/route.ts`) passes through the Keycloak JWT when an OIDC session exists (auth.ts stores `account.access_token` in `session.accessToken`). When no OIDC token is available (Credentials login or AUTH_DISABLED=true), it falls back to self-signed HS256 JWTs via `AUTH_PROXY_JWT_SECRET`.

#### Run frontend Playwright tests (full Keycloak chain)
```bash
# 1. Ensure Docker services are up with Keycloak hostname
docker compose up -d db es keycloak app auth-proxy

# 2. Keycloak setup
python3 scripts/keycloak-setup.py

# 3. Seed ES
bash seed_es.sh

# 4. Run everything (starts frontend, runs tests, cleans up)
bash scripts/run-frontend-tests.sh
```

Key files in `scripts/`:
| File | Purpose |
|---|---|
| `scripts/run-all.sh` | Master — full bootstrap from zero (backend + frontend) |
| `scripts/integration-check.sh` | Backend only: build → start → Keycloak → seed → hurl |
| `scripts/run-frontend-tests.sh` | Frontend only: start dev server → Playwright → cleanup |
| `scripts/keycloak-setup.py` | Keycloak realm/client/user/protocol-mapper setup via API |
| `scripts/common.sh` | Shared helpers (`wait_for_http`, `check_step`) |
| `scripts/.env.frontend` | Shared env vars for frontend tests |

### Keycloak JWT/auth-proxy troubleshooting
- **"Account is not fully set up"** — User must have `firstName` + `lastName` set (Keycloak 26 User Profile requirement for direct grant).
- **Custom attributes not in JWT** — Must (1) register in User Profile via `PUT /admin/realms/{realm}/users/profile`, (2) add protocol mappers on client, (3) set attributes on user. Order matters: profile first.
- **JWT validation fails** — If auth-proxy logs show `Skipping JWK` for encryption keys, that's normal. If all keys are skipped, check `require_auth` config. If validation still fails, check `aud` — Keycloak tokens include `aud: "account"` and `jsonwebtoken` enables `validate_aud: true` by default. Set `validation.validate_aud = false` in auth-proxy if not checking audience.
- **Keycloak hostname** — Set `KC_HOSTNAME_URL=http://localhost:8080` on Keycloak so OIDC discovery returns `localhost` URLs the browser can reach. Set auth-proxy `jwt_issuer: ""` to skip issuer validation when hostname differs between internal/external access.
- **Playwright needs system deps** — `apt-get install -y libnspr4 libnss3 libgbm1 libasound2` for headless Chromium.

### Key conventions
- **GraphQL naming**: Table names use the config's `name:` field as-is. `user_permissions` → `user_permissionsList`, `createUser_permissions`, `deleteUser_permissions` (underscores preserved, no camelCase).
- **has_many ORDER**: Always `ORDER BY t.<primary_key>` inside `json_agg` — needed for deterministic results.
- **Auto-increment IDs**: `DELETE` does NOT reset sequences. Use `TRUNCATE ... RESTART IDENTITY` when cleanup needs predictable IDs.
- **Docker builds**: `target/` is 5.3 GB — `build-images.sh` copies only needed binaries to temp dirs to avoid timeout.
- **Docker base**: Morphis binary is glibc-linked — must use Debian images, never Alpine.
- **Test image**: Alpine-based hurl image — `apk add` for extra packages (curl, bash, jq, python3, postgresql-client).

### Test structure
| File | What it tests |
|---|---|
| `tests/health.hurl` | App health endpoint |
| `tests/mutations.hurl` | CRUD (creates orphan `HL` size — cleaned up after) |
| `tests/queries.hurl` | List/single queries on seed data |
| `tests/relations.hurl` | has_many, belongs_to, deep nesting |
| `tests/search.hurl` | ES search queries |
| `tests/row_filters.hurl` | Column RLS + subquery RLS + RBAC |

### Test execution order (entrypoint manages side effects)
health → mutations → **cleanup** → queries → relations → search → row_filters