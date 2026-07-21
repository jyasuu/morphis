#!/usr/bin/env python3
"""Benchmark suite for morphis GraphQL API.

Measures performance of:
  1. GraphQL queries (single, list, filtered, paginated, nested relations)
  2. Search queries (full-text, filtered, scrolled)
  3. pgx LISTEN pipeline latency (INSERT -> pg_notify -> pg_x -> ES)

Environment variables:
  GRAPHQL_URL   - GraphQL endpoint (default: http://app:4000/graphql)
  ES_HOST       - Elasticsearch host   (default: es)
  ES_PORT       - Elasticsearch port   (default: 9200)
  ES_USER       - ES username          (default: elastic)
  ES_PASS       - ES password          (default: morphis_es_pass)
  DB_URL        - PostgreSQL DSN       (default: postgres://postgres:postgres@db:5432/morphis)
  ITERATIONS    - requests per benchmark (default: 50)
  WARMUP        - warmup iterations    (default: 5)
  DATA_SIZE     - number of materials to generate (default: 500)
  BM_PREFIX     - benchmark material prefix (default: BM-)
"""

import base64
import json
import os
import statistics
import subprocess
import sys
import time
import urllib.request
import urllib.error
from typing import Any

GRAPHQL_URL = os.getenv("GRAPHQL_URL", "http://app:4000/graphql")
ES_HOST = os.getenv("ES_HOST", "es")
ES_PORT = int(os.getenv("ES_PORT", "9200"))
ES_USER = os.getenv("ES_USER", "elastic")
ES_PASS = os.getenv("ES_PASS", "morphis_es_pass")
ES_BASE_URL = f"http://{ES_HOST}:{ES_PORT}"
DB_URL = os.getenv("DB_URL", "postgres://postgres:postgres@db:5432/morphis")
ITERATIONS = int(os.getenv("ITERATIONS", "50"))
WARMUP = int(os.getenv("WARMUP", "5"))
DATA_SIZE = int(os.getenv("DATA_SIZE", "5000"))
BM_PREFIX = os.getenv("BM_PREFIX", "BM-")


def log(msg: str) -> None:
    print(f"[benchmark] {msg}")


def es_request(path: str, method: str = "GET", body: dict | None = None, timeout: int = 10) -> Any:
    """Make an authenticated request to Elasticsearch."""
    url = f"{ES_BASE_URL}/{path.lstrip('/')}"
    creds = f"{ES_USER}:{ES_PASS}"
    auth = base64.b64encode(creds.encode()).decode()
    data = json.dumps(body).encode() if body else None
    req = urllib.request.Request(
        url,
        data=data,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Basic {auth}",
        },
        method=method,
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        raise RuntimeError(f"ES error ({e.code}): {body}")
    except Exception as e:
        raise RuntimeError(f"ES request failed: {e}")


def gql(query: str, variables: dict | None = None) -> dict:
    payload = {"query": query}
    if variables:
        payload["variables"] = variables
    req = urllib.request.Request(
        GRAPHQL_URL,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        raise RuntimeError(f"GraphQL error ({e.code}): {body}")
    except Exception as e:
        raise RuntimeError(f"GraphQL request failed: {e}")


def es_get(doc_id: str) -> dict | None:
    try:
        return es_request(f"materials/_doc/{doc_id}", timeout=10)
    except RuntimeError as e:
        if "404" in str(e):
            return None
        raise


def es_refresh() -> None:
    try:
        es_request("materials/_refresh", method="POST", timeout=10)
    except Exception:
        pass


def es_count() -> int:
    try:
        result = es_request("materials/_count", timeout=10)
        return result.get("count", 0)
    except Exception:
        return 0


def es_index_doc(doc_id: str, doc: dict) -> None:
    es_request(f"materials/_doc/{doc_id}", method="PUT", body=doc, timeout=10)


def run_sql(sql: str) -> str:
    """Execute a single SQL command via psql."""
    env = os.environ.copy()
    env["PGPASSWORD"] = DB_URL.split(":")[2].split("@")[0]
    args = ["psql", DB_URL, "-t", "-A", "-c", sql]
    try:
        result = subprocess.run(
            args, capture_output=True, text=True, timeout=120, env=env
        )
        if result.returncode != 0:
            log(f"SQL warning: {result.stderr}")
        return result.stdout.strip()
    except Exception as e:
        raise RuntimeError(f"SQL execution failed: {e}")


def run_sql_file(filepath: str, var: str | None = None) -> str:
    """Execute a SQL file via psql."""
    env = os.environ.copy()
    env["PGPASSWORD"] = DB_URL.split(":")[2].split("@")[0]
    args = ["psql", DB_URL, "-t", "-A", "-f", filepath]
    if var:
        args.extend(["-v", var])
    try:
        result = subprocess.run(
            args, capture_output=True, text=True, timeout=600, env=env
        )
        if result.returncode != 0:
            log(f"SQL file warning: {result.stderr}")
        return result.stdout.strip()
    except Exception as e:
        raise RuntimeError(f"SQL file execution failed: {e}")


def get_benchmark_mat_no(i: int) -> str:
    return f"{BM_PREFIX}{i:07d}"


def first_benchmark_mat() -> str:
    return get_benchmark_mat_no(1)


# ---------------------------------------------------------------------------
# Benchmark helpers
# ---------------------------------------------------------------------------


class BenchmarkResult:
    def __init__(self, name: str):
        self.name = name
        self.times: list[float] = []

    def record(self, elapsed: float) -> None:
        self.times.append(elapsed)

    def summary(self) -> str:
        if not self.times:
            return f"  {self.name}: NO DATA"
        n = len(self.times)
        total = sum(self.times)
        avg = total / n
        mn = min(self.times)
        mx = max(self.times)
        rps = n / total if total > 0 else 0
        med = statistics.median(self.times)
        p99 = sorted(self.times)[int(n * 0.99) - 1] if n >= 100 else mx
        return (
            f"  {self.name:40s}  "
            f"n={n:4d}  total={total:6.3f}s  avg={avg*1000:8.2f}ms  "
            f"med={med*1000:8.2f}ms  p99={p99*1000:8.2f}ms  "
            f"min={mn*1000:8.2f}ms  max={mx*1000:8.2f}ms  "
            f"rps={rps:7.1f}"
        )


def run_benchmark(name: str, fn, iterations: int = ITERATIONS, warmup: int = WARMUP):
    result = BenchmarkResult(name)
    log(f"  warming up {name} ({warmup} iterations)...")
    for _ in range(warmup):
        try:
            fn()
        except Exception as e:
            log(f"  warmup error: {e}")
    log(f"  benchmarking {name} ({iterations} iterations)...")
    successes = 0
    for _ in range(iterations):
        start = time.monotonic()
        try:
            fn()
            elapsed = time.monotonic() - start
            result.record(elapsed)
            successes += 1
        except Exception as e:
            log(f"  error: {e}")
    log(f"  {name}: {successes}/{iterations} successful")
    return result


# ---------------------------------------------------------------------------
# GraphQL benchmarks
# ---------------------------------------------------------------------------


def bench_single_by_pk():
    """Query single material by primary key."""
    mat_no = first_benchmark_mat()
    q = '{ materials(id: "' + mat_no + '") { mat_no name status tenant_id } }'
    gql(q)


def bench_list_limit():
    """List materials with limit."""
    gql("{ materialsList(limit: 100) { mat_no name status } }")


def bench_list_filtered():
    """List materials with status filter."""
    gql('{ materialsList(filter: { status: "active" }, limit: 100) { mat_no name status } }')


def bench_list_paginated():
    """List materials with offset pagination."""
    offset = max(1, DATA_SIZE // 2)
    gql(f'{{ materialsList(limit: 100, offset: {offset}) {{ mat_no name status }} }}')


def bench_list_with_relations():
    """List materials with nested sizes."""
    gql("{ materialsList(limit: 50) { mat_no name sizes { size_code name } } }")


def bench_list_deep_nested():
    """List materials with deep nesting (sizes, colorways, features with attributes)."""
    gql("""{ materialsList(limit: 20) {
      mat_no name
      sizes { size_code name }
      colorways { colorway_code name hex }
      material_features {
        feature_name description
        feature_attributes { attr_name attr_value }
      }
    } }""")


def bench_single_with_relations():
    """Single material with all nested relations."""
    mat_no = first_benchmark_mat()
    q = """{ materials(id: "%s") {
      mat_no name status tenant_id
      sizes { size_code name }
      colorways { colorway_code name hex }
      material_features {
        feature_name description
        feature_attributes { attr_name attr_value }
      }
    } }""" % mat_no
    gql(q)


def bench_create_material():
    """Create a single material via mutation."""
    ts = str(time.time_ns())
    q = 'mutation { createMaterials(input: { mat_no: "BMT-' + ts + '", name: "Bench Temp ' + ts + '", status: "active" }) { mat_no name } }'
    gql(q)


def run_graphql_benchmarks() -> list[BenchmarkResult]:
    log("--- GraphQL Query Benchmarks ---")
    results = []
    results.append(run_benchmark("single_by_pk", bench_single_by_pk))
    results.append(run_benchmark("list_limit_100", bench_list_limit))
    results.append(run_benchmark("list_filtered", bench_list_filtered))
    results.append(run_benchmark("list_paginated", bench_list_paginated))
    results.append(run_benchmark("list_with_sizes", bench_list_with_relations))
    results.append(run_benchmark("list_deep_nested", bench_list_deep_nested))
    results.append(run_benchmark("single_with_all_nested", bench_single_with_relations))
    results.append(run_benchmark("create_material", bench_create_material, iterations=min(ITERATIONS, 20)))
    return results


# ---------------------------------------------------------------------------
# Search benchmarks
# ---------------------------------------------------------------------------


def bench_search_all():
    """Search all materials (empty query = match_all)."""
    gql("{ searchMaterials(query: \"\") { mat_no name } }")


def bench_search_term():
    """Search with a specific term."""
    gql('{ searchMaterials(query: "Premium") { mat_no name status } }')


def bench_search_filtered():
    """Search with filter."""
    gql('{ searchMaterials(filter: { status: "active" }) { mat_no name status } }')


def bench_search_filtered_query():
    """Search with both query and filter."""
    gql('{ searchMaterials(query: "Benchmark", filter: { status: "active" }, limit: 50) { mat_no name status } }')


def bench_search_paginated():
    """Search with pagination."""
    offset = max(1, DATA_SIZE // 3)
    gql(f'{{ searchMaterials(query: "Benchmark", limit: 100, offset: {offset}) {{ mat_no name }} }}')


def bench_search_nested():
    """Search across nested fields."""
    gql('{ searchMaterials(query: "Construction") { mat_no name material_features { feature_name } } }')


def run_search_benchmarks() -> list[BenchmarkResult]:
    log("--- Search Benchmarks ---")
    results = []
    results.append(run_benchmark("search_all", bench_search_all, iterations=min(ITERATIONS, 20)))
    results.append(run_benchmark("search_term", bench_search_term))
    results.append(run_benchmark("search_filtered", bench_search_filtered))
    results.append(run_benchmark("search_query+filter", bench_search_filtered_query))
    results.append(run_benchmark("search_paginated", bench_search_paginated))
    results.append(run_benchmark("search_nested", bench_search_nested))
    return results


# ---------------------------------------------------------------------------
# pgx LISTEN pipeline benchmark
# ---------------------------------------------------------------------------


def bench_pgx_latency_single() -> BenchmarkResult:
    """Measure time from INSERT (via GraphQL mutation) to document visible in ES.

    Uses the pg_notify trigger + pg_x pipeline to sync from PG -> ES.
    """
    log("--- pgx LISTEN Pipeline Benchmark ---")
    result = BenchmarkResult("pgx_insert_to_es")
    es_total_before = es_count()
    log(f"  ES document count before: {es_total_before}")

    sampler_iters = min(ITERATIONS, 20)
    log(f"  benchmarking single-insert latency ({sampler_iters} samples)...")

    successes = 0
    for i in range(sampler_iters):
        ts = str(time.time_ns())
        mat_no = f"PGX-{ts[-6:]}"
        name = f"PGX Latency Test {ts}"

        start = time.monotonic()
        # 1. INSERT via GraphQL mutation
        q = 'mutation { createMaterials(input: { mat_no: "' + mat_no + '", name: "' + name + '", status: "active" }) { mat_no name } }'
        try:
            resp = gql(q)
            if resp.get("errors"):
                log(f"  mutation error at iteration {i}: {resp['errors']}")
                continue
        except Exception as e:
            log(f"  mutation failed at iteration {i}: {e}")
            continue

        # 2. Poll ES until document appears (timeout after 30s)
        poll_start = time.monotonic()
        found = False
        while time.monotonic() - poll_start < 30:
            doc = es_get(mat_no)
            if doc and doc.get("found"):
                elapsed = time.monotonic() - start
                result.record(elapsed)
                successes += 1
                found = True
                break
            time.sleep(0.05)  # 50ms poll interval
        if not found:
            log(f"  TIMEOUT waiting for {mat_no} in ES (30s)")
            # Cleanup
            try:
                gql('mutation { deleteMaterials(id: "' + mat_no + '") { mat_no } }')
            except Exception:
                pass

    es_total_after = es_count()
    log(f"  ES document count after: {es_total_after} (+{es_total_after - es_total_before})")
    log(f"  pgx pipeline: {successes}/{sampler_iters} successful")
    return result


def bench_pgx_throughput() -> BenchmarkResult:
    """Measure throughput: insert N materials, measure total time for all to appear in ES."""
    log("--- pgx Throughput Benchmark ---")
    result = BenchmarkResult("pgx_throughput_100")
    batch_size = 100

    # Insert batch_size materials
    start = time.monotonic()
    mat_nos = []
    for i in range(batch_size):
        ts = str(time.time_ns()) + str(i)
        mat_no = f"PGXT-{ts[-8:]}"
        mat_nos.append(mat_no)
        q = 'mutation { createMaterials(input: { mat_no: "' + mat_no + '", name: "PGX Throughput ' + ts + '", status: "active" }) { mat_no } }'
        try:
            gql(q)
        except Exception as e:
            log(f"  insert error at {i}: {e}")

    # Refresh ES index
    es_refresh()

    # Wait for all to appear in ES
    deadline = time.monotonic() + 60
    remaining = set(mat_nos)
    while remaining and time.monotonic() < deadline:
        for mat_no in list(remaining):
            doc = es_get(mat_no)
            if doc and doc.get("found"):
                remaining.remove(mat_no)
        if remaining:
            time.sleep(0.5)

    total_elapsed = time.monotonic() - start
    found_count = batch_size - len(remaining)
    if found_count > 0:
        per_item = total_elapsed / found_count
        result.record(per_item)  # store avg per-item latency
    log(f"  pgx throughput: {found_count}/{batch_size} indexed in {total_elapsed:.3f}s "
        f"({found_count / total_elapsed:.1f} docs/sec)" if total_elapsed > 0 else "  N/A")
    return result


def bench_es_direct_refresh() -> BenchmarkResult:
    """Measure direct ES index refresh latency after SQL INSERT + ES reindex."""
    log("--- ES Direct Index Benchmark ---")
    result = BenchmarkResult("es_direct_refresh")
    ts = str(time.time_ns())
    mat_no = f"DIRECT-{ts[-8:]}"

    # Direct SQL insert (bypasses GraphQL)
    run_sql(f"INSERT INTO materials (mat_no, name, status) VALUES ('{mat_no}', 'Direct Test {ts}', 'active')")

    # Reindex directly via ES API
    start = time.monotonic()
    es_doc = {
        "mat_no": mat_no,
        "name": f"Direct Test {ts}",
        "status": "active",
        "material_features": [],
        "feature_attributes": [],
    }
    es_index_doc(mat_no, es_doc)
    es_refresh()
    elapsed = time.monotonic() - start
    result.record(elapsed)
    log(f"  Direct ES index: {elapsed*1000:.1f}ms")
    return result


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def print_divider(title: str):
    print()
    print("=" * 80)
    print(f"  {title}")
    print("=" * 80)


def main():
    only_pgx = "--only-pgx" in sys.argv
    skip_generate = "--skip-generate" in sys.argv
    include_throughput = "--with-throughput" in sys.argv or "--all" in sys.argv

    if not skip_generate and not only_pgx:
        print_divider("Data Generation")
        log(f"Generating {DATA_SIZE} benchmark materials...")
        result = run_sql_file("/benchmark/generate_data.sql", f"num={DATA_SIZE}")
        for line in result.split("\n"):
            if line.strip():
                log(f"  {line.strip()}")
        log("Data generation complete.")

    all_results: list[BenchmarkResult] = []

    if not only_pgx:
        print_divider("Benchmark Configuration")
        log(f"  GraphQL endpoint: {GRAPHQL_URL}")
        log(f"  ES endpoint:      {ES_BASE_URL}")
        log(f"  Iterations:       {ITERATIONS}")
        log(f"  Warmup:           {WARMUP}")
        log(f"  Data size:        {DATA_SIZE} materials")
        print()

        all_results.extend(run_graphql_benchmarks())
        all_results.extend(run_search_benchmarks())

    print_divider("pgx LISTEN Pipeline Benchmarks")
    all_results.append(bench_pgx_latency_single())

    if include_throughput:
        all_results.append(bench_pgx_throughput())
        all_results.append(bench_es_direct_refresh())

    print_divider("Summary")
    print(f"{'Benchmark':40s}  {'n':>4s}  {'total':>8s}  {'avg':>8s}  {'med':>8s}  {'p99':>8s}  {'min':>8s}  {'max':>8s}  {'rps':>7s}")
    print("-" * 110)
    for r in all_results:
        print(r.summary())

    print_divider("Done")
    log("Benchmark complete.")


if __name__ == "__main__":
    main()
