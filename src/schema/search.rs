use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef, ValueAccessor,
};
use sqlx::{Pool, Postgres};

use crate::config::{RowFilterConfig, SearchIndexConfig, SearchJoinConfig};

use super::db;
use super::util::{capitalize_first, gql_val};
use super::{AppContext, Identity};

pub(crate) fn add_search_field(
    mut query: Object,
    index_cfg: &SearchIndexConfig,
    row_filters: Vec<RowFilterConfig>,
) -> Object {
    let idx_cfg = index_cfg.clone();
    let type_name = idx_cfg.graphql_type.clone();
    query = query.field(
        Field::new(
            format!("search{}", capitalize_first(&idx_cfg.index)),
            TypeRef::named_nn_list_nn(&type_name),
            move |ctx| {
                let idx_cfg = idx_cfg.clone();
                let row_filters = row_filters.clone();
                FieldFuture::new(async move {
                    let app_ctx = ctx
                        .data::<std::sync::Arc<AppContext>>()
                        .map_err(|_| async_graphql::Error::new("internal context missing"))?;
                    let query_str = ctx
                        .args
                        .get("query")
                        .and_then(|v| v.string().ok().map(String::from))
                        .unwrap_or_default();
                    let filter = ctx.args.get("filter");
                    let limit = ctx
                        .args
                        .get("limit")
                        .and_then(|v| v.u64().ok())
                        .map(|n| n as usize)
                        .unwrap_or(50);
                    let offset = ctx
                        .args
                        .get("offset")
                        .and_then(|v| v.u64().ok())
                        .map(|n| n as usize)
                        .unwrap_or(0);
                    let identity = ctx.data::<Identity>().ok();
                    let results = es_search(
                        app_ctx,
                        &idx_cfg,
                        &query_str,
                        filter.as_ref(),
                        limit,
                        offset,
                        identity,
                        &row_filters,
                    )
                    .await?;
                    let items: Vec<FieldValue> = results
                        .into_iter()
                        .map(|r| FieldValue::value(gql_val(r)))
                        .collect();
                    Ok(Some(FieldValue::list(items)))
                })
            },
        )
        .argument(InputValue::new("query", TypeRef::named(TypeRef::STRING)))
        .argument(InputValue::new(
            "filter",
            TypeRef::named(format!(
                "{}SearchFilter",
                capitalize_first(&index_cfg.index)
            )),
        ))
        .argument(InputValue::new("limit", TypeRef::named(TypeRef::INT)))
        .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT))),
    );
    query
}

#[allow(clippy::too_many_arguments)]
async fn es_search(
    app_ctx: &AppContext,
    index_cfg: &SearchIndexConfig,
    query_str: &str,
    filters: Option<&ValueAccessor<'_>>,
    limit: usize,
    offset: usize,
    identity: Option<&Identity>,
    row_filters: &[RowFilterConfig],
) -> Result<Vec<serde_json::Value>, async_graphql::Error> {
    let (es_client, es_url) = match (&app_ctx.es_client, &app_ctx.es_url) {
        (Some(c), Some(u)) => (c.clone(), u.clone()),
        _ => return Err(async_graphql::Error::new("Elasticsearch not configured")),
    };

    let all_searchable = collect_searchable_fields(index_cfg);
    let mut must_clauses = build_es_filter(filters, &all_searchable);
    let filter_clauses = build_es_row_filters(app_ctx, identity, row_filters).await?;
    must_clauses.extend(filter_clauses);

    let mut bool_body = serde_json::json!({
        "must": must_clauses
    });
    if !query_str.is_empty() {
        bool_body["should"] = serde_json::json!([{ "multi_match": { "query": query_str, "fields": all_searchable, "type": "cross_fields" } }]);
        bool_body["minimum_should_match"] = serde_json::json!(1);
    }
    let mut es_query = serde_json::json!({
        "query": { "bool": bool_body },
        "size": limit
    });
    if offset > 0 {
        es_query["from"] = serde_json::json!(offset);
    }

    let url = format!(
        "{}/{}/_search",
        es_url.trim_end_matches('/'),
        index_cfg.index
    );
    let resp = es_client
        .post(&url)
        .json(&es_query)
        .send()
        .await
        .map_err(|e| async_graphql::Error::new(format!("ES request failed: {}", e)))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| async_graphql::Error::new(format!("ES parse failed: {}", e)))?;

    let hits = body["hits"]["hits"].as_array().cloned().unwrap_or_default();

    let mut results = Vec::new();
    for hit in hits {
        let source = hit
            .get("_source")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let enriched = es_enrich_source(source, index_cfg, &app_ctx.pool).await;
        results.push(enriched);
    }
    Ok(results)
}

async fn build_es_row_filters(
    app_ctx: &AppContext,
    identity: Option<&Identity>,
    row_filters: &[RowFilterConfig],
) -> Result<Vec<serde_json::Value>, async_graphql::Error> {
    let identity = match identity {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };
    let mut clauses = Vec::new();
    for rf in row_filters {
        let Some(val) = identity.header_value(rf.header_name()) else {
            continue;
        };
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
                let ttl = std::time::Duration::from_secs(cache_ttl_secs.unwrap_or(60));
                let cols: Vec<String> = match_columns.clone();
                let rows = {
                    let mut cache = app_ctx.permission_cache.lock().await;
                    if let Some(cached) = cache.get(&cache_key) {
                        cached
                    } else {
                        let sql = format!(
                            "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT DISTINCT {} FROM {} WHERE {} = $1) t",
                            cols.join(", "),
                            from_source,
                            user_column,
                        );
                        let result = db::fetch_joined_rows(&app_ctx.pool, &sql, val).await?;
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

fn collect_searchable_fields(cfg: &SearchIndexConfig) -> Vec<String> {
    let mut fields = cfg.searchable_fields.clone();
    for jf in &cfg.join_fields {
        tracing::debug!("Collecting searchable fields for join: {}", jf.name);
        for f in &jf.searchable_fields {
            fields.push(format!("{}.{}", jf.index_field, f));
        }
        for nested in &jf.join_fields {
            for f in &nested.searchable_fields {
                fields.push(format!("{}.{}.{}", jf.index_field, nested.index_field, f));
            }
        }
    }
    fields
}

fn build_es_filter(
    filter: Option<&ValueAccessor>,
    _all_fields: &[String],
) -> Vec<serde_json::Value> {
    let mut must = Vec::new();
    if let Some(f) = filter
        && let Ok(obj) = f.object()
    {
        for (key, val) in obj.iter() {
            if val.is_null() {
                continue;
            }
            let key_str = key.as_str();
            if let Ok(s) = val.string() {
                if !s.is_empty() {
                    must.push(serde_json::json!({
                        "term": { key_str: s }
                    }));
                }
            } else if let Ok(n) = val.i64() {
                must.push(serde_json::json!({
                    "term": { key_str: n }
                }));
            } else if let Ok(n) = val.f64() {
                must.push(serde_json::json!({
                    "term": { key_str: n }
                }));
            }
        }
    }
    must
}

async fn es_enrich_nested(
    source: serde_json::Value,
    join_fields: &[SearchJoinConfig],
    pool: &Pool<Postgres>,
) -> serde_json::Value {
    if join_fields.is_empty() {
        return source;
    }
    let mut enriched = match source {
        serde_json::Value::Object(m) => m,
        other => return other,
    };
    for jf in join_fields {
        let local_val = enriched
            .get(&jf.local_field)
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_default();
        if local_val.is_empty() {
            continue;
        }
        let sql = format!(
            "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT * FROM {} WHERE {} = $1) t",
            jf.table, jf.foreign_field
        );
        let rows = db::fetch_joined_rows(pool, &sql, &local_val)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("ES enrichment query failed: {:?}", e);
                vec![]
            });
        let enriched_rows: Vec<serde_json::Value> =
            futures::future::join_all(rows.into_iter().map(|item| {
                let jf_clone = jf.clone();
                async move { es_enrich_nested(item, &jf_clone.join_fields, pool).await }
            }))
            .await;
        enriched.insert(
            jf.index_field.clone(),
            serde_json::Value::Array(enriched_rows),
        );
    }
    serde_json::Value::Object(enriched)
}

async fn es_enrich_source(
    source: serde_json::Value,
    cfg: &SearchIndexConfig,
    pool: &Pool<Postgres>,
) -> serde_json::Value {
    es_enrich_nested(source, &cfg.join_fields, pool).await
}
