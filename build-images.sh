#!/usr/bin/env bash
set -euo pipefail

# Builds all Docker images for the integration test suite.
# Uses temporary build directories to avoid sending the 5.3GB target/ as context.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TMP_DIR=$(mktemp -d)
trap "rm -rf '$TMP_DIR'" EXIT

echo "=== Checking binaries ==="
for bin in target/release/morphis target/release/auth-proxy; do
  if [ ! -f "$SCRIPT_DIR/$bin" ]; then
    echo "ERROR: missing $bin — run 'cargo build --release -p morphis && cargo build --release -p auth-proxy' first"
    exit 1
  fi
done

echo ""
echo "=== Building morphis:local ==="
mkdir -p "$TMP_DIR/morphis"
cp "$SCRIPT_DIR/target/release/morphis" "$TMP_DIR/morphis/"
cat > "$TMP_DIR/morphis/Dockerfile" << 'DOCKERFILE'
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libpq5 libssl3 && rm -rf /var/lib/apt/lists/*
COPY morphis /usr/local/bin/morphis
WORKDIR /app
EXPOSE 4000
CMD ["morphis"]
DOCKERFILE
docker build -t morphis:local "$TMP_DIR/morphis"

echo ""
echo "=== Building auth-proxy:local ==="
mkdir -p "$TMP_DIR/auth-proxy"
cp "$SCRIPT_DIR/target/release/auth-proxy" "$TMP_DIR/auth-proxy/"
cat > "$TMP_DIR/auth-proxy/Dockerfile" << 'DOCKERFILE'
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY auth-proxy /usr/local/bin/auth-proxy
WORKDIR /app
EXPOSE 9080
CMD ["auth-proxy"]
DOCKERFILE
docker build -t auth-proxy:local "$TMP_DIR/auth-proxy"

echo ""
echo "=== Building tests:local ==="
mkdir -p "$TMP_DIR/tests"
cp "$SCRIPT_DIR/seed_es.sh" "$TMP_DIR/tests/"
cp -r "$SCRIPT_DIR/tests/"* "$TMP_DIR/tests/"
cat > "$TMP_DIR/tests/Dockerfile" << 'DOCKERFILE'
FROM ghcr.io/orange-opensource/hurl:latest
USER root
RUN apk add --no-cache curl bash jq python3 postgresql-client
COPY . /tests/
RUN chmod +x /tests/docker-entrypoint.sh
ENTRYPOINT ["/tests/docker-entrypoint.sh"]
DOCKERFILE
docker build -t tests:local "$TMP_DIR/tests"

echo ""
echo "=== All images built ==="
docker images --format "table {{.Repository}}\t{{.Tag}}\t{{.Size}}" | grep local || true
