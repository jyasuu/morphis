use std::collections::HashMap;

use async_graphql::{
    Name, Value,
    dataloader::DataLoader,
    dynamic::{
        Field, FieldFuture, FieldValue, InputObject, InputValue, Object, ObjectAccessor,
        ResolverContext, TypeRef, ValueAccessor,
    },
};

use crate::config::{ColumnConfig, ColumnType, Config, RelationType, RowFilterConfig, TableConfig};

use super::db;
use super::relation::{RelKey, RelSpec, RelationLoader, build_filter_suffix, relation_field_value};
use super::search::apply_row_filters;
use super::util::{capitalize_first, gql_val, gql_value_to_sql_string, value_as_string};
use super::{AppContext, Identity};

/// Everything a single table contributes to the GraphQL schema.
pub(crate) struct TableSurface {
    pub object: Object,
    pub query_fields: Vec<Field>,
    pub mutation_fields: Vec<Field>,
    pub inputs: Vec<InputObject>,
}

pub(crate) fn build_table_surface(
    config: &Config,
    name: &str,
    table_config: &TableConfig,
) -> TableSurface {
    let name_caps = capitalize_first(name);
    let type_map: HashMap<String, String> = config
        .tables
        .iter()
        .map(|(name, tc)| (tc.table.clone(), name.clone()))
        .collect();

    let pk_args = build_pk_args(table_config);

    let inputs = vec![
        build_filter_input(&name_caps, table_config),
        build_create_input(&name_caps, table_config),
        build_update_input(&name_caps, table_config),
    ];

    let object = build_object_type(table_config, &config.tables, &type_map);

    let mut query_fields = Vec::new();
    if table_config.crud.read {
        query_fields.push(build_single_query_field(name, table_config, &pk_args));
        query_fields.push(build_list_query_field(name, table_config));
    }

    let mut mutation_fields = Vec::new();
    if table_config.crud.create {
        mutation_fields.push(build_create_field(&name_caps, table_config));
    }
    if table_config.crud.update {
        mutation_fields.push(build_update_field(&name_caps, table_config, &pk_args));
    }
    if table_config.crud.delete {
        mutation_fields.push(build_delete_field(&name_caps, table_config, &pk_args));
    }

    TableSurface {
        object,
        query_fields,
        mutation_fields,
        inputs,
    }
}

fn build_pk_args(table_config: &TableConfig) -> Vec<(String, String, bool)> {
    if table_config.primary_key.len() > 1 {
        table_config
            .primary_key
            .iter()
            .map(|pk_name| (pk_name.clone(), pk_name.clone(), is_int_col(table_config, pk_name)))
            .collect()
    } else {
        let pk = &table_config.primary_key[0];
        vec![("id".to_string(), pk.clone(), is_int_col(table_config, pk))]
    }
}

fn is_int_col(table_config: &TableConfig, col: &str) -> bool {
    table_config.columns.iter().any(|c| {
        c.name == col && matches!(c.col_type, ColumnType::Int | ColumnType::Int64)
    })
}

fn scalar_type_name(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int => TypeRef::INT,
        ColumnType::Int64 => "BigInt",
        ColumnType::Float => TypeRef::FLOAT,
        ColumnType::Boolean => TypeRef::BOOLEAN,
        _ => TypeRef::STRING,
    }
}

/// Every SQL parameter is bound as text (see `db::Bind`), so any comparison
/// or assignment against a non-text column needs an explicit cast — Postgres
/// has no implicit `text -> int4/int8/float8/bool` coercion (verified: it
/// fails with "operator does not exist: integer = text" otherwise). We cast
/// to the *widest* type in each family (`bigint`, `boolean`) rather than a
/// narrower one so this works uniformly across int4/int8-ish columns without
/// needing the exact declared width — Postgres indexes on int4 columns are
/// still used against a `bigint`-cast literal (int4/int8 share a btree
/// opfamily), so this doesn't cost an index scan.
fn sql_cast(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int | ColumnType::Int64 => "::bigint",
        ColumnType::Float => "::double precision",
        ColumnType::Boolean => "::boolean",
        _ => "",
    }
}

// ── Object type (columns + relations) ─────────────────────────

fn related_pk_order(rel_cfg: &TableConfig) -> String {
    rel_cfg
        .primary_key
        .iter()
        .map(|pk| format!("t.{}", pk))
        .collect::<Vec<_>>()
        .join(", ")
}

fn build_object_type(
    table_config: &TableConfig,
    all_tables: &HashMap<String, TableConfig>,
    type_map: &HashMap<String, String>,
) -> Object {
    let mut obj = Object::new(&table_config.table);
    for col in &table_config.columns {
        let field_type = if col.nullable {
            TypeRef::named(scalar_type_name(&col.col_type))
        } else {
            TypeRef::named_nn(scalar_type_name(&col.col_type))
        };
        let col_name = col.name.clone();
        obj = obj.field(Field::new(col.name.clone(), field_type, move |ctx| {
            let col_name = col_name.clone();
            FieldFuture::new(async move {
                let parent = ctx
                    .parent_value
                    .as_value()
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
        }));
    }

    for rel in &table_config.relations {
        let Some(rel_cfg) = all_tables.get(
            type_map
                .get(&rel.table)
                .map(String::as_str)
                .unwrap_or(""),
        ) else {
            continue;
        };
        let return_type_name = type_map.get(&rel.table).cloned().unwrap_or_default();
        let rel_row_filters = rel_cfg.row_filters.clone();
        obj = obj.field(build_relation_field(
            rel,
            rel_cfg,
            &return_type_name,
            rel_row_filters,
        ));
    }
    obj
}

fn build_relation_field(
    rel: &crate::config::RelationConfig,
    rel_cfg: &TableConfig,
    return_type_name: &str,
    row_filters: Vec<RowFilterConfig>,
) -> Field {
    let fk_pairs = rel.field_pairs();
    let local_fields: Vec<String> = fk_pairs.iter().map(|(l, _)| l.to_string()).collect();
    let foreign_fields: Vec<String> = fk_pairs.iter().map(|(_, f)| f.to_string()).collect();
    let is_list = matches!(rel.rel_type, RelationType::HasMany);
    let rel_table = rel.table.clone();
    let order_by = if is_list {
        related_pk_order(rel_cfg)
    } else {
        String::new()
    };
    let return_type = if is_list {
        TypeRef::named_nn_list_nn(return_type_name)
    } else {
        TypeRef::named(return_type_name)
    };

    Field::new(rel.name.clone(), return_type, move |ctx| {
        let local_fields = local_fields.clone();
        let foreign_fields = foreign_fields.clone();
        let rel_table = rel_table.clone();
        let order_by = order_by.clone();
        let row_filters = row_filters.clone();
        let is_list = is_list;

        FieldFuture::new(async move {
            let parent = ctx
                .parent_value
                .as_value()
                .ok_or_else(|| async_graphql::Error::new("not a value"))?;

            let fk = local_fields
                .iter()
                .map(|local_f| {
                    let local_val = match &parent {
                        Value::Object(map) => {
                            map.get(&Name::new(local_f)).cloned().unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    };
                    gql_value_to_sql_string(&local_val)
                })
                .collect::<Vec<_>>();

            let (filter_suffix, filter_params) =
                if let Ok(identity) = ctx.data::<Identity>() {
                    build_filter_suffix(identity, &row_filters, fk.len())
                } else {
                    (String::new(), Vec::new())
                };

            let spec = RelSpec {
                table: rel_table,
                foreign_fields,
                is_list,
                order_by,
                filter_suffix,
                filter_params,
            };
            let key = RelKey { spec, fk };

            let loader = ctx
                .data::<DataLoader<RelationLoader>>()
                .map_err(|_| async_graphql::Error::new("internal context missing"))?;

            let mut children = loader.load_many(vec![key.clone()]).await.map_err(|e| {
                async_graphql::Error::new(e.message)
            })?;
            let items = children.remove(&key).unwrap_or_default();

            Ok(relation_field_value(items, is_list))
        })
    })
}

// ── Input types ───────────────────────────────────────────────

fn build_create_input(name: &str, table_config: &TableConfig) -> InputObject {
    build_input_object(&format!("Create{}Input", name), table_config, false)
}

fn build_update_input(name: &str, table_config: &TableConfig) -> InputObject {
    build_input_object(&format!("Update{}Input", name), table_config, true)
}

fn build_input_object(
    name: &str,
    table_config: &TableConfig,
    all_nullable: bool,
) -> InputObject {
    let mut input = InputObject::new(name);
    for col in &table_config.columns {
        let is_pk = table_config.primary_key.contains(&col.name);
        if !all_nullable && is_pk && col.auto_increment {
            continue;
        }
        let nullable = all_nullable || col.nullable;
        let type_ref = if nullable {
            TypeRef::named(scalar_type_name(&col.col_type))
        } else {
            TypeRef::named_nn(scalar_type_name(&col.col_type))
        };
        input = input.field(InputValue::new(col.name.clone(), type_ref));
    }
    input
}

fn build_filter_input(name: &str, table_config: &TableConfig) -> InputObject {
    let mut input = InputObject::new(format!("{}FilterInput", name));
    for col in &table_config.columns {
        input = input.field(InputValue::new(
            col.name.clone(),
            TypeRef::named(scalar_type_name(&col.col_type)),
        ));
    }
    input
}

// ── Query fields ──────────────────────────────────────────────

fn build_single_query_field(
    name: &str,
    table_config: &TableConfig,
    pk_args: &[(String, String, bool)],
) -> Field {
    let pk_args = pk_args.to_vec();
    let table_name = table_config.table.clone();
    let row_filters = table_config.row_filters.clone();

    let closure_pk_args = pk_args.clone();
    let mut single_field = Field::new(
        name.to_string(),
        TypeRef::named(table_name.clone()),
        move |ctx| {
            let pk_args = closure_pk_args.clone();
            let table_name = table_name.clone();
            let row_filters = row_filters.clone();

            FieldFuture::new(async move {
                let (where_clauses, mut params) = pk_where_clauses(&pk_args, &ctx, 1)?;
                let mut sql = format!(
                    "SELECT row_to_json(t)::text FROM (SELECT * FROM {} WHERE {}",
                    table_name,
                    where_clauses.join(" AND ")
                );
                if let Ok(identity) = ctx.data::<Identity>() {
                    apply_row_filters(&mut sql, &mut params, identity, &row_filters);
                }
                sql.push_str(" LIMIT 1) t");
                let app_ctx = ctx
                    .data::<std::sync::Arc<AppContext>>()
                    .map_err(|_| async_graphql::Error::new("internal context missing"))?;

                match db::fetch_json(&app_ctx.pool, &sql, &db::text_binds(&params)).await? {
                    Some(row) => Ok(Some(FieldValue::value(gql_val(row)))),
                    None => Ok(FieldValue::NONE),
                }
            })
        },
    );

    for (arg_name, _, is_int) in &pk_args {
        let arg_type = if *is_int {
            TypeRef::named_nn(TypeRef::INT)
        } else {
            TypeRef::named_nn(TypeRef::STRING)
        };
        single_field = single_field.argument(InputValue::new(arg_name.clone(), arg_type));
    }
    single_field
}

fn build_list_query_field(name: &str, table_config: &TableConfig) -> Field {
    let list_name = format!("{}List", name);
    let table_name = table_config.table.clone();
    let col_names: Vec<String> = table_config
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let row_filters = table_config.row_filters.clone();

    Field::new(
        list_name,
        TypeRef::named_nn_list_nn(table_name.clone()),
        move |ctx| {
            let table_name = table_name.clone();
            let col_names = col_names.clone();
            let row_filters = row_filters.clone();

            FieldFuture::new(async move {
                let filter_arg = ctx.args.get("filter");
                let order_by = ctx
                    .args
                    .get("order_by")
                    .and_then(|v| v.string().ok().map(String::from));
                let limit = ctx.args.get("limit").and_then(|v| v.u64().ok());
                let offset = ctx.args.get("offset").and_then(|v| v.u64().ok());

                let mut sql = format!(
                    "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT * FROM {}",
                    table_name
                );
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

                let app_ctx = ctx
                    .data::<std::sync::Arc<AppContext>>()
                    .map_err(|_| async_graphql::Error::new("internal context missing"))?;
                let rows = db::fetch_rows(&app_ctx.pool, &sql, &db::text_binds(&params)).await?;
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
    .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)))
}

// ── Mutation fields ───────────────────────────────────────────

/// Column names and SQL parameters for a create. Null inputs are skipped so the
/// column's default applies; auto-set columns are excluded because their values
/// come from the request identity, not the input.
fn create_column_binds(
    columns: &[ColumnConfig],
    input: &ObjectAccessor,
    auto_set_columns: &[&str],
) -> (Vec<String>, Vec<String>, Vec<&'static str>) {
    let mut out_columns = Vec::new();
    let mut params = Vec::new();
    let mut casts = Vec::new();
    for col in columns {
        if auto_set_columns.contains(&col.name.as_str()) {
            continue;
        }
        if let Some(val) = input.get(&col.name) {
            if val.is_null() {
                continue;
            }
            out_columns.push(col.name.clone());
            params.push(value_as_string(&val));
            casts.push(sql_cast(&col.col_type));
        }
    }
    (out_columns, params, casts)
}

/// For each column present in the update input: `(column, param)` where `None`
/// means an explicit `col = NULL` (no bound parameter) and `Some(v)` is the
/// bound value. Columns absent from the input are left untouched.
fn update_column_binds(
    columns: &[ColumnConfig],
    input: &ObjectAccessor,
) -> Vec<(String, Option<String>, &'static str)> {
    columns
        .iter()
        .filter_map(|col| {
            let val = input.get(&col.name)?;
            let cast = sql_cast(&col.col_type);
            if val.is_null() {
                Some((col.name.clone(), None, cast))
            } else {
                Some((col.name.clone(), Some(value_as_string(&val)), cast))
            }
        })
        .collect()
}

fn build_create_field(name_caps: &str, table_config: &TableConfig) -> Field {
    let table_config = table_config.clone();
    let table_name = table_config.table.clone();
    let row_filters = table_config.row_filters.clone();

    Field::new(
        format!("create{}", name_caps),
        TypeRef::named_nn(table_name.clone()),
        move |ctx| {
            let table_config = table_config.clone();
            let table_name = table_name.clone();
            let row_filters = row_filters.clone();

            FieldFuture::new(async move {
                let input = ctx
                    .args
                    .get("input")
                    .ok_or_else(|| async_graphql::Error::new("input is required"))?;
                let obj = input.object()?;

                let auto_set_columns: Vec<&str> = row_filters
                    .iter()
                    .filter(|rf| rf.is_auto_set())
                    .filter_map(|rf| match rf {
                        RowFilterConfig::ColumnFilter { column, .. } => Some(column.as_str()),
                        _ => None,
                    })
                    .collect();

                let mut columns = Vec::new();
                let mut params = Vec::new();
                let mut casts = Vec::new();
                let (new_columns, new_params, new_casts) =
                    create_column_binds(&table_config.columns, &obj, &auto_set_columns);
                columns.extend(new_columns);
                params.extend(new_params);
                casts.extend(new_casts);

                if let Ok(identity) = ctx.data::<Identity>() {
                    for rf in &row_filters {
                        if rf.is_auto_set()
                            && let RowFilterConfig::ColumnFilter { column, from_header, .. } = rf
                            && let Some(val) = identity.header_value(from_header)
                        {
                            columns.push(column.clone());
                            params.push(val.to_string());
                            // Row-filter identity values (tenant IDs, user IDs, etc.) are
                            // always sourced from string headers, so no cast is needed.
                            casts.push("");
                        }
                    }
                }

                let placeholders: Vec<String> = (1..=params.len())
                    .map(|i| format!("${}{}", i, casts[i - 1]))
                    .collect();

                let sql = format!(
                    "WITH ins AS (INSERT INTO {} ({}) VALUES ({}) RETURNING *) SELECT row_to_json(ins)::text FROM ins",
                    table_name,
                    columns.join(", "),
                    placeholders.join(", ")
                );

                let app_ctx = ctx
                    .data::<std::sync::Arc<AppContext>>()
                    .map_err(|_| async_graphql::Error::new("internal context missing"))?;
                match db::fetch_json(&app_ctx.pool, &sql, &db::text_binds(&params)).await {
                    Ok(Some(row)) => {
                        tracing::info!(table = %table_name, "Created row");
                        Ok(Some(FieldValue::value(gql_val(row))))
                    }
                    Ok(None) => {
                        tracing::error!(table = %table_name, "Create returned no row");
                        Err(async_graphql::Error::new("no row returned"))
                    }
                    Err(e) => {
                        let msg = &e.message;
                        tracing::error!(table = %table_name, error = %msg, "Failed to create row");
                        Err(e)
                    }
                }
            })
        },
    )
    .argument(InputValue::new(
        "input",
        TypeRef::named_nn(format!("Create{}Input", name_caps)),
    ))
}

fn build_update_field(
    name_caps: &str,
    table_config: &TableConfig,
    pk_args: &[(String, String, bool)],
) -> Field {
    let table_config = table_config.clone();
    let table_name = table_config.table.clone();
    let pk_args = pk_args.to_vec();
    let row_filters = table_config.row_filters.clone();

    let closure_pk_args = pk_args.clone();
    let mut update_field = Field::new(
        format!("update{}", name_caps),
        TypeRef::named_nn(table_name.clone()),
        move |ctx| {
            let table_config = table_config.clone();
            let table_name = table_name.clone();
            let pk_args = closure_pk_args.clone();
            let row_filters = row_filters.clone();

            FieldFuture::new(async move {
                let input = ctx
                    .args
                    .get("input")
                    .ok_or_else(|| async_graphql::Error::new("input is required"))?;
                let obj = input.object()?;

                let mut set_clauses = Vec::new();
                let mut params = Vec::new();

                for (column, param, cast) in update_column_binds(&table_config.columns, &obj) {
                    match param {
                        None => set_clauses.push(format!("{column} = NULL")),
                        Some(v) => {
                            set_clauses.push(format!("{column} = ${}{cast}", params.len() + 1));
                            params.push(v);
                        }
                    }
                }

                let pk_start = params.len() + 1;
                let (where_clauses, pk_params) = pk_where_clauses(&pk_args, &ctx, pk_start)?;
                params.extend(pk_params);

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

                let app_ctx = ctx
                    .data::<std::sync::Arc<AppContext>>()
                    .map_err(|_| async_graphql::Error::new("internal context missing"))?;
                match db::fetch_json(&app_ctx.pool, &sql, &db::text_binds(&params)).await {
                    Ok(Some(row)) => {
                        tracing::info!(table = %table_name, "Updated row");
                        Ok(Some(FieldValue::value(gql_val(row))))
                    }
                    Ok(None) => {
                        tracing::error!(table = %table_name, "Update returned no row");
                        Err(async_graphql::Error::new("no row returned"))
                    }
                    Err(e) => {
                        let msg = &e.message;
                        tracing::error!(table = %table_name, error = %msg, "Failed to update row");
                        Err(e)
                    }
                }
            })
        },
    );

    for (arg_name, _, is_int) in &pk_args {
        let arg_type = if *is_int {
            TypeRef::named_nn(TypeRef::INT)
        } else {
            TypeRef::named_nn(TypeRef::STRING)
        };
        update_field = update_field.argument(InputValue::new(arg_name.clone(), arg_type));
    }
    update_field.argument(InputValue::new(
        "input",
        TypeRef::named_nn(format!("Update{}Input", name_caps)),
    ))
}

fn build_delete_field(
    name_caps: &str,
    table_config: &TableConfig,
    pk_args: &[(String, String, bool)],
) -> Field {
    let table_name = table_config.table.clone();
    let pk_args = pk_args.to_vec();
    let row_filters = table_config.row_filters.clone();

    let closure_pk_args = pk_args.clone();
    let mut delete_field = Field::new(
        format!("delete{}", name_caps),
        TypeRef::named_nn(table_name.clone()),
        move |ctx| {
            let table_name = table_name.clone();
            let pk_args = closure_pk_args.clone();
            let row_filters = row_filters.clone();

            FieldFuture::new(async move {
                let (where_clauses, mut params) = pk_where_clauses(&pk_args, &ctx, 1)?;
                let mut sql = format!(
                    "WITH del AS (DELETE FROM {} WHERE {}",
                    table_name,
                    where_clauses.join(" AND ")
                );
                if let Ok(identity) = ctx.data::<Identity>() {
                    apply_row_filters(&mut sql, &mut params, identity, &row_filters);
                }
                sql.push_str(" RETURNING *) SELECT row_to_json(del)::text FROM del");

                let app_ctx = ctx
                    .data::<std::sync::Arc<AppContext>>()
                    .map_err(|_| async_graphql::Error::new("internal context missing"))?;
                match db::fetch_json(&app_ctx.pool, &sql, &db::text_binds(&params)).await {
                    Ok(Some(row)) => {
                        tracing::info!(table = %table_name, "Deleted row");
                        Ok(Some(FieldValue::value(gql_val(row))))
                    }
                    Ok(None) => {
                        tracing::error!(table = %table_name, "Delete returned no row");
                        Err(async_graphql::Error::new("no row returned"))
                    }
                    Err(e) => {
                        let msg = &e.message;
                        tracing::error!(table = %table_name, error = %msg, "Failed to delete row");
                        Err(e)
                    }
                }
            })
        },
    );

    for (arg_name, _, is_int) in &pk_args {
        let arg_type = if *is_int {
            TypeRef::named_nn(TypeRef::INT)
        } else {
            TypeRef::named_nn(TypeRef::STRING)
        };
        delete_field = delete_field.argument(InputValue::new(arg_name.clone(), arg_type));
    }
    delete_field
}

fn pk_where_clauses(
    pk_args: &[(String, String, bool)],
    ctx: &ResolverContext<'_>,
    start: usize,
) -> Result<(Vec<String>, Vec<String>), async_graphql::Error> {
    let mut where_clauses = Vec::new();
    let mut params = Vec::new();
    for (i, (arg_name, col_name, is_int)) in pk_args.iter().enumerate() {
        let val = if *is_int {
            ctx.args
                .get(arg_name.as_str())
                .and_then(|v| v.i64().ok())
                .map(|n| n.to_string())
        } else {
            ctx.args
                .get(arg_name.as_str())
                .and_then(|v| v.string().ok())
                .map(String::from)
        };
        let val = val.ok_or_else(|| {
            async_graphql::Error::new(format!("{} is required", arg_name))
        })?;
        let cast = if *is_int { "::int" } else { "" };
        where_clauses.push(format!("{} = ${}{}", col_name, start + i, cast));
        params.push(val);
    }
    Ok((where_clauses, params))
}

// ── Filter compilation ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum FilterValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

fn build_filter_clauses(
    pairs: Vec<(String, FilterValue)>,
    allowed_columns: &[String],
) -> (String, Vec<String>) {
    let mut clauses = Vec::new();
    let mut params = Vec::new();

    for (key, val) in pairs {
        if !allowed_columns.contains(&key) {
            continue;
        }
        // The GraphQL filter input types each field by the column's own
        // ColumnType (see build_filter_input), so the FilterValue variant
        // here already reflects the target column's type — cast the bound
        // text parameter to match, same as pk_where_clauses/sql_cast do for
        // PK lookups and create/update, or Postgres rejects the comparison
        // outright (e.g. "operator does not exist: integer = text").
        let (param, cast) = match val {
            FilterValue::String(s) => (s, ""),
            FilterValue::Int(n) => (n.to_string(), "::bigint"),
            FilterValue::Float(f) => (f.to_string(), "::double precision"),
            FilterValue::Bool(b) => (b.to_string(), "::boolean"),
        };
        clauses.push(format!("{} = ${}{}", key, params.len() + 1, cast));
        params.push(param);
    }

    (clauses.join(" AND "), params)
}

fn build_filter_sql(filter: ValueAccessor, allowed_columns: &[String]) -> (String, Vec<String>) {
    let obj = match filter.object() {
        Ok(o) => o,
        Err(_) => return (String::new(), vec![]),
    };

    let mut pairs = Vec::new();
    for (key, val) in obj.iter() {
        if val.is_null() {
            continue;
        }
        let value = if let Ok(s) = val.string() {
            FilterValue::String(s.to_string())
        } else if let Ok(n) = val.i64() {
            FilterValue::Int(n)
        } else if let Ok(n) = val.f64() {
            FilterValue::Float(n)
        } else if let Ok(b) = val.boolean() {
            FilterValue::Bool(b)
        } else {
            continue;
        };
        pairs.push((key.to_string(), value));
    }

    build_filter_clauses(pairs, allowed_columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_config(primary_key: Vec<&str>) -> TableConfig {
        TableConfig {
            table: "items".into(),
            columns: vec![
                crate::config::ColumnConfig {
                    name: "id".into(),
                    col_type: ColumnType::Int,
                    nullable: false,
                    unique: true,
                    auto_increment: true,
                    default: None,
                    prompt: None,
                    examples: None,
                },
                crate::config::ColumnConfig {
                    name: "code".into(),
                    col_type: ColumnType::String,
                    nullable: false,
                    unique: false,
                    auto_increment: false,
                    default: None,
                    prompt: None,
                    examples: None,
                },
            ],
            primary_key: primary_key.iter().map(|s| s.to_string()).collect(),
            relations: vec![],
            row_filters: vec![],
            crud: crate::config::CrudConfig::default(),
            prompt: None,
            common_queries: vec![],
        }
    }

    #[test]
    fn pk_args_single_pk_aliased_to_id() {
        let cfg = table_config(vec!["id"]);
        assert_eq!(build_pk_args(&cfg), vec![("id".to_string(), "id".to_string(), true)]);
    }

    #[test]
    fn pk_args_string_pk_not_int() {
        let cfg = table_config(vec!["code"]);
        assert_eq!(build_pk_args(&cfg), vec![("id".to_string(), "code".to_string(), false)]);
    }

    #[test]
    fn pk_args_composite_keeps_raw_names() {
        let cfg = table_config(vec!["id", "code"]);
        assert_eq!(
            build_pk_args(&cfg),
            vec![
                ("id".to_string(), "id".to_string(), true),
                ("code".to_string(), "code".to_string(), false),
            ]
        );
    }

    #[test]
    fn input_type_names_follow_conventions() {
        let cfg = table_config(vec!["id"]);
        let filter = build_filter_input("Items", &cfg);
        let create = build_create_input("Items", &cfg);
        let update = build_update_input("Items", &cfg);
        assert_eq!(filter.type_name(), "ItemsFilterInput");
        assert_eq!(create.type_name(), "CreateItemsInput");
        assert_eq!(update.type_name(), "UpdateItemsInput");
    }

    #[test]
    fn object_type_name_is_table_name() {
        let cfg = table_config(vec!["id"]);
        let obj = build_object_type(&cfg, &HashMap::new(), &HashMap::new());
        assert_eq!(obj.type_name(), "items");
    }

    fn allowed(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn filter_clauses_string() {
        let pairs = vec![("name".to_string(), FilterValue::String("test".into()))];
        let (sql, params) = build_filter_clauses(pairs, &allowed(&["name"]));
        assert_eq!(sql, "name = $1");
        assert_eq!(params, vec!["test"]);
    }

    #[test]
    fn filter_clauses_int() {
        // Postgres has no implicit text -> integer cast for `=`, so the
        // bound text parameter needs an explicit cast or every numeric
        // filter fails at runtime with "operator does not exist: integer =
        // text" (verified against a live Postgres instance).
        let pairs = vec![("feature_id".to_string(), FilterValue::Int(3))];
        let (sql, params) = build_filter_clauses(pairs, &allowed(&["feature_id"]));
        assert_eq!(sql, "feature_id = $1::bigint");
        assert_eq!(params, vec!["3"]);
    }

    #[test]
    fn filter_clauses_float() {
        let pairs = vec![("price".to_string(), FilterValue::Float(3.5))];
        let (sql, params) = build_filter_clauses(pairs, &allowed(&["price"]));
        assert_eq!(sql, "price = $1::double precision");
        assert_eq!(params, vec!["3.5"]);
    }

    #[test]
    fn filter_clauses_bool() {
        let pairs = vec![("active".to_string(), FilterValue::Bool(true))];
        let (sql, params) = build_filter_clauses(pairs, &allowed(&["active"]));
        assert_eq!(sql, "active = $1::boolean");
        assert_eq!(params, vec!["true"]);
    }

    #[test]
    fn filter_clauses_bind_order_across_types() {
        let pairs = vec![
            ("name".to_string(), FilterValue::String("n".into())),
            ("feature_id".to_string(), FilterValue::Int(7)),
            ("flag".to_string(), FilterValue::Bool(false)),
        ];
        let (sql, params) = build_filter_clauses(pairs, &allowed(&["name", "feature_id", "flag"]));
        assert_eq!(
            sql,
            "name = $1 AND feature_id = $2::bigint AND flag = $3::boolean"
        );
        assert_eq!(params, vec!["n", "7", "false"]);
    }

    #[test]
    fn filter_clauses_unknown_columns_skipped() {
        let pairs = vec![
            ("name".to_string(), FilterValue::String("n".into())),
            ("INJECTION".to_string(), FilterValue::String("evil".into())),
        ];
        let (sql, params) = build_filter_clauses(pairs, &allowed(&["name"]));
        assert_eq!(sql, "name = $1");
        assert_eq!(params, vec!["n"]);
    }

    #[test]
    fn filter_clauses_empty() {
        let (sql, params) = build_filter_clauses(vec![], &allowed(&["name"]));
        assert_eq!(sql, "");
        assert!(params.is_empty());
    }
}
