<!-- BEGIN:nextjs-agent-rules -->
# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` before writing any code. Heed deprecation notices.
<!-- END:nextjs-agent-rules -->

## Dev

Run in cloud IDE (CodeSandbox, etc.) — polling mode avoids WebSocket HMR issues:
```bash
AUTH_SECRET=dev-secret-do-not-use-in-prod AUTH_DISABLED=true NEXT_HMR_POLL_INTERVAL=2000 npx next dev --port 3000
```

For production, set `AUTH_SECRET`, `AUTH_ADMIN_PASSWORD`, and optionally
`AUTH_GOOGLE_ID`/`AUTH_GOOGLE_SECRET` + `AUTH_GITHUB_ID`/`AUTH_GITHUB_SECRET`.

### Generic OIDC/OAuth2 (Keycloak, Azure AD, Okta, Auth0, etc.)

Set `AUTH_OIDC_ISSUER`, `AUTH_OIDC_CLIENT_ID`, `AUTH_OIDC_CLIENT_SECRET` to enable.
Optionally set `AUTH_OIDC_NAME` (default: "SSO") for the button label.
For the login page button visibility, also set `NEXT_PUBLIC_AUTH_OIDC_NAME` to the same value.

### Integration test: frontend → auth-proxy → morphis

Run the full chain through auth-proxy (JWT validation):

```bash
# 1. Start Docker services
docker compose up -d db es app auth-proxy

# 2. Seed auth tables (first time only)
docker compose exec -T db psql -U postgres -d morphis -c "
CREATE TABLE IF NOT EXISTS user_permissions (
  id SERIAL PRIMARY KEY, user_id VARCHAR(100) NOT NULL,
  tenant_id VARCHAR(100) NOT NULL, region VARCHAR(100) NOT NULL
);
CREATE TABLE IF NOT EXISTS protected_data (
  id VARCHAR(100) PRIMARY KEY, name VARCHAR(100) NOT NULL,
  value VARCHAR(100), region VARCHAR(100)
);
INSERT INTO user_permissions (user_id, tenant_id, region) VALUES
  ('admin', 'default', 'main') ON CONFLICT DO NOTHING;
INSERT INTO protected_data (id, name, value, region) VALUES
  ('PD001', 'Test Secret', 'sensitive-value', 'main') ON CONFLICT DO NOTHING;
ALTER TABLE materials ADD COLUMN IF NOT EXISTS tenant_id VARCHAR(100);
UPDATE materials SET tenant_id = 'default' WHERE tenant_id IS NULL;
"

# 3. Seed ES
bash seed_es.sh

# 4. Start frontend pointing to auth-proxy
AUTH_SECRET=dev-secret-do-not-use-in-prod AUTH_DISABLED=true \
  GRAPHQL_URL=http://localhost:9080 \
  AUTH_PROXY_JWT_SECRET=test-secret-key-for-integration-tests \
  npx next dev --port 3000

# 5. In another terminal, run tests
AUTH_SECRET=dev-secret-do-not-use-in-prod AUTH_DISABLED=true \
  GRAPHQL_URL=http://localhost:9080 \
  AUTH_PROXY_JWT_SECRET=test-secret-key-for-integration-tests \
  npx playwright test --workers 1
```

## Runtime i18n

Locale files live in `public/locales/` and are fetched at runtime by the browser — no rebuild needed.

### File structure
```
public/locales/
  index.json   # Available locales (deployer edits to add/remove)
  en.json      # English translations
  zh-TW.json   # Traditional Chinese
  ...          # Deployer adds more
```

### Adding a new locale
1. Add entry to `public/locales/index.json`:
   ```json
   { "code": "ja", "label": "日本語" }
   ```
2. Create `public/locales/ja.json` with translations (copy existing locale as template)
3. Done — no build step required

### Translation key structure
- UI strings use dotted paths: `list.search`, `table.actions`, `form.create`
- Entity names under `entity.*`: `entity.materials`, `entity.colorways`
- Field names under `field.<entity>.*`: `field.materials.mat_no`
- Unrecognized entities fall back to `name.replace(/_/g, " ")`
- Unrecognized fields fall back to raw field name

### Fallback behavior
If a locale file fails to load (network error, missing file), the bundled `messages/en.json` is used as fallback. This also covers SSR — initial render always shows English until the client fetches the preferred locale.
