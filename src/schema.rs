use std::sync::Arc;

use async_graphql::{
    Name, Value,
    dynamic::{
        indexmap::IndexMap, Field, FieldFuture, FieldValue, InputObject, InputValue, Object,
        Schema, TypeRef, ValueAccessor,
    },
};
use sqlx::{Pool, Postgres, Row};

use crate::config::{Config, SearchIndexConfig, SearchJoinConfig, TableConfig};

#[derive(Clone)]
struct AppContext {
    pool: Pool<Postgres>,
    es_client: Option<reqwest::Client>,
    es_url: Option<String>,
}

pub async fn build_schema(config: Arc<Config>, pool: Pool<Postgres>) -> Schema {
    let es_client = config.elasticsearch.as_ref().map(|_| reqwest::Client::new());
    let es_url = config.elasticsearch.as_ref().map(|c| c.url.clone());
    let ctx = Arc::new(AppContext { pool, es_client, es_url });

    let mut schema_builder = Schema::build("Query", Some("Mutation"), None);
    schema_builder = schema_builder.data(ctx);

    let mut table_objects = Vec::new();
    let mut table_type_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (name, table_config) in &config.tables {
        table_type_map.insert(table_config.table.clone(), name.clone());
    }

    for (name, table_config) in &config.tables {
        if table_config.primary_key.is_empty() {
            panic!("Table '{}' has no primary_key defined", name);
        }
        if table_config.primary_key.len() > 1 {
            panic!(
                "Table '{}' has composite primary key ({:?}); only single-column PKs are supported",
                name, table_config.primary_key
            );
        }
        let name_caps = capitalize_first(name);
        let filter = build_filter_input(&name_caps, table_config);
        schema_builder = schema_builder.register(filter);
        let input = build_create_input(&name_caps, table_config);
        schema_builder = schema_builder.register(input);
        let update_input = build_update_input(&name_caps, table_config);
        schema_builder = schema_builder.register(update_input);

        let obj = build_table_object(name, table_config, &config.tables, &table_type_map);
        schema_builder = schema_builder.register(obj);
        table_objects.push((name.clone(), table_config.table.clone(), table_config.clone()));
    }

    let mut query = build_query_object(&config, &table_objects);

    for index_cfg in &config.search_indexes {
        tracing::debug!("Registering search index: {}", index_cfg.name);
        let sf = index_cfg.searchable_fields.clone();
        let mut input_obj = InputObject::new(format!("{}SearchFilter", capitalize_first(&index_cfg.index)));
        for f in &sf {
            input_obj = input_obj.field(InputValue::new(f.clone(), TypeRef::named(TypeRef::STRING)));
        }
        // Nested join field filters omitted for simplicity;
        // full-text search via `query` param searches all fields including nested.
        schema_builder = schema_builder.register(input_obj);

        query = add_search_field(query, index_cfg);
    }

    let mutation = build_mutation_object(&config, &table_objects);

    schema_builder = schema_builder.register(query);
    schema_builder = schema_builder.register(mutation);

    schema_builder.finish().unwrap()
}

fn build_table_object(
    _name: &str,
    table_config: &TableConfig,
    all_tables: &std::collections::HashMap<String, TableConfig>,
    table_type_map: &std::collections::HashMap<String, String>,
) -> Object {
    let mut obj = Object::new(&table_config.table);
    for col in &table_config.columns {
        let field_type = match col.col_type.to_string().as_str() {
            "Int" | "Int64" => TypeRef::named_nn(TypeRef::INT),
            "Float" => TypeRef::named_nn(TypeRef::FLOAT),
            "Boolean" => TypeRef::named_nn(TypeRef::BOOLEAN),
            _ => TypeRef::named_nn(TypeRef::STRING),
        };
        let col_name = col.name.clone();
        obj = obj.field(Field::new(
            col.name.clone(),
            field_type,
                move |ctx| {
                let col_name = col_name.clone();
                FieldFuture::new(async move {
                    let parent = ctx.parent_value.as_value()
                        .ok_or_else(|| async_graphql::Error::new("not a value"))?;
                    let val = match parent {
                        Value::Object(map) => {
                            let key = Name::new(col_name.as_str());
                            map.get(&key).cloned().unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    };
                    Ok(Some(FieldValue::value(val)))
                })
            },
        ));
    }

    for rel in &table_config.relations {
        let Some(rel_cfg) = all_tables.get(
            table_type_map.get(&rel.table).map(String::as_str).unwrap_or("")
        ) else { continue; };
        let rel_table = rel.table.clone();
        let local_field = rel.local_field.clone();
        let foreign_field = rel.foreign_field.clone();
        let related_pk = rel_cfg.primary_key[0].clone();
        let foreign_int = rel_cfg.columns.iter().any(|c| c.name == foreign_field && matches!(c.col_type, crate::config::ColumnType::Int | crate::config::ColumnType::Int64));
        let pk_int = rel_cfg.columns.iter().any(|c| c.name == related_pk && matches!(c.col_type, crate::config::ColumnType::Int | crate::config::ColumnType::Int64));
        let return_type_name = table_type_map.get(&rel.table).cloned().unwrap_or_default();

        match rel.rel_type {
            crate::config::RelationType::HasMany => {
                obj = obj.field(Field::new(rel.name.clone(), TypeRef::named_nn_list_nn(&return_type_name), move |ctx| {
                    let local_field = local_field.clone();
                    let foreign_field = foreign_field.clone();
                    let rel_table = rel_table.clone();
                    let foreign_int = foreign_int;
                    FieldFuture::new(async move {
                        let parent = ctx.parent_value.as_value()
                            .ok_or_else(|| async_graphql::Error::new("not a value"))?;
                        let local_val = match parent {
                            Value::Object(map) => map.get(&Name::new(&local_field)).cloned().unwrap_or(Value::Null),
                            _ => Value::Null,
                        };
                        let val_str = gql_value_to_sql_string(&local_val);
                        let cast = if foreign_int { "::int" } else { "" };
                        let sql = format!(
                            "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT * FROM {} WHERE {} = $1{}) t",
                            rel_table, foreign_field, cast
                        );
                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        let rows = fetch_many(&app_ctx.pool, &sql, &[val_str]).await?;
                        let items: Vec<FieldValue> = rows
                            .into_iter()
                            .map(|r| FieldValue::value(gql_val(r)))
                            .collect();
                        Ok(Some(FieldValue::list(items)))
                    })
                }));
            }
            crate::config::RelationType::HasOne => {
                obj = obj.field(Field::new(rel.name.clone(), TypeRef::named(&return_type_name), move |ctx| {
                    let local_field = local_field.clone();
                    let foreign_field = foreign_field.clone();
                    let rel_table = rel_table.clone();
                    let foreign_int = foreign_int;
                    FieldFuture::new(async move {
                        let parent = ctx.parent_value.as_value()
                            .ok_or_else(|| async_graphql::Error::new("not a value"))?;
                        let local_val = match parent {
                            Value::Object(map) => map.get(&Name::new(&local_field)).cloned().unwrap_or(Value::Null),
                            _ => Value::Null,
                        };
                        let val_str = gql_value_to_sql_string(&local_val);
                        let cast = if foreign_int { "::int" } else { "" };
                        let sql = format!(
                            "SELECT row_to_json(t)::text FROM (SELECT * FROM {} WHERE {} = $1{} LIMIT 1) t",
                            rel_table, foreign_field, cast
                        );
                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        match fetch_one(&app_ctx.pool, &sql, &[val_str]).await? {
                            Some(row) => Ok(Some(FieldValue::value(gql_val(row)))),
                            None => Ok(FieldValue::NONE),
                        }
                    })
                }));
            }
            crate::config::RelationType::BelongsTo => {
                obj = obj.field(Field::new(rel.name.clone(), TypeRef::named(&return_type_name), move |ctx| {
                    let local_field = local_field.clone();
                    let rel_table = rel_table.clone();
                    let related_pk = related_pk.clone();
                    let pk_int = pk_int;
                    FieldFuture::new(async move {
                        let parent = ctx.parent_value.as_value()
                            .ok_or_else(|| async_graphql::Error::new("not a value"))?;
                        let local_val = match parent {
                            Value::Object(map) => map.get(&Name::new(&local_field)).cloned().unwrap_or(Value::Null),
                            _ => Value::Null,
                        };
                        let val_str = gql_value_to_sql_string(&local_val);
                        let cast = if pk_int { "::int" } else { "" };
                        let sql = format!(
                            "SELECT row_to_json(t)::text FROM (SELECT * FROM {} WHERE {} = $1{} LIMIT 1) t",
                            rel_table, related_pk, cast
                        );
                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        match fetch_one(&app_ctx.pool, &sql, &[val_str]).await? {
                            Some(row) => Ok(Some(FieldValue::value(gql_val(row)))),
                            None => Ok(FieldValue::NONE),
                        }
                    })
                }));
            }
        }
    }
    obj
}

async fn fetch_one(
    pool: &Pool<Postgres>,
    sql: &str,
    params: &[String],
) -> Result<Option<serde_json::Value>, async_graphql::Error> {
    let mut query = sqlx::query(sql);
    for p in params {
        query = query.bind(p);
    }
    match query.fetch_optional(pool).await {
        Ok(Some(row)) => {
            let json_str: String = row.try_get(0).map_err(|e| async_graphql::Error::new(e.to_string()))?;
            serde_json::from_str(&json_str)
                .map(Some)
                .map_err(|e| async_graphql::Error::new(e.to_string()))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(async_graphql::Error::new(e.to_string())),
    }
}

async fn fetch_many(
    pool: &Pool<Postgres>,
    sql: &str,
    params: &[String],
) -> Result<Vec<serde_json::Value>, async_graphql::Error> {
    let mut query = sqlx::query(sql);
    for p in params {
        query = query.bind(p);
    }
    match query.fetch_optional(pool).await {
        Ok(Some(row)) => {
            let json_str: String = row.try_get(0).map_err(|e| async_graphql::Error::new(e.to_string()))?;
            let val: serde_json::Value =
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Array(vec![]));
            match val {
                serde_json::Value::Array(arr) => Ok(arr),
                _ => Ok(vec![val]),
            }
        }
        Ok(None) => Ok(vec![]),
        Err(e) => Err(async_graphql::Error::new(e.to_string())),
    }
}

fn json_to_gql(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Boolean(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                async_graphql::Number::from_f64(f).map_or(Value::Null, Value::Number)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => Value::List(arr.into_iter().map(json_to_gql).collect()),
        serde_json::Value::Object(obj) => {
            let map: IndexMap<Name, Value> = obj
                .into_iter()
                .map(|(k, v)| (Name::new(k), json_to_gql(v)))
                .collect();
            Value::Object(map)
        }
    }
}

fn gql_val(v: serde_json::Value) -> Value {
    json_to_gql(v)
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

fn gql_value_to_sql_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

async fn es_search(
    pool: &Pool<Postgres>,
    es_client: &reqwest::Client,
    es_url: &str,
    index_cfg: &SearchIndexConfig,
    query_str: &str,
    filters: Option<&async_graphql::dynamic::ValueAccessor<'_>>,
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
    let es_query = serde_json::json!({
        "query": { "bool": bool_body },
        "size": 50
    });

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
    filter: Option<&async_graphql::dynamic::ValueAccessor>,
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

async fn fetch_joined_rows(
    pool: &Pool<Postgres>,
    sql: &str,
    local_val: &str,
) -> Vec<serde_json::Value> {
    let mut query = sqlx::query(sql);
    query = query.bind(local_val);
    match query.fetch_optional(pool).await {
        Ok(Some(row)) => row
            .try_get::<String, _>(0)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| match v { serde_json::Value::Array(arr) => Some(arr), _ => None })
            .unwrap_or_default(),
        _ => vec![],
    }
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
        let rows = fetch_joined_rows(pool, &sql, &local_val).await;
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

fn add_search_field(mut query: Object, index_cfg: &SearchIndexConfig) -> Object {
    let idx_cfg = index_cfg.clone();
    let type_name = idx_cfg.graphql_type.clone();
    query = query.field(
        Field::new(
            format!("search{}", capitalize_first(&idx_cfg.index)),
            TypeRef::named_nn_list_nn(&type_name),
            move |ctx| {
                let idx_cfg = idx_cfg.clone();
                    FieldFuture::new(async move {
                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        let (es_client, es_url) = match (&app_ctx.es_client, &app_ctx.es_url) {
                            (Some(c), Some(u)) => (c.clone(), u.clone()),
                            _ => return Err(async_graphql::Error::new("Elasticsearch not configured")),
                        };
                        let query_str = ctx.args.get("query").and_then(|v| v.string().ok().map(String::from)).unwrap_or_default();
                        let filter = ctx.args.get("filter");
                        let results = es_search(&app_ctx.pool, &es_client, &es_url, &idx_cfg, &query_str, filter.as_ref()).await?;
                        let items: Vec<FieldValue> = results.into_iter().map(|r| FieldValue::value(gql_val(r))).collect();
                        Ok(Some(FieldValue::list(items)))
                    })
                },
            )
            .argument(InputValue::new("query", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new("filter", TypeRef::named(format!("{}SearchFilter", capitalize_first(&index_cfg.index))))),
        );
    query
}

fn build_query_object(_config: &Config, tables: &[(String, String, TableConfig)]) -> Object {
    let mut query = Object::new("Query");

    for (name, table_name, table_config) in tables {
        let pk = table_config.primary_key[0].clone();
        let tn = table_name.clone();

        let is_pk_int = table_config.columns.iter().any(|c| {
            table_config.primary_key.contains(&c.name)
                && matches!(c.col_type, crate::config::ColumnType::Int | crate::config::ColumnType::Int64)
        });
        let arg_type = if is_pk_int { TypeRef::named_nn(TypeRef::INT) } else { TypeRef::named_nn(TypeRef::STRING) };

        let tn_first = tn.clone();
        let tn_first_closure = tn.clone();
        query = query.field(
            Field::new(
                name.clone(),
                TypeRef::named(tn_first),
                move |ctx| {
                    let pk = pk.clone();
                    let table_name = tn_first_closure.clone();
                    let is_pk_int = is_pk_int;

                    FieldFuture::new(async move {
                        let id = if is_pk_int {
                            ctx.args.get("id").and_then(|v| v.i64().ok()).map(|n| n.to_string())
                        } else {
                            ctx.args.get("id").and_then(|v| v.string().ok()).map(String::from)
                        };
                        let id =
                            id.ok_or_else(|| async_graphql::Error::new("id is required"))?;

                        let cast = if is_pk_int { "::int" } else { "" };
                        let sql =
                            format!("SELECT row_to_json(t)::text FROM (SELECT * FROM {} WHERE {} = $1{} LIMIT 1) t", table_name, pk, cast);
                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();

                        match fetch_one(&app_ctx.pool, &sql, &[id]).await? {
                            Some(row) => Ok(Some(FieldValue::value(gql_val(row)))),
                            None => Ok(FieldValue::NONE),
                        }
                    })
                },
            )
            .argument(InputValue::new("id", arg_type)),
        );

        let list_name = format!("{}List", name);
        let tn_list = tn.clone();
        let tn_list_closure = tn.clone();
        let col_names: Vec<String> = table_config.columns.iter().map(|c| c.name.clone()).collect();

        query = query.field(
            Field::new(
                list_name,
                TypeRef::named_nn_list_nn(tn_list),
                move |ctx| {
                    let table_name = tn_list_closure.clone();
                    let col_names = col_names.clone();

                    FieldFuture::new(async move {
                        let filter_arg = ctx.args.get("filter");
                        let order_by = ctx
                            .args
                            .get("order_by")
                            .and_then(|v| v.string().ok().map(String::from));
                        let limit = ctx.args.get("limit").and_then(|v| v.u64().ok());
                        let offset = ctx.args.get("offset").and_then(|v| v.u64().ok());

                        let mut sql = format!("SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT * FROM {}", table_name);
                        let mut params = Vec::new();

                        if let Some(filter) = filter_arg {
                            let (clause, p) = build_filter_sql(filter);
                            if !clause.is_empty() {
                                sql.push_str(&format!(" WHERE {}", clause));
                                params = p;
                            }
                        }

                        if let Some(order) = order_by {
                            let sanitized: Vec<&str> = order
                                .split(',')
                                .filter_map(|seg| {
                                    let seg = seg.trim();
                                    let col = seg.split_whitespace().next().unwrap_or("");
                                    if col_names.contains(&col.to_string()) {
                                        Some(seg)
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if !sanitized.is_empty() {
                                sql.push_str(&format!(" ORDER BY {}", sanitized.join(", ")));
                            }
                        }
                        if let Some(l) = limit {
                            sql.push_str(&format!(" LIMIT {}", l));
                        }
                        if let Some(o) = offset {
                            sql.push_str(&format!(" OFFSET {}", o));
                        }
                        sql.push_str(") t");

                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        let rows = fetch_many(&app_ctx.pool, &sql, &params).await?;
                        let items: Vec<FieldValue> = rows
                            .into_iter()
                            .map(|r| FieldValue::value(gql_val(r)))
                            .collect();
                        Ok(Some(FieldValue::list(items)))
                    })
                },
            )
            .argument(InputValue::new(
                "filter",
                TypeRef::named(format!("{}FilterInput", capitalize_first(name))),
            ))
            .argument(InputValue::new("order_by", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new("limit", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT))),
        );
    }

    query
}

fn build_mutation_object(
    _config: &Config,
    tables: &[(String, String, TableConfig)],
) -> Object {
    let mut mutation = Object::new("Mutation");

    for (name, table_name, table_config) in tables {
        let create_table_name = table_name.clone();
        let create_table_config = table_config.clone();

        let name_caps = capitalize_first(name);
        let is_pk_int = table_config.columns.iter().any(|c| {
            table_config.primary_key.contains(&c.name)
                && matches!(c.col_type, crate::config::ColumnType::Int | crate::config::ColumnType::Int64)
        });
        let pk_arg_type = if is_pk_int { TypeRef::named_nn(TypeRef::INT) } else { TypeRef::named_nn(TypeRef::STRING) };

        mutation = mutation.field(
            Field::new(
                format!("create{}", name_caps),
                TypeRef::named_nn(table_name.clone()),
                move |ctx| {
                    let table_config = create_table_config.clone();
                    let table_name = create_table_name.clone();

                    FieldFuture::new(async move {
                        let input = ctx
                            .args
                            .get("input")
                            .ok_or_else(|| async_graphql::Error::new("input is required"))?;
                        let obj = input.object()?;

                        let mut columns = Vec::new();
                        let mut params = Vec::new();

                        for col in &table_config.columns {
                            if let Some(val) = obj.get(&col.name) {
                                columns.push(col.name.clone());
                                params.push(value_as_string(&val));
                            }
                        }

                        let placeholders: Vec<String> =
                            (1..=params.len()).map(|i| format!("${}", i)).collect();

                        let sql = format!(
                            "WITH ins AS (INSERT INTO {} ({}) VALUES ({}) RETURNING *) SELECT row_to_json(ins)::text FROM ins",
                            table_name,
                            columns.join(", "),
                            placeholders.join(", ")
                        );

                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        let row = fetch_one(&app_ctx.pool, &sql, &params)
                            .await?
                            .ok_or_else(|| async_graphql::Error::new("no row returned"))?;

                        Ok(Some(FieldValue::value(gql_val(row))))
                    })
                },
            )
            .argument(InputValue::new(
                "input",
                TypeRef::named_nn(format!("Create{}Input", name_caps)),
            )),
        );

        let update_table_name = table_name.clone();
        let update_table_config = table_config.clone();
        let update_pk = table_config.primary_key[0].clone();

        mutation = mutation.field(
            Field::new(
                format!("update{}", name_caps),
                TypeRef::named_nn(table_name.clone()),
                move |ctx| {
                    let table_config = update_table_config.clone();
                    let table_name = update_table_name.clone();
                    let pk = update_pk.clone();
                    let is_pk_int = is_pk_int;

                    FieldFuture::new(async move {
                        let id = if is_pk_int {
                            ctx.args.get("id").and_then(|v| v.i64().ok()).map(|n| n.to_string())
                        } else {
                            ctx.args.get("id").and_then(|v| v.string().ok()).map(String::from)
                        };
                        let id = id.ok_or_else(|| async_graphql::Error::new("id is required"))?;
                        let input = ctx
                            .args
                            .get("input")
                            .ok_or_else(|| async_graphql::Error::new("input is required"))?;
                        let obj = input.object()?;

                        let mut set_clauses = Vec::new();
                        let mut params = Vec::new();

                        for col in &table_config.columns {
                            if let Some(val) = obj.get(&col.name) {
                                set_clauses
                                    .push(format!("{} = ${}", col.name, params.len() + 1));
                                params.push(value_as_string(&val));
                            }
                        }

                        params.push(id);
                        let cast = if is_pk_int { "::int" } else { "" };
                        let sql = format!(
                            "WITH upd AS (UPDATE {} SET {} WHERE {} = ${}{} RETURNING *) SELECT row_to_json(upd)::text FROM upd",
                            table_name,
                            set_clauses.join(", "),
                            pk,
                            params.len(),
                            cast,
                        );

                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        let row = fetch_one(&app_ctx.pool, &sql, &params)
                            .await?
                            .ok_or_else(|| async_graphql::Error::new("no row returned"))?;

                        Ok(Some(FieldValue::value(gql_val(row))))
                    })
                },
            )
            .argument(InputValue::new("id", pk_arg_type.clone()))
            .argument(InputValue::new(
                "input",
                TypeRef::named_nn(format!("Update{}Input", name_caps)),
            )),
        );

        let delete_table_name = table_name.clone();
        let delete_pk = table_config.primary_key[0].clone();

        mutation = mutation.field(
            Field::new(
                format!("delete{}", name_caps),
                TypeRef::named_nn(table_name.clone()),
                move |ctx| {
                    let table_name = delete_table_name.clone();
                    let pk = delete_pk.clone();
                    let is_pk_int = is_pk_int;

                    FieldFuture::new(async move {
                        let id = if is_pk_int {
                            ctx.args.get("id").and_then(|v| v.i64().ok()).map(|n| n.to_string())
                        } else {
                            ctx.args.get("id").and_then(|v| v.string().ok()).map(String::from)
                        };
                        let id = id.ok_or_else(|| async_graphql::Error::new("id is required"))?;

                        let cast = if is_pk_int { "::int" } else { "" };
                        let sql =
                            format!("WITH del AS (DELETE FROM {} WHERE {} = $1{} RETURNING *) SELECT row_to_json(del)::text FROM del", table_name, pk, cast);

                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        let row = fetch_one(&app_ctx.pool, &sql, &[id])
                            .await?
                            .ok_or_else(|| async_graphql::Error::new("no row returned"))?;

                        Ok(Some(FieldValue::value(gql_val(row))))
                    })
                },
            )
            .argument(InputValue::new("id", pk_arg_type)),
        );
    }

    mutation
}

fn build_create_input(name: &str, table_config: &TableConfig) -> InputObject {
    build_input_object(&format!("Create{}Input", name), table_config, false)
}

fn build_update_input(name: &str, table_config: &TableConfig) -> InputObject {
    build_input_object(&format!("Update{}Input", name), table_config, true)
}

fn build_input_object(name: &str, table_config: &TableConfig, all_nullable: bool) -> InputObject {
    let mut input = InputObject::new(name);
    for col in &table_config.columns {
        let is_pk = table_config.primary_key.contains(&col.name);
        let is_auto = matches!(col.col_type.to_string().as_str(), "Int" | "Int64");
        if !all_nullable && is_pk && is_auto {
            continue;
        }
        let nullable = all_nullable || col.nullable;
        let scalar = match col.col_type.to_string().as_str() {
            "Int" | "Int64" => TypeRef::INT,
            "Float" => TypeRef::FLOAT,
            "Boolean" => TypeRef::BOOLEAN,
            _ => TypeRef::STRING,
        };
        let type_ref = if nullable {
            TypeRef::named(scalar)
        } else {
            TypeRef::named_nn(scalar)
        };
        input = input.field(InputValue::new(col.name.clone(), type_ref));
    }
    input
}

fn build_filter_input(name: &str, table_config: &TableConfig) -> InputObject {
    let mut input = InputObject::new(format!("{}FilterInput", name));
    for col in &table_config.columns {
        input = input.field(InputValue::new(col.name.clone(), TypeRef::named(TypeRef::STRING)));
    }
    input
}

fn build_filter_sql(filter: ValueAccessor) -> (String, Vec<String>) {
    let obj = match filter.object() {
        Ok(o) => o,
        Err(_) => return (String::new(), vec![]),
    };

    let mut clauses = Vec::new();
    let mut params = Vec::new();

    for (key, val) in obj.iter() {
        if val.is_null() {
            continue;
        }
        if let Ok(s) = val.string() {
            clauses.push(format!("{} = ${}", key, params.len() + 1));
            params.push(s.to_string());
        }
    }

    (clauses.join(" AND "), params)
}

fn value_as_string(val: &ValueAccessor) -> String {
    if let Ok(s) = val.string() {
        s.to_string()
    } else if let Ok(n) = val.i64() {
        n.to_string()
    } else if let Ok(n) = val.f64() {
        n.to_string()
    } else if let Ok(b) = val.boolean() {
        b.to_string()
    } else {
        String::new()
    }
}
