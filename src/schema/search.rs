use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef, ValueAccessor,
};
use sqlx::{Pool, Postgres};

use crate::config::{SearchIndexConfig, SearchJoinConfig};

use super::db;
use super::util::{capitalize_first, gql_val};
use super::AppContext;

pub(crate) fn add_search_field(mut query: Object, index_cfg: &SearchIndexConfig) -> Object {
    let idx_cfg = index_cfg.clone();
    let type_name = idx_cfg.graphql_type.clone();
    query = query.field(
        Field::new(
            format!("search{}", capitalize_first(&idx_cfg.index)),
            TypeRef::named_nn_list_nn(&type_name),
            move |ctx| {
                let idx_cfg = idx_cfg.clone();
                    FieldFuture::new(async move {
                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>().unwrap();
                        let (es_client, es_url) = match (&app_ctx.es_client, &app_ctx.es_url) {
                            (Some(c), Some(u)) => (c.clone(), u.clone()),
                            _ => return Err(async_graphql::Error::new("Elasticsearch not configured")),
                        };
                        let query_str = ctx.args.get("query").and_then(|v| v.string().ok().map(String::from)).unwrap_or_default();
                        let filter = ctx.args.get("filter");
                        let limit = ctx.args.get("limit").and_then(|v| v.u64().ok()).map(|n| n as usize).unwrap_or(50);
                        let offset = ctx.args.get("offset").and_then(|v| v.u64().ok()).map(|n| n as usize).unwrap_or(0);
                        let results = es_search(&app_ctx.pool, &es_client, &es_url, &idx_cfg, &query_str, filter.as_ref(), limit, offset).await?;
                        let items: Vec<FieldValue> = results.into_iter().map(|r| FieldValue::value(gql_val(r))).collect();
                        Ok(Some(FieldValue::list(items)))
                    })
                },
            )
            .argument(InputValue::new("query", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new("filter", TypeRef::named(format!("{}SearchFilter", capitalize_first(&index_cfg.index)))))
            .argument(InputValue::new("limit", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT))),
        );
    query
}

async fn es_search(
    pool: &Pool<Postgres>,
    es_client: &reqwest::Client,
    es_url: &str,
    index_cfg: &SearchIndexConfig,
    query_str: &str,
    filters: Option<&ValueAccessor<'_>>,
    limit: usize,
    offset: usize,
) -> Result<Vec<serde_json::Value>, async_graphql::Error> {
    let all_searchable = collect_searchable_fields(index_cfg);
    let must_clauses = build_es_filter(filters, &all_searchable);
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

    let url = format!("{}/{}/_search", es_url.trim_end_matches('/'), index_cfg.index);
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

    let hits = body["hits"]["hits"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut results = Vec::new();
    for hit in hits {
        let source = hit.get("_source").cloned().unwrap_or(serde_json::Value::Null);
        let enriched = es_enrich_source(source, index_cfg, pool).await;
        results.push(enriched);
    }
    Ok(results)
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
    if let Some(f) = filter {
        if let Ok(obj) = f.object() {
            for (key, val) in obj.iter() {
                if val.is_null() { continue; }
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
        let rows = db::fetch_joined_rows(pool, &sql, &local_val).await;
        let enriched_rows: Vec<serde_json::Value> =
            futures::future::join_all(rows.into_iter().map(|item| {
                let jf_clone = jf.clone();
                async move { es_enrich_nested(item, &jf_clone.join_fields, pool).await }
            }))
            .await;
        enriched.insert(jf.index_field.clone(), serde_json::Value::Array(enriched_rows));
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
