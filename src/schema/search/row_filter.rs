use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_graphql::Error;
use serde_json::Value;
use sqlx::{Pool, Postgres};
use tokio::sync::Mutex;

use crate::config::RowFilterConfig;
use crate::schema::{Identity, db};

/// Cached result of a subquery row filter, keyed by source table + user column
/// + identity value.
#[derive(Debug, Clone)]
struct PermissionCacheEntry {
    values: Vec<Value>,
    expires_at: Instant,
}

/// TTL'd cache for subquery row-filter results. Owned by the search module —
/// the only user of the subquery strategy.
#[derive(Debug, Clone)]
pub(crate) struct PermissionCache {
    store: HashMap<String, PermissionCacheEntry>,
}

impl PermissionCache {
    pub(crate) fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<Vec<Value>> {
        self.store.get(key).and_then(|entry| {
            if Instant::now() < entry.expires_at {
                Some(entry.values.clone())
            } else {
                None
            }
        })
    }

    pub(crate) fn set(&mut self, key: String, values: Vec<Value>, ttl: Duration) {
        self.store.insert(
            key,
            PermissionCacheEntry {
                expires_at: Instant::now() + ttl,
                values,
            },
        );
    }
}

/// The row filters that apply to an identity, as `(filter, header value)`
/// pairs. Both compilers below consume this — one interpretation, two outputs.
fn matching_row_filters<'a, 'b>(
    identity: &'b Identity,
    row_filters: &'a [RowFilterConfig],
) -> Vec<(&'a RowFilterConfig, &'b str)> {
    row_filters
        .iter()
        .filter_map(|rf| identity.header_value(rf.header_name()).map(|v| (rf, v)))
        .collect()
}

/// SQL compiler: appends `WHERE`/`AND` clauses and bound params for PG resolvers.
pub(crate) fn apply_row_filters(
    sql: &mut String,
    params: &mut Vec<String>,
    identity: &Identity,
    row_filters: &[RowFilterConfig],
) {
    for (rf, val) in matching_row_filters(identity, row_filters) {
        let clause = match rf {
            RowFilterConfig::ColumnFilter { column, .. } => {
                if params.is_empty() {
                    format!(" WHERE {} = ${}", column, params.len() + 1)
                } else {
                    format!(" AND {} = ${}", column, params.len() + 1)
                }
            }
            RowFilterConfig::SubqueryFilter {
                columns,
                match_columns,
                from_source,
                user_column,
                ..
            } => {
                let prefix = if params.is_empty() {
                    " WHERE "
                } else {
                    " AND "
                };
                format!(
                    "{} ({}) IN (SELECT {} FROM {} WHERE {} = ${})",
                    prefix,
                    columns.join(", "),
                    match_columns.join(", "),
                    from_source,
                    user_column,
                    params.len() + 1,
                )
            }
        };
        sql.push_str(&clause);
        params.push(val.to_string());
    }
}

/// ES compiler: resolves the same row filters into ES bool clauses. Subquery
/// filters reuse the shared permission cache instead of emitting SQL.
pub(crate) async fn apply_es(
    pool: &Pool<Postgres>,
    permission_cache: &Arc<Mutex<PermissionCache>>,
    identity: Option<&Identity>,
    row_filters: &[RowFilterConfig],
) -> Result<Vec<Value>, Error> {
    let identity = match identity {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };
    let mut clauses = Vec::new();
    for (rf, val) in matching_row_filters(identity, row_filters) {
        match rf {
            RowFilterConfig::ColumnFilter { column, .. } => {
                clauses.push(serde_json::json!({
                    "term": { column: val }
                }));
            }
            RowFilterConfig::SubqueryFilter {
                columns,
                match_columns,
                from_source,
                user_column,
                cache_ttl_secs,
                ..
            } => {
                let cache_key = format!("{}:{}:{}", from_source, user_column, val);
                let ttl = Duration::from_secs(cache_ttl_secs.unwrap_or(60));
                let cols: Vec<String> = match_columns.clone();
                let rows = {
                    let mut cache = permission_cache.lock().await;
                    if let Some(cached) = cache.get(&cache_key) {
                        cached
                    } else {
                        let sql = format!(
                            "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT DISTINCT {} FROM {} WHERE {} = $1) t",
                            cols.join(", "),
                            from_source,
                            user_column,
                        );
                        let result =
                            db::fetch_rows(pool, &sql, &[db::Bind::Text(val)]).await?;
                        cache.set(cache_key, result.clone(), ttl);
                        result
                    }
                };
                if rows.is_empty() {
                    let false_clause = serde_json::json!({
                        "term": { "__no_match": "__impossible" }
                    });
                    clauses.push(false_clause);
                } else {
                    let mut should = Vec::new();
                    for row in &rows {
                        let mut must = Vec::new();
                        if let Some(obj) = row.as_object() {
                            for (col_idx, col) in columns.iter().enumerate() {
                                if let Some(mcol) = match_columns.get(col_idx)
                                    && let Some(v) = obj.get(mcol.as_str())
                                {
                                    must.push(serde_json::json!({ "term": { col: v } }));
                                }
                            }
                        }
                        if !must.is_empty() {
                            should.push(serde_json::json!({ "bool": { "must": must } }));
                        }
                    }
                    if !should.is_empty() {
                        clauses.push(serde_json::json!({
                            "bool": { "should": should, "minimum_should_match": 1 }
                        }));
                    }
                }
            }
        }
    }
    Ok(clauses)
}

/// PG compiler: resolves row filters into SQL WHERE fragments for PGroonga
/// search queries. Reuses the same PermissionCache as the ES compiler.
pub(crate) async fn apply_pg(
    pool: &Pool<Postgres>,
    permission_cache: &Arc<Mutex<PermissionCache>>,
    identity: Option<&Identity>,
    row_filters: &[RowFilterConfig],
    params: &mut Vec<String>,
) -> Result<Vec<String>, Error> {
    let identity = match identity {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };
    let mut fragments = Vec::new();
    for (rf, val) in matching_row_filters(identity, row_filters) {
        match rf {
            RowFilterConfig::ColumnFilter { column, .. } => {
                params.push(val.to_string());
                fragments.push(format!("m.{} = ${}", column, params.len()));
            }
            RowFilterConfig::SubqueryFilter {
                columns,
                match_columns,
                from_source,
                user_column,
                cache_ttl_secs,
                ..
            } => {
                let cache_key = format!("{}:{}:{}", from_source, user_column, val);
                let ttl = Duration::from_secs(cache_ttl_secs.unwrap_or(60));
                let cols: Vec<String> = match_columns.clone();
                let rows = {
                    let mut cache = permission_cache.lock().await;
                    if let Some(cached) = cache.get(&cache_key) {
                        cached
                    } else {
                        let sql = format!(
                            "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT DISTINCT {} FROM {} WHERE {} = $1) t",
                            cols.join(", "),
                            from_source,
                            user_column,
                        );
                        let result =
                            db::fetch_rows(pool, &sql, &[db::Bind::Text(val)]).await?;
                        cache.set(cache_key, result.clone(), ttl);
                        result
                    }
                };
                if rows.is_empty() {
                    fragments.push("1 = 0".to_string());
                } else {
                    let mut or_parts = Vec::new();
                    for row in &rows {
                        if let Some(obj) = row.as_object() {
                            let mut and_parts = Vec::new();
                            for (col_idx, col) in columns.iter().enumerate() {
                                if let Some(mcol) = match_columns.get(col_idx)
                                    && let Some(v) = obj.get(mcol.as_str())
                                {
                                    if let Some(s) = v.as_str() {
                                        params.push(s.to_string());
                                        and_parts
                                            .push(format!("m.{} = ${}", col, params.len()));
                                    } else if let Some(n) = v.as_i64() {
                                        params.push(n.to_string());
                                        and_parts
                                            .push(format!("m.{} = ${}", col, params.len()));
                                    }
                                }
                            }
                            if !and_parts.is_empty() {
                                or_parts.push(format!(
                                    "({})",
                                    and_parts.join(" AND ")
                                ));
                            }
                        }
                    }
                    if !or_parts.is_empty() {
                        fragments.push(format!(
                            "({})",
                            or_parts.join(" OR ")
                        ));
                    }
                }
            }
        }
    }
    Ok(fragments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn identity_with(header: &str, value: &str) -> Identity {
        let mut headers = HashMap::new();
        headers.insert(header.to_string(), value.to_string());
        Identity::from_raw(headers)
    }

    fn column_filter() -> RowFilterConfig {
        RowFilterConfig::ColumnFilter {
            column: "tenant_id".into(),
            from_header: "X-Tenant-ID".into(),
            auto_set: false,
        }
    }

    fn subquery_filter() -> RowFilterConfig {
        RowFilterConfig::SubqueryFilter {
            from_header: "X-User-ID".into(),
            columns: vec!["tenant_id".into()],
            match_columns: vec!["tenant_id".into()],
            from_source: "user_permissions".into(),
            user_column: "user_id".into(),
            cache_ttl_secs: Some(60),
        }
    }

    #[test]
    fn sql_compiler_scopes_query_by_tenant_identity() {
        let mut sql = String::from("SELECT * FROM materials");
        let mut params = Vec::new();
        let row_filters = vec![column_filter()];

        apply_row_filters(
            &mut sql,
            &mut params,
            &identity_with("x-tenant-id", "tenant-a"),
            &row_filters,
        );
        assert!(sql.contains(" WHERE tenant_id = $1"));
        assert_eq!(params, vec!["tenant-a"]);

        sql = String::from("SELECT * FROM materials");
        params.clear();
        apply_row_filters(
            &mut sql,
            &mut params,
            &identity_with("x-tenant-id", "tenant-b"),
            &row_filters,
        );
        assert!(sql.contains("tenant_id = $1"));
        assert_eq!(params, vec!["tenant-b"]);

        sql = String::from("SELECT * FROM materials");
        params.clear();
        apply_row_filters(&mut sql, &mut params, &Identity::default(), &row_filters);
        assert!(!sql.contains("tenant_id"));
        assert!(params.is_empty());
    }

    #[test]
    fn sql_compiler_emits_subquery_for_missing_identity_noop() {
        let mut sql = String::from("SELECT * FROM materials");
        let mut params = Vec::new();
        apply_row_filters(
            &mut sql,
            &mut params,
            &Identity::default(),
            &[subquery_filter()],
        );
        assert_eq!(sql, "SELECT * FROM materials");
        assert!(params.is_empty());
    }

    #[tokio::test]
    async fn sql_and_es_compilers_agree_on_column_filter() {
        let identity = identity_with("x-tenant-id", "tenant-a");
        let row_filters = vec![column_filter()];

        let mut sql = String::from("SELECT * FROM materials");
        let mut params = Vec::new();
        apply_row_filters(&mut sql, &mut params, &identity, &row_filters);
        assert!(sql.contains(" WHERE tenant_id = $1"));
        assert_eq!(params, vec!["tenant-a"]);

        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://user:pass@localhost:5432/morphis")
            .unwrap();
        let cache = Arc::new(Mutex::new(PermissionCache::new()));
        let es = apply_es(&pool, &cache, Some(&identity), &row_filters)
            .await
            .unwrap();
        assert_eq!(
            es,
            vec![serde_json::json!({ "term": { "tenant_id": "tenant-a" } })]
        );
    }

    #[tokio::test]
    async fn es_compiler_resolves_subquery_from_cache() {
        let identity = identity_with("x-user-id", "user-1");
        let row_filters = vec![subquery_filter()];
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://user:pass@localhost:5432/morphis")
            .unwrap();
        let cache = Arc::new(Mutex::new(PermissionCache::new()));
        cache.lock().await.set(
            "user_permissions:user_id:user-1".into(),
            vec![serde_json::json!({ "tenant_id": "tenant-a" })],
            Duration::from_secs(60),
        );

        let es = apply_es(&pool, &cache, Some(&identity), &row_filters)
            .await
            .unwrap();
        assert_eq!(
            es,
            vec![serde_json::json!({
                "bool": {
                    "should": [
                        { "bool": { "must": [{ "term": { "tenant_id": "tenant-a" } }] } }
                    ],
                    "minimum_should_match": 1
                }
            })]
        );
    }

    #[tokio::test]
    async fn es_compiler_emits_impossible_clause_when_subquery_empty() {
        let identity = identity_with("x-user-id", "user-nobody");
        let row_filters = vec![subquery_filter()];
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://user:pass@localhost:5432/morphis")
            .unwrap();
        let cache = Arc::new(Mutex::new(PermissionCache::new()));
        cache.lock().await.set(
            "user_permissions:user_id:user-nobody".into(),
            vec![],
            Duration::from_secs(60),
        );

        let es = apply_es(&pool, &cache, Some(&identity), &row_filters)
            .await
            .unwrap();
        assert_eq!(
            es,
            vec![serde_json::json!({ "term": { "__no_match": "__impossible" } })]
        );
    }

    #[test]
    fn cache_hits_within_ttl_and_expires_after() {
        let mut cache = PermissionCache::new();
        cache.set("k".into(), vec![serde_json::json!({ "x": 1 })], Duration::from_secs(60));
        assert_eq!(cache.get("k"), Some(vec![serde_json::json!({ "x": 1 })]));

        std::thread::sleep(Duration::from_millis(30));
        let mut short = PermissionCache::new();
        short.set("k".into(), vec![serde_json::json!({ "x": 1 })], Duration::from_millis(5));
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(short.get("k"), None);
    }
}
