# ES vs PGroonga Benchmark Report

**Date:** 2026-08-20
**Dataset:** 10,000 benchmark materials with sizes, colorways, material_features, feature_attributes
**Iterations:** 30 per query, 5 warmup
**Method:** Both backends through the same GraphQL resolver (fair comparison)

## Setup

| Parameter | Value |
|---|---|
| PGroonga | `TokenBigram` tokenizer, `search_doc JSONB` on `materials` table |
| Elasticsearch | `multi_match` across `name`, `status`, `mat_no`, nested fields |
| Data | 10k materials, 50k sizes, 30k colorways, 30k features, 60k attributes |
| Method | Backend swapped via config.yaml restart between runs |
| Network | Same Docker network (inter-container latency negligible) |

## Results

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

**Geometric mean latency:**
- PGroonga: **23.98 ms**
- Elasticsearch: **24.09 ms**
- **Ratio: 1.00x (identical)**

## Architecture: Search as Index

Both backends return `(mat_no, score)` pairs from the search engine. The GraphQL resolver batch-loads full documents from PostgreSQL by primary key. Nested relations (sizes, colorways, features) are resolved on-demand by the existing DataLoader.

```
search() → [(mat_no, score)] → SELECT * FROM materials WHERE mat_no = ANY($1) → GraphQL DataLoader → nested relations
```

This eliminates the slow nested `json_agg` enrichment query that previously added ~500ms per search request.

## Performance Breakdown

| Layer | Time | % of total |
|---|---|---|
| Search engine (PGroonga `&@~` or ES `multi_match`) | ~6ms | 25% |
| Material PK load (`SELECT * FROM materials WHERE mat_no = ANY($1)`) | ~0.2ms | 1% |
| GraphQL type resolution + DataLoader | ~13ms | 54% |
| HTTP + JSON serialization | ~5ms | 20% |
| **Total** | **~24ms** | **100%** |

The search engine accounts for only 25% of total latency. The GraphQL layer (type resolution, DataLoader, JSON serialization) dominates at 54%.

## Analysis

### Why both backends perform identically

1. **Same code path.** Both backends use the same GraphQL resolver, same material load query, same DataLoader for nested relations. The only difference is the search query (PGroonga `&@~` vs ES `multi_match`), which is ~6ms for both.

2. **GraphQL overhead dominates.** The 13ms of async-graphql type resolution and DataLoader batching is the same for both backends. This is the real bottleneck, not the search engine.

3. **`deep_nested` penalty is shared.** The 346ms for `deep_nested` comes from the DataLoader resolving `material_features` → `feature_attributes` for 100 results. This is identical for both backends.

### Tradeoffs

| Factor | PGroonga | Elasticsearch |
|---|---|---|
| Search latency (10k docs) | ~24ms | ~24ms |
| Infrastructure | Zero (runs in PostgreSQL) | 3 containers (ES + pgx + RabbitMQ) |
| Data consistency | Immediate (triggers) | Eventual (sync pipeline) |
| Operational complexity | Low | High |
| CJK support | Native (TokenBigram) | Requires ICU plugin |
| Data scale ceiling | ~100k materials | Millions |

## Recommendation

At the project's current scale, **PGroonga is the better choice**. Both backends perform identically through GraphQL, but PGroonga has:
- Zero additional infrastructure (no ES cluster, no pgx, no RabbitMQ)
- Immediate data consistency (triggers vs eventual sync)
- Lower operational complexity
- Native CJK support

If search latency becomes a bottleneck above ~10k materials, the optimization target should be the GraphQL layer (caching, DataLoader tuning, or response streaming), not the search engine.
