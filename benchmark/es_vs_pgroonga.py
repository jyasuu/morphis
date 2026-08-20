#!/usr/bin/env python3
"""Head-to-head benchmark: Elasticsearch vs PGroonga search backends.

Generates large datasets in PostgreSQL, seeds Elasticsearch with matching
documents, then runs identical search queries through both backends.

PGroonga path: GraphQL searchMaterials endpoint (SQL + pgroonga operators)
ES path:       Direct ES REST API (multi_match queries, same fields)

Environment:
  GRAPHQL_URL   - GraphQL endpoint   (default: http://app:4000/graphql)
  ES_URL        - ES base URL        (default: http://es:9200)
  DB_URL        - PostgreSQL DSN     (default: postgresql://postgres:postgres@db:5432/morphis)
  ITERATIONS    - requests per query  (default: 50)
  WARMUP        - warmup iterations   (default: 5)
  DATA_SIZE     - materials to gen    (default: 10000)
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

# ── Config ──────────────────────────────────────────────────────

GRAPHQL_URL = os.getenv("GRAPHQL_URL", "http://app:4000/graphql")
ES_URL = os.getenv("ES_URL", "http://es:9200")
DB_URL = os.getenv("DB_URL", "postgres://postgres:postgres@db:5432/morphis")
ITERATIONS = int(os.getenv("ITERATIONS", "50"))
WARMUP = int(os.getenv("WARMUP", "5"))
DATA_SIZE = int(os.getenv("DATA_SIZE", "10000"))
ES_USER = os.getenv("ES_USER", "elastic")
ES_PASS = os.getenv("ES_PASS", "morphis_es_pass")
BENCH_PREFIX = "BM-"

# ── Helpers ─────────────────────────────────────────────────────

def log(msg: str):
    print(f"[bench] {msg}", flush=True)


def gql(query: str) -> dict:
    req = urllib.request.Request(
        GRAPHQL_URL,
        data=json.dumps({"query": query}).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.loads(resp.read())


def es_request(path: str, method: str = "GET", body: dict | None = None, timeout: int = 30) -> Any:
    url = f"{ES_URL}/{path.lstrip('/')}"
    creds = base64.b64encode(f"{ES_USER}:{ES_PASS}".encode()).decode()
    data = json.dumps(body).encode() if body else None
    req = urllib.request.Request(
        url, data=data, method=method,
        headers={"Content-Type": "application/json", "Authorization": f"Basic {creds}"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read())


def run_sql(sql: str) -> str:
    env = os.environ.copy()
    env["PGPASSWORD"] = DB_URL.split(":")[2].split("@")[0]
    args = ["psql", DB_URL, "-t", "-A", "-c", sql]
    r = subprocess.run(args, capture_output=True, text=True, timeout=600, env=env)
    if r.returncode != 0:
        log(f"  SQL warning: {r.stderr.strip()[:200]}")
    return r.stdout.strip()


def run_sql_file(path: str, var: str | None = None) -> str:
    env = os.environ.copy()
    env["PGPASSWORD"] = DB_URL.split(":")[2].split("@")[0]
    args = ["psql", DB_URL, "-t", "-A", "-f", path]
    if var:
        args.extend(["-v", var])
    r = subprocess.run(args, capture_output=True, text=True, timeout=600, env=env)
    if r.returncode != 0:
        log(f"  SQL file warning: {r.stderr.strip()[:200]}")
    return r.stdout.strip()


# ── Benchmark infra ─────────────────────────────────────────────

class BenchResult:
    def __init__(self, backend: str, name: str):
        self.backend = backend
        self.name = name
        self.times: list[float] = []

    def record(self, elapsed: float):
        self.times.append(elapsed)

    def stats(self) -> dict:
        n = len(self.times)
        if n == 0:
            return {"n": 0, "avg": 0, "med": 0, "p95": 0, "p99": 0, "min": 0, "max": 0, "rps": 0}
        s = sorted(self.times)
        total = sum(s)
        return {
            "n": n,
            "avg": total / n,
            "med": statistics.median(s),
            "p95": s[int(n * 0.95)] if n >= 20 else s[-1],
            "p99": s[int(n * 0.99)] if n >= 100 else s[-1],
            "min": s[0],
            "max": s[-1],
            "rps": n / total if total > 0 else 0,
        }


def bench(label: str, fn, iterations: int = ITERATIONS, warmup: int = WARMUP) -> BenchResult:
    r = BenchResult("", label)
    for _ in range(warmup):
        try:
            fn()
        except Exception:
            pass
    for _ in range(iterations):
        t0 = time.monotonic()
        try:
            fn()
            r.record(time.monotonic() - t0)
        except Exception as e:
            log(f"  {label} error: {e}")
    return r


# ── Data generation ─────────────────────────────────────────────

def count_bm() -> int:
    out = run_sql("SELECT count(*) FROM materials WHERE mat_no LIKE 'BM-%'")
    return int(out) if out else 0


def generate_data(n: int):
    existing = count_bm()
    if existing >= n:
        log(f"  {existing} benchmark materials already present (>= {n}), skipping generation")
        return
    log(f"  Generating {n} benchmark materials (have {existing})...")

    # Disable PGroonga triggers during bulk insert for speed
    log("  Disabling search_doc triggers...")
    run_sql("ALTER TABLE sizes DISABLE TRIGGER ALL")
    run_sql("ALTER TABLE colorways DISABLE TRIGGER ALL")
    run_sql("ALTER TABLE material_features DISABLE TRIGGER ALL")
    run_sql("ALTER TABLE feature_attributes DISABLE TRIGGER ALL")
    run_sql("ALTER TABLE materials DISABLE TRIGGER ALL")

    result = run_sql_file("/benchmark/generate_data.sql", f"num={n}")
    for line in result.split("\n"):
        if line.strip():
            log(f"  {line.strip()}")

    # Re-enable triggers
    log("  Re-enabling triggers...")
    run_sql("ALTER TABLE materials ENABLE TRIGGER ALL")
    run_sql("ALTER TABLE sizes ENABLE TRIGGER ALL")
    run_sql("ALTER TABLE colorways ENABLE TRIGGER ALL")
    run_sql("ALTER TABLE material_features ENABLE TRIGGER ALL")
    run_sql("ALTER TABLE feature_attributes ENABLE TRIGGER ALL")

    log(f"  Data generation complete. Total BM materials: {count_bm()}")


def rebuild_search_docs():
    # Check if search_doc is already populated
    sample = run_sql("SELECT search_doc::text FROM materials WHERE mat_no = 'BM-0000001' LIMIT 1")
    if sample and sample.strip() != "{}":
        log("  search_doc already populated, skipping rebuild")
        return
    log("  Rebuilding search_doc for all benchmark materials...")
    run_sql("ALTER TABLE materials DISABLE TRIGGER search_doc_material_change_trigger")
    run_sql("""
        UPDATE materials SET search_doc = (
            SELECT jsonb_build_object(
                'mat_no', m.mat_no, 'name', m.name, 'status', m.status, 'tenant_id', m.tenant_id,
                'sizes', (SELECT COALESCE(jsonb_agg(jsonb_build_object(
                    'id', s.id, 'size_code', s.size_code, 'mat_no', s.mat_no, 'name', s.name
                )), '[]'::jsonb) FROM sizes s WHERE s.mat_no = m.mat_no),
                'colorways', (SELECT COALESCE(jsonb_agg(jsonb_build_object(
                    'id', c.id, 'colorway_code', c.colorway_code, 'mat_no', c.mat_no,
                    'name', c.name, 'hex', c.hex
                )), '[]'::jsonb) FROM colorways c WHERE c.mat_no = m.mat_no),
                'material_features', (SELECT COALESCE(jsonb_agg(jsonb_build_object(
                    'id', mf.id, 'feature_id', mf.id, 'mat_no', mf.mat_no,
                    'feature_name', mf.feature_name, 'description', mf.description,
                    'feature_attributes', (SELECT COALESCE(jsonb_agg(jsonb_build_object(
                        'id', fa.id, 'feature_id', fa.feature_id, 'attr_name', fa.attr_name, 'attr_value', fa.attr_value
                    )), '[]'::jsonb) FROM feature_attributes fa WHERE fa.feature_id = mf.id)
                )), '[]'::jsonb) FROM material_features mf WHERE mf.mat_no = m.mat_no)
            )
            FROM materials m WHERE m.mat_no = materials.mat_no
        )
        WHERE materials.mat_no LIKE 'BM-%'
    """)
    run_sql("ALTER TABLE materials ENABLE TRIGGER search_doc_material_change_trigger")
    log("  search_doc rebuild complete")


# ── ES seeding ──────────────────────────────────────────────────

def es_bulk_index(docs: list[dict]):
    """Bulk-index documents into ES."""
    lines = []
    for doc in docs:
        lines.append(json.dumps({"index": {"_index": "materials", "_id": doc["mat_no"]}}))
        lines.append(json.dumps(doc))
    body = "\n".join(lines) + "\n"
    url = f"{ES_URL}/_bulk"
    creds = base64.b64encode(f"{ES_USER}:{ES_PASS}".encode()).decode()
    req = urllib.request.Request(
        url, data=body.encode(), method="POST",
        headers={"Content-Type": "application/x-ndjson", "Authorization": f"Basic {creds}"},
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        result = json.loads(resp.read())
        if result.get("errors"):
            err_items = [i for i in result["items"] if i.get("index", {}).get("error")]
            if err_items:
                log(f"  Bulk errors: {len(err_items)} documents failed")
    es_request("_refresh", method="POST")


def seed_es(batch_size: int = 2000):
    """Read benchmark materials from PG and index into ES."""
    es_count_result = es_request("_count")
    es_count = es_count_result.get("count", 0)
    pg_count = count_bm()
    log(f"  ES has {es_count} docs, PG has {pg_count} BM materials")

    if es_count >= pg_count * 0.9:
        log(f"  ES already seeded (~{es_count} docs), skipping")
        return

    log(f"  Seeding ES from PostgreSQL (batch size {batch_size})...")
    # Use COPY for fast bulk read from PG
    env = os.environ.copy()
    env["PGPASSWORD"] = DB_URL.split(":")[2].split("@")[0]
    csv_args = [
        "psql", DB_URL, "-t", "-A", "-F", "|", "-c",
        "SELECT mat_no, name, status, COALESCE(tenant_id,''), search_doc::text "
        "FROM materials WHERE mat_no LIKE 'BM-%' ORDER BY mat_no"
    ]
    r = subprocess.run(csv_args, capture_output=True, text=True, timeout=120, env=env)
    lines = [l for l in r.stdout.strip().split("\n") if l.strip()]
    log(f"  Read {len(lines)} rows from PG, indexing into ES...")

    batch = []
    total = 0
    for line in lines:
        parts = line.split("|", 4)
        if len(parts) < 5:
            continue
        mat_no, name, status, tenant_id, search_doc_str = parts
        try:
            doc = json.loads(search_doc_str)
        except json.JSONDecodeError:
            doc = {"mat_no": mat_no, "name": name, "status": status, "tenant_id": tenant_id}
        batch.append(doc)
        if len(batch) >= batch_size:
            es_bulk_index(batch)
            total += len(batch)
            if total % 10000 == 0:
                log(f"    indexed {total} / {len(lines)} ...")
            batch = []
    if batch:
        es_bulk_index(batch)
        total += len(batch)

    es_request("_refresh", method="POST")
    final = es_request("_count").get("count", 0)
    log(f"  ES seeding complete: {final} documents")


# ── Search queries ──────────────────────────────────────────────

SEARCH_QUERIES = [
    # (name, pgroonga_gql, es_body)
    (
        "match_all",
        '{ searchMaterials(query: "", limit: 100) { node { mat_no } } }',
        {"query": {"match_all": {}}, "size": 100},
    ),
    (
        "term_single",
        '{ searchMaterials(query: "Premium", limit: 100) { node { mat_no } } }',
        {"query": {"multi_match": {"query": "Premium", "fields": ["name", "status", "mat_no"]}}, "size": 100},
    ),
    (
        "term_multi_word",
        '{ searchMaterials(query: "Premium Benchmark", queryOperator: AND, limit: 100) { node { mat_no } } }',
        {"query": {"multi_match": {"query": "Premium Benchmark", "operator": "AND", "fields": ["name", "status", "mat_no"]}}, "size": 100},
    ),
    (
        "term_or",
        '{ searchMaterials(query: "Premium Economy", limit: 100) { node { mat_no } } }',
        {"query": {"multi_match": {"query": "Premium Economy", "operator": "OR", "fields": ["name", "status", "mat_no"]}}, "size": 100},
    ),
    (
        "filter_eq",
        '{ searchMaterials(query: "", filter: { status: { eq: "active" } }, limit: 100) { node { mat_no } } }',
        {"query": {"term": {"status": "active"}}, "size": 100},
    ),
    (
        "filter_ne",
        '{ searchMaterials(query: "", filter: { status: { ne: "active" } }, limit: 100) { node { mat_no } } }',
        {"query": {"bool": {"must_not": [{"term": {"status": "active"}}]}}, "size": 100},
    ),
    (
        "filter_in",
        '{ searchMaterials(query: "", filter: { status: { in: ["active", "discontinued"] } }, limit: 100) { node { mat_no } } }',
        {"query": {"terms": {"status": ["active", "discontinued"]}}, "size": 100},
    ),
    (
        "query+filter",
        '{ searchMaterials(query: "Premium", filter: { status: { eq: "active" } }, limit: 100) { node { mat_no } } }',
        {"query": {"bool": {"must": [{"multi_match": {"query": "Premium", "fields": ["name", "status", "mat_no"]}}, {"term": {"status": "active"}}]}}, "size": 100},
    ),
    (
        "deep_nested",
        '{ searchMaterials(query: "Construction", limit: 100) { node { mat_no material_features { feature_name feature_attributes { attr_name attr_value } } } } }',
        {"query": {"multi_match": {"query": "Construction", "fields": ["material_features.feature_name", "material_features.description"]}}, "size": 100},
    ),
    (
        "paginated_deep",
        '{ searchMaterials(query: "Benchmark", limit: 20, offset: 5000) { node { mat_no } } }',
        {"query": {"multi_match": {"query": "Benchmark", "fields": ["name"]}}, "size": 20, "from": 5000},
    ),
]


def run_pgroonga_search(name: str, gql_query: str):
    resp = gql(gql_query)
    if resp.get("errors"):
        raise RuntimeError(f"GQL errors: {resp['errors']}")
    hits = resp.get("data", {}).get("searchMaterials") or []
    return hits


def run_es_search(name: str, es_body: dict):
    result = es_request("materials/_search", method="POST", body=es_body)
    hits = result.get("hits", {}).get("hits", [])
    return hits


# ── Main ────────────────────────────────────────────────────────

def divider(title: str):
    print()
    print("=" * 100)
    print(f"  {title}")
    print("=" * 100)


def print_table(rows: list[dict]):
    """Print a comparison table."""
    fmt = "  {:<20s}  {:>10s}  {:>10s}  {:>10s}  {:>10s}  {:>10s}  {:>10s}  {:>10s}"
    hdr = fmt.format("Query", "Backend", "n", "avg(ms)", "med(ms)", "p95(ms)", "p99(ms)", "rps")
    print(hdr)
    print("  " + "-" * 96)
    for row in rows:
        s = row["stats"]
        print(fmt.format(
            row["name"], row["backend"], str(s["n"]),
            f"{s['avg']*1000:.1f}", f"{s['med']*1000:.1f}",
            f"{s['p95']*1000:.1f}", f"{s['p99']*1000:.1f}",
            f"{s['rps']:.0f}",
        ))


def main():
    divider("ES vs PGroonga — Head-to-Head Benchmark")
    log(f"  DATA_SIZE:     {DATA_SIZE}")
    log(f"  ITERATIONS:    {ITERATIONS}")
    log(f"  WARMUP:        {WARMUP}")
    log(f"  GRAPHQL_URL:   {GRAPHQL_URL}")
    log(f"  ES_URL:        {ES_URL}")
    log("")

    # ── 1. Generate data ──
    divider("1. Data Generation")
    generate_data(DATA_SIZE)

    # ── 2. Rebuild search_doc for PGroonga ──
    divider("2. PGroonga: rebuild search_doc")
    rebuild_search_docs()

    # ── 3. Seed ES ──
    divider("3. Elasticsearch: seed index")
    seed_es()

    # ── 4. Verify counts ──
    divider("4. Verification")
    pg_sample = run_sql("SELECT mat_no, name, status FROM materials WHERE mat_no = 'BM-0000001'")
    log(f"  PG sample: {pg_sample}")

    es_sample_result = es_request("materials/_doc/BM-0000001")
    es_sample_name = es_sample_result.get("_source", {}).get("name", "N/A")
    log(f"  ES sample: BM-0000001 | {es_sample_name}")

    pg_count = count_bm()
    es_count = es_request("_count").get("count", 0)
    log(f"  PG BM materials: {pg_count}")
    log(f"  ES documents:    {es_count}")

    # Quick correctness check
    pg_search = gql('{ searchMaterials(query: "Premium", limit: 5) { node { mat_no } } }')
    pg_hits = len((pg_search.get("data") or {}).get("searchMaterials") or [])
    es_search = es_request("materials/_search", method="POST", body={
        "query": {"multi_match": {"query": "Premium", "fields": ["name"]}}, "size": 5
    })
    es_hits = len(es_search.get("hits", {}).get("hits", []))
    log(f"  Quick search 'Premium': PG={pg_hits} hits, ES={es_hits} hits")

    # ── 5. Run benchmarks ──
    divider("5. Search Benchmarks")
    all_rows = []

    for name, pg_gql, es_body in SEARCH_QUERIES:
        log(f"  Running: {name}")
        pg_result = bench(f"pg_{name}", lambda g=pg_gql: run_pgroonga_search(name, g))
        pg_result.backend = "PGroonga"
        es_result = bench(f"es_{name}", lambda b=es_body: run_es_search(name, b))
        es_result.backend = "Elasticsearch"

        all_rows.append({"name": name, "backend": "PGroonga", "stats": pg_result.stats()})
        all_rows.append({"name": name, "backend": "Elasticsearch", "stats": es_result.stats()})

        pg_s = pg_result.stats()
        es_s = es_result.stats()
        if es_s["avg"] > 0:
            ratio = pg_s["avg"] / es_s["avg"] if es_s["avg"] > 0 else 0
            log(f"    PG avg={pg_s['avg']*1000:.1f}ms  ES avg={es_s['avg']*1000:.1f}ms  ratio={ratio:.2f}x")
        else:
            log(f"    PG avg={pg_s['avg']*1000:.1f}ms  ES avg=N/A")

    # ── 6. Results ──
    divider("6. Results")
    print()
    print_table(all_rows)

    # ── 7. Summary ──
    divider("7. Summary")
    pg_avgs = [r["stats"]["avg"] for r in all_rows if r["backend"] == "PGroonga" and r["stats"]["n"] > 0]
    es_avgs = [r["stats"]["avg"] for r in all_rows if r["backend"] == "Elasticsearch" and r["stats"]["n"] > 0]
    if pg_avgs and es_avgs:
        pg_geo = statistics.geometric_mean(pg_avgs) if all(v > 0 for v in pg_avgs) else 0
        es_geo = statistics.geometric_mean(es_avgs) if all(v > 0 for v in es_avgs) else 0
        log(f"  PGroonga  geometric mean latency: {pg_geo*1000:.2f} ms")
        log(f"  ES        geometric mean latency: {es_geo*1000:.2f} ms")
        if es_geo > 0:
            overall = pg_geo / es_geo
            faster = "PGroonga" if overall < 1 else "Elasticsearch"
            factor = 1 / overall if overall < 1 else overall
            log(f"  Overall: {faster} is {factor:.2f}x faster (geometric mean)")

    log("  Benchmark complete.")


if __name__ == "__main__":
    main()
