mod db;
mod input;
mod mutation;
mod query;
mod search;
mod table;
mod util;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use async_graphql::dynamic::{InputObject, InputValue, Schema, TypeRef};
use sqlx::{Pool, Postgres};

use crate::config::{Config, PermissionCache, RowFilterConfig};

#[derive(Clone)]
pub(crate) struct AppContext {
    pub pool: Pool<Postgres>,
    pub es_client: Option<reqwest::Client>,
    pub es_url: Option<String>,
    pub permission_cache: Arc<Mutex<PermissionCache>>,
}

#[derive(Clone, Default)]
pub(crate) struct Identity {
    headers: HashMap<String, String>,
}

impl Identity {
    pub fn from_raw(headers: HashMap<String, String>) -> Self {
        Self { headers }
    }

    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(String::as_str)
    }
}

pub(crate) fn apply_row_filters(
    sql: &mut String,
    params: &mut Vec<String>,
    identity: &Identity,
    row_filters: &[RowFilterConfig],
) {
    for rf in row_filters {
        if let Some(val) = identity.header_value(rf.header_name()) {
            let clause = match rf {
                RowFilterConfig::ColumnFilter { column, .. } => {
                    if params.is_empty() {
                        format!(" WHERE {} = ${}", column, params.len() + 1)
                    } else {
                        format!(" AND {} = ${}", column, params.len() + 1)
                    }
                }
                RowFilterConfig::SubqueryFilter {
                    columns,
                    match_columns,
                    from_source,
                    user_column,
                    ..
                } => {
                    let prefix = if params.is_empty() {
                        " WHERE "
                    } else {
                        " AND "
                    };
                    format!(
                        "{} ({}) IN (SELECT {} FROM {} WHERE {} = ${})",
                        prefix,
                        columns.join(", "),
                        match_columns.join(", "),
                        from_source,
                        user_column,
                        params.len() + 1,
                    )
                }
            };
            sql.push_str(&clause);
            params.push(val.to_string());
        }
    }
}

pub async fn build_schema(config: Arc<Config>, pool: Pool<Postgres>) -> Schema {
    let es_client = config
        .elasticsearch
        .as_ref()
        .map(|_| reqwest::Client::new());
    let es_url = config.elasticsearch.as_ref().map(|c| c.url.clone());
    let ctx = Arc::new(AppContext {
        pool,
        es_client,
        es_url,
        permission_cache: Arc::new(Mutex::new(PermissionCache::new())),
    });

    let mut schema_builder = Schema::build("Query", Some("Mutation"), None);
    schema_builder = schema_builder.data(ctx);

    let mut table_type_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (name, table_config) in &config.tables {
        table_type_map.insert(table_config.table.clone(), name.clone());
    }

    let mut table_objects = Vec::new();
    for (name, table_config) in &config.tables {
        if table_config.primary_key.is_empty() {
            panic!("Table '{}' has no primary_key defined", name);
        }
        let name_caps = util::capitalize_first(name);
        let filter = input::build_filter_input(&name_caps, table_config);
        schema_builder = schema_builder.register(filter);
        let input_obj = input::build_create_input(&name_caps, table_config);
        schema_builder = schema_builder.register(input_obj);
        let update_input = input::build_update_input(&name_caps, table_config);
        schema_builder = schema_builder.register(update_input);

        let obj = table::build_table_object(name, table_config, &config.tables, &table_type_map);
        schema_builder = schema_builder.register(obj);
        table_objects.push((
            name.clone(),
            table_config.table.clone(),
            table_config.clone(),
        ));
    }

    let mut query = query::build_query_object(&config, &table_objects);

    for index_cfg in &config.search_indexes {
        tracing::debug!("Registering search index: {}", index_cfg.name);
        let sf = index_cfg.searchable_fields.clone();
        let mut input_obj = InputObject::new(format!(
            "{}SearchFilter",
            util::capitalize_first(&index_cfg.index)
        ));
        for f in &sf {
            input_obj =
                input_obj.field(InputValue::new(f.clone(), TypeRef::named(TypeRef::STRING)));
        }
        schema_builder = schema_builder.register(input_obj);
        let search_row_filters = config
            .tables
            .get(&index_cfg.graphql_type)
            .map(|t| t.row_filters.clone())
            .unwrap_or_default();
        query = search::add_search_field(query, index_cfg, search_row_filters);
    }

    let mutation = mutation::build_mutation_object(&config, &table_objects);

    schema_builder = schema_builder.register(query);
    schema_builder = schema_builder.register(mutation);

    schema_builder.finish().unwrap()
}
