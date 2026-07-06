#!/bin/sh
set -e

echo "============================================"
echo "  morphis Benchmark Suite"
echo "============================================"
echo ""

echo "=== Waiting for app health ==="
until curl -sf http://app:4000/health > /dev/null 2>&1; do
  echo "  waiting for app:4000 ..."
  sleep 2
done
echo "  app is ready"

echo ""
echo "=== Waiting for Elasticsearch ==="
until curl -sf -u elastic:morphis_es_pass http://es:9200 > /dev/null 2>&1; do
  echo "  waiting for es:9200 ..."
  sleep 3
done
echo "  ES is ready"

echo ""
echo "=== Checking pgx-listen pipeline ==="
# Verify that pgx-listen is connected and the materials index exists
ES_DOC_COUNT=$(curl -s -u elastic:morphis_es_pass "http://es:9200/materials/_count" | python3 -c "import json,sys; print(json.load(sys.stdin).get('count', 0))" 2>/dev/null || echo "0")
echo "  Current ES materials index doc count: $ES_DOC_COUNT"

echo ""
echo "=== Environment ==="
echo "  DATA_SIZE:      ${DATA_SIZE:-500} materials"
echo "  ITERATIONS:     ${ITERATIONS:-30} per benchmark"
echo "  WARMUP:         ${WARMUP:-3}"
echo "  GRAPHQL_URL:    http://app:4000/graphql"
echo "  ES_HOST:        ${ES_HOST:-es}:${ES_PORT:-9200}"
echo ""

echo "=== Running benchmarks ==="
cd /benchmark
echo "  Args: $*"
python3 /benchmark/benchmark.py "$@"

echo ""
echo "=== Benchmark suite complete ==="
