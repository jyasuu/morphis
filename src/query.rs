use crate::config::ColumnType;

pub fn col_type_to_sql(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int => "INTEGER",
        ColumnType::Int64 => "BIGINT",
        ColumnType::Float => "REAL",
        ColumnType::Boolean => "BOOLEAN",
        ColumnType::String => "VARCHAR(255)",
        ColumnType::Text => "TEXT",
        ColumnType::Uuid => "UUID",
        ColumnType::DateTime => "TIMESTAMPTZ",
        ColumnType::Date => "DATE",
        ColumnType::Json => "JSONB",
    }
}

pub fn graphql_scalar(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int => "Int",
        ColumnType::Int64 => "Int64",
        ColumnType::Float => "Float",
        ColumnType::Boolean => "Boolean",
        ColumnType::String => "String",
        ColumnType::Text => "String",
        ColumnType::Uuid => "UUID",
        ColumnType::DateTime => "DateTime",
        ColumnType::Date => "Date",
        ColumnType::Json => "JSON",
    }
}
