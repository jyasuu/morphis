use sqlx::{Pool, Postgres, Row};

pub(crate) async fn fetch_one(
    pool: &Pool<Postgres>,
    sql: &str,
    params: &[String],
) -> Result<Option<serde_json::Value>, async_graphql::Error> {
    let mut query = sqlx::query(sql);
    for p in params {
        query = query.bind(p);
    }
    match query.fetch_optional(pool).await {
        Ok(Some(row)) => {
            let json_str: String = row.try_get(0).map_err(|e| async_graphql::Error::new(e.to_string()))?;
            serde_json::from_str(&json_str)
                .map(Some)
                .map_err(|e| async_graphql::Error::new(e.to_string()))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(async_graphql::Error::new(e.to_string())),
    }
}

pub(crate) async fn fetch_many(
    pool: &Pool<Postgres>,
    sql: &str,
    params: &[String],
) -> Result<Vec<serde_json::Value>, async_graphql::Error> {
    let mut query = sqlx::query(sql);
    for p in params {
        query = query.bind(p);
    }
    match query.fetch_one(pool).await {
        Ok(row) => {
            let json_str: String = row.try_get(0).map_err(|e| async_graphql::Error::new(e.to_string()))?;
            let val: serde_json::Value =
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Array(vec![]));
            match val {
                serde_json::Value::Array(arr) => Ok(arr),
                _ => Ok(vec![val]),
            }
        }
        Err(e) => Err(async_graphql::Error::new(e.to_string())),
    }
}

pub(crate) async fn fetch_joined_rows(
    pool: &Pool<Postgres>,
    sql: &str,
    local_val: &str,
) -> Result<Vec<serde_json::Value>, async_graphql::Error> {
    let mut query = sqlx::query(sql);
    query = query.bind(local_val);
    let row = query.fetch_optional(pool).await
        .map_err(|e| async_graphql::Error::new(format!("fetch_joined_rows query failed: {}", e)))?;
    match row {
        Some(row) => {
            let json_str: String = row.try_get(0)
                .map_err(|e| async_graphql::Error::new(format!("fetch_joined_rows column error: {}", e)))?;
            let val: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| async_graphql::Error::new(format!("fetch_joined_rows JSON parse error: {}", e)))?;
            match val {
                serde_json::Value::Array(arr) => Ok(arr),
                _ => Ok(vec![val]),
            }
        }
        None => Ok(vec![]),
    }
}
