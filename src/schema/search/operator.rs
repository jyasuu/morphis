use std::collections::HashMap;

use async_graphql::dynamic::{Enum, InputObject, InputValue, ObjectAccessor, TypeRef, ValueAccessor};

use crate::config::{ColumnType, SearchIndexConfig, SearchJoinConfig, TableConfig};
use crate::schema::util::{capitalize_first, capitalize_words};

/// Shape of a single operator value inside an operator input object.
#[derive(Clone, Copy)]
enum OpShape {
    /// A single nullable scalar (`eq: String`).
    Scalar,
    /// A non-null list of scalars (`in: [String!]`).
    List,
}

/// Metadata for one operator, shared by schema generation and interpretation.
struct Operator {
    name: &'static str,
    shape: OpShape,
}

const fn op(name: &'static str, shape: OpShape) -> Operator {
    Operator { name, shape }
}

const STRING_OPERATORS: &[Operator] = &[
    op("eq", OpShape::Scalar),
    op("ne", OpShape::Scalar),
    op("in", OpShape::List),
    op("all", OpShape::List),
    op("contains", OpShape::Scalar),
    op("starts_with", OpShape::Scalar),
    op("ends_with", OpShape::Scalar),
];

const NUMERIC_OPERATORS: &[Operator] = &[
    op("eq", OpShape::Scalar),
    op("ne", OpShape::Scalar),
    op("in", OpShape::List),
    op("all", OpShape::List),
    op("gt", OpShape::Scalar),
    op("gte", OpShape::Scalar),
    op("lt", OpShape::Scalar),
    op("lte", OpShape::Scalar),
];

const BOOLEAN_OPERATORS: &[Operator] = &[
    op("eq", OpShape::Scalar),
    op("ne", OpShape::Scalar),
];

/// The union of every operator name, derived from the registry — the single
/// source consulted by both schema generation and ES interpretation.
fn all_operators() -> impl Iterator<Item = &'static Operator> {
    STRING_OPERATORS
        .iter()
        .chain(NUMERIC_OPERATORS.iter())
        .chain(BOOLEAN_OPERATORS.iter())
}

/// True when the keys of a filter value object match the operator registry.
fn is_operator_object(keys: &[String]) -> bool {
    keys.iter()
        .any(|k| all_operators().any(|o| o.name == k.as_str()))
}

/// The operator input type name for a column type.
fn operator_type_name(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int => "IntOperatorsInput",
        ColumnType::Int64 => "BigIntOperatorsInput",
        ColumnType::Float => "FloatOperatorsInput",
        ColumnType::Boolean => "BooleanOperatorsInput",
        _ => "StringOperatorsInput",
    }
}

fn build_operator_input(
    type_name: &'static str,
    scalar_ty: &'static str,
    operators: &[Operator],
) -> InputObject {
    let mut input = InputObject::new(type_name);
    for o in operators {
        let ty = match o.shape {
            OpShape::Scalar => TypeRef::named(scalar_ty),
            OpShape::List => TypeRef::named_nn_list(scalar_ty),
        };
        input = input.field(InputValue::new(o.name, ty));
    }
    input
}

/// The operator input objects shared by every search index.
pub(crate) fn operator_inputs() -> Vec<InputObject> {
    vec![
        build_operator_input(
            "StringOperatorsInput",
            TypeRef::STRING,
            STRING_OPERATORS,
        ),
        build_operator_input("IntOperatorsInput", TypeRef::INT, NUMERIC_OPERATORS),
        build_operator_input("BigIntOperatorsInput", "BigInt", NUMERIC_OPERATORS),
        build_operator_input(
            "FloatOperatorsInput",
            TypeRef::FLOAT,
            NUMERIC_OPERATORS,
        ),
        build_operator_input(
            "BooleanOperatorsInput",
            TypeRef::BOOLEAN,
            BOOLEAN_OPERATORS,
        ),
    ]
}

/// The `queryOperator` enum controlling how a `search{Index}` free-text query
/// combines its terms across searchable fields.
pub(crate) fn query_operator_enum() -> Enum {
    Enum::new("QueryOperator")
        .item("AND")
        .item("OR")
}

fn lookup_column_type<'a>(
    field_name: &str,
    table_config: &'a TableConfig,
) -> Option<&'a ColumnType> {
    table_config
        .columns
        .iter()
        .find(|c| c.name == field_name)
        .map(|c| &c.col_type)
}

fn resolve_table_config<'a>(
    table_name: &str,
    tables: &'a HashMap<String, TableConfig>,
) -> Option<&'a TableConfig> {
    tables.values().find(|t| t.table == table_name)
}

fn build_nested_search_filters(
    join_fields: &[SearchJoinConfig],
    accumulator: &mut Vec<InputObject>,
    tables: &HashMap<String, TableConfig>,
) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    for jf in join_fields {
        let type_name = format!("{}Filter", capitalize_words(&jf.index_field));
        let nested = build_nested_search_filters(&jf.join_fields, accumulator, tables);
        let mut input = InputObject::new(&type_name);
        let target_table = resolve_table_config(&jf.table, tables);
        for f in &jf.searchable_fields {
            let op_type = target_table
                .and_then(|tc| lookup_column_type(f, tc))
                .map(operator_type_name)
                .unwrap_or("StringOperatorsInput");
            input = input.field(InputValue::new(f.clone(), TypeRef::named(op_type)));
        }
        for (field_name, nested_type) in nested {
            input = input.field(InputValue::new(field_name, TypeRef::named(nested_type)));
        }
        accumulator.push(input);
        fields.push((jf.index_field.clone(), type_name));
    }
    fields
}

/// Nested filter input objects for an index, plus the ordered
/// `(field, type)` pairs to attach to the top-level search filter.
pub(crate) fn nested_filter_inputs(
    index_cfg: &SearchIndexConfig,
    tables: &HashMap<String, TableConfig>,
) -> (Vec<InputObject>, Vec<(String, String)>) {
    let mut nested_filters = Vec::new();
    let nested_fields =
        build_nested_search_filters(&index_cfg.join_fields, &mut nested_filters, tables);
    (nested_filters, nested_fields)
}

/// The top-level `<Index>SearchFilter` input object for an index.
pub(crate) fn build_search_filter_input(
    index_cfg: &SearchIndexConfig,
    tables: &HashMap<String, TableConfig>,
    nested_fields: &[(String, String)],
) -> InputObject {
    let source_table = tables.get(&index_cfg.graphql_type);
    let mut input_obj =
        InputObject::new(format!("{}SearchFilter", capitalize_first(&index_cfg.index)));
    for f in &index_cfg.searchable_fields {
        let op_type = source_table
            .and_then(|tc| lookup_column_type(f, tc))
            .map(operator_type_name)
            .unwrap_or("StringOperatorsInput");
        input_obj = input_obj.field(InputValue::new(f.clone(), TypeRef::named(op_type)));
    }
    for (field_name, type_name) in nested_fields {
        input_obj = input_obj.field(InputValue::new(
            field_name.clone(),
            TypeRef::named(type_name.clone()),
        ));
    }
    input_obj
}

/// Compile a filter input value into ES `must` clauses.
pub(super) fn build_es_filter(filter: Option<&ValueAccessor>) -> Vec<serde_json::Value> {
    let mut must = Vec::new();
    if let Some(f) = filter {
        must.extend(build_es_filter_val(f, ""));
    }
    must
}

fn build_es_filter_val(val: &ValueAccessor, path_prefix: &str) -> Vec<serde_json::Value> {
    let mut must = Vec::new();
    if let Ok(obj) = val.object() {
        for (key, child) in obj.iter() {
            if child.is_null() {
                continue;
            }
            let full_path = if path_prefix.is_empty() {
                key.to_string()
            } else {
                format!("{}.{}", path_prefix, key)
            };
            if let Ok(child_obj) = child.object() {
                let child_keys: Vec<String> =
                    child_obj.iter().map(|(k, _)| k.to_string()).collect();
                if is_operator_object(&child_keys) {
                    let field = string_field_path(&full_path);
                    must.extend(build_es_operator_clauses(&field, &child_obj));
                } else {
                    must.extend(build_es_filter_val(&child, &full_path));
                }
            } else if let Ok(s) = child.string() {
                if !s.is_empty() {
                    must.push(serde_json::json!({
                        "term": { full_path: s }
                    }));
                }
            } else if let Ok(n) = child.i64() {
                must.push(serde_json::json!({
                    "term": { full_path: n }
                }));
            } else if let Ok(n) = child.f64() {
                must.push(serde_json::json!({
                    "term": { full_path: n }
                }));
            }
        }
    }
    must
}

fn string_field_path(base: &str) -> String {
    if base.ends_with(".keyword") {
        base.to_string()
    } else {
        format!("{}.keyword", base)
    }
}

fn build_es_operator_clauses(
    field: &str,
    ops: &ObjectAccessor,
) -> Vec<serde_json::Value> {
    let mut clauses = Vec::new();

    if let Some(val) = ops.get("eq") {
        if let Ok(s) = val.string() {
            clauses.push(serde_json::json!({ "term": { field: s } }));
        } else if let Ok(n) = val.i64() {
            clauses.push(serde_json::json!({ "term": { field: n } }));
        } else if let Ok(n) = val.f64() {
            clauses.push(serde_json::json!({ "term": { field: n } }));
        } else if let Ok(b) = val.boolean() {
            clauses.push(serde_json::json!({ "term": { field: b } }));
        }
    }

    if let Some(val) = ops.get("ne") {
        let mut must_not = Vec::new();
        if let Ok(s) = val.string() {
            must_not.push(serde_json::json!({ "term": { field: s } }));
        } else if let Ok(n) = val.i64() {
            must_not.push(serde_json::json!({ "term": { field: n } }));
        } else if let Ok(n) = val.f64() {
            must_not.push(serde_json::json!({ "term": { field: n } }));
        } else if let Ok(b) = val.boolean() {
            must_not.push(serde_json::json!({ "term": { field: b } }));
        }
        if !must_not.is_empty() {
            clauses.push(serde_json::json!({ "bool": { "must_not": must_not } }));
        }
    }

    if let Some(val) = ops.get("in")
        && let Ok(arr) = val.list()
    {
        let values: Vec<serde_json::Value> = arr
            .iter()
            .filter_map(|v| {
                if let Ok(s) = v.string() {
                    Some(serde_json::json!(s))
                } else if let Ok(n) = v.i64() {
                    Some(serde_json::json!(n))
                } else if let Ok(n) = v.f64() {
                    Some(serde_json::json!(n))
                } else {
                    None
                }
            })
            .collect();
        if !values.is_empty() {
            clauses.push(serde_json::json!({ "terms": { field: values } }));
        }
    }

    if let Some(val) = ops.get("all")
        && let Ok(arr) = val.list()
    {
        for v in arr.iter() {
            if let Ok(s) = v.string() {
                clauses.push(serde_json::json!({ "term": { field: s } }));
            } else if let Ok(n) = v.i64() {
                clauses.push(serde_json::json!({ "term": { field: n } }));
            } else if let Ok(n) = v.f64() {
                clauses.push(serde_json::json!({ "term": { field: n } }));
            }
        }
    }

    if let Some(val) = ops.get("contains")
        && let Ok(s) = val.string()
        && !s.is_empty()
    {
        let query_field = if field.ends_with(".keyword") {
            field.strip_suffix(".keyword").unwrap_or(field)
        } else {
            field
        };
        clauses.push(serde_json::json!({ "match_phrase": { query_field: s } }));
    }

    if let Some(val) = ops.get("starts_with")
        && let Ok(s) = val.string()
        && !s.is_empty()
    {
        clauses.push(serde_json::json!({ "prefix": { field: s } }));
    }

    if let Some(val) = ops.get("ends_with")
        && let Ok(s) = val.string()
        && !s.is_empty()
    {
        clauses.push(serde_json::json!({ "wildcard": { field: format!("*{}", s) } }));
    }

    for op in &["gt", "gte", "lt", "lte"] {
        if let Some(val) = ops.get(op) {
            if let Ok(n) = val.i64() {
                clauses.push(serde_json::json!({ "range": { field: { *op: n } } }));
            } else if let Ok(n) = val.f64() {
                clauses.push(serde_json::json!({ "range": { field: { *op: n } } }));
            }
        }
    }

    clauses
}

/// Compile a filter input value into SQL WHERE fragments with bind parameters.
/// Returns `(sql_fragments, params)` where each param is a String to be bound
/// via sqlx. Fragments use `$N` placeholders matching the params vector.
pub(super) fn build_pg_filter(
    filter: Option<&ValueAccessor>,
) -> (Vec<String>, Vec<String>) {
    let mut fragments = Vec::new();
    let mut params = Vec::new();
    if let Some(f) = filter {
        build_pg_filter_val(f, "", &mut fragments, &mut params);
    }
    (fragments, params)
}

fn build_pg_filter_val(
    val: &ValueAccessor,
    path_prefix: &str,
    fragments: &mut Vec<String>,
    params: &mut Vec<String>,
) {
    if let Ok(obj) = val.object() {
        for (key, child) in obj.iter() {
            if child.is_null() {
                continue;
            }
            let full_path = if path_prefix.is_empty() {
                key.to_string()
            } else {
                format!("{}.{}", path_prefix, key)
            };
            if let Ok(child_obj) = child.object() {
                let child_keys: Vec<String> =
                    child_obj.iter().map(|(k, _)| k.to_string()).collect();
                if is_operator_object(&child_keys) {
                    build_pg_operator_clauses(&full_path, &child_obj, fragments, params);
                } else {
                    build_pg_filter_val(&child, &full_path, fragments, params);
                }
            } else if let Ok(s) = child.string()
                && !s.is_empty()
            {
                params.push(build_nested_jsonb(&full_path, serde_json::Value::String(s.to_string())));
                fragments.push(format!("search_doc @> ${}::jsonb", params.len()));
            }
        }
    }
}

fn build_pg_operator_clauses(
    field: &str,
    ops: &ObjectAccessor,
    fragments: &mut Vec<String>,
    params: &mut Vec<String>,
) {
    if let Some(val) = ops.get("eq") {
        let jsonb_val = if let Ok(s) = val.string() {
            serde_json::Value::String(s.to_string())
        } else if let Ok(n) = val.i64() {
            serde_json::json!(n)
        } else if let Ok(n) = val.f64() {
            serde_json::json!(n)
        } else if let Ok(b) = val.boolean() {
            serde_json::json!(b)
        } else {
            return;
        };
        params.push(build_nested_jsonb(field, jsonb_val));
        fragments.push(format!("search_doc @> ${}::jsonb", params.len()));
    }

    if let Some(val) = ops.get("ne") {
        let jsonb_val = if let Ok(s) = val.string() {
            serde_json::Value::String(s.to_string())
        } else if let Ok(n) = val.i64() {
            serde_json::json!(n)
        } else if let Ok(n) = val.f64() {
            serde_json::json!(n)
        } else if let Ok(b) = val.boolean() {
            serde_json::json!(b)
        } else {
            return;
        };
        params.push(build_nested_jsonb(field, jsonb_val));
        fragments.push(format!("NOT (search_doc @> ${}::jsonb)", params.len()));
    }

    if let Some(val) = ops.get("in")
        && let Ok(arr) = val.list()
    {
        let mut or_parts = Vec::new();
        for v in arr.iter() {
            let jsonb_val = if let Ok(s) = v.string() {
                serde_json::Value::String(s.to_string())
            } else if let Ok(n) = v.i64() {
                serde_json::json!(n)
            } else if let Ok(n) = v.f64() {
                serde_json::json!(n)
            } else {
                continue;
            };
            params.push(build_nested_jsonb(field, jsonb_val));
            or_parts.push(format!("search_doc @> ${}::jsonb", params.len()));
        }
        if !or_parts.is_empty() {
            fragments.push(format!("({})", or_parts.join(" OR ")));
        }
    }

    if let Some(val) = ops.get("all")
        && let Ok(arr) = val.list()
    {
        for v in arr.iter() {
            let jsonb_val = if let Ok(s) = v.string() {
                serde_json::Value::String(s.to_string())
            } else if let Ok(n) = v.i64() {
                serde_json::json!(n)
            } else if let Ok(n) = v.f64() {
                serde_json::json!(n)
            } else {
                continue;
            };
            params.push(build_nested_jsonb(field, jsonb_val));
            fragments.push(format!("search_doc @> ${}::jsonb", params.len()));
        }
    }

    if let Some(val) = ops.get("contains")
        && let Ok(s) = val.string()
        && !s.is_empty()
    {
        params.push(s.to_string());
        fragments.push(format!("search_doc &@~ ${}", params.len()));
    }

    if let Some(val) = ops.get("starts_with")
        && let Ok(s) = val.string()
        && !s.is_empty()
    {
        let json_path = json_path_to_pg(field);
        params.push(s.to_string());
        fragments.push(format!(
            "jsonb_extract_path_text(search_doc, {}) ILIKE ${} || '%'",
            json_path, params.len()
        ));
    }

    if let Some(val) = ops.get("ends_with")
        && let Ok(s) = val.string()
        && !s.is_empty()
    {
        let json_path = json_path_to_pg(field);
        params.push(s.to_string());
        fragments.push(format!(
            "jsonb_extract_path_text(search_doc, {}) ILIKE '%' || ${}",
            json_path, params.len()
        ));
    }

    for op in &["gt", "gte", "lt", "lte"] {
        if let Some(val) = ops.get(op) {
            let pg_op = match *op {
                "gt" => ">",
                "gte" => ">=",
                "lt" => "<",
                "lte" => "<=",
                _ => unreachable!(),
            };
            if let Ok(n) = val.i64() {
                let json_path = json_path_to_pg(field);
                params.push(n.to_string());
                fragments.push(format!(
                    "jsonb_extract_path_text(search_doc, {})::numeric {} ${}",
                    json_path, pg_op, params.len()
                ));
            } else if let Ok(n) = val.f64() {
                let json_path = json_path_to_pg(field);
                params.push(n.to_string());
                fragments.push(format!(
                    "jsonb_extract_path_text(search_doc, {})::numeric {} ${}",
                    json_path, pg_op, params.len()
                ));
            }
        }
    }
}

/// Convert a dotted field path like `material_features.feature_name` to a
/// PostgreSQL jsonb path like `'material_features', 'feature_name'`.
fn json_path_to_pg(field: &str) -> String {
    let parts: Vec<&str> = field.split('.').collect();
    parts
        .iter()
        .map(|p| format!("'{}'", p))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build a JSONB containment value from a dotted field path and a scalar.
///
/// For `name` → `{"name": "value"}` (top-level)
/// For `material_features.description` → `{"material_features": [{"description": "value"}]}`
///   (intermediate paths are wrapped in arrays because they represent has_many relations)
fn build_nested_jsonb(field: &str, value: serde_json::Value) -> String {
    let parts: Vec<&str> = field.split('.').collect();
    if parts.len() == 1 {
        return serde_json::json!({ field: value }).to_string();
    }
    let mut current = serde_json::json!({ *parts.last().unwrap(): value });
    for part in parts.iter().rev().skip(1) {
        current = serde_json::json!({ (*part): [current] });
    }
    current.to_string()
}
