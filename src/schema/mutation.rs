use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};

use crate::config::{ColumnType, Config, RowFilterConfig, TableConfig};

use super::db;
use super::util::{capitalize_first, gql_val, value_as_string};
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

pub(crate) fn build_mutation_object(
    _config: &Config,
    tables: &[(String, String, TableConfig)],
) -> Object {
    let mut mutation = Object::new("Mutation");

    for (name, table_name, table_config) in tables {
        let create_table_name = table_name.clone();
        let create_table_config = table_config.clone();
        let create_row_filters = table_config.row_filters.clone();

        let name_caps = capitalize_first(name);
        let pk_args = build_pk_args(table_config);

        mutation = mutation.field(
            Field::new(
                format!("create{}", name_caps),
                TypeRef::named_nn(table_name.clone()),
                move |ctx| {
                    let table_config = create_table_config.clone();
                    let table_name = create_table_name.clone();
                    let row_filters = create_row_filters.clone();

                    FieldFuture::new(async move {
                        let input = ctx
                            .args
                            .get("input")
                            .ok_or_else(|| async_graphql::Error::new("input is required"))?;
                        let obj = input.object()?;

                        let auto_set_columns: Vec<&str> = row_filters.iter()
                            .filter(|rf| rf.is_auto_set())
                            .filter_map(|rf| match rf {
                                RowFilterConfig::ColumnFilter { column, .. } => Some(column.as_str()),
                                _ => None,
                            })
                            .collect();

                        let mut columns = Vec::new();
                        let mut params = Vec::new();

                        for col in &table_config.columns {
                            if auto_set_columns.contains(&col.name.as_str()) {
                                continue;
                            }
                            if let Some(val) = obj.get(&col.name) {
                                columns.push(col.name.clone());
                                params.push(value_as_string(&val));
                            }
                        }

                        if let Ok(identity) = ctx.data::<Identity>() {
                            for rf in &row_filters {
                                if rf.is_auto_set()
                                    && let RowFilterConfig::ColumnFilter { column, from_header, .. } = rf
                                    && let Some(val) = identity.header_value(from_header)
                                {
                                    columns.push(column.clone());
                                    params.push(val.to_string());
                                }
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

                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>()
                            .map_err(|_| async_graphql::Error::new("internal context missing"))?;
                        let row = db::fetch_one(&app_ctx.pool, &sql, &params)
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
        let update_pk_args = pk_args.clone();
        let update_row_filters = table_config.row_filters.clone();

        let mut update_field = Field::new(
            format!("update{}", name_caps),
            TypeRef::named_nn(table_name.clone()),
            move |ctx| {
                let table_config = update_table_config.clone();
                let table_name = update_table_name.clone();
                let pk_args = update_pk_args.clone();
                let row_filters = update_row_filters.clone();

                FieldFuture::new(async move {
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

                    let pk_start = params.len() + 1;
                    let mut where_clauses = Vec::new();
                    for (i, (arg_name, col_name, is_int)) in pk_args.iter().enumerate() {
                        let val = if *is_int {
                            ctx.args.get(arg_name.as_str()).and_then(|v| v.i64().ok()).map(|n| n.to_string())
                        } else {
                            ctx.args.get(arg_name.as_str()).and_then(|v| v.string().ok()).map(String::from)
                        };
                        let val = val.ok_or_else(|| async_graphql::Error::new(format!("{} is required", arg_name)))?;
                        let cast = if *is_int { "::int" } else { "" };
                        where_clauses.push(format!("{} = ${}{}", col_name, pk_start + i, cast));
                        params.push(val);
                    }

                    let mut sql = format!(
                        "WITH upd AS (UPDATE {} SET {} WHERE {}",
                        table_name,
                        set_clauses.join(", "),
                        where_clauses.join(" AND "),
                    );
                    if let Ok(identity) = ctx.data::<Identity>() {
                        apply_row_filters(&mut sql, &mut params, identity, &row_filters);
                    }
                    sql.push_str(" RETURNING *) SELECT row_to_json(upd)::text FROM upd");

                    let app_ctx = ctx.data::<std::sync::Arc<AppContext>>()
                        .map_err(|_| async_graphql::Error::new("internal context missing"))?;
                    let row = db::fetch_one(&app_ctx.pool, &sql, &params)
                        .await?
                        .ok_or_else(|| async_graphql::Error::new("no row returned"))?;

                    Ok(Some(FieldValue::value(gql_val(row))))
                })
            },
        );

        for (arg_name, _, is_int) in &pk_args {
            let arg_type = if *is_int { TypeRef::named_nn(TypeRef::INT) } else { TypeRef::named_nn(TypeRef::STRING) };
            update_field = update_field.argument(InputValue::new(arg_name.clone(), arg_type));
        }
        mutation = mutation.field(
            update_field.argument(InputValue::new(
                "input",
                TypeRef::named_nn(format!("Update{}Input", name_caps)),
            ))
        );

        let delete_table_name = table_name.clone();
        let delete_pk_args = pk_args.clone();
        let delete_row_filters = table_config.row_filters.clone();

        let mut delete_field = Field::new(
            format!("delete{}", name_caps),
            TypeRef::named_nn(table_name.clone()),
            move |ctx| {
                let table_name = delete_table_name.clone();
                let pk_args = delete_pk_args.clone();
                let row_filters = delete_row_filters.clone();

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

                    let mut sql =
                        format!("WITH del AS (DELETE FROM {} WHERE {}", table_name, where_clauses.join(" AND "));
                    if let Ok(identity) = ctx.data::<Identity>() {
                        apply_row_filters(&mut sql, &mut params, identity, &row_filters);
                    }
                    sql.push_str(" RETURNING *) SELECT row_to_json(del)::text FROM del");

                    let app_ctx = ctx.data::<std::sync::Arc<AppContext>>()
                        .map_err(|_| async_graphql::Error::new("internal context missing"))?;
                    let row = db::fetch_one(&app_ctx.pool, &sql, &params)
                        .await?
                        .ok_or_else(|| async_graphql::Error::new("no row returned"))?;

                    Ok(Some(FieldValue::value(gql_val(row))))
                })
            },
        );

        for (arg_name, _, is_int) in &pk_args {
            let arg_type = if *is_int { TypeRef::named_nn(TypeRef::INT) } else { TypeRef::named_nn(TypeRef::STRING) };
            delete_field = delete_field.argument(InputValue::new(arg_name.clone(), arg_type));
        }
        mutation = mutation.field(delete_field);
    }

    mutation
}
