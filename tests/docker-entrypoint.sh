#!/bin/sh
set -e

echo "=== Waiting for app health ==="
until curl -sf http://app:4000/health > /dev/null 2>&1; do
  echo "  waiting for app:4000 ..."
  sleep 2
done
echo "  app is ready"

echo ""
echo "=== Waiting for Keycloak ==="
until curl -sf http://keycloak:8080/realms/morphis > /dev/null 2>&1; do
  echo "  waiting for keycloak:8080 ..."
  sleep 3
done
echo "  keycloak is ready"

echo ""
echo "=== Getting Keycloak token ==="
KEYCLOAK_TOKEN=$(curl -s -X POST http://keycloak:8080/realms/morphis/protocol/openid-connect/token \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  -d 'client_id=morphis-test' \
  -d 'client_secret=morphis-test-secret' \
  -d 'grant_type=password' \
  -d 'username=testuser' \
  -d 'password=testpass' | jq -r '.access_token')
if [ -z "$KEYCLOAK_TOKEN" ] || [ "$KEYCLOAK_TOKEN" = "null" ]; then
  echo "  ERROR: Failed to get Keycloak token"
  exit 1
fi
echo "  got token: ${KEYCLOAK_TOKEN:0:20}..."

echo ""
echo "=== Seeding Elasticsearch ==="
ES_URL=http://elastic:morphis_es_pass@es:9200 ES_AUTH="-u elastic:morphis_es_pass" /tests/seed_es.sh

echo ""
echo "=== Cleaning up stale test data ==="
# Clean up ES test docs
curl -s -u elastic:morphis_es_pass -X DELETE http://es:9200/materials/_doc/ES-TEST-A > /dev/null
curl -s -u elastic:morphis_es_pass -X DELETE http://es:9200/materials/_doc/ES-TEST-B > /dev/null
curl -s -u elastic:morphis_es_pass -X POST http://es:9200/materials/_refresh > /dev/null
# Clean up materials
for mat in HTEST RLTEST1 RLTEST2 DTEST ES-TEST-A ES-TEST-B; do
  curl -s -X POST http://app:4000/graphql \
    -H 'Content-Type: application/json' \
    -d "{\"query\":\"mutation { deleteMaterials(id: \\\"$mat\\\") { mat_no } }\"}" > /dev/null
done
# Clean up orphaned sizes (HL created by mutations test)
curl -s -X POST http://app:4000/graphql -H 'Content-Type: application/json' \
  -d '{"query":"{ sizesList(filter: { size_code: \"HL\" }) { id } }"}' | python3 -c "
import json,sys
d=json.load(sys.stdin)
for s in d.get('data',{}).get('sizesList',[]):
    print(s['id'])
" 2>/dev/null | while read id; do
  curl -s -X POST http://app:4000/graphql \
    -H 'Content-Type: application/json' \
    -d "{\"query\":\"mutation { deleteSizes(id: $id) { id } }\"}" > /dev/null
done
# Clean up using direct SQL to reset sequences too
PGPASSWORD=postgres psql -h db -U postgres -d morphis -c "
  TRUNCATE user_permissions, protected_data RESTART IDENTITY CASCADE;
" > /dev/null 2>&1 || true
echo "  cleanup done"

echo ""
echo "=== Adjusting test URLs for Docker ==="
cd /tests
for f in *.hurl; do
  sed -i 's|http://localhost:4000|http://app:4000|g; s|http://localhost:9200|http://es:9200|g; s|http://localhost:9080|http://auth-proxy:9080|g' "$f"
  echo "  patched $f"
done

echo ""
echo "=== Running hurl tests ==="
FAIL=0
for f in health.hurl; do
  name="$(basename "$f")"
  echo "--- $name ---"
  if hurl --test "$f"; then
    echo "  PASS"
  else
    echo "  FAIL"
    FAIL=1
  fi
  echo ""
done

# mutations creates side effects (HL size) that affect other tests.
# Clean up after it before running the rest.
for f in mutations.hurl; do
  name="$(basename "$f")"
  echo "--- $name ---"
  if hurl --test "$f"; then
    echo "  PASS"
  else
    echo "  FAIL"
    FAIL=1
  fi
  echo ""
done

echo "=== Clean up after mutations ==="
curl -s -X POST http://app:4000/graphql -H 'Content-Type: application/json' \
  -d "{\"query\":\"mutation { deleteMaterials(id: \\\"HTEST\\\") { mat_no } }\"}" > /dev/null
PGPASSWORD=postgres psql -h db -U postgres -d morphis -c "
  DELETE FROM sizes WHERE size_code = 'HL';
" > /dev/null 2>&1
echo "  done"

for f in queries.hurl relations.hurl search.hurl; do
  name="$(basename "$f")"
  echo "--- $name ---"
  if hurl --test "$f"; then
    echo "  PASS"
  else
    echo "  FAIL"
    FAIL=1
  fi
  echo ""
done

# Seed user_permissions for subquery row filter tests
PGPASSWORD=postgres psql -h db -U postgres -d morphis -c "
  INSERT INTO user_permissions (user_id, tenant_id, region) VALUES
    ('tenant-alpha', 'tenant-alpha', 'test'),
    ('tenant-beta', 'tenant-beta', 'test');
  INSERT INTO user_permissions (user_id, tenant_id, region) VALUES
    ('user-a', '-', 'us'),
    ('user-a', '-', 'eu'),
    ('user-b', '-', 'us');
  INSERT INTO protected_data (id, name, region) VALUES
    ('PDATA-001', 'Protected US', 'us'),
    ('PDATA-002', 'Protected EU', 'eu'),
    ('PDATA-003', 'Protected US 2', 'us');
" > /dev/null 2>&1

# Seed ES-TEST-A and ES-TEST-B in PG (needed for pgroonga search backend).
# With ES backend these are created directly in ES by the test; with pgroonga
# the search only queries PG, so the rows must exist here too.
PGPASSWORD=postgres psql -h db -U postgres -d morphis -c "
  INSERT INTO materials (mat_no, name, status, tenant_id) VALUES
    ('ES-TEST-A', 'ES Row Filter Alpha', 'active', 'alpha'),
    ('ES-TEST-B', 'ES Row Filter Beta', 'active', 'beta')
  ON CONFLICT (mat_no) DO UPDATE SET name = EXCLUDED.name, status = EXCLUDED.status, tenant_id = EXCLUDED.tenant_id;
" > /dev/null 2>&1

# Run RLS tests (data seeded above via SQL)
for f in row_filters.hurl; do
  name="$(basename "$f")"
  echo "--- $name ---"
  if hurl --test "$f"; then
    echo "  PASS"
  else
    echo "  FAIL"
    FAIL=1
  fi
  echo ""
done

echo "=== Auth-proxy + Keycloak tests ==="
for f in auth_proxy.hurl; do
  name="$(basename "$f")"
  echo "--- $name ---"
  if hurl --test --variable "TOKEN=$KEYCLOAK_TOKEN" "$f"; then
    echo "  PASS"
  else
    echo "  FAIL"
    FAIL=1
  fi
  echo ""
done

echo "=== Generating MCP JWT ==="
MCP_SECRET="morphis-mcp-secret-change-in-production"
MCP_TOKEN=$(python3 -c "
import hmac, hashlib, base64, json, time
secret = b'$MCP_SECRET'
payload = {'sub':'testuser','tenant_id':'default','role':'admin','exp':int(time.time())+3600}
header = base64.urlsafe_b64encode(json.dumps({'alg':'HS256','typ':'JWT'}).encode()).rstrip(b'=').decode()
payload_b64 = base64.urlsafe_b64encode(json.dumps(payload).encode()).rstrip(b'=').decode()
sig = base64.urlsafe_b64encode(hmac.new(secret, f'{header}.{payload_b64}'.encode(), hashlib.sha256).digest()).rstrip(b'=').decode()
print(f'{header}.{payload_b64}.{sig}')
")
echo "  got MCP token: ${MCP_TOKEN:0:20}..."

echo "=== MCP endpoint tests ==="
for f in mcp.hurl; do
  name="$(basename "$f")"
  echo "--- $name ---"
  if hurl --test --variable "MCP_TOKEN=$MCP_TOKEN" "$f"; then
    echo "  PASS"
  else
    echo "  FAIL"
    FAIL=1
  fi
  echo ""
done

echo ""
echo "=== Child-table write re-indexes parent material (pipeline) ==="
# A change to a child table (material_features) must fire the child trigger,
# notify the pipeline, and re-index the parent material document. This is the
# P6 contract assertion: seeded documents and pipeline documents share one shape.
PGPASSWORD=postgres psql -h db -U postgres -d morphis -c \
  "UPDATE material_features SET description = 'PIPELINE-REINDEX-PROBE' WHERE id = 8;" > /dev/null
FOUND=""
for i in $(seq 1 45); do
  FOUND=$(curl -s -X POST http://app:4000/graphql \
    -H 'Content-Type: application/json' \
    -d '{"query":"{ searchMaterials(query: \"\", filter: { material_features: { description: { eq: \"PIPELINE-REINDEX-PROBE\" } } }) { node { mat_no } } }"}' \
    | python3 -c "import json,sys; d=json.load(sys.stdin); r=d.get('data',{}).get('searchMaterials') or []; print(r[0]['node']['mat_no'] if r else '')" 2>/dev/null || true)
  if [ "$FOUND" = "M003" ]; then
    echo "  PASS: child write re-indexed parent M003"
    break
  fi
  echo "  waiting for pipeline re-index (attempt $i)..."
  sleep 2
done
if [ "$FOUND" != "M003" ]; then
  echo "  FAIL: child-table write did not re-index the parent material"
  FAIL=1
fi

echo "=== Nested child-table write re-indexes parent (feature_attributes) ==="
# The deepest level: feature_attributes rows are grouped by the numeric
# feature id, which regressed once (keys became empty, wiping children to []).
PGPASSWORD=postgres psql -h db -U postgres -d morphis -c \
  "UPDATE feature_attributes SET attr_value = 'PIPELINE-ATTR-PROBE' WHERE feature_id = 8;" > /dev/null
FOUND=""
for i in $(seq 1 45); do
  FOUND=$(curl -s -X POST http://app:4000/graphql \
    -H 'Content-Type: application/json' \
    -d '{"query":"{ searchMaterials(query: \"\", filter: { material_features: { feature_attributes: { attr_value: { eq: \"PIPELINE-ATTR-PROBE\" } } } }) { node { mat_no } } }"}' \
    | python3 -c "import json,sys; d=json.load(sys.stdin); r=d.get('data',{}).get('searchMaterials') or []; print(r[0]['node']['mat_no'] if r else '')" 2>/dev/null || true)
  if [ "$FOUND" = "M003" ]; then
    echo "  PASS: nested child write re-indexed parent M003"
    break
  fi
  echo "  waiting for nested pipeline re-index (attempt $i)..."
  sleep 2
done
if [ "$FOUND" != "M003" ]; then
  echo "  FAIL: nested child-table write did not re-index the parent material"
  FAIL=1
fi

# Clean up test data and re-seed shared state for downstream consumers (frontend tests, etc.)
PGPASSWORD=postgres psql -h db -U postgres -d morphis -c "
  TRUNCATE user_permissions, protected_data RESTART IDENTITY CASCADE;
  INSERT INTO user_permissions (user_id, tenant_id, region) VALUES
    ('admin', 'default', 'main');
" > /dev/null 2>&1

if [ "$FAIL" -eq 0 ]; then
  echo "All tests passed!"
else
  echo "Some tests failed!"
fi
exit $FAIL
