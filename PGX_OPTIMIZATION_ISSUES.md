# pg_x Optimization Opportunities

Found during audit of `pg_x` for use in the morphis project (GraphQL → pg_notify → pg_x → Elasticsearch pipeline).

---

## 1. Single-doc ES indexing (high impact)

**File**: `src/downstream/elasticsearch.rs:120`, also `src/commands/consume.rs:164`

Every notification results in an individual `POST /{index}/_doc/{id}` HTTP request to Elasticsearch. For high-throughput scenarios this creates massive overhead — each request has TCP + HTTP overhead, and ES handles each doc individually.

**Recommendation**: Buffer documents and flush via ES Bulk API (`POST _bulk`). A configurable buffer (e.g., 500 docs or 5 seconds, whichever comes first) would increase throughput ~10-50x.

```rust
// Instead of:
self.client.post(&url).json(&result).send().await?;

// Use a BulkBuffer:
struct BulkBuffer {
    buffer: Vec<String>,
    max_size: usize,
    flush_interval: Duration,
}
impl BulkBuffer {
    async fn push(&mut self, index: &str, id: &str, doc: &Value) -> Result<()> {
        self.buffer.push(format!(r#"{{"index":{{"_index":"{index}","_id":"{id}"}}}}"#));
        self.buffer.push(serde_json::to_string(doc)?);
        if self.buffer.len() >= self.max_size * 2 {
            self.flush(&self.client, es_url).await?;
        }
    }
}
```

The replicate command (`src/commands/replicate/`) already has a batching pattern that could be reused.

---

## 2. N+1 child resolvers when `batch_by` not configured (high impact)

**File**: `src/graphql/executor.rs:319-351`

When a resolver lacks `batch_by`, the code falls through to `resolve_children_with_depth()` which executes one SQL query per parent per child field. This is the textbook N+1 problem.

```rust
// N+1 path (batch_by not set):
for field in child_fields {
    let rows = client.query(&resolver.sql, &[&param_value]).await?;
    // ...
}

// Batched path (batch_by set):
loader.execute(pool).await?;  // ONE SQL query with ANY($1)
```

**Recommendation**: Make this harder to misconfigure:
- Warn at startup if any `to-many` resolver lacks `batch_by`
- Consider making `batch_by` required for `to-many` resolvers
- Document the performance cliff clearly in the config spec

---

## 3. Sequential sibling resolution (medium impact)

**File**: `src/graphql/executor.rs:46-131`

Sibling child fields (e.g., `sizes`, `colorways`, `features` at the same depth) are resolved sequentially in a `for` loop:

```rust
for field in child_fields {
    let param_value = parent_obj.get(param_name);
    let rows = client.query(&resolver.sql, &[&param_vec]).await?;
    // ...
}
```

Since siblings share no data dependencies, they could run concurrently.

**Recommendation**: Use `futures::future::join_all` or `tokio::join!` for sibling fields at the same depth level:

```rust
let futures: Vec<_> = child_fields.iter().map(|field| {
    async {
        // resolve field
    }
}).collect();
futures::future::join_all(futures).await;
```

For a schema with 5 siblings at 100ms each, this reduces wall time from 500ms to ~100ms.

---

## 4. Single Postgres connection (medium impact)

**File**: `src/graphql/pool.rs:10`

The `QueryConn` wraps a single `tokio_postgres::Client` — all resolver queries share one connection. Since the resolver execution tree does multiple sequential queries per level, a pool would allow concurrent queries to sibling resolvers.

```rust
pub struct QueryConn {
    client: tokio_postgres::Client,  // single connection
}
```

**Recommendation**: Use a connection pool (`deadpool-postgres` or `bb8`). When sibling resolvers run concurrently (from #3), they'd each grab a connection from the pool instead of serializing on one.

---

## 5. Cross-message DataLoader caching (medium impact)

**File**: `src/graphql/dataloader.rs:77`

The DataLoader cache lives only for a single `execute_batched()` call. When multiple messages reference the same parent keys (e.g., multiple orders for the same product line), the same resolver queries run repeatedly.

**Recommendation**: Add an optional shared cache layer (e.g., `moka` or a simple `HashMap` with TTL) that persists across messages for hot keys:

```rust
pub struct CachedDataLoader {
    inner: DataLoader,
    cache: HashMap<Key, Vec<Value>>,
    ttl: Duration,
    last_refreshed: Instant,
}
```

This is especially valuable in the CONSUME mode (broker → GraphQL → ES) where message volume is highest.

---

## 6. Prepared statement reuse (low impact)

**File**: `src/graphql/executor.rs:94, 206`

SQL strings are sent raw each time. For high-throughput scenarios, prepared statements avoid repeated SQL parsing on the PG side.

```rust
// Each call:
let rows = client.query(&sql, &[&params]).await?;

// Could be:
let stmt = client.prepare(&sql).await?;
let rows = client.query(&stmt, &[&params]).await?;
```

Cache prepared statements by SQL string hash in the pool.

---

## 7. Configurable backpressure strategy (low impact)

**File**: `src/commands/listen.rs:358-361`

When the bounded channel (1024) is full, the oldest notification is silently dropped:

```rust
if channel.try_send(notification).is_err() {
    tracing::warn!("... dropping notification ...");
}
```

**Recommendation**: Make the drop-vs-block-vs-grow behavior configurable. For replay/backfill scenarios, users may prefer blocking or growing the channel. Add a config option:

```toml
[connections.main.listen]
channel_full_behavior = "block"  # or "drop_oldest" | "grow"
```

---

## Summary

| Priority | Issue | Expected Gain |
|---|---|---|
| 🔴 High | ES Bulk API | 10-50x throughput |
| 🔴 High | N+1 without batch_by | 10-100x on nested queries |
| 🟡 Medium | Sequential sibling resolution | 2-5x wall time on deep queries |
| 🟡 Medium | Single PG connection | 2-3x on concurrent resolvers |
| 🟡 Medium | Cross-message caching | 2-5x on hot keys |
| 🟢 Low | Prepared statements | 10-20% CPU reduction |
| 🟢 Low | Configurable backpressure | Operational safety |
