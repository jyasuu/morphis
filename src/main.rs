mod config;
mod db;
mod mcp;
mod schema;

use std::sync::Arc;

use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{Router, extract::Extension, routing::get};
use tower_http::cors::CorsLayer;

use schema::Identity;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "morphis=info".into()),
        )
        .init();

    let config_path = std::env::var("MORPHIS_CONFIG").unwrap_or_else(|_| "config.yaml".to_string());
    let config = Arc::new(config::Config::from_file(&config_path)?);

    tracing::info!("Loaded config with {} tables", config.tables.len());

    let pool = db::connect(&config.database).await?;

    let schema = schema::build_schema(config.clone(), pool.clone()).await;

    let mut app = Router::new()
        .route("/graphql", get(graphql_handler).post(graphql_handler))
        .route("/playground", get(graphql_playground))
        .route("/health", get(health))
        .layer(CorsLayer::permissive())
        .layer(Extension(schema));

    // Mount MCP sub-router if enabled
    if let Some(mcp_router) = mcp::build_mcp_router(config.clone(), pool.clone()) {
        app = app.merge(mcp_router);
    }

    let addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!("Morphis server starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn graphql_handler(
    Extension(schema): Extension<async_graphql::dynamic::Schema>,
    headers: axum::http::HeaderMap,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let identity = Identity::from_raw(
        headers
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_lowercase(), v.to_string()))
            })
            .collect(),
    );
    let mut request = req.into_inner();
    request.data.insert(identity);
    schema.execute(request).await.into()
}

async fn graphql_playground() -> axum::response::Html<&'static str> {
    axum::response::Html(GRAPHQL_PLAYGROUND_HTML)
}

async fn health() -> &'static str {
    "ok"
}

const GRAPHQL_PLAYGROUND_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Morphis GraphQL Playground</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/graphql-playground-react/build/static/css/index.css" />
  <link rel="shortcut icon" href="https://cdn.jsdelivr.net/npm/graphql-playground-react/build/favicon.png" />
  <script src="https://cdn.jsdelivr.net/npm/graphql-playground-react/build/static/js/middleware.js"></script>
</head>
<body>
  <div id="root"></div>
  <script>window.addEventListener('load', function () { GraphQLPlayground.init(document.getElementById('root'), { endpoint: '/graphql' }); });</script>
</body>
</html>"#;
