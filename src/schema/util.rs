use async_graphql::{
    Name, Value,
    dynamic::{ValueAccessor, indexmap::IndexMap},
};

pub(crate) fn json_to_gql(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Boolean(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                async_graphql::Number::from_f64(f).map_or(Value::Null, Value::Number)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => Value::List(arr.into_iter().map(json_to_gql).collect()),
        serde_json::Value::Object(obj) => {
            let map: IndexMap<Name, Value> = obj
                .into_iter()
                .map(|(k, v)| (Name::new(k), json_to_gql(v)))
                .collect();
            Value::Object(map)
        }
    }
}

pub(crate) fn gql_val(v: serde_json::Value) -> Value {
    json_to_gql(v)
}

pub(crate) fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

pub(crate) fn capitalize_words(s: &str) -> String {
    s.split('_')
        .map(|word| capitalize_first(word))
        .collect()
}

pub(crate) fn gql_value_to_sql_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn value_as_string(val: &ValueAccessor) -> String {
    if let Ok(s) = val.string() {
        s.to_string()
    } else if let Ok(n) = val.i64() {
        n.to_string()
    } else if let Ok(n) = val.f64() {
        n.to_string()
    } else if let Ok(b) = val.boolean() {
        b.to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("materials"), "Materials");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("a"), "A");
        assert_eq!(capitalize_first("ABC"), "ABC");
    }

    #[test]
    fn test_capitalize_words() {
        assert_eq!(capitalize_words("material_features"), "MaterialFeatures");
        assert_eq!(capitalize_words("feature_attributes"), "FeatureAttributes");
        assert_eq!(capitalize_words("single"), "Single");
        assert_eq!(capitalize_words(""), "");
    }

    #[test]
    fn test_json_to_gql_null() {
        assert_eq!(json_to_gql(serde_json::Value::Null), Value::Null);
    }

    #[test]
    fn test_json_to_gql_bool() {
        assert_eq!(
            json_to_gql(serde_json::Value::Bool(true)),
            Value::Boolean(true)
        );
        assert_eq!(
            json_to_gql(serde_json::Value::Bool(false)),
            Value::Boolean(false)
        );
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_json_to_gql_number() {
        let n = json_to_gql(serde_json::json!(42));
        assert_eq!(n, Value::Number(42.into()));

        let f = json_to_gql(serde_json::json!(3.14));
        assert!(matches!(f, Value::Number(_)));
    }

    #[test]
    fn test_json_to_gql_string() {
        assert_eq!(
            json_to_gql(serde_json::Value::String("hello".into())),
            Value::String("hello".into())
        );
    }

    #[test]
    fn test_json_to_gql_array() {
        let arr = serde_json::json!([1, "two", false]);
        let result = json_to_gql(arr);
        assert!(matches!(result, Value::List(_)));
        if let Value::List(items) = result {
            assert_eq!(items.len(), 3);
        }
    }

    #[test]
    fn test_json_to_gql_object() {
        let obj = serde_json::json!({"a": 1, "b": "hello"});
        let result = json_to_gql(obj);
        assert!(matches!(result, Value::Object(_)));
        if let Value::Object(map) = result {
            assert_eq!(map.get(&Name::new("a")), Some(&Value::Number(1.into())));
            assert_eq!(
                map.get(&Name::new("b")),
                Some(&Value::String("hello".into()))
            );
        }
    }

    #[test]
    fn test_gql_value_to_sql_string() {
        assert_eq!(gql_value_to_sql_string(&Value::String("abc".into())), "abc");
        assert_eq!(gql_value_to_sql_string(&Value::Number(42.into())), "42");
        assert_eq!(gql_value_to_sql_string(&Value::Boolean(true)), "true");
        assert_eq!(gql_value_to_sql_string(&Value::Boolean(false)), "false");
        assert_eq!(gql_value_to_sql_string(&Value::Null), "");
    }

    #[test]
    fn test_gql_val_alias() {
        let v = serde_json::json!({"x": 1});
        let result = gql_val(v);
        assert!(matches!(result, Value::Object(_)));
    }
}
