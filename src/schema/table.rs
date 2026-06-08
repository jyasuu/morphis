use std::collections::HashMap;

use async_graphql::{
    Name, Value,
    dynamic::{Field, FieldFuture, FieldValue, Object, TypeRef},
};

use crate::config::{ColumnType, RelationType, TableConfig};

use super::db;
use super::util::{gql_val, gql_value_to_sql_string};
use super::{apply_row_filters, AppContext, Identity};

fn related_pk_order(rel_cfg: &TableConfig) -> String {
    rel_cfg.primary_key.iter()
        .map(|pk| format!("t.{}", pk))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn build_table_object(
    _name: &str,
    table_config: &TableConfig,
    all_tables: &HashMap<String, TableConfig>,
    table_type_map: &HashMap<String, String>,
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
        let fk_pairs = rel.field_pairs();
        let related_pk_order_by = related_pk_order(rel_cfg);
        let rel_row_filters = rel_cfg.row_filters.clone();
        let return_type_name = table_type_map.get(&rel.table).cloned().unwrap_or_default();

        match rel.rel_type {
            RelationType::HasMany => {
                let local_fields: Vec<String> = fk_pairs.iter().map(|(l, _)| l.to_string()).collect();
                let foreign_fields: Vec<String> = fk_pairs.iter().map(|(_, f)| f.to_string()).collect();
                let foreign_int_check: Vec<bool> = fk_pairs.iter().map(|(_, f_name)| {
                    rel_cfg.columns.iter().any(|c| c.name == *f_name && matches!(c.col_type, ColumnType::Int | ColumnType::Int64))
                }).collect();

                obj = obj.field(Field::new(rel.name.clone(), TypeRef::named_nn_list_nn(&return_type_name), move |ctx| {
                    let local_fields = local_fields.clone();
                    let foreign_fields = foreign_fields.clone();
                    let foreign_int_check = foreign_int_check.clone();
                    let rel_table = rel_table.clone();
                    let related_pk_order_by = related_pk_order_by.clone();
                    let row_filters = rel_row_filters.clone();

                    FieldFuture::new(async move {
                        let parent = ctx.parent_value.as_value()
                            .ok_or_else(|| async_graphql::Error::new("not a value"))?;

                        let mut where_clauses = Vec::new();
                        let mut params = Vec::new();
                        for (i, ((local_f, foreign_f), is_int)) in local_fields.iter().zip(foreign_fields.iter()).zip(foreign_int_check.iter()).enumerate() {
                            let local_val = match &parent {
                                Value::Object(map) => map.get(&Name::new(local_f)).cloned().unwrap_or(Value::Null),
                                _ => Value::Null,
                            };
                            let val_str = gql_value_to_sql_string(&local_val);
                            let cast = if *is_int { "::int" } else { "" };
                            where_clauses.push(format!("{} = ${}{}", foreign_f, i + 1, cast));
                            params.push(val_str);
                        }

                        let mut sql = format!(
                            "SELECT COALESCE(json_agg(row_to_json(t) ORDER BY {}), '[]'::json)::text FROM (SELECT * FROM {} WHERE {}",
                            related_pk_order_by, rel_table, where_clauses.join(" AND ")
                        );
                        if let Ok(identity) = ctx.data::<Identity>() {
                            apply_row_filters(&mut sql, &mut params, identity, &row_filters);
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
                }));
            }
            RelationType::HasOne => {
                let local_fields: Vec<String> = fk_pairs.iter().map(|(l, _)| l.to_string()).collect();
                let foreign_fields: Vec<String> = fk_pairs.iter().map(|(_, f)| f.to_string()).collect();
                let foreign_int_check: Vec<bool> = fk_pairs.iter().map(|(_, f_name)| {
                    rel_cfg.columns.iter().any(|c| c.name == *f_name && matches!(c.col_type, ColumnType::Int | ColumnType::Int64))
                }).collect();

                obj = obj.field(Field::new(rel.name.clone(), TypeRef::named(&return_type_name), move |ctx| {
                    let local_fields = local_fields.clone();
                    let foreign_fields = foreign_fields.clone();
                    let foreign_int_check = foreign_int_check.clone();
                    let rel_table = rel_table.clone();
                    let row_filters = rel_row_filters.clone();

                    FieldFuture::new(async move {
                        let parent = ctx.parent_value.as_value()
                            .ok_or_else(|| async_graphql::Error::new("not a value"))?;

                        let mut where_clauses = Vec::new();
                        let mut params = Vec::new();
                        for (i, ((local_f, foreign_f), is_int)) in local_fields.iter().zip(foreign_fields.iter()).zip(foreign_int_check.iter()).enumerate() {
                            let local_val = match &parent {
                                Value::Object(map) => map.get(&Name::new(local_f)).cloned().unwrap_or(Value::Null),
                                _ => Value::Null,
                            };
                            let val_str = gql_value_to_sql_string(&local_val);
                            let cast = if *is_int { "::int" } else { "" };
                            where_clauses.push(format!("{} = ${}{}", foreign_f, i + 1, cast));
                            params.push(val_str);
                        }

                        let mut sql = format!(
                            "SELECT row_to_json(t)::text FROM (SELECT * FROM {} WHERE {}",
                            rel_table, where_clauses.join(" AND ")
                        );
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
                }));
            }
            RelationType::BelongsTo => {
                let local_fields: Vec<String> = fk_pairs.iter().map(|(l, _)| l.to_string()).collect();
                let related_pk_cols: Vec<String> = fk_pairs.iter().map(|(_, f_name)| f_name.to_string()).collect();
                let pk_int_check: Vec<bool> = fk_pairs.iter().map(|(_, f_name)| {
                    rel_cfg.columns.iter().any(|c| c.name == *f_name && matches!(c.col_type, ColumnType::Int | ColumnType::Int64))
                }).collect();

                obj = obj.field(Field::new(rel.name.clone(), TypeRef::named(&return_type_name), move |ctx| {
                    let local_fields = local_fields.clone();
                    let related_pk_cols = related_pk_cols.clone();
                    let pk_int_check = pk_int_check.clone();
                    let rel_table = rel_table.clone();
                    let row_filters = rel_row_filters.clone();

                    FieldFuture::new(async move {
                        let parent = ctx.parent_value.as_value()
                            .ok_or_else(|| async_graphql::Error::new("not a value"))?;

                        let mut where_clauses = Vec::new();
                        let mut params = Vec::new();
                        for (i, ((local_f, related_f), is_int)) in local_fields.iter().zip(related_pk_cols.iter()).zip(pk_int_check.iter()).enumerate() {
                            let local_val = match &parent {
                                Value::Object(map) => map.get(&Name::new(local_f)).cloned().unwrap_or(Value::Null),
                                _ => Value::Null,
                            };
                            let val_str = gql_value_to_sql_string(&local_val);
                            let cast = if *is_int { "::int" } else { "" };
                            where_clauses.push(format!("{} = ${}{}", related_f, i + 1, cast));
                            params.push(val_str);
                        }

                        let mut sql = format!(
                            "SELECT row_to_json(t)::text FROM (SELECT * FROM {} WHERE {}",
                            rel_table, where_clauses.join(" AND ")
                        );
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
                }));
            }
        }
    }
    obj
}
