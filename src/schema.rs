use std::sync::Arc;

use async_graphql::{
    Name, Value,
    dynamic::{
        indexmap::IndexMap, Field, FieldFuture, FieldValue, InputObject, InputValue, Object,
        Schema, TypeRef, ValueAccessor,
    },
};
use sqlx::{Pool, Postgres, Row};

use crate::config::{Config, TableConfig};

#[derive(Clone)]
struct AppContext {
    pool: Pool<Postgres>,
}

pub async fn build_schema(config: Arc<Config>, pool: Pool<Postgres>) -> Schema {
    let ctx = Arc::new(AppContext { pool });

    let mut schema_builder = Schema::build("Query", Some("Mutation"), None);
    schema_builder = schema_builder.data(ctx);

    let mut table_objects = Vec::new();

    for (name, table_config) in &config.tables {
        let input = build_create_input(name, table_config);
        schema_builder = schema_builder.register(input);
        let update_input = build_update_input(name, table_config);
        schema_builder = schema_builder.register(update_input);
        let filter = build_filter_input(name, table_config);
        schema_builder = schema_builder.register(filter);

        let obj = build_table_object(name, table_config);
        schema_builder = schema_builder.register(obj);
        table_objects.push((name.clone(), table_config.table.clone(), table_config.clone()));
    }

    let query = build_query_object(&config, &table_objects);
    let mutation = build_mutation_object(&config, &table_objects);

    schema_builder = schema_builder.register(query);
    schema_builder = schema_builder.register(mutation);

    schema_builder.finish().unwrap()
}

fn build_table_object(_name: &str, table_config: &TableConfig) -> Object {
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
            let json_str: String = row.try_get(0).unwrap_or_default();
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
            let json_str: String = row.try_get(0).unwrap_or_default();
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

fn build_query_object(_config: &Config, tables: &[(String, String, TableConfig)]) -> Object {
    let mut query = Object::new("Query");

    for (name, table_name, table_config) in tables {
        let pk = table_config.primary_key[0].clone();
        let tn = table_name.clone();

        let tn_first = tn.clone();
        let tn_first_closure = tn.clone();
        query = query.field(
            Field::new(
                name.clone(),
                TypeRef::named(tn_first),
                move |ctx| {
                    let pk = pk.clone();
                    let table_name = tn_first_closure.clone();

                    FieldFuture::new(async move {
                        let id = ctx
                            .args
                            .get("id")
                            .and_then(|v| v.string().ok().map(String::from));
                        let id =
                            id.ok_or_else(|| async_graphql::Error::new("id is required"))?;

                        let sql =
                            format!("SELECT row_to_json(t)::text FROM (SELECT * FROM {} WHERE {} = $1 LIMIT 1) t", table_name, pk);
                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();

                        match fetch_one(&app_ctx.pool, &sql, &[id]).await? {
                            Some(row) => Ok(Some(FieldValue::value(gql_val(row)))),
                            None => Ok(FieldValue::NONE),
                        }
                    })
                },
            )
            .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::STRING))),
        );

        let list_name = format!("{}List", name);
        let tn_list = tn.clone();
        let tn_list_closure = tn.clone();

        query = query.field(
            Field::new(
                list_name,
                TypeRef::named_nn_list_nn(tn_list),
                move |ctx| {
                    let table_name = tn_list_closure.clone();

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
                            sql.push_str(&format!(" ORDER BY {}", order));
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
                TypeRef::named(format!("{}FilterInput", name)),
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

        mutation = mutation.field(
            Field::new(
                format!("create{}", name),
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
                TypeRef::named_nn(format!("Create{}Input", name)),
            )),
        );

        let update_table_name = table_name.clone();
        let update_table_config = table_config.clone();
        let update_pk = table_config.primary_key[0].clone();

        mutation = mutation.field(
            Field::new(
                format!("update{}", name),
                TypeRef::named_nn(table_name.clone()),
                move |ctx| {
                    let table_config = update_table_config.clone();
                    let table_name = update_table_name.clone();
                    let pk = update_pk.clone();

                    FieldFuture::new(async move {
                        let id = ctx
                            .args
                            .get("id")
                            .and_then(|v| v.string().ok().map(String::from))
                            .ok_or_else(|| async_graphql::Error::new("id is required"))?;
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
                        let sql = format!(
                            "WITH upd AS (UPDATE {} SET {} WHERE {} = ${} RETURNING *) SELECT row_to_json(upd)::text FROM upd",
                            table_name,
                            set_clauses.join(", "),
                            pk,
                            params.len(),
                        );

                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        let row = fetch_one(&app_ctx.pool, &sql, &params)
                            .await?
                            .ok_or_else(|| async_graphql::Error::new("no row returned"))?;

                        Ok(Some(FieldValue::value(gql_val(row))))
                    })
                },
            )
            .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::STRING)))
            .argument(InputValue::new(
                "input",
                TypeRef::named_nn(format!("Update{}Input", name)),
            )),
        );

        let delete_table_name = table_name.clone();
        let delete_pk = table_config.primary_key[0].clone();

        mutation = mutation.field(
            Field::new(
                format!("delete{}", name),
                TypeRef::named_nn(table_name.clone()),
                move |ctx| {
                    let table_name = delete_table_name.clone();
                    let pk = delete_pk.clone();

                    FieldFuture::new(async move {
                        let id = ctx
                            .args
                            .get("id")
                            .and_then(|v| v.string().ok().map(String::from))
                            .ok_or_else(|| async_graphql::Error::new("id is required"))?;

                        let sql =
                            format!("WITH del AS (DELETE FROM {} WHERE {} = $1 RETURNING *) SELECT row_to_json(del)::text FROM del", table_name, pk);

                        let app_ctx = ctx.data::<Arc<AppContext>>().unwrap();
                        let row = fetch_one(&app_ctx.pool, &sql, &[id])
                            .await?
                            .ok_or_else(|| async_graphql::Error::new("no row returned"))?;

                        Ok(Some(FieldValue::value(gql_val(row))))
                    })
                },
            )
            .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::STRING))),
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
