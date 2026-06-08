use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};
use crate::config::{ColumnType, Config, TableConfig};

use super::db;
use super::input::build_filter_sql;
use super::util::{capitalize_first, gql_val};
use super::{apply_row_filters, AppContext, Identity};

fn build_pk_args(table_config: &TableConfig) -> Vec<(String, String, bool)> {
    if table_config.primary_key.len() > 1 {
        table_config.primary_key.iter().map(|pk_name| {
            let is_int = table_config.columns.iter().any(|c| {
                c.name == *pk_name && matches!(c.col_type, ColumnType::Int | ColumnType::Int64)
            });
            (pk_name.clone(), pk_name.clone(), is_int)
        }).collect()
    } else {
        let pk = &table_config.primary_key[0];
        let is_int = table_config.columns.iter().any(|c| {
            c.name == *pk && matches!(c.col_type, ColumnType::Int | ColumnType::Int64)
        });
        vec![("id".to_string(), pk.clone(), is_int)]
    }
}

pub(crate) fn build_query_object(_config: &Config, tables: &[(String, String, TableConfig)]) -> Object {
    let mut query = Object::new("Query");

    for (name, table_name, table_config) in tables {
        if !table_config.crud.read { continue; }
        let pk_args = build_pk_args(table_config);
        let tn = table_name.clone();
        let row_filters = table_config.row_filters.clone();

        let tn_first = tn.clone();
        let pk_args_closure = pk_args.clone();
        let tn_first_closure = tn.clone();
        let row_filters_closure = row_filters.clone();

        let mut single_field = Field::new(
            name.clone(),
            TypeRef::named(tn_first),
            move |ctx| {
                let pk_args = pk_args_closure.clone();
                let table_name = tn_first_closure.clone();
                let row_filters = row_filters_closure.clone();

                FieldFuture::new(async move {
                    let mut where_clauses = Vec::new();
                    let mut params = Vec::new();
                    for (i, (arg_name, col_name, is_int)) in pk_args.iter().enumerate() {
                        let val = if *is_int {
                            ctx.args.get(arg_name.as_str()).and_then(|v| v.i64().ok()).map(|n| n.to_string())
                        } else {
                            ctx.args.get(arg_name.as_str()).and_then(|v| v.string().ok()).map(String::from)
                        };
                        let val = val.ok_or_else(|| async_graphql::Error::new(format!("{} is required", arg_name)))?;
                        let cast = if *is_int { "::int" } else { "" };
                        where_clauses.push(format!("{} = ${}{}", col_name, i + 1, cast));
                        params.push(val);
                    }

                    let mut sql = format!("SELECT row_to_json(t)::text FROM (SELECT * FROM {} WHERE {}", table_name, where_clauses.join(" AND "));
                    if let Ok(identity) = ctx.data::<Identity>() {
                        apply_row_filters(&mut sql, &mut params, identity, &row_filters);
                    }
                    sql.push_str(" LIMIT 1) t");
                    let app_ctx = ctx.data::<std::sync::Arc<AppContext>>()
                        .map_err(|_| async_graphql::Error::new("internal context missing"))?;

                    match db::fetch_one(&app_ctx.pool, &sql, &params).await? {
                        Some(row) => Ok(Some(FieldValue::value(gql_val(row)))),
                        None => Ok(FieldValue::NONE),
                    }
                })
            },
        );

        for (arg_name, _, is_int) in &pk_args {
            let arg_type = if *is_int { TypeRef::named_nn(TypeRef::INT) } else { TypeRef::named_nn(TypeRef::STRING) };
            single_field = single_field.argument(InputValue::new(arg_name.clone(), arg_type));
        }

        query = query.field(single_field);

        let list_name = format!("{}List", name);
        let tn_list = tn.clone();
        let tn_list_closure = tn.clone();
        let col_names: Vec<String> = table_config.columns.iter().map(|c| c.name.clone()).collect();
        let list_row_filters = table_config.row_filters.clone();

        query = query.field(
            Field::new(
                list_name,
                TypeRef::named_nn_list_nn(tn_list),
                move |ctx| {
                    let table_name = tn_list_closure.clone();
                    let col_names = col_names.clone();
                    let row_filters = list_row_filters.clone();

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
                        if let Ok(identity) = ctx.data::<Identity>() {
                            apply_row_filters(&mut sql, &mut params, identity, &row_filters);
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

                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>()
                            .map_err(|_| async_graphql::Error::new("internal context missing"))?;
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
