# PGroonga Search Backend

## Problem Statement

The current search implementation depends on Elasticsearch as an external service, requiring a multi-component data synchronization pipeline (PostgreSQL triggers → pg_notify → RabbitMQ → pgx-listen → pgx-consume → Elasticsearch). This pipeline adds operational complexity: four additional Docker containers, eventual consistency between PostgreSQL and Elasticsearch, denormalized document maintenance, and a separate index lifecycle. For the project's data scale (single-digit materials with ~25 child rows), this infrastructure is disproportionate. Additionally, Elasticsearch does not natively support CJK (Chinese/Japanese/Korean) full-text search, which is a future requirement for the materials domain.

## Solution

Add PGroonga as an alternative search backend, configurable per search index. PGroonga is a PostgreSQL extension that provides fast full-text search for all languages (including CJK) directly inside the database — no external search engine or synchronization pipeline required. A denormalized `search_doc` JSONB column on the `materials` table, populated by PostgreSQL triggers, serves as the PGroonga index. The existing GraphQL `searchMaterials` API remains unchanged; the backend is selected via a `backend` field in `config.yaml`. Elasticsearch remains available as an option for indexes that need it.

## User Stories

1. As a platform operator, I want to configure PGroonga as the search backend for an index, so that I can eliminate the Elasticsearch cluster and its sync pipeline for that index
2. As a platform operator, I want to keep Elasticsearch available as an option, so that I can switch backends without data migration
3. As a platform operator, I want PGroonga indexes and triggers to be created unconditionally in the database schema, so that I can switch backends at config time without running DDL migrations
4. As a platform operator, I want the PostgreSQL Docker image to include PGroonga pre-installed, so that no custom Dockerfile or entrypoint scripting is needed
5. As a developer, I want the `searchMaterials` GraphQL field to work identically regardless of backend, so that API consumers do not need to know which backend is active
6. As a developer, I want free-text search to work across all searchable fields in a material document, so that users can find materials by typing keywords from any field
7. As a developer, I want structured filters (eq, ne, in, all, contains, starts_with, ends_with, gt/gte/lt/lte) to produce correct results on PGroonga, even if the behavioral semantics differ from Elasticsearch for text operators
8. As a developer, I want the `queryOperator` AND/OR modes to work correctly with PGroonga's query syntax
9. As a developer, I want empty queries to return all materials with a score of 0, ordered by mat_no
10. As a developer, I want search results to be sorted by relevance score descending, with mat_no as a deterministic tiebreaker
11. As a developer, I want the `esQuery` raw Elasticsearch escape hatch to be disabled when the backend is PGroonga
12. As a developer, I want PGroonga to use the `TokenBigram` tokenizer by default, so that Chinese text search works out of the box
13. As a developer, I want the PGroonga tokenizer to be configurable per search index in `config.yaml`
14. As a developer, I want row-level security filters to produce SQL WHERE clauses for PGroonga, reusing the existing `PermissionCache` subquery mechanism
15. As a developer, I want PGroonga search to return `(mat_no, score)` pairs, and the existing DataLoader mechanism to fetch full material documents, so that no new SQL enrichment logic is needed
16. As a developer, I want search results to be re-fetched from normalized PostgreSQL tables after PGroonga returns hit IDs, so that data freshness is guaranteed
17. As a developer, I want the `search_doc` JSONB column to have the same shape as the Elasticsearch document, so that the contract test fixtures remain valid
18. As a developer, I want a single `rebuild_search_doc()` SQL function called by per-table triggers, so that `search_doc` stays in sync when child rows change
19. As a developer, I want triggers on all child tables (sizes, colorways, material_features, feature_attributes) to fire on every INSERT, UPDATE, and DELETE, so that no mutation is missed
20. As a developer, I want the `search_doc` column to be `JSONB NOT NULL DEFAULT '{}'`, so that PGroonga indexing has no null-related edge cases
21. As a developer, I want the denormalized `search_doc` to include `tenant_id`, so that row-level security filtering can use it
22. As a developer, I want a `build_pg_filter()` function that produces SQL WHERE fragments with bind parameters, so that user input is never interpolated into SQL strings
23. As a developer, I want the `contains` operator to map to PGroonga's `&@~` query syntax (searching all text values), accepting that it differs from Elasticsearch's field-scoped `match_phrase`
24. As a developer, I want the `starts_with` operator to use PGroonga's `&^` prefix operator via the `` &` `` script syntax for field-targeted prefix search
25. As a developer, I want the `ends_with` operator to use SQL `LIKE '%term'` since PGroonga has no indexed ends-with for JSONB
26. As a developer, I want the `eq` operator to use PostgreSQL's `@>` JSONB containment operator for field-precise exact matching
27. As a developer, I want the `ne` operator to use `NOT (@>)` for field-precise exclusion
28. As a developer, I want the `gt/gte/lt/lte` operators to use PGroonga's `` &` `` script syntax with path-targeted number comparisons
29. As a developer, I want the `in` operator to produce multiple `@>` clauses OR'd together
30. As a developer, I want the `all` operator to produce multiple `@>` clauses AND'd together
31. As a developer, I want the free-text `query` parameter to be escaped before passing to PGroonga's `&@~` operator, so that Groonga special characters (OR, -, ", (, ), *, ~, column:) in user input are treated as literal text
32. As a developer, I want the `SearchBackend` enum to have two variants: `Elasticsearch(EsClient)` and `Pgroonga` (stateless, uses sqlx directly)
33. As a developer, I want the search resolver to branch early on backend type — building SQL for PGroonga and JSON for Elasticsearch — with shared post-search logic (score pairing, DataLoader fetch)
34. As a developer, I want the PGroonga index to be created with `TokenBigram` tokenizer by default in `db/init.sql`
35. As a developer, I want the `SearchIndexConfig` struct to gain an optional `backend` field, while `index` and `searchable_columns` remain for Elasticsearch compatibility
36. As a developer, I want the `searchMaterials` GraphQL field name to remain unchanged, so that existing client queries are not broken
37. As a developer, I want the PGroonga Docker image tag to be `groonga/pgroonga:4.0.8-debian-17`, matching the project's PostgreSQL 17 version
38. As a platform operator, I want the Elasticsearch infrastructure (pgx, RabbitMQ, ES) to be removable from docker-compose when no index uses the Elasticsearch backend
39. As a developer, I want the existing `seed_es.sh` script to remain but not be part of the default test flow, since PGroonga seeds via the normalized `db/seed.sql` and triggers populate `search_doc`
40. As a developer, I want the existing hurl integration tests to run against PGroonga, with only divergent assertions (scoring, contains behavior) updated
41. As a developer, I want the MCP `graphql` tool to work transparently with PGroonga since it uses the same GraphQL endpoint
42. As a developer, I want the `apply_pg()` row-level security function to reuse the existing `PermissionCache` for subquery-based filters, producing SQL WHERE fragments instead of ES JSON clauses
43. As a developer, I want the `rebuild_search_doc()` trigger function to execute a nested `jsonb_build_object` query assembling data from all child tables, called by lightweight per-table triggers
44. As a platform operator, I want both ES triggers (`pg_notify`) and PGroonga triggers (`rebuild_search_doc`) to be created unconditionally, so that either backend can be activated at any time
45. As a developer, I want the `contract.rs` test to continue validating that `MaterialDocument` fixtures match the expected shape, since the PGroonga `search_doc` uses the same document structure

## Implementation Decisions

### Architecture

- **Backend selection**: Per-index `backend` field in `config.yaml` under `search_indexes`. Values: `"elasticsearch"` (default if omitted) or `"pgroonga"`.
- **Backend enum**: `SearchBackend { Elasticsearch(EsClient), Pgroonga }` — `Pgroonga` has no state since queries go through sqlx directly.
- **Search resolver branching**: The `search()` function branches early on backend type. PGroonga path: build SQL WHERE → execute → get `(mat_no, score)` pairs → fetch full documents via existing DataLoader. Elasticsearch path: build JSON query → execute → `es_batch_enrich()`.
- **Document fetch after PGroonga hit**: Use the existing DataLoader `load_many(mat_nos)` mechanism — no new SQL enrichment function needed. This reuses the existing relation-loading code in `db.rs`.

### Data Model

- **New column**: `search_doc JSONB NOT NULL DEFAULT '{}'` on the `materials` table. Contains the full denormalized document (same shape as `MaterialDocument` in `contract.rs`), including `tenant_id` for row-level filtering.
- **Document shape**: Identical to Elasticsearch document — `mat_no`, `name`, `status`, `tenant_id`, `sizes` (array of objects), `colorways` (array of objects), `material_features` (array of objects with nested `feature_attributes`).
- **Population**: SQL function `rebuild_search_doc(p_mat_no TEXT)` executes a nested `jsonb_build_object` query assembling data from all child tables via subqueries with `jsonb_agg`. Called by per-table `AFTER INSERT OR UPDATE OR DELETE` triggers.
- **Trigger scope**: All mutations on `sizes`, `colorways`, `material_features`, `feature_attributes` fire `rebuild_search_doc()`. Triggers resolve parent `mat_no` from the changed row (special-casing `feature_attributes` which joins through `material_features`).
- **Coexistence with ES triggers**: Both ES `pg_notify` triggers and PGroonga `rebuild_search_doc` triggers are created unconditionally in `db/init.sql`. Either backend can be activated at config time.

### Database Schema Changes

- `db/init.sql`: Add `CREATE EXTENSION IF NOT EXISTS pgroonga;`, add `search_doc JSONB NOT NULL DEFAULT '{}'` column to `materials`, add `CREATE INDEX ... USING pgroonga (search_doc)` with `TokenBigram` tokenizer, add `rebuild_search_doc()` function, add per-table triggers.
- No changes to existing table structures, foreign keys, or `pg_notify` triggers.

### Filter Compilation

- **New function**: `build_pg_filter(filter: Option<&ValueAccessor>) -> (String, Vec<Value>)` returns a SQL WHERE clause string with bind parameter placeholders and a parallel vector of parameter values. User input is never interpolated into SQL strings — all values go through sqlx bind parameters.
- **Operator mapping**:

| GraphQL filter | PGroonga SQL |
|---|---|
| `eq` on string | `search_doc @> $1` (JSONB containment, field-precise) |
| `ne` on string | `NOT (search_doc @> $1)` |
| `contains` | `search_doc &@~ $1` (searches ALL text values, not field-scoped) |
| `starts_with` | `` search_doc &` 'paths @ ".field" && string @^ $1' `` (field-targeted prefix) |
| `ends_with` | `jsonb_extract_path_text(search_doc, 'field') ILIKE '%' \|\| $1` (sequential scan) |
| `in` | Multiple `@>` clauses OR'd: `(search_doc @> $1 OR search_doc @> $2 OR ...)` |
| `all` | Multiple `@>` clauses AND'd: `(search_doc @> $1 AND search_doc @> $2 AND ...)` |
| `gt/gte/lt/lte` | `` search_doc &` 'paths @ ".field" && number >= $1' `` (path-targeted range) |

- **Nested path handling**: For filters on `material_features.feature_name`, the `` &` `` operator uses the Groonga path syntax: `paths @ ".material_features[].feature_name"`. The path is derived mechanically from the GraphQL filter field name.

### Free-Text Search

- **Query escaping**: All user input to the free-text `query` parameter is escaped before passing to `&@~`. Escape characters: `\` → `\\`, `"` → `\"`, `(` → `\(`, `)` → `\)`, `*` → `\*`, `~` → `\~`, `-` → `\-`, `column:` → `column\:`. This ensures Groonga query syntax special characters are treated as literal text.
- **AND/OR**: `queryOperator: AND` produces space-separated words (implicit AND in Groonga syntax). `queryOperator: OR` joins words with ` OR `.
- **Empty query**: Returns all materials with `score = 0`, ordered by `mat_no ASC`. No `&@~` clause is added.

### Row-Level Security

- **New function**: `apply_pg(pool, cache, identity, row_filters) -> Result<(String, Vec<Value>)>` produces SQL WHERE fragments with bind parameters. Reuses `PermissionCache` for subquery-based filters. Same logic structure as `apply_es()` but outputs SQL instead of ES JSON.

### Config Changes

- **`SearchIndexConfig`**: Add `backend: Option<String>` field. `index` and `searchable_columns` remain for Elasticsearch compatibility; PGroonga ignores them.
- **`config.yaml`**: The `search_indexes.materials_search` block gains `backend: pgroonga`. The tokenizer is configurable via a `tokenizer` field (default: `TokenBigram`).

### Infrastructure

- **Docker image**: Replace `postgres:16` with `groonga/pgroonga:4.0.8-debian-17` in `docker-compose.yml`. This is a drop-in replacement — PGroonga is pre-installed, no custom Dockerfile needed.
- **ES pipeline**: When no index uses `backend: elasticsearch`, the pgx, RabbitMQ, and Elasticsearch containers are not started. This is handled by `docker compose` profiles or conditional startup.
- **Seed data**: PGroonga seeding uses the existing `db/seed.sql` (normalizes tables). Triggers automatically populate `search_doc`. The `seed_es.sh` script remains but is not part of the default flow.

### API Contract

- **GraphQL field**: `searchMaterials` — name unchanged, arguments unchanged.
- **Search hit type**: `MaterialsSearchHit` with `node: materials!` and `score: Float!` — unchanged.
- **`esQuery` argument**: Disabled when backend is PGroonga. Returns an error if provided.
- **Score**: Raw `pgroonga_score()` value exposed directly. No normalization. Different meaning than Elasticsearch scores, but consumers only sort by score, not compare absolute values.
- **Behavioral difference**: `contains` on a specific field (e.g., `name: { contains: "silk" }`) searches ALL text values in the document with PGroonga, not just the named field. This is an accepted divergence from Elasticsearch behavior.

### Search Flow (PGroonga)

1. Parse GraphQL arguments (query, queryOperator, filter, limit, offset)
2. Build row-level security SQL via `apply_pg()`
3. Build filter SQL via `build_pg_filter()`
4. Build free-text clause (escaped query with AND/OR syntax)
5. Combine into single SQL: `SELECT mat_no, pgroonga_score(search_doc) AS score FROM materials WHERE <rls> AND <filters> AND <freetext> ORDER BY score DESC, mat_no ASC LIMIT $n OFFSET $m`
6. Execute SQL → `Vec<(String, f64)>`
7. Load full materials via DataLoader `load_many(mat_nos)`
8. Pair materials with scores → `Vec<SearchHit>`

## Testing Decisions

### Test Seam

The primary test seam is the **GraphQL `searchMaterials` endpoint** — the same seam used by the existing `tests/search.hurl` integration tests. This is the highest possible seam: it exercises the full stack from HTTP through GraphQL schema, search resolver, filter compilation, database query, and response serialization. No new seams are needed.

### Test Strategy

- **Integration tests** (`tests/search.hurl`): Run the existing 32 hurl requests against PGroonga. Most tests (free-text search, eq, ne, in, pagination, queryOperator AND/OR, nested relation resolution) should pass unchanged. Tests for `contains`, `starts_with`, `ends_with`, and scoring assertions may need updated expected values to reflect PGroonga's behavioral differences.
- **Unit tests** (existing in `search/mod.rs`, `operator.rs`, `row_filter.rs`): Add parallel `build_pg_filter()` tests mirroring the existing `build_es_filter()` test patterns. Add `apply_pg()` tests mirroring `apply_es()` tests.
- **Contract tests** (`contract.rs`): Continue validating `MaterialDocument` fixtures — the PGroonga `search_doc` uses the same document structure.
- **Stub PGroonga client**: Consider adding a `PgroongaStub` variant to `SearchBackend` for unit testing the PGroonga code path without a live database, similar to how `EsClient::Stub` works for Elasticsearch tests.

### What Makes a Good Test

- Test external behavior (GraphQL query → expected response), not internal implementation details (which SQL operators are generated).
- The existing hurl tests are the gold standard — they test the API contract.
- New unit tests for `build_pg_filter()` should test the SQL output string and bind parameter values, verifying that user input is never interpolated.

### Prior Art

- `tests/search.hurl` (441 lines, 32 tests) — existing integration tests
- `src/schema/search/mod.rs` unit tests (lines 378-670) — existing unit tests with stub ES
- `src/schema/search/row_filter.rs` unit tests (lines 194-359) — existing row filter tests
- `src/schema/search/contract.rs` — document contract validation

## Out of Scope

- Removing Elasticsearch support — ES remains a configurable backend option
- Removing the `pg_notify` triggers — they coexist with PGroonga triggers
- Modifying the MCP server — it uses GraphQL passthrough and works transparently
- Modifying the auth-proxy or Keycloak integration — search is independent of auth
- Vector/semantic search — PGroonga 4.0.5+ supports this but is not in scope
- Aggregation/faceting — PGroonga does not support search-native aggregations; SQL `GROUP BY` is sufficient
- Chinese seed data — the tokenizer is forward-compatible; actual CJK data is not in scope
- Migrating existing ES data to PGroonga — a fresh `db/seed.sql` + trigger population handles initialization

## Further Notes

- The `TokenBigram` tokenizer handles Chinese, Japanese, Korean, and English text without additional configuration. It is the correct default for a multilingual materials domain.
- PGroonga's `&@~` operator is NOT crash-safe in PostgreSQL versions before 18. The project uses PostgreSQL 17 via the pgroonga image. If crash safety is critical, consider upgrading to PG 18 where WAL support is included.
- The `LIKE '%term'` fallback for `ends_with` has no index acceleration. At the current data scale (single-digit materials) this is acceptable. If performance becomes an issue, a `pg_trgm` GIN index can be added later.
- The denormalized `search_doc` approach means child row mutations trigger a full document rebuild. For the current scale this is O(children per material) ≈ O(10). If material counts grow to thousands, consider incremental JSONB updates instead of full rebuilds.
