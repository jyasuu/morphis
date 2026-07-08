# morphis Benchmark Suite

Load generation + performance benchmarks for GraphQL queries, Elasticsearch search, and the pgx LISTEN/NOTIFY pipeline.

## Usage

```bash
# Full benchmark (generates data, runs all tests)
docker compose --profile benchmark up benchmark

# Custom size and iterations
BENCH_DATA_SIZE=5000 BENCH_ITERATIONS=100 BENCH_WARMUP=10 \
  docker compose run --rm benchmark

# Skip data generation, include throughput tests
docker compose run --rm benchmark --with-throughput --skip-generate

# Only pgx pipeline latency tests
docker compose run --rm benchmark --only-pgx
```

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `BENCH_DATA_SIZE` | 500 | Number of benchmark materials to generate |
| `BENCH_ITERATIONS` | 30 | Requests per benchmark |
| `BENCH_WARMUP` | 3 | Warmup iterations before measurement |

## Results

### Jul 8, 2026 — ES enrichment batching (commit `b402140`)

Replaced per-item N+1 SQL enrichment queries with batch `ANY($1)` fetches. 2,000 materials, 100 iterations each, 10 warmup.

#### GraphQL Queries

| Benchmark | avg | med | p99 | rps |
|---|---|---|---|---|
| Single by PK | 0.86ms | 0.80ms | 1.79ms | 1,160/s |
| List limit 100 | 1.43ms | 1.33ms | 2.61ms | 698/s |
| List filtered | 1.49ms | 1.39ms | 2.85ms | 671/s |
| List paginated | 1.39ms | 1.32ms | 2.46ms | 722/s |
| List + sizes | 15.39ms | 14.98ms | 22.57ms | 65/s |
| List deep nested | 33.46ms | 32.84ms | 49.21ms | 30/s |
| Single + all nested | 4.52ms | 4.26ms | 7.93ms | 221/s |
| Create mutation | 9.92ms | 4.15ms | 105.71ms | 101/s |

#### Search Queries (Elasticsearch)

| Benchmark | Before (avg) | After (avg) | Speedup |
|---|---|---|---|
| Match all | 37.24ms | 12.22ms | **3.0x** |
| Term search | 35.45ms | 13.38ms | **2.6x** |
| Filtered search | 31.98ms | 8.86ms | **3.6x** |
| Query + filter | 32.69ms | 9.29ms | **3.5x** |
| Paginated search | 57.72ms | 3.58ms | **16.1x** |
| Nested field search | 39.68ms | 21.07ms | **1.9x** |

The paginated search improvement (16x) is because ES offset scanning + per-item SQL enrichment was compounding; now it's 2 batch queries regardless of result count.

#### pgx LISTEN Pipeline

| Metric | Value |
|---|---|
| Single-insert median latency | 62.44ms |
| Batch throughput (100 docs) | 60.2 docs/sec (1.66s) |
| Direct ES index baseline | 38.33ms |

### Jul 7, 2026 — Baseline (before enrichment batching)

2,000 materials, 100 iterations each, 10 warmup.

| Search Benchmark | avg | med | p99 | rps |
|---|---|---|---|---|
| Match all | 37.24ms | 36.94ms | 44.57ms | 27/s |
| Term search | 35.45ms | 34.77ms | 47.74ms | 28/s |
| Filtered search | 31.98ms | 31.73ms | 40.34ms | 31/s |
| Query + filter | 32.69ms | 32.03ms | 42.96ms | 31/s |
| Paginated search | 57.72ms | 56.22ms | 77.38ms | 17/s |
| Nested field search | 39.68ms | 38.85ms | 52.86ms | 25/s |

## Key Takeaways

- **PK lookups** are ~0.7ms median, >1,100 req/s — the `row_to_json` + single-row path is efficient
- **Flat list queries** do ~700 req/s at ~1.4ms
- **Adding one has_many join** (sizes) drops to 65 req/s / 15ms — the N+1 pattern from `async-graphql` per-parent resolution is the bottleneck
- **Deep nesting** (3 join levels) drops to 30 req/s / 33ms
- **Search is now nearly pure ES latency** — batch enrichment eliminated the N+1 overhead; further gains would come from ES cluster scaling or query optimization
- **pgx pipeline overhead** is ~60ms median from INSERT → pg_notify → pg_x → ES index
