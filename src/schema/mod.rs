pub(crate) mod db;
mod limits;
mod relation;
mod search;
mod table;
mod util;
mod where_filter;

use std::sync::Arc;

use async_graphql::dynamic::{Object, Scalar, Schema};
use async_graphql::dataloader::DataLoader;
use sqlx::{Pool, Postgres};

use crate::config::Config;

#[derive(Clone)]
pub(crate) struct AppContext {
    pub pool: Pool<Postgres>,
}

/// Claim-derived request identity consumed by row filters and RBAC.
///
/// Re-exported from the shared `identity-auth` crate so the HTTP handler, the MCP tools and
/// the schema all operate on the same type.
pub(crate) use identity_auth::Identity;

/// Shared in-process GraphQL execution seam.
///
/// Both the HTTP `/graphql` handler and the MCP `graphql` / `graphql_schema` tools cross this
/// seam, so a query + identity yields the same result regardless of entry point. The `Identity`
/// travels explicitly from the caller into the request data, where row filters and RBAC read it.
pub(crate) async fn execute(
    schema: &Schema,
    request: async_graphql::Request,
    identity: Identity,
) -> async_graphql::Response {
    let mut request = request;
    request.data.insert(identity);
    schema.execute(request).await
}

pub async fn build_schema(config: Arc<Config>, pool: Pool<Postgres>) -> Schema {
    let search_service = Arc::new(search::SearchService::new(&config, pool.clone()));
    build_schema_with_search(config, pool, search_service).await
}

pub(crate) async fn build_schema_with_search(
    config: Arc<Config>,
    pool: Pool<Postgres>,
    search_service: Arc<search::SearchService>,
) -> Schema {
    let ctx = Arc::new(AppContext { pool: pool.clone() });

    // Depth and complexity limits reject abusive queries before any SQL runs.
    // Depth uses our own extension because async-graphql's built-in limit also
    // counts introspection, which the admin UI and MCP tools rely on.
    let mut schema_builder = Schema::build("Query", Some("Mutation"), None)
        .extension(limits::DepthLimit::new(config.limits.max_depth))
        .limit_complexity(config.limits.max_complexity);
    schema_builder = schema_builder.data(ctx);
    schema_builder = schema_builder.data(DataLoader::new(
        relation::RelationLoader::new(pool),
        tokio::spawn,
    ));
    schema_builder = schema_builder.register(Scalar::new("BigInt"));

    let mut query = Object::new("Query");
    let mut mutation = Object::new("Mutation");
    for (name, table_config) in &config.tables {
        if table_config.primary_key.is_empty() {
            panic!("Table '{}' has no primary_key defined", name);
        }
        let surface = table::build_table_surface(&config, name, table_config);
        for input_obj in surface.inputs {
            schema_builder = schema_builder.register(input_obj);
        }
        schema_builder = schema_builder.register(surface.object);
        for field in surface.query_fields {
            query = query.field(field);
        }
        for field in surface.mutation_fields {
            mutation = mutation.field(field);
        }
    }

    // Register operator input types (idempotent — same types shared across all indexes)
    for input_obj in search::operator_inputs() {
        schema_builder = schema_builder.register(input_obj);
    }
    // SQL `where` operator types (shared by every table's `{Name}WhereInput`)
    for input_obj in where_filter::operator_inputs() {
        schema_builder = schema_builder.register(input_obj);
    }
    schema_builder = schema_builder.register(search::query_operator_enum());

    for index_cfg in &config.search_indexes {
        tracing::debug!("Registering search index: {}", index_cfg.name);

        // Build nested filter input types from join_fields
        let (nested_filters, nested_fields) =
            search::nested_filter_inputs(index_cfg, &config.tables);
        for input_obj in nested_filters {
            schema_builder = schema_builder.register(input_obj);
        }

        // Build top-level search filter input
        schema_builder = schema_builder.register(search::build_search_filter_input(
            index_cfg,
            &config.tables,
            &nested_fields,
        ));

        let search_row_filters = config
            .tables
            .get(&index_cfg.graphql_type)
            .map(|t| t.row_filters.clone())
            .unwrap_or_default();
        schema_builder = schema_builder.register(search::build_search_hit_object(index_cfg));
        query = search::add_search_field(
            query,
            index_cfg,
            search_row_filters,
            search_service.clone(),
            config.limits.clone(),
        );
    }

    schema_builder = schema_builder.register(query);
    schema_builder = schema_builder.register(mutation);

    schema_builder.finish().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_runs_introspection_in_process_without_http() {
        let config = Arc::new(crate::config::Config::from_file("config.yaml").unwrap());
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy(&config.database.url)
            .unwrap();
        let schema = build_schema(config.clone(), pool).await;

        let resp = execute(
            &schema,
            async_graphql::Request::new("{ __schema { queryType { name } } }"),
            Identity::default(),
        )
        .await;

        assert!(resp.is_ok(), "introspection returned errors");
        let data = serde_json::to_value(&resp.data).unwrap();
        assert_eq!(data["__schema"]["queryType"]["name"], "Query");
    }

    #[tokio::test]
    async fn schema_shape_matches_naming_conventions() {
        let config = Arc::new(crate::config::Config::from_file("config.yaml").unwrap());
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy(&config.database.url)
            .unwrap();
        let schema = build_schema(config.clone(), pool).await;

        let resp = execute(
            &schema,
            async_graphql::Request::new(
                "{ __schema { types { name } queryType { fields { name } } mutationType { fields { name } } } }",
            ),
            Identity::default(),
        )
        .await;

        assert!(resp.is_ok(), "introspection returned errors");
        let data = serde_json::to_value(&resp.data).unwrap();

        let types = data["__schema"]["types"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t["name"].as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for expected in [
            "materials",
            "sizes",
            "MaterialsFilterInput",
            "CreateMaterialsInput",
            "UpdateMaterialsInput",
            "CreateSizesInput",
        ] {
            assert!(types.contains(&expected), "missing type {expected}");
        }

        let query_fields = data["__schema"]["queryType"]["fields"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| f["name"].as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for expected in ["materials", "materialsList", "sizesList"] {
            assert!(query_fields.contains(&expected), "missing query {expected}");
        }

        let mutation_fields = data["__schema"]["mutationType"]["fields"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| f["name"].as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for expected in ["createMaterials", "updateMaterials", "deleteMaterials"] {
            assert!(
                mutation_fields.contains(&expected),
                "missing mutation {expected}"
            );
        }
        assert!(
            !mutation_fields.contains(&"createUser_permissions"),
            "crud-disabled table must not generate mutations"
        );
    }

    /// The standard graphql-js introspection query (what the frontend and
    /// GraphQL tooling send): 7 nested `ofType` levels under `__schema`.
    const FULL_INTROSPECTION: &str = r#"
    query IntrospectionQuery {
      __schema {
        queryType { name } mutationType { name } subscriptionType { name }
        types { ...FullType }
        directives { name description locations args { ...InputValue } }
      }
    }
    fragment FullType on __Type {
      kind name description
      fields(includeDeprecated: true) {
        name description args { ...InputValue } type { ...TypeRef }
        isDeprecated deprecationReason
      }
      inputFields { ...InputValue }
      interfaces { ...TypeRef }
      enumValues(includeDeprecated: true) { name description isDeprecated deprecationReason }
      possibleTypes { ...TypeRef }
    }
    fragment InputValue on __InputValue { name description type { ...TypeRef } defaultValue }
    fragment TypeRef on __Type {
      kind name
      ofType { kind name ofType { kind name ofType { kind name ofType { kind name
        ofType { kind name ofType { kind name ofType { kind name } } } } } } }
    }"#;

    async fn test_schema(tweak: impl FnOnce(&mut crate::config::Config)) -> Schema {
        let mut config = crate::config::Config::from_file("config.yaml").unwrap();
        tweak(&mut config);
        let config = Arc::new(config);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy(&config.database.url)
            .unwrap();
        build_schema(config, pool).await
    }

    fn errors(resp: &async_graphql::Response) -> String {
        resp.errors
            .iter()
            .map(|e| e.message.clone())
            .collect::<Vec<_>>()
            .join(" | ")
    }

    #[tokio::test]
    async fn full_introspection_still_passes_under_default_limits() {
        let schema = test_schema(|_| {}).await;
        let resp = execute(
            &schema,
            async_graphql::Request::new(FULL_INTROSPECTION),
            Identity::default(),
        )
        .await;
        assert!(resp.is_ok(), "introspection rejected: {}", errors(&resp));
    }

    #[tokio::test]
    async fn full_introspection_passes_even_with_a_tight_depth_limit() {
        let schema = test_schema(|c| c.limits.max_depth = 3).await;
        let resp = execute(
            &schema,
            async_graphql::Request::new(FULL_INTROSPECTION),
            Identity::default(),
        )
        .await;
        assert!(resp.is_ok(), "introspection rejected: {}", errors(&resp));
    }

    #[tokio::test]
    async fn queries_nested_beyond_max_depth_are_rejected_before_sql_runs() {
        let schema = test_schema(|c| c.limits.max_depth = 4).await;
        let deep = r#"{ materials(id: "M001") { material_features { feature_attributes {
            material_feature { material { material_features { feature_name } } } } } } }"#;
        let resp = execute(&schema, async_graphql::Request::new(deep), Identity::default()).await;
        assert!(!resp.is_ok());
        assert!(
            errors(&resp).to_lowercase().contains("too deep"),
            "unexpected error: {}",
            errors(&resp)
        );
    }

    #[tokio::test]
    async fn queries_above_max_complexity_are_rejected() {
        let schema = test_schema(|c| c.limits.max_complexity = 3).await;
        let wide = r#"{ materialsList { mat_no name status } }"#;
        let resp = execute(&schema, async_graphql::Request::new(wide), Identity::default()).await;
        assert!(!resp.is_ok());
        assert!(
            errors(&resp).to_lowercase().contains("complex"),
            "unexpected error: {}",
            errors(&resp)
        );
    }

    #[tokio::test]
    async fn list_queries_expose_a_typed_where_argument() {
        let schema = test_schema(|_| {}).await;
        let resp = execute(
            &schema,
            async_graphql::Request::new(
                r#"{ __type(name: "MaterialsWhereInput") { inputFields { name type { name } } } }"#,
            ),
            Identity::default(),
        )
        .await;
        assert!(resp.is_ok(), "{}", errors(&resp));
        let data = serde_json::to_value(&resp.data).unwrap();
        let fields = data["__type"]["inputFields"].as_array().expect("input type exists");
        let mat_no = fields.iter().find(|f| f["name"] == "mat_no").expect("mat_no field");
        assert_eq!(mat_no["type"]["name"], "SqlStringOperatorsInput");
    }

    #[tokio::test]
    async fn negative_limit_and_unsupported_operators_fail_before_any_sql() {
        // The lazy pool points at an unreachable DB, so reaching SQL would
        // surface a connection error instead of these validation messages.
        let schema = test_schema(|_| {}).await;

        let resp = execute(
            &schema,
            async_graphql::Request::new("{ materialsList(limit: -1) { mat_no } }"),
            Identity::default(),
        )
        .await;
        assert!(errors(&resp).contains("non-negative"), "{}", errors(&resp));

        let resp = execute(
            &schema,
            async_graphql::Request::new(
                r#"{ materialsList(where: { name: { all: ["x"] } }) { mat_no } }"#,
            ),
            Identity::default(),
        )
        .await;
        assert!(errors(&resp).contains("unknown field \"all\""), "{}", errors(&resp));

        let resp = execute(
            &schema,
            async_graphql::Request::new(
                r#"{ materialsList(where: { name: { in: [] } }) { mat_no } }"#,
            ),
            Identity::default(),
        )
        .await;
        assert!(errors(&resp).contains("at least one value"), "{}", errors(&resp));
    }
}
