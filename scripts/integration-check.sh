#!/usr/bin/env bash
set -euo pipefail

# Full integration check from zero: builds, starts services, sets up Keycloak,
# seeds ES, runs hurl test suite.
# Prerequisites: Docker, Docker Compose, Python 3

cd "$(dirname "$0")/.."

echo "=== 1. Build images ==="
./build-images.sh
docker compose build --no-cache tests

echo "=== 2. Start services ==="
docker compose up -d db es keycloak app auth-proxy

echo "=== 3. Wait for services ==="
echo "Waiting for Keycloak..."
for i in $(seq 1 60); do
    if curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/realms/master 2>/dev/null | grep -q 200; then
        echo "Keycloak ready"
        break
    fi
    sleep 2
done

echo "Waiting for Morphis..."
for i in $(seq 1 30); do
    if curl -s -o /dev/null -w "%{http_code}" http://localhost:8081/health 2>/dev/null | grep -q 200; then
        echo "Morphis ready"
        break
    fi
    sleep 2
done

echo "=== 4. Keycloak setup ==="
python3 scripts/keycloak-setup.py

echo "=== 5. Seed ES ==="
bash seed_es.sh

echo "=== 6. Run hurl tests ==="
docker compose run --rm tests

echo "=== All checks passed ==="
