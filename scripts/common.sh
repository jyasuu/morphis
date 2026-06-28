# Common helpers for integration scripts.
# Usage: source "$(dirname "$0")/common.sh"

# Wait for an HTTP endpoint to return a specific status code.
# Usage: wait_for_http <name> <url> [expected_code=200] [max_retries=30] [sleep_secs=2]
wait_for_http() {
  local name="$1"
  local url="$2"
  local expected="${3:-200}"
  local max="${4:-30}"
  local sleep="${5:-2}"
  echo "Waiting for $name ($url) ..."
  for i in $(seq 1 "$max"); do
    local code
    code=$(curl -s -o /dev/null -w "%{http_code}" "$url" 2>/dev/null || echo "000")
    if [ "$code" = "$expected" ]; then
      echo "  $name ready (HTTP $code)"
      return 0
    fi
    sleep "$sleep"
  done
  echo "  ERROR: $name not ready after $((max * sleep))s (last HTTP $code)" >&2
  return 1
}

# Check a command's exit status with a descriptive message.
# Usage: check_step <name> <command...>
check_step() {
  local name="$1"
  shift
  echo "=== $name ==="
  if "$@"; then
    echo "  OK"
    return 0
  else
    local rc=$?
    echo "  FAILED (exit $rc)" >&2
    return "$rc"
  fi
}
