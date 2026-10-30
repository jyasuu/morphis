//! The `where` argument of `{table}List`: per-column comparison operators
//! compiled to parameterised SQL.
//!
//! It sits beside the original equality-only `filter` argument rather than
//! replacing it, so existing queries keep working. The operator names match the
//! search vocabulary (`eq`, `ne`, `in`, `gt`, `gte`, `lt`, `lte`, `contains`,
//! `starts_with`, `ends_with`), but the input types are SQL-specific
//! (`Sql*OperatorsInput`): search's string operators have no range comparisons,
//! and Date / DateTime / Uuid columns are all `String` in GraphQL, so reusing
//! them would make date-range filters impossible. `all` is search-only
//! (multi-valued fields) and is not offered here.
//!
//! Column names come from trusted config and values are always bound as
//! parameters, never interpolated. Operators on one column, and columns
//! within one `where`, are combined with AND.

use async_graphql::dynamic::{InputObject, InputValue, TypeRef, ValueAccessor};

use crate::config::{ColumnType, TableConfig};

/// Largest `in` list accepted, to stay far below Postgres' bind-parameter cap.
const MAX_IN_VALUES: usize = 500;

/// An operator's value, already stringified (every bind is text; the SQL side
/// casts, see [`param_cast`]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OpValue {
    Scalar(String),
    List(Vec<String>),
}

/// All operators supplied for one column.
#[derive(Debug, Clone)]
pub(crate) struct ColumnOps {
    pub column: String,
    pub col_type: ColumnType,
    pub ops: Vec<(String, OpValue)>,
}

/// `eq`/`ne`/`in` plus range comparisons: every type except Boolean.
const COMPARABLE: &[(&str, bool)] = &[
    ("eq", false),
    ("ne", false),
    ("in", true),
    ("gt", false),
    ("gte", false),
    ("lt", false),
    ("lte", false),
];
const TEXT_ONLY: &[(&str, bool)] = &[
    ("contains", false),
    ("starts_with", false),
    ("ends_with", false),
];
const BOOLEAN: &[(&str, bool)] = &[("eq", false), ("ne", false)];

fn operator_input(type_name: &'static str, scalar: &'static str, groups: &[&[(&str, bool)]]) -> InputObject {
    let mut input = InputObject::new(type_name);
    for (name, is_list) in groups.iter().flat_map(|g| g.iter()) {
        let ty = if *is_list {
            TypeRef::named_nn_list(scalar)
        } else {
            TypeRef::named(scalar)
        };
        input = input.field(InputValue::new(*name, ty));
    }
    input
}

/// The shared operator input types, registered once for the whole schema.
pub(crate) fn operator_inputs() -> Vec<InputObject> {
    vec![
        operator_input("SqlStringOperatorsInput", TypeRef::STRING, &[COMPARABLE, TEXT_ONLY]),
        operator_input("SqlIntOperatorsInput", TypeRef::INT, &[COMPARABLE]),
        operator_input("SqlBigIntOperatorsInput", "BigInt", &[COMPARABLE]),
        operator_input("SqlFloatOperatorsInput", TypeRef::FLOAT, &[COMPARABLE]),
        operator_input("SqlBooleanOperatorsInput", TypeRef::BOOLEAN, &[BOOLEAN]),
    ]
}

fn operator_type_name(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int => "SqlIntOperatorsInput",
        ColumnType::Int64 => "SqlBigIntOperatorsInput",
        ColumnType::Float => "SqlFloatOperatorsInput",
        ColumnType::Boolean => "SqlBooleanOperatorsInput",
        _ => "SqlStringOperatorsInput",
    }
}

/// `{Name}WhereInput`: one optional operator object per column.
pub(crate) fn build_where_input(name_caps: &str, table_config: &TableConfig) -> InputObject {
    let mut input = InputObject::new(where_input_name(name_caps));
    for col in &table_config.columns {
        input = input.field(InputValue::new(
            col.name.clone(),
            TypeRef::named(operator_type_name(&col.col_type)),
        ));
    }
    input
}

pub(crate) fn where_input_name(name_caps: &str) -> String {
    format!("{}WhereInput", name_caps)
}

/// Cast applied to each bound text parameter so it compares against the
/// column's real type (Postgres has no implicit text -> typed coercion).
fn param_cast(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int | ColumnType::Int64 => "::bigint",
        ColumnType::Float => "::double precision",
        ColumnType::Boolean => "::boolean",
        ColumnType::Uuid => "::uuid",
        ColumnType::Date => "::date",
        ColumnType::DateTime => "::timestamptz",
        ColumnType::String | ColumnType::Text | ColumnType::Json => "",
    }
}

/// The column expression used on the left-hand side. JSON has no ordering or
/// equality operator in Postgres, so it is compared as text.
fn column_expr(column: &str, col_type: &ColumnType) -> String {
    match col_type {
        ColumnType::Json => format!("{}::text", column),
        _ => column.to_string(),
    }
}

/// Escape LIKE metacharacters so user input is matched literally.
fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Compile operators into a SQL condition (no leading `WHERE`) and its bound
/// parameters, numbering placeholders from `first_param`.
///
/// Returns an empty string when there is nothing to filter on.
pub(crate) fn compile_where(
    columns: &[ColumnOps],
    first_param: usize,
) -> Result<(String, Vec<String>), String> {
    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();
    let next = |params: &Vec<String>| first_param + params.len();

    for col in columns {
        let lhs = column_expr(&col.column, &col.col_type);
        let cast = param_cast(&col.col_type);
        // LIKE-family operators need text on both sides, whatever the column type.
        let text_lhs = format!("{}::text", col.column);

        for (op, value) in &col.ops {
            match (op.as_str(), value) {
                ("eq" | "ne" | "gt" | "gte" | "lt" | "lte", OpValue::Scalar(v)) => {
                    let sym = match op.as_str() {
                        "eq" => "=",
                        "ne" => "<>",
                        "gt" => ">",
                        "gte" => ">=",
                        "lt" => "<",
                        _ => "<=",
                    };
                    clauses.push(format!("{} {} ${}{}", lhs, sym, next(&params), cast));
                    params.push(v.clone());
                }
                ("in", OpValue::List(values)) => {
                    if values.is_empty() {
                        return Err(format!("`in` on '{}' needs at least one value", col.column));
                    }
                    if values.len() > MAX_IN_VALUES {
                        return Err(format!(
                            "`in` on '{}' accepts at most {} values",
                            col.column, MAX_IN_VALUES
                        ));
                    }
                    let placeholders = values
                        .iter()
                        .map(|v| {
                            let p = format!("${}{}", next(&params), cast);
                            params.push(v.clone());
                            p
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    clauses.push(format!("{} IN ({})", lhs, placeholders));
                }
                ("contains" | "starts_with" | "ends_with", OpValue::Scalar(v)) => {
                    let escaped = escape_like(v);
                    let pattern = match op.as_str() {
                        "contains" => format!("%{}%", escaped),
                        "starts_with" => format!("{}%", escaped),
                        _ => format!("%{}", escaped),
                    };
                    clauses.push(format!(
                        "{} ILIKE ${} ESCAPE '\\'",
                        text_lhs,
                        next(&params)
                    ));
                    params.push(pattern);
                }
                ("all", _) => {
                    return Err(format!(
                        "operator `all` is only meaningful for search; '{}' is a single-valued SQL column",
                        col.column
                    ));
                }
                (other, _) => {
                    return Err(format!(
                        "unsupported operator `{}` on '{}'",
                        other, col.column
                    ));
                }
            }
        }
    }

    Ok((clauses.join(" AND "), params))
}

fn accessor_to_string(v: &ValueAccessor<'_>) -> Option<String> {
    if let Ok(s) = v.string() {
        Some(s.to_string())
    } else if let Ok(n) = v.i64() {
        Some(n.to_string())
    } else if let Ok(f) = v.f64() {
        Some(f.to_string())
    } else if let Ok(b) = v.boolean() {
        Some(b.to_string())
    } else {
        None
    }
}

/// Read the GraphQL `where` object into [`ColumnOps`]. Null operators are
/// skipped, matching how the equality `filter` treats null.
pub(crate) fn extract_column_ops(
    where_arg: ValueAccessor<'_>,
    table_config: &TableConfig,
) -> Result<Vec<ColumnOps>, String> {
    let obj = where_arg
        .object()
        .map_err(|_| "`where` must be an object".to_string())?;

    let mut out = Vec::new();
    for (key, val) in obj.iter() {
        if val.is_null() {
            continue;
        }
        let col = table_config
            .columns
            .iter()
            .find(|c| c.name == key.as_str())
            .ok_or_else(|| format!("unknown column '{}' in `where`", key))?;
        let ops_obj = val
            .object()
            .map_err(|_| format!("`where.{}` must be an operator object", key))?;

        let mut ops = Vec::new();
        for (op, op_val) in ops_obj.iter() {
            if op_val.is_null() {
                continue;
            }
            let value = if let Ok(list) = op_val.list() {
                OpValue::List(
                    list.iter()
                        .filter_map(|item| accessor_to_string(&item))
                        .collect(),
                )
            } else {
                match accessor_to_string(&op_val) {
                    Some(s) => OpValue::Scalar(s),
                    None => continue,
                }
            };
            ops.push((op.to_string(), value));
        }
        if !ops.is_empty() {
            out.push(ColumnOps {
                column: col.name.clone(),
                col_type: col.col_type.clone(),
                ops,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(column: &str, col_type: ColumnType, ops: Vec<(&str, OpValue)>) -> ColumnOps {
        ColumnOps {
            column: column.into(),
            col_type,
            ops: ops.into_iter().map(|(o, v)| (o.to_string(), v)).collect(),
        }
    }

    fn s(v: &str) -> OpValue {
        OpValue::Scalar(v.into())
    }

    #[test]
    fn comparison_operators_cast_to_the_column_type() {
        let (sql, params) = compile_where(
            &[ops("qty", ColumnType::Int, vec![("gte", s("5")), ("lt", s("10"))])],
            1,
        )
        .unwrap();
        assert_eq!(sql, "qty >= $1::bigint AND qty < $2::bigint");
        assert_eq!(params, vec!["5", "10"]);
    }

    #[test]
    fn placeholders_start_at_first_param() {
        let (sql, params) =
            compile_where(&[ops("name", ColumnType::String, vec![("ne", s("x"))])], 3).unwrap();
        assert_eq!(sql, "name <> $3");
        assert_eq!(params, vec!["x"]);
    }

    #[test]
    fn in_expands_to_one_placeholder_per_value() {
        let (sql, params) = compile_where(
            &[ops(
                "status",
                ColumnType::String,
                vec![("in", OpValue::List(vec!["a".into(), "b".into()]))],
            )],
            2,
        )
        .unwrap();
        assert_eq!(sql, "status IN ($2, $3)");
        assert_eq!(params, vec!["a", "b"]);
    }

    #[test]
    fn empty_and_oversized_in_lists_are_rejected() {
        let empty = ops("c", ColumnType::String, vec![("in", OpValue::List(vec![]))]);
        assert!(compile_where(&[empty], 1).is_err());
        let big = ops(
            "c",
            ColumnType::String,
            vec![("in", OpValue::List(vec!["x".into(); MAX_IN_VALUES + 1]))],
        );
        assert!(compile_where(&[big], 1).is_err());
    }

    #[test]
    fn like_operators_escape_metacharacters_and_match_case_insensitively() {
        let (sql, params) = compile_where(
            &[ops(
                "name",
                ColumnType::String,
                vec![("contains", s("50%_off\\")), ("starts_with", s("wo")), ("ends_with", s("ol"))],
            )],
            1,
        )
        .unwrap();
        assert_eq!(
            sql,
            "name::text ILIKE $1 ESCAPE '\\' AND name::text ILIKE $2 ESCAPE '\\' AND name::text ILIKE $3 ESCAPE '\\'"
        );
        assert_eq!(params, vec!["%50\\%\\_off\\\\%", "wo%", "%ol"]);
    }

    #[test]
    fn typed_columns_cast_params_and_json_compares_as_text() {
        let (sql, _) = compile_where(
            &[
                ops("created", ColumnType::DateTime, vec![("gt", s("2026-01-01T00:00:00Z"))]),
                ops("day", ColumnType::Date, vec![("eq", s("2026-01-01"))]),
                ops("uid", ColumnType::Uuid, vec![("eq", s("00000000-0000-0000-0000-000000000000"))]),
                ops("doc", ColumnType::Json, vec![("eq", s("{}"))]),
            ],
            1,
        )
        .unwrap();
        assert_eq!(
            sql,
            "created > $1::timestamptz AND day = $2::date AND uid = $3::uuid AND doc::text = $4"
        );
    }

    #[test]
    fn columns_combine_with_and_in_order() {
        let (sql, params) = compile_where(
            &[
                ops("a", ColumnType::Int, vec![("eq", s("1"))]),
                ops("b", ColumnType::Boolean, vec![("eq", s("true"))]),
            ],
            1,
        )
        .unwrap();
        assert_eq!(sql, "a = $1::bigint AND b = $2::boolean");
        assert_eq!(params, vec!["1", "true"]);
    }

    #[test]
    fn nothing_to_filter_yields_empty_sql() {
        let (sql, params) = compile_where(&[], 1).unwrap();
        assert!(sql.is_empty() && params.is_empty());
    }

    #[test]
    fn all_and_unknown_operators_are_errors_not_silent_noops() {
        let all = ops("c", ColumnType::String, vec![("all", OpValue::List(vec!["x".into()]))]);
        assert!(compile_where(&[all], 1).unwrap_err().contains("`all`"));
        let bogus = ops("c", ColumnType::String, vec![("regex", s("x"))]);
        assert!(compile_where(&[bogus], 1).is_err());
    }

    #[test]
    fn injection_attempts_stay_in_bound_params() {
        let (sql, params) = compile_where(
            &[ops("name", ColumnType::String, vec![("eq", s("x'; DROP TABLE t; --"))])],
            1,
        )
        .unwrap();
        assert_eq!(sql, "name = $1");
        assert_eq!(params, vec!["x'; DROP TABLE t; --"]);
    }
}
