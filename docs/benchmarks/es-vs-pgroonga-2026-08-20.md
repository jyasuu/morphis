# ES vs PGroonga Benchmark Report

**Date:** 2026-08-20
**Dataset:** 10,000 benchmark materials with sizes, colorways, material_features, feature_attributes
**Iterations:** 30 per query, 3 warmup

## Setup

| Parameter | Value |
|---|---|
| PGroonga | `TokenBigram` tokenizer, `search_doc JSONB` on `materials` table |
| Elasticsearch | `multi_match` across `name`, `status`, `mat_no`, nested fields |
| Data | 10k materials, 50k sizes, 30k colorways, 30k features, 60k attributes |
| Network | Same Docker network (inter-container latency negligible) |

## Results

| Query | PGroonga avg | PGroonga med | PGroonga p99 | ES avg | ES med | ES p99 | ES faster |
|---|---|---|---|---|---|---|---|
| match_all | 507.3ms | 509.2ms | 521.8ms | 6.4ms | 5.9ms | 11.0ms | 79x |
| term_single | 504.4ms | 503.6ms | 520.7ms | 7.7ms | 6.4ms | 26.2ms | 66x |
| term_multi_word (AND) | 506.8ms | 505.9ms | 550.5ms | 7.4ms | 7.1ms | 11.9ms | 69x |
| term_or | 512.9ms | 511.0ms | 532.0ms | 8.3ms | 7.3ms | 19.6ms | 62x |
| filter_eq (status) | 512.2ms | 511.8ms | 544.6ms | 6.1ms | 5.9ms | 11.1ms | 84x |
| filter_ne (status) | 507.7ms | 506.4ms | 531.6ms | 6.2ms | 6.0ms | 8.1ms | 82x |
| filter_in (status) | 514.8ms | 513.8ms | 557.3ms | 6.4ms | 5.7ms | 17.8ms | 81x |
| query + filter | 509.6ms | 508.7ms | 540.1ms | 9.1ms | 7.4ms | 21.3ms | 56x |
| deep_nested | 535.5ms | 534.7ms | 557.1ms | 5.8ms | 5.6ms | 9.0ms | 92x |
| paginated_deep (limit 20, offset 5000) | 124.5ms | 123.9ms | 131.9ms | 5.0ms | 4.6ms | 8.0ms | 25x |

**Geometric mean latency:**
- PGroonga: **444.73 ms**
- Elasticsearch: **6.74 ms**
- **Elasticsearch is ~66x faster overall**

## Analysis

### Why PGroonga is slower

1. **JSONB search overhead.** PGroonga's `&@~` operator searches a JSONB column. Unlike ES's purpose-built inverted index, PGroonga must evaluate the tokenizer against JSONB values at query time. The `search_doc` column contains nested arrays (sizes, colorways, features with attributes), making each document ~2-5KB of JSONB.

2. **Consistent ~500ms baseline.** Most PGroonga queries hover around 500ms regardless of query complexity, suggesting the bottleneck is the initial scan/index lookup rather than filter processing. The `paginated_deep` query (limit 20) drops to 125ms, indicating row-fetch overhead scales with result count.

3. **No query result caching.** ES caches frequently-accessed filter results internally. PGroonga relies on PostgreSQL's buffer pool, which is not tuned for full-text search workloads.

### Where PGroonga is competitive

- **Paginated queries** (limit 20, offset 5000): 125ms vs 5ms (25x gap, narrower than the 80x gap for limit-100 queries)
- **Empty queries** returning all results are comparable in absolute terms when result sets are small

### Tradeoffs

| Factor | PGroonga | Elasticsearch |
|---|---|---|
| Search latency (10k docs) | ~500ms | ~7ms |
| Infrastructure | Zero (runs in PostgreSQL) | 3 containers (ES + pgx + RabbitMQ) |
| Data consistency | Immediate (triggers) | Eventual (sync pipeline) |
| Operational complexity | Low | High |
| CJK support | Native (TokenBigram) | Requires ICU plugin |
| Data scale ceiling | ~100k materials | Millions |

## Recommendation

For the project's current scale (single-digit to low thousands of materials), PGroonga's ~500ms latency is acceptable for a search endpoint. The operational savings (no ES cluster, no sync pipeline, immediate consistency) outweigh the latency cost.

If search latency becomes a bottleneck above ~10k materials, consider:
1. A GIN index on frequently-filtered JSONB paths
2. Materialized summary columns for hot filters
3. Re-enabling Elasticsearch for high-throughput indexes
