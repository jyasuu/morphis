mod db;
mod input;
mod mutation;
mod query;
mod search;
mod table;
mod util;

use std::sync::Arc;

use async_graphql::dynamic::{InputObject, InputValue, Schema, TypeRef};
use sqlx::{Pool, Postgres};

use crate::config::Config;

#[derive(Clone)]
pub(crate) struct AppContext {
    pub pool: Pool<Postgres>,
    pub es_client: Option<reqwest::Client>,
    pub es_url: Option<String>,
}

pub async fn build_schema(config: Arc<Config>, pool: Pool<Postgres>) -> Schema {
    let es_client = config.elasticsearch.as_ref().map(|_| reqwest::Client::new());
    let es_url = config.elasticsearch.as_ref().map(|c| c.url.clone());
    let ctx = Arc::new(AppContext { pool, es_client, es_url });

    let mut schema_builder = Schema::build("Query", Some("Mutation"), None);
    schema_builder = schema_builder.data(ctx);

    let mut table_type_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (name, table_config) in &config.tables {
        table_type_map.insert(table_config.table.clone(), name.clone());
    }

    let mut table_objects = Vec::new();
    for (name, table_config) in &config.tables {
        if table_config.primary_key.is_empty() {
            panic!("Table '{}' has no primary_key defined", name);
        }
        if table_config.primary_key.len() > 1 {
            panic!(
                "Table '{}' has composite primary key ({:?}); only single-column PKs are supported",
                name, table_config.primary_key
            );
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
        table_objects.push((name.clone(), table_config.table.clone(), table_config.clone()));
    }

    let mut query = query::build_query_object(&config, &table_objects);

    for index_cfg in &config.search_indexes {
        tracing::debug!("Registering search index: {}", index_cfg.name);
        let sf = index_cfg.searchable_fields.clone();
        let mut input_obj = InputObject::new(format!("{}SearchFilter", util::capitalize_first(&index_cfg.index)));
        for f in &sf {
            input_obj = input_obj.field(InputValue::new(f.clone(), TypeRef::named(TypeRef::STRING)));
        }
        schema_builder = schema_builder.register(input_obj);
        query = search::add_search_field(query, index_cfg);
    }

    let mutation = mutation::build_mutation_object(&config, &table_objects);

    schema_builder = schema_builder.register(query);
    schema_builder = schema_builder.register(mutation);

    schema_builder.finish().unwrap()
}
