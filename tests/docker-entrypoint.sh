#!/bin/sh
set -e

echo "=== Waiting for app health ==="
until curl -sf http://app:4000/health > /dev/null 2>&1; do
  echo "  waiting for app:4000 ..."
  sleep 2
done
echo "  app is ready"

echo ""
echo "=== Seeding Elasticsearch ==="
ES_URL=http://es:9200 /tests/seed_es.sh

echo ""
echo "=== Cleaning up stale test data ==="
# Clean up ES test docs
curl -s -X DELETE http://es:9200/materials/_doc/ES-TEST-A > /dev/null
curl -s -X DELETE http://es:9200/materials/_doc/ES-TEST-B > /dev/null
curl -s -X POST http://es:9200/materials/_refresh > /dev/null
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
  sed -i 's|http://localhost:4000|http://app:4000|g; s|http://localhost:9200|http://es:9200|g' "$f"
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

# Run RLS tests last (they create and clean up their own data)
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

if [ "$FAIL" -eq 0 ]; then
  echo "All tests passed!"
else
  echo "Some tests failed!"
fi
exit $FAIL
