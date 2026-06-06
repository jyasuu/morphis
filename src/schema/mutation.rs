use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};

use crate::config::{ColumnType, Config, RowFilterConfig, TableConfig};

use super::db;
use super::util::{capitalize_first, gql_val, value_as_string};
use super::{apply_row_filters, AppContext, Identity};

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
        let is_pk_int = table_config.columns.iter().any(|c| {
            table_config.primary_key.contains(&c.name)
                && matches!(c.col_type, ColumnType::Int | ColumnType::Int64)
        });
        let pk_arg_type = if is_pk_int { TypeRef::named_nn(TypeRef::INT) } else { TypeRef::named_nn(TypeRef::STRING) };

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

                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>().unwrap();
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
        let update_pk = table_config.primary_key[0].clone();
        let update_row_filters = table_config.row_filters.clone();

        mutation = mutation.field(
            Field::new(
                format!("update{}", name_caps),
                TypeRef::named_nn(table_name.clone()),
                move |ctx| {
                    let table_config = update_table_config.clone();
                    let table_name = update_table_name.clone();
                    let pk = update_pk.clone();
                    let is_pk_int = is_pk_int;
                    let row_filters = update_row_filters.clone();

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
                        let mut sql = format!(
                            "WITH upd AS (UPDATE {} SET {} WHERE {} = ${}{}",
                            table_name,
                            set_clauses.join(", "),
                            pk,
                            params.len(),
                            cast,
                        );
                        if let Ok(identity) = ctx.data::<Identity>() {
                            apply_row_filters(&mut sql, &mut params, identity, &row_filters);
                        }
                        sql.push_str(" RETURNING *) SELECT row_to_json(upd)::text FROM upd");

                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>().unwrap();
                        let row = db::fetch_one(&app_ctx.pool, &sql, &params)
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
        let delete_row_filters = table_config.row_filters.clone();

        mutation = mutation.field(
            Field::new(
                format!("delete{}", name_caps),
                TypeRef::named_nn(table_name.clone()),
                move |ctx| {
                    let table_name = delete_table_name.clone();
                    let pk = delete_pk.clone();
                    let is_pk_int = is_pk_int;
                    let row_filters = delete_row_filters.clone();

                    FieldFuture::new(async move {
                        let id = if is_pk_int {
                            ctx.args.get("id").and_then(|v| v.i64().ok()).map(|n| n.to_string())
                        } else {
                            ctx.args.get("id").and_then(|v| v.string().ok()).map(String::from)
                        };
                        let id = id.ok_or_else(|| async_graphql::Error::new("id is required"))?;

                        let cast = if is_pk_int { "::int" } else { "" };
                        let mut sql =
                            format!("WITH del AS (DELETE FROM {} WHERE {} = $1{}", table_name, pk, cast);
                        let mut params = vec![id];
                        if let Ok(identity) = ctx.data::<Identity>() {
                            apply_row_filters(&mut sql, &mut params, identity, &row_filters);
                        }
                        sql.push_str(" RETURNING *) SELECT row_to_json(del)::text FROM del");

                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>().unwrap();
                        let row = db::fetch_one(&app_ctx.pool, &sql, &params)
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
