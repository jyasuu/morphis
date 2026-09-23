mod es_client;
mod operator;
mod row_filter;

#[cfg(test)]
mod contract;

pub(crate) use operator::{build_search_filter_input, nested_filter_inputs, operator_inputs, query_operator_enum};
pub(crate) use row_filter::apply_row_filters;

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::sync::Mutex;

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, InputValue, Object, TypeRef, ValueAccessor};
use async_graphql::{Name, Value};
use sqlx::{Pool, Postgres};

use identity_auth::circuit_breaker::CircuitBreaker;

use crate::config::{Config, RowFilterConfig, SearchIndexConfig, SearchJoinConfig};

use super::db;
use super::util::{capitalize_first, gql_val};
use super::Identity;

/// Dependencies for the search module, received explicitly at construction
/// instead of being read out of the app context field-by-field.
pub(crate) struct SearchService {
    pool: Pool<Postgres>,
    es: es_client::EsClient,
    permission_cache: Arc<Mutex<row_filter::PermissionCache>>,
}

impl SearchService {
    pub(crate) fn new(config: &Config, pool: Pool<Postgres>) -> Self {
        let es = match &config.elasticsearch {
            Some(es_cfg) => es_client::EsClient::live(
                reqwest::Client::new(),
                es_cfg.url.clone(),
                CircuitBreaker::new(config.circuit_breakers.es.to_circuit_breaker_config()),
            ),
            None => es_client::EsClient::unavailable(),
        };
        Self {
            pool,
            es,
            permission_cache: Arc::new(Mutex::new(row_filter::PermissionCache::new())),
        }
    }
}

/// Add the `search{Index}` query field for an index.
pub(crate) fn add_search_field(
    mut query: Object,
    index_cfg: &SearchIndexConfig,
    row_filters: Vec<RowFilterConfig>,
    service: Arc<SearchService>,
) -> Object {
    let idx_cfg = Arc::new(index_cfg.clone());
    let searchable_fields = Arc::new(collect_searchable_fields(index_cfg));
    let type_name = idx_cfg.graphql_type.clone();
    let hit_type_name = format!("{}SearchHit", type_name);
    query = query.field(
        Field::new(
            format!("search{}", capitalize_first(&idx_cfg.index)),
            TypeRef::named_nn_list_nn(&hit_type_name),
            move |ctx| {
                let idx_cfg = idx_cfg.clone();
                let searchable_fields = searchable_fields.clone();
                let row_filters = row_filters.clone();
                let service = service.clone();
                FieldFuture::new(async move {
                    let query_str = ctx
                        .args
                        .get("query")
                        .and_then(|v| v.string().ok().map(String::from))
                        .unwrap_or_default();
                    let query_operator = ctx
                        .args
                        .get("queryOperator")
                        .and_then(|v| v.enum_name().ok().map(String::from))
                        .unwrap_or_else(|| "OR".to_string());
                    let es_query_raw = ctx
                        .args
                        .get("esQuery")
                        .and_then(|v| v.string().ok().map(String::from));
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
                    let results = search(
                        &service,
                        &idx_cfg,
                        &searchable_fields,
                        &query_str,
                        &query_operator,
                        es_query_raw.as_deref(),
                        filter.as_ref(),
                        limit,
                        offset,
                        identity,
                        &row_filters,
                    )
                    .await?;
                    let items: Vec<FieldValue> = results
                        .into_iter()
                        .map(|(source, score)| {
                            FieldValue::value(gql_val(serde_json::json!({
                                "node": source,
                                "score": score,
                            })))
                        })
                        .collect();
                    Ok(Some(FieldValue::list(items)))
                })
            },
        )
        .argument(InputValue::new("query", TypeRef::named(TypeRef::STRING)))
        .argument(InputValue::new(
            "queryOperator",
            TypeRef::named("QueryOperator"),
        ))
        .argument(InputValue::new("esQuery", TypeRef::named(TypeRef::STRING)))
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

/// The per-index hit object type for a search index: `node` carries the matched
/// document and `score` the Elasticsearch relevance score.
pub(crate) fn build_search_hit_object(index_cfg: &SearchIndexConfig) -> Object {
    let type_name = index_cfg.graphql_type.clone();
    let hit_type_name = format!("{}SearchHit", type_name);
    let node_type = type_name.clone();

    let node_field = Field::new("node", TypeRef::named_nn(node_type), move |ctx| {
        FieldFuture::new(async move {
            let parent = ctx
                .parent_value
                .as_value()
                .ok_or_else(|| async_graphql::Error::new("not a value"))?;
            let val = match parent {
                Value::Object(map) => map.get(&Name::new("node")).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
            Ok(Some(FieldValue::value(val)))
        })
    });

    let score_field = Field::new("score", TypeRef::named_nn(TypeRef::FLOAT), |ctx| {
        FieldFuture::new(async move {
            let parent = ctx
                .parent_value
                .as_value()
                .ok_or_else(|| async_graphql::Error::new("not a value"))?;
            let val = match parent {
                Value::Object(map) => map.get(&Name::new("score")).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
            Ok(Some(FieldValue::value(val)))
        })
    });

    Object::new(&hit_type_name)
        .field(node_field)
        .field(score_field)
}

/// Single interface for the search module: a query against one index config
/// returns the hits the filter semantics say it should, each paired with the
/// relevance score Elasticsearch assigned it.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn search(
    service: &SearchService,
    index_cfg: &SearchIndexConfig,
    searchable_fields: &[String],
    query_str: &str,
    query_operator: &str,
    es_query_raw: Option<&str>,
    filters: Option<&ValueAccessor<'_>>,
    limit: usize,
    offset: usize,
    identity: Option<&Identity>,
    row_filters: &[RowFilterConfig],
) -> Result<Vec<(serde_json::Value, f64)>, async_graphql::Error> {
    tracing::debug!(
        index = %index_cfg.index,
        query = %query_str.chars().take(80).collect::<String>(),
        limit,
        offset,
        "ES search called"
    );

    let filter_clauses = row_filter::apply_es(
        &service.pool,
        &service.permission_cache,
        identity,
        row_filters,
    )
    .await?;

    let bool_body = if let Some(raw) = es_query_raw {
        if !index_cfg.allow_raw_es_query {
            return Err(async_graphql::Error::new(
                "Raw ES queries not enabled for this index",
            ));
        }
        let raw_query: serde_json::Value = serde_json::from_str(raw)
            .map_err(|e| async_graphql::Error::new(format!("Invalid esQuery JSON: {}", e)))?;
        let mut must = vec![raw_query];
        must.extend(filter_clauses);
        serde_json::json!({ "must": must })
    } else {
        let all_searchable = searchable_fields;
        let mut must_clauses = operator::build_es_filter(filters);
        must_clauses.extend(filter_clauses);
        let mut bool_body = serde_json::json!({ "must": must_clauses });
        if !query_str.trim().is_empty() {
            let mut multi_match = serde_json::json!({
                "query": query_str,
                "fields": all_searchable,
                "type": "cross_fields",
            });
            if query_operator == "AND" {
                multi_match["operator"] = serde_json::json!("and");
            }
            bool_body["should"] = serde_json::json!([{ "multi_match": multi_match }]);
            bool_body["minimum_should_match"] = serde_json::json!(1);
        }
        bool_body
    };

    let mut es_query = serde_json::json!({
        "query": { "bool": bool_body },
        "size": limit
    });
    if offset > 0 {
        es_query["from"] = serde_json::json!(offset);
    }

    let body = service
        .es
        .search(&index_cfg.index, &es_query)
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

    let hits = body["hits"]["hits"].as_array().cloned().unwrap_or_default();

    let mut sources: Vec<serde_json::Value> = hits
        .iter()
        .map(|hit| {
            hit.get("_source")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        })
        .collect();
    let scores: Vec<f64> = hits
        .iter()
        .map(|hit| hit.get("_score").and_then(serde_json::Value::as_f64).unwrap_or(0.0))
        .collect();

    es_batch_enrich(&service.pool, &mut sources, &index_cfg.join_fields).await?;
    Ok(sources.into_iter().zip(scores).collect())
}

fn collect_searchable_fields(cfg: &SearchIndexConfig) -> Vec<String> {
    let mut fields = cfg.searchable_fields.clone();
    for jf in &cfg.join_fields {
        tracing::debug!(join = %jf.name, "Collecting searchable fields for join");
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

/// Normalise a JSON scalar to its string key form. FK columns come back from
/// `json_agg(row_to_json(t))` as JSON numbers when the column is `int`, so key
/// extraction must not rely on `as_str()` (which returns `None` for numbers and
/// would wipe nested children to `[]`).
fn key_value(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        Some(s.to_string())
    } else {
        v.as_i64().map(|n| n.to_string())
    }
}

/// Compensation layer: re-fetches child rows from Postgres and re-attaches them
/// to search hits so returned documents stay fresh even when the ES document is
/// stale. Kept until the P6 document contract is stable and child-table writes
/// re-index the parent (db/triggers.sql + contract/materials fixtures prove the
/// pipeline keeps documents current); removable once that is proven by the
/// integration suite.
fn es_batch_enrich<'a>(
    pool: &'a Pool<Postgres>,
    sources: &'a mut [serde_json::Value],
    join_fields: &'a [SearchJoinConfig],
) -> Pin<Box<dyn Future<Output = Result<(), async_graphql::Error>> + Send + 'a>> {
    Box::pin(async move {
        for jf in join_fields {
            if sources.is_empty() {
                continue;
            }

            let keys: Vec<String> = sources
                .iter()
                .filter_map(|s| {
                    s.get(&jf.local_field)
                        .and_then(key_value)
                        .filter(|v| !v.is_empty())
                })
                .collect();

            if keys.is_empty() {
                for source in sources.iter_mut() {
                    if let Some(obj) = source.as_object_mut() {
                        obj.insert(jf.index_field.clone(), serde_json::Value::Array(vec![]));
                    }
                }
                continue;
            }

            let sql = format!(
                "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT * FROM {} WHERE {}::text = ANY($1)) t",
                jf.table, jf.foreign_field
            );
            let mut all_children: Vec<serde_json::Value> =
                db::fetch_rows(pool, &sql, &[db::Bind::Array(&keys)])
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(error = %e.message, "ES batch enrichment query failed");
                        vec![]
                    });

            if !jf.join_fields.is_empty() {
                es_batch_enrich(pool, &mut all_children, &jf.join_fields).await?;
            }

            let mut grouped: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
            for child in &all_children {
                if let Some(v) = child.get(&jf.foreign_field).and_then(key_value) {
                    grouped.entry(v).or_default().push(child.clone());
                }
            }

            for source in sources.iter_mut() {
                let key = source.get(&jf.local_field).and_then(key_value).unwrap_or_default();
                let children = grouped.remove(&key).unwrap_or_default();
                if let Some(obj) = source.as_object_mut() {
                    obj.insert(jf.index_field.clone(), serde_json::Value::Array(children));
                }
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::Schema;
    use crate::schema::{build_schema_with_search, execute};

    fn stub_service(pool: Pool<Postgres>, docs: Vec<serde_json::Value>) -> Arc<SearchService> {
        Arc::new(SearchService {
            pool,
            es: es_client::EsClient::stub(docs),
            permission_cache: Arc::new(Mutex::new(row_filter::PermissionCache::new())),
        })
    }

    fn mat_nos(resp: &async_graphql::Response) -> Vec<String> {
        let data = serde_json::to_value(&resp.data).unwrap();
        data["searchMaterials"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v["node"]["mat_no"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn key_value_normalises_strings_and_numbers() {
        assert_eq!(key_value(&serde_json::json!("M001")), Some("M001".to_string()));
        assert_eq!(key_value(&serde_json::json!(7)), Some("7".to_string()));
        assert_eq!(key_value(&serde_json::json!(-3)), Some("-3".to_string()));
        assert_eq!(key_value(&serde_json::Value::Null), None);
    }

    async fn run(schema: &Schema, query: &str) -> async_graphql::Response {
        execute(schema, async_graphql::Request::new(query), Identity::default()).await
    }

    #[tokio::test]
    async fn filter_operators_are_interpreted_against_stub_es() {
        let mut cfg: crate::config::Config = crate::config::Config::from_file("config.yaml").unwrap();
        for index in &mut cfg.search_indexes {
            index.join_fields = Vec::new();
        }
        let config = Arc::new(cfg);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy(&config.database.url)
            .unwrap();
        let docs = vec![
            serde_json::json!({ "mat_no": "M-1", "name": "Alpha", "status": "active", "count": 5 }),
            serde_json::json!({ "mat_no": "M-2", "name": "Beta", "status": "active", "count": 20 }),
            serde_json::json!({ "mat_no": "M-3", "name": "Gamma", "status": "discontinued", "count": 8 }),
        ];
        let service = stub_service(pool.clone(), docs);
        let schema = build_schema_with_search(config, pool, service).await;

        let resp = run(&schema, "{ searchMaterials(filter: { status: { eq: \"active\" } }) { node { mat_no } } }").await;
        assert!(resp.is_ok(), "eq filter errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-1", "M-2"]);

        let resp = run(
            &schema,
            "{ searchMaterials(filter: { mat_no: { in: [\"M-1\", \"M-3\"] } }) { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "in filter errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-1", "M-3"]);

        let resp =
            run(&schema, "{ searchMaterials(filter: { name: { contains: \"et\" } }) { node { mat_no } } }")
                .await;
        assert!(resp.is_ok(), "contains filter errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-2"]);

        let resp = run(
            &schema,
            "{ searchMaterials(filter: { name: { starts_with: \"Ga\" } }) { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "starts_with filter errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-3"]);

        let resp =
            run(&schema, "{ searchMaterials(filter: { status: { ne: \"active\" } }) { node { mat_no } } }")
                .await;
        assert!(resp.is_ok(), "ne filter errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-3"]);

        let resp = run(&schema, "{ searchMaterials(query: \"alpha\") { node { mat_no } } }").await;
        assert!(resp.is_ok(), "query text errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-1"]);

        let resp = run(
            &schema,
            "{ searchMaterials(esQuery: \"{\\\"term\\\":{\\\"mat_no.keyword\\\":\\\"M-2\\\"}}\") { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "raw esQuery errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-2"]);

        let resp = run(
            &schema,
            "{ searchMaterials(esQuery: \"{\\\"range\\\":{\\\"count\\\":{\\\"gte\\\":10}}}\") { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "raw range errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-2"]);

        let resp = run(&schema, "{ searchMaterials(limit: 1, offset: 1) { node { mat_no } } }").await;
        assert!(resp.is_ok(), "pagination errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-2"]);
    }

    #[tokio::test]
    async fn query_operator_and_requires_every_term_to_match() {
        let mut cfg: crate::config::Config = crate::config::Config::from_file("config.yaml").unwrap();
        for index in &mut cfg.search_indexes {
            index.join_fields = Vec::new();
        }
        let config = Arc::new(cfg);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy(&config.database.url)
            .unwrap();
        let docs = vec![
            serde_json::json!({ "mat_no": "M-1", "name": "Alpha Cotton", "status": "active" }),
            serde_json::json!({ "mat_no": "M-2", "name": "Beta Wool", "status": "active" }),
            serde_json::json!({ "mat_no": "M-3", "name": "Gamma Cotton Wool", "status": "discontinued" }),
        ];
        let service = stub_service(pool.clone(), docs);
        let schema = build_schema_with_search(config, pool, service).await;

        let resp = run(&schema, "{ searchMaterials(query: \"wool\") { node { mat_no } } }").await;
        assert!(resp.is_ok(), "OR query errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-2", "M-3"]);

        let resp = run(
            &schema,
            "{ searchMaterials(query: \"cotton wool\", queryOperator: OR) { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "OR query errored: {:?}", resp.errors);
        assert_eq!(
            mat_nos(&resp),
            vec!["M-3", "M-1", "M-2"],
            "both terms matched ranks first"
        );

        let resp = run(
            &schema,
            "{ searchMaterials(query: \"cotton wool\", queryOperator: AND) { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "AND query errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-3"]);

        let resp = run(
            &schema,
            "{ searchMaterials(query: \"wool\", queryOperator: AND) { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "AND single-term errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-2", "M-3"]);

        let resp = run(
            &schema,
            "{ searchMaterials(query: \"cotton wool\") { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "default operator errored: {:?}", resp.errors);
        assert_eq!(
            mat_nos(&resp),
            vec!["M-3", "M-1", "M-2"],
            "default OR ranks best match first"
        );

        let resp = run(
            &schema,
            "{ searchMaterials(query: \"\", queryOperator: AND) { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "empty query errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-1", "M-2", "M-3"]);

        let resp = run(
            &schema,
            "{ searchMaterials(query: \"   \", queryOperator: AND) { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "whitespace query errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-1", "M-2", "M-3"]);
    }

    #[tokio::test]
    async fn query_operator_and_spans_top_level_and_joined_fields() {
        let cfg: crate::config::Config = crate::config::Config::from_file("config.yaml").unwrap();
        let config = Arc::new(cfg);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(1))
            .connect_lazy(&config.database.url)
            .unwrap();
        let docs = vec![
            serde_json::json!({
                "mat_no": "M-1",
                "name": "Alpha Cotton",
                "status": "active",
                "material_features": [
                    { "feature_name": "Wool", "description": "insulating", "feature_attributes": [] }
                ]
            }),
            serde_json::json!({
                "mat_no": "M-2",
                "name": "Beta",
                "status": "active",
                "material_features": [
                    { "feature_name": "Wool", "description": "soft", "feature_attributes": [] }
                ]
            }),
            serde_json::json!({
                "mat_no": "M-3",
                "name": "Gamma",
                "status": "discontinued",
                "material_features": [
                    { "feature_name": "Cotton", "description": "breathable", "feature_attributes": [] }
                ]
            }),
        ];
        let service = stub_service(pool.clone(), docs);
        let schema = build_schema_with_search(config, pool, service).await;

        let resp = run(
            &schema,
            "{ searchMaterials(query: \"cotton wool\", queryOperator: AND) { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "AND joined errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-1"]);

        let resp = run(
            &schema,
            "{ searchMaterials(query: \"cotton wool\") { node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "OR joined errored: {:?}", resp.errors);
        assert_eq!(mat_nos(&resp), vec!["M-1", "M-2", "M-3"]);
    }

    #[tokio::test]
    async fn search_hits_expose_score_and_node() {
        let mut cfg: crate::config::Config = crate::config::Config::from_file("config.yaml").unwrap();
        for index in &mut cfg.search_indexes {
            index.join_fields = Vec::new();
        }
        let config = Arc::new(cfg);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy(&config.database.url)
            .unwrap();
        let docs = vec![
            serde_json::json!({ "mat_no": "M-1", "name": "Alpha Cotton", "status": "active" }),
            serde_json::json!({ "mat_no": "M-2", "name": "Beta Cotton Wool", "status": "active" }),
        ];
        let service = stub_service(pool.clone(), docs);
        let schema = build_schema_with_search(config, pool, service).await;

        // Text query: score reflects the stub's term-count relevance and node
        // resolves the document fields.
        let resp = run(
            &schema,
            "{ searchMaterials(query: \"cotton wool\") { score node { mat_no name status } } }",
        )
        .await;
        assert!(resp.is_ok(), "hit shape errored: {:?}", resp.errors);
        let data = serde_json::to_value(&resp.data).unwrap();
        let hits = data["searchMaterials"].as_array().unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0]["node"]["mat_no"], "M-2");
        assert_eq!(hits[0]["node"]["name"], "Beta Cotton Wool");
        assert_eq!(hits[0]["score"], 2.0, "both terms matched -> score 2.0");
        assert_eq!(hits[1]["node"]["mat_no"], "M-1");
        assert_eq!(hits[1]["score"], 1.0, "one term matched -> score 1.0");

        // Filter-only query: same hit shape, constant score.
        let resp = run(
            &schema,
            "{ searchMaterials(filter: { status: { eq: \"active\" } }) { score node { mat_no } } }",
        )
        .await;
        assert!(resp.is_ok(), "filter-only errored: {:?}", resp.errors);
        let data = serde_json::to_value(&resp.data).unwrap();
        let hits = data["searchMaterials"].as_array().unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h["score"] == 0.0), "filter-only scores 0.0");
    }
}
