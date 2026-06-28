#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
FAIL=0

for f in "$DIR"/*.hurl; do
  name="$(basename "$f")"
  echo "=== $name ==="
  if hurl --test "$f"; then
    echo "  PASS"
  else
    echo "  FAIL"
    FAIL=1
  fi
  echo
done

if [ "$FAIL" -eq 0 ]; then
  echo "All tests passed!"
else
  echo "Some tests failed!"
  exit 1
fi
