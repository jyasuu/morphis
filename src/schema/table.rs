use std::collections::HashMap;

use async_graphql::{
    Name, Value,
    dynamic::{Field, FieldFuture, FieldValue, Object, TypeRef},
};

use crate::config::{ColumnType, RelationType, TableConfig};

use super::db;
use super::util::{gql_val, gql_value_to_sql_string};
use super::AppContext;

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
        let local_field = rel.local_field.clone();
        let foreign_field = rel.foreign_field.clone();
        let related_pk = rel_cfg.primary_key[0].clone();
        let foreign_int = rel_cfg.columns.iter().any(|c| c.name == foreign_field && matches!(c.col_type, ColumnType::Int | ColumnType::Int64));
        let pk_int = rel_cfg.columns.iter().any(|c| c.name == related_pk && matches!(c.col_type, ColumnType::Int | ColumnType::Int64));
        let return_type_name = table_type_map.get(&rel.table).cloned().unwrap_or_default();

        match rel.rel_type {
            RelationType::HasMany => {
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
                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>().unwrap();
                        let rows = db::fetch_many(&app_ctx.pool, &sql, &[val_str]).await?;
                        let items: Vec<FieldValue> = rows
                            .into_iter()
                            .map(|r| FieldValue::value(gql_val(r)))
                            .collect();
                        Ok(Some(FieldValue::list(items)))
                    })
                }));
            }
            RelationType::HasOne => {
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
                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>().unwrap();
                        match db::fetch_one(&app_ctx.pool, &sql, &[val_str]).await? {
                            Some(row) => Ok(Some(FieldValue::value(gql_val(row)))),
                            None => Ok(FieldValue::NONE),
                        }
                    })
                }));
            }
            RelationType::BelongsTo => {
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
                        let app_ctx = ctx.data::<std::sync::Arc<AppContext>>().unwrap();
                        match db::fetch_one(&app_ctx.pool, &sql, &[val_str]).await? {
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
