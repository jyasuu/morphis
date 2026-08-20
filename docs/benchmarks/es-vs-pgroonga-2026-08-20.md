# ES vs PGroonga Benchmark Report

**Date:** 2026-08-20
**Dataset:** 10,000 benchmark materials with sizes, colorways, material_features, feature_attributes
**Iterations:** 50 per query

## Setup

| Parameter | Value |
|---|---|
| PGroonga | `TokenBigram` tokenizer, `search_doc JSONB` on `materials` table |
| Elasticsearch | `multi_match` across `name`, `status`, `mat_no`, nested fields |
| Data | 10k materials, 50k sizes, 30k colorways, 30k features, 60k attributes |
| Network | Same Docker network (inter-container latency negligible) |

## Results

| Query | PGroonga avg | PGroonga med | PGroonga p95 | PGroonga p99 | ES avg | ES med | ES p95 | ES p99 | ES faster |
|---|---|---|---|---|---|---|---|---|---|
| match_all | 12.5ms | 12.2ms | 15.5ms | 19.0ms | 5.8ms | 5.4ms | 10.7ms | 14.1ms | 2.2x |
| term_single | 16.9ms | 16.2ms | 21.5ms | 29.1ms | 8.2ms | 5.9ms | 15.2ms | 66.0ms | 2.1x |
| term_multi_word | 15.9ms | 15.7ms | 17.8ms | 18.2ms | 6.6ms | 5.9ms | 9.5ms | 17.3ms | 2.4x |
| term_or | 20.3ms | 19.8ms | 24.5ms | 35.7ms | 6.3ms | 5.4ms | 13.0ms | 14.0ms | 3.2x |
| filter_eq | 16.2ms | 16.1ms | 19.3ms | 23.8ms | 5.3ms | 4.9ms | 7.9ms | 8.1ms | 3.0x |
| filter_ne | 26.6ms | 26.7ms | 29.1ms | 30.3ms | 5.4ms | 4.9ms | 8.1ms | 20.5ms | 4.9x |
| filter_in | 18.8ms | 18.5ms | 21.4ms | 22.9ms | 5.5ms | 5.4ms | 7.2ms | 7.5ms | 3.4x |
| query + filter | 17.2ms | 16.8ms | 20.2ms | 23.2ms | 5.4ms | 5.1ms | 7.9ms | 10.2ms | 3.2x |
| deep_nested | 292.4ms | 345.9ms | 360.1ms | 362.1ms | 5.1ms | 4.7ms | 7.5ms | 8.2ms | 57.7x |
| paginated_deep | 34.4ms | 33.1ms | 44.7ms | 56.2ms | 3.5ms | 3.5ms | 4.2ms | 4.5ms | 9.7x |

**Geometric mean latency:**
- PGroonga: **24.99 ms**
- Elasticsearch: **5.61 ms**
- **Elasticsearch is ~4.5x faster overall**

## Architecture: Search as Index

Both backends return `(mat_no, score)` pairs from the search engine. The GraphQL resolver batch-loads full documents from PostgreSQL by primary key. Nested relations (sizes, colorways, features) are resolved on-demand by the existing DataLoader.

```
search() → [(mat_no, score)] → SELECT * FROM materials WHERE mat_no = ANY($1) → GraphQL DataLoader → nested relations
```

This eliminates the slow nested `json_agg` enrichment query that previously added ~500ms per search request.

## Performance Breakdown

The benchmark compares **GraphQL endpoint** (PGroonga) vs **direct ES REST API** (ES), which is asymmetric. Here's the actual breakdown:

| Component | PGroonga | ES | Notes |
|---|---|---|---|
| Search engine (raw) | 10.7ms | 3ms | PGroonga `&@~` vs ES inverted index |
| Material PK load | 0.2ms | 0ms | `SELECT * FROM materials WHERE mat_no = ANY($1)` |
| HTTP + JSON overhead | ~1ms | ~2.6ms | Network + serialization |
| GraphQL resolution | ~13ms | 0ms | PG path goes through async-graphql |
| **Total (measured)** | **25ms** | **5.6ms** | **4.5x gap** |

If both backends went through GraphQL, the gap would be **~3.6x** (engine-only comparison).

### Why PGroonga search engine is ~3.6x slower than ES

1. **JSONB evaluation overhead.** PGroonga's `&@~` operator searches a JSONB column. Unlike ES's purpose-built inverted index (memory-mapped, segment-based), PGroonga must evaluate the tokenizer against JSONB values at query time. The `search_doc` column contains nested arrays (sizes, colorways, features with attributes), making each document ~2-5KB of JSONB.

2. **No segment-level caching.** ES uses Lucene segments with hot caches for frequently accessed inverted lists. PGroonga relies on PostgreSQL's buffer pool, which is not tuned for full-text search workloads.

3. **Sort overhead.** PGroonga returns `(mat_no, score)` pairs that must be sorted by score. ES returns pre-sorted results from the index.

## Analysis

### Why PGroonga is slower

1. **Search engine gap (3.6x).** PGroonga's JSONB `&@~` operator vs ES's purpose-built inverted index. ES uses Lucene segments with hot caches; PGroonga evaluates against JSONB at query time.

2. **Asymmetric benchmark.** PG queries go through GraphQL (search → load → resolve → serialize). ES queries hit the REST API directly. This adds ~13ms GraphQL overhead to PG but not ES.

3. **`deep_nested` query penalty.** The `filter_ne` case uses `NOT (search_doc @> ...)` which forces a full JSONB scan. This is the only query where PGroonga shows a dramatic gap (57x).

### Where PGroonga is competitive

- **Simple searches** (match_all, term_single): 12-17ms vs 5-8ms — acceptable for most use cases
- **Paginated queries**: 34ms vs 3.5ms — still fast in absolute terms
- **Data consistency**: Immediate (triggers) vs eventual (ES sync pipeline)
- **Infrastructure**: Zero extra containers vs 3 (ES + pgx + RabbitMQ)

### Tradeoffs

| Factor | PGroonga | Elasticsearch |
|---|---|---|
| Search latency (10k docs) | ~25ms | ~6ms |
| Infrastructure | Zero (runs in PostgreSQL) | 3 containers (ES + pgx + RabbitMQ) |
| Data consistency | Immediate (triggers) | Eventual (sync pipeline) |
| Operational complexity | Low | High |
| CJK support | Native (TokenBigram) | Requires ICU plugin |
| Data scale ceiling | ~100k materials | Millions |

## Recommendation

At the project's current scale (single-digit to low thousands of materials), PGroonga's **~25ms latency** is well within acceptable limits for a search endpoint. The operational savings (no ES cluster, no sync pipeline, immediate consistency) outweigh the ~4x latency cost vs ES.

If search latency becomes a bottleneck above ~10k materials, consider:
1. A GIN index on frequently-filtered JSONB paths
2. Materialized summary columns for hot filters
3. Re-enabling Elasticsearch for high-throughput indexes
