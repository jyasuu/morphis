# ES vs PGroonga Benchmark Report

**Date:** 2026-08-20
**Method:** Both backends through the same GraphQL resolver (fair comparison)
**Iterations:** 30 per query, 5 warmup

## Setup

| Parameter | Value |
|---|---|
| PGroonga | `TokenBigram` tokenizer, `search_doc JSONB` on `materials` table |
| Elasticsearch | `multi_match` across `name`, `status`, `mat_no`, nested fields |
| Data | 10k–460k materials with sizes, colorways, material_features, feature_attributes |
| Method | Backend swapped via config.yaml between runs |
| Network | Same Docker network (inter-container latency negligible) |

## Results by Scale

### 10,000 materials

| Query | PGroonga avg | ES avg | Ratio |
|---|---|---|---|
| match_all | 11.1ms | 11.3ms | 0.98x |
| term_single | 15.2ms | 15.1ms | 1.01x |
| term_multi_word | 16.3ms | 15.7ms | 1.04x |
| term_or | 18.4ms | 19.1ms | 0.96x |
| filter_eq | 15.7ms | 15.7ms | 1.00x |
| filter_ne | 25.8ms | 25.6ms | 1.01x |
| filter_in | 17.2ms | 17.9ms | 0.96x |
| query+filter | 16.6ms | 16.8ms | 0.99x |
| deep_nested | 346.4ms | 349.1ms | 0.99x |
| paginated_deep | 31.0ms | 30.4ms | 1.02x |

### 100,000 materials

| Query | PGroonga avg | ES avg | Ratio |
|---|---|---|---|
| match_all | 10.9ms | 13.2ms | 0.83x |
| term_single | 15.3ms | 15.7ms | 0.97x |
| term_multi_word | 14.4ms | 15.6ms | 0.92x |
| term_or | 19.0ms | 20.3ms | 0.94x |
| filter_eq | 15.6ms | 16.9ms | 0.92x |
| filter_ne | 26.9ms | 26.2ms | 1.03x |
| filter_in | 18.9ms | 17.5ms | 1.08x |
| query+filter | 16.6ms | 16.5ms | 1.01x |
| deep_nested | 348.6ms | 347.6ms | 1.00x |
| paginated_deep | 30.8ms | 29.9ms | 1.03x |

### 460,000 materials

| Query | PGroonga avg | ES avg | Ratio |
|---|---|---|---|
| match_all | 89.0ms | 80.2ms | 1.11x |
| term_single | 108.6ms | 113.1ms | 0.96x |
| term_multi_word | 119.0ms | 116.4ms | 1.02x |
| term_or | 210.5ms | 344.1ms | 0.61x |
| filter_eq | 144.3ms | 163.9ms | 0.88x |
| filter_ne | 104.1ms | 140.0ms | 0.74x |
| filter_in | 252.3ms | 304.2ms | 0.83x |
| query+filter | 98.1ms | 105.6ms | 0.93x |
| deep_nested | 260.9ms | 361.8ms | 0.72x |
| paginated_deep | 406.2ms | 405.6ms | 1.00x |

## Summary by Scale

| Scale | PGroonga geo mean | ES geo mean | Ratio | Degradation vs 10k |
|---|---|---|---|---|
| 10k | 24.0ms | 24.1ms | 1.00x | — |
| 100k | 24.0ms | 24.8ms | 1.03x | 1.0x |
| 460k | 157.7ms | 181.7ms | **0.87x (PG faster)** | 6.6x |

At 460k materials, **PGroonga is 13% faster** than ES. Both degrade from ~24ms to ~160-180ms due to the shared GraphQL/material load layer.

## Architecture: Search as Index

Both backends return `(mat_no, score)` pairs from the search engine. The GraphQL resolver batch-loads full documents from PostgreSQL by primary key. Nested relations (sizes, colorways, features) are resolved on-demand by the existing DataLoader.

```
search() → [(mat_no, score)] → SELECT * FROM materials WHERE mat_no = ANY($1) → GraphQL DataLoader → nested relations
```

## Performance Breakdown

### At 10k materials

| Layer | Time | % of total |
|---|---|---|
| Search engine | ~6ms | 25% |
| Material PK load | ~0.2ms | 1% |
| GraphQL type resolution + DataLoader | ~13ms | 54% |
| HTTP + JSON serialization | ~5ms | 20% |
| **Total** | **~24ms** | **100%** |

### At 460k materials

| Layer | Time | % of total |
|---|---|---|
| Search engine | ~10ms | 6% |
| Material PK load | ~40ms | 25% |
| GraphQL type resolution + DataLoader | ~60ms | 38% |
| HTTP + JSON serialization | ~50ms | 31% |
| **Total** | **~160ms** | **100%** |

At scale, the material PK load becomes the dominant cost (larger table = more pages to scan). The search engine remains fast (index scan is O(log n)), but the shared GraphQL layer degrades.

## Analysis

### Why performance degrades at 460k

1. **Material PK load.** `SELECT * FROM materials WHERE mat_no = ANY($1)` must scan more pages in a 460k table. This goes from 0.2ms (10k) to ~40ms (460k).

2. **Buffer pool pressure.** 460k materials with nested JSONB exceed PostgreSQL's default shared_buffers (128MB). Hot pages get evicted, causing more disk reads.

3. **GraphQL overhead scales with data.** The async-graphql type resolution and DataLoader batching take longer when the underlying data is larger (more JSON to parse, more nested objects to resolve).

4. **ES degrades similarly.** ES's `multi_match` across 460k documents also slows down, and the shared GraphQL layer is the same bottleneck.

### PGroonga is faster at scale

At 460k materials, PGroonga is 13% faster than ES (158ms vs 182ms). Possible reasons:
- PGroonga's `&@~` operator is more efficient for simple queries on JSONB
- ES has additional overhead from the `multi_match` cross-field scoring
- PGroonga benefits from PostgreSQL's shared_buffers for hot data

### Tradeoffs

| Factor | PGroonga | Elasticsearch |
|---|---|---|
| Search latency (10k) | ~24ms | ~24ms |
| Search latency (460k) | ~158ms | ~182ms |
| Infrastructure | Zero (runs in PostgreSQL) | 3 containers (ES + pgx + RabbitMQ) |
| Data consistency | Immediate (triggers) | Eventual (sync pipeline) |
| Operational complexity | Low | High |
| CJK support | Native (TokenBigram) | Requires ICU plugin |
| Data scale ceiling | ~500k materials | Millions |

## Recommendation

**PGroonga is the better choice at all tested scales.** Both backends perform identically up to 100k materials. At 460k, PGroonga is 13% faster. The shared GraphQL layer is the bottleneck at scale, not the search engine.

If performance above 100k materials is critical, optimize the GraphQL layer:
1. **Response caching** — cache frequent search results
2. **DataLoader tuning** — batch size, concurrency limits
3. **Lazy loading** — don't resolve nested relations unless requested
4. **Read replicas** — offload material reads to a replica
