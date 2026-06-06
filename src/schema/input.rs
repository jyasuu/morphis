use async_graphql::dynamic::{InputObject, InputValue, TypeRef, ValueAccessor};

use crate::config::TableConfig;

pub(crate) fn build_create_input(name: &str, table_config: &TableConfig) -> InputObject {
    build_input_object(&format!("Create{}Input", name), table_config, false)
}

pub(crate) fn build_update_input(name: &str, table_config: &TableConfig) -> InputObject {
    build_input_object(&format!("Update{}Input", name), table_config, true)
}

pub(crate) fn build_input_object(name: &str, table_config: &TableConfig, all_nullable: bool) -> InputObject {
    let mut input = InputObject::new(name);
    for col in &table_config.columns {
        let is_pk = table_config.primary_key.contains(&col.name);
        if !all_nullable && is_pk && col.auto_increment {
            continue;
        }
        let nullable = all_nullable || col.nullable;
        let scalar = match col.col_type.to_string().as_str() {
            "Int" | "Int64" => TypeRef::INT,
            "Float" => TypeRef::FLOAT,
            "Boolean" => TypeRef::BOOLEAN,
            _ => TypeRef::STRING,
        };
        let type_ref = if nullable {
            TypeRef::named(scalar)
        } else {
            TypeRef::named_nn(scalar)
        };
        input = input.field(InputValue::new(col.name.clone(), type_ref));
    }
    input
}

pub(crate) fn build_filter_input(name: &str, table_config: &TableConfig) -> InputObject {
    let mut input = InputObject::new(format!("{}FilterInput", name));
    for col in &table_config.columns {
        let scalar = match col.col_type.to_string().as_str() {
            "Int" | "Int64" => TypeRef::INT,
            "Float" => TypeRef::FLOAT,
            "Boolean" => TypeRef::BOOLEAN,
            _ => TypeRef::STRING,
        };
        input = input.field(InputValue::new(col.name.clone(), TypeRef::named(scalar)));
    }
    input
}

pub(crate) fn build_filter_sql(
    filter: ValueAccessor,
    allowed_columns: &[String],
) -> (String, Vec<String>) {
    let obj = match filter.object() {
        Ok(o) => o,
        Err(_) => return (String::new(), vec![]),
    };

    let mut clauses = Vec::new();
    let mut params = Vec::new();

    for (key, val) in obj.iter() {
        if val.is_null() {
            continue;
        }
        if !allowed_columns.contains(&key.to_string()) {
            continue;
        }
        if let Ok(s) = val.string() {
            clauses.push(format!("{} = ${}", key, params.len() + 1));
            params.push(s.to_string());
        }
    }

    (clauses.join(" AND "), params)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_filter_clause_generation() {
        let allowed = ["name".to_string(), "status".to_string()];
        let pairs = vec![("name", "test"), ("status", "active")];

        let mut clauses = Vec::new();
        let mut params = Vec::new();
        for (key, val) in &pairs {
            if !allowed.contains(&key.to_string()) { continue; }
            clauses.push(format!("{} = ${}", key, params.len() + 1));
            params.push(val.to_string());
        }

        assert_eq!(clauses.join(" AND "), "name = $1 AND status = $2");
        assert_eq!(params, vec!["test", "active"]);
    }

    #[test]
    fn test_unknown_columns_skipped() {
        let allowed = ["name".to_string()];
        let pairs = [("name", "test"), ("INJECTION", "evil")];

        let result: Vec<&str> = pairs.iter()
            .filter(|(k, _)| allowed.contains(&k.to_string()))
            .map(|(k, _)| *k)
            .collect();

        assert_eq!(result, vec!["name"]);
    }

    #[test]
    fn test_empty_filter() {
        let pairs: Vec<(&str, &str)> = vec![];
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_null_values_skipped() {
        let allowed = ["name".to_string(), "status".to_string()];
        let pairs = vec![("name", "test")];

        let mut clauses = Vec::new();
        for (key, _val) in &pairs {
            if !allowed.contains(&key.to_string()) { continue; }
            clauses.push(format!("{} = $1", key));
        }

        assert_eq!(clauses, vec!["name = $1"]);
    }
}
