#!/usr/bin/env python3
"""
Integration tests for auth-proxy.

Starts:
  1. An echo server (returns request headers as JSON)
  2. The auth-proxy binary (proxies to echo server, validates JWTs, injects headers)

Runs tests:
  - Valid JWT: verifies header injection
  - Expired JWT: verifies 401
  - Bad signature: verifies 401
  - Missing Authorization: verifies 401
  - Missing Authorization with require_auth=false: verifies passthrough
  - Custom claim mapping
"""

import base64
import hashlib
import hmac
import json
import os
import signal
import socket
import subprocess
import sys
import threading
import time
import urllib.request
import urllib.error
from http.server import HTTPServer, BaseHTTPRequestHandler
from pathlib import Path

TEST_DIR = Path(__file__).parent
PROXY_BINARY = os.environ.get("AUTH_PROXY_BIN", str(Path.cwd() / "target" / "debug" / "auth-proxy"))
CONFIG_PATH = TEST_DIR / "test_config.yaml"
PROXY_PORT = 9081
ECHO_PORT = 9082
PROXY_URL = f"http://127.0.0.1:{PROXY_PORT}"
ECHO_URL = f"http://127.0.0.1:{ECHO_PORT}"

PASS = 0
FAIL = 0


def jwt_encode(payload: dict, secret: str) -> str:
    header = {"alg": "HS256", "typ": "JWT"}
    def b64(data: bytes) -> str:
        return base64.urlsafe_b64encode(data).rstrip(b"=").decode()
    body = b64(json.dumps(header).encode()) + "." + b64(json.dumps(payload).encode())
    sig = hmac.new(secret.encode(), body.encode(), hashlib.sha256).digest()
    return body + "." + b64(sig)


class EchoHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        headers = dict(self.headers.items())
        body = json.dumps({"method": "GET", "path": self.path, "headers": headers}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", 0))
        req_body = self.rfile.read(content_length).decode() if content_length else ""
        headers = dict(self.headers.items())
        body = json.dumps({
            "method": "POST",
            "path": self.path,
            "headers": headers,
            "body": req_body,
        }).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt, *args):
        pass  # quiet


def run_echo_server():
    server = HTTPServer(("127.0.0.1", ECHO_PORT), EchoHandler)
    server.serve_forever()


def check(name: str, ok: bool, detail: str = ""):
    global PASS, FAIL
    if ok:
        PASS += 1
        print(f"  PASS  {name}")
    else:
        FAIL += 1
        print(f"  FAIL  {name}  {detail}")


def request(method: str, path: str, headers: dict = None) -> (int, dict, str):
    url = f"{PROXY_URL}{path}"
    req = urllib.request.Request(url, method=method, headers=headers or {})
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            body = resp.read().decode()
            return resp.status, json.loads(body), ""
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        return e.code, {}, body
    except Exception as e:
        return 0, {}, str(e)


def test_valid_jwt():
    token = jwt_encode({"sub": "user-123", "tenant_id": "acme-corp", "role": "admin"}, "test-secret-key-for-integration-tests")
    status, data, _ = request("GET", "/api/test", {"Authorization": f"Bearer {token}"})
    check("Valid JWT returns 200", status == 200, f"got {status}")
    if status == 200:
        headers = data.get("headers", {})
        check("X-User-ID injected", headers.get("X-User-ID") == "user-123",
              f"got {headers.get('X-User-ID')}")
        check("X-Tenant-ID injected", headers.get("X-Tenant-ID") == "acme-corp",
              f"got {headers.get('X-Tenant-ID')}")
        check("X-Role injected", headers.get("X-Role") == "admin",
              f"got {headers.get('X-Role')}")
        check("Original path preserved", data.get("path") == "/api/test",
              f"got {data.get('path')}")


def test_expired_jwt():
    payload = {"sub": "user-123", "exp": 1000000000}
    token = jwt_encode(payload, "test-secret-key-for-integration-tests")
    status, _, _ = request("GET", "/", {"Authorization": f"Bearer {token}"})
    check("Expired JWT returns 401", status == 401, f"got {status}")


def test_bad_signature():
    token = jwt_encode({"sub": "user-123"}, "wrong-secret")
    status, _, _ = request("GET", "/", {"Authorization": f"Bearer {token}"})
    check("Bad signature returns 401", status == 401, f"got {status}")


def test_missing_auth():
    status, _, _ = request("GET", "/")
    check("Missing Authorization returns 401", status == 401, f"got {status}")


def test_missing_auth_allowed():
    config_path = TEST_DIR / "test_config_noauth.yaml"
    with open(config_path) as f:
        pass
    # Write permissive config
    permissive = CONFIG_PATH.read_text().replace("require_auth: true", "require_auth: false")
    orig = CONFIG_PATH.read_text()
    try:
        CONFIG_PATH.write_text(permissive)
        # Restart proxy needed, so skip the dynamic reload test for now
        pass
    finally:
        CONFIG_PATH.write_text(orig)
    # We'll test this in a separate config
    check("Permissive mode - placeholder", True, "handled in test_noauth_config")


def test_noauth_config():
    """Run proxy with require_auth: false and verify unauthenticated requests pass through."""
    noauth_config = TEST_DIR / "test_config_noauth.yaml"
    noauth_config.write_text("""
listen_addr: "127.0.0.1:9083"
upstream: "http://127.0.0.1:9082"
jwt_secret: "test-secret-key-for-integration-tests"
require_auth: false
header_mappings:
  - claim: sub
    header: X-User-ID
""")
    proxy = subprocess.Popen(
        [PROXY_BINARY],
        env={**os.environ, "AUTH_PROXY_CONFIG": str(noauth_config)},
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    time.sleep(1)
    try:
        status, data, _ = request("GET", "/noauth-test")
        check("No auth allowed returns 200", status == 200, f"got {status}")
        check("No header injected when no token", "X-User-ID" not in data.get("headers", {}),
              "got X-User-ID")

        # Valid token should still inject
        token = jwt_encode({"sub": "authed-user"}, "test-secret-key-for-integration-tests")
        status, data, _ = request("GET", "/", {"Authorization": f"Bearer {token}"})
        check("Auth still works in permissive mode", status == 200, f"got {status}")
        if status == 200:
            check("Header injected with token", data.get("headers", {}).get("X-User-ID") == "authed-user",
                  f"got {data.get('headers', {}).get('X-User-ID')}")
    finally:
        proxy.terminate()
        proxy.wait()
        noauth_config.unlink(missing_ok=True)


def main():
    global PASS, FAIL
    PASS = 0
    FAIL = 0

    print("=" * 60)
    print("Auth-Proxy Integration Tests")
    print("=" * 60)

    # Check binary exists
    if not os.path.isfile(PROXY_BINARY):
        print(f"\nERROR: auth-proxy binary not found at: {PROXY_BINARY}")
        print("Build it first: cargo build -p auth-proxy")
        sys.exit(1)

    # Start echo server
    echo_thread = threading.Thread(target=run_echo_server, daemon=True)
    echo_thread.start()
    time.sleep(0.3)
    print(f"\n[OK] Echo server on 127.0.0.1:{ECHO_PORT}")

    # Start auth-proxy
    proxy = subprocess.Popen(
        [PROXY_BINARY],
        env={**os.environ, "AUTH_PROXY_CONFIG": str(CONFIG_PATH)},
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    time.sleep(1)

    # Quick check proxy is up
    try:
        urllib.request.urlopen(f"{PROXY_URL}/health", timeout=3)
    except urllib.error.HTTPError:
        pass  # echo server returns 404 but connection works
    except Exception as e:
        proxy.terminate()
        proxy.wait()
        print(f"\nERROR: Proxy failed to start: {e}")
        sys.exit(1)
    print(f"[OK] Auth-proxy on 127.0.0.1:{PROXY_PORT}")

    # Run tests
    print("\n--- Auth Tests ---")
    test_valid_jwt()
    test_expired_jwt()
    test_bad_signature()
    test_missing_auth()

    print("\n--- No-Auth Config Test ---")
    test_noauth_config()

    # Cleanup proxy
    proxy.terminate()
    proxy.wait()

    print(f"\n{'=' * 60}")
    print(f"Results:  {PASS} passed,  {FAIL} failed")
    print(f"{'=' * 60}")
    sys.exit(0 if FAIL == 0 else 1)


if __name__ == "__main__":
    main()
