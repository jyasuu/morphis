use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};
use crate::config::{ColumnType, Config, TableConfig};

use super::db;
use super::input::build_filter_sql;
use super::util::{capitalize_first, gql_val};
use super::AppContext;

pub(crate) fn build_query_object(_config: &Config, tables: &[(String, String, TableConfig)]) -> Object {
    let mut query = Object::new("Query");

    for (name, table_name, table_config) in tables {
        let pk = table_config.primary_key[0].clone();
        let tn = table_name.clone();

        let is_pk_int = table_config.columns.iter().any(|c| {
            table_config.primary_key.contains(&c.name)
                && matches!(c.col_type, ColumnType::Int | ColumnType::Int64)
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
                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>().unwrap();

                        match db::fetch_one(&app_ctx.pool, &sql, &[id]).await? {
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
                            let (clause, p) = build_filter_sql(filter, &col_names);
                            if !clause.is_empty() {
                                sql.push_str(&format!(" WHERE {}", clause));
                                params = p;
                            }
                        }

                        if let Some(order) = order_by {
                            let sanitized: Vec<String> = order
                                .split(',')
                                .filter_map(|seg| {
                                    let seg = seg.trim();
                                    if seg.is_empty() {
                                        return None;
                                    }
                                    let parts: Vec<&str> = seg.split_whitespace().collect();
                                    match parts.as_slice() {
                                        [col] if col_names.contains(&col.to_string()) => {
                                            Some(seg.to_string())
                                        }
                                        [col, dir]
                                            if col_names.contains(&col.to_string())
                                                && matches!(
                                    *dir,
                                    "ASC" | "DESC" | "asc" | "desc"
                                ) =>
                                        {
                                            Some(seg.to_string())
                                        }
                                        _ => None,
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

                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>().unwrap();
                        let rows = db::fetch_many(&app_ctx.pool, &sql, &params).await?;
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
