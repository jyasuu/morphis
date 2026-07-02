mod circuit_breaker;
mod config;
mod db;
mod mcp;
mod schema;

use std::sync::Arc;

use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Router, extract::Extension, middleware, response::Response, routing::get,
};
use jsonwebtoken::{DecodingKey, Validation};
use tower_http::cors::CorsLayer;

use circuit_breaker::CircuitBreaker;
use config::AuthConfig;
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

    let auth_config = config.auth.clone().unwrap_or(AuthConfig {
        enabled: false,
        jwks_url: None,
        issuer: None,
        audience: None,
        jwt_secret: None,
        identity_mappings: vec![],
    });

    let jwks_breaker = auth_config
        .jwks_url
        .as_ref()
        .map(|_| CircuitBreaker::new(config.circuit_breakers.jwks.to_circuit_breaker_config()));

    let mut app = Router::new()
        .route("/graphql", get(graphql_handler).post(graphql_handler))
        .route("/playground", get(graphql_playground))
        .route("/health", get(health))
        .layer(CorsLayer::permissive())
        .layer(Extension(schema));

    if auth_config.enabled {
        let auth = Arc::new(auth_config);
        app = app.layer(middleware::from_fn(move |req, next: middleware::Next| {
            let auth = auth.clone();
            let jwks_breaker = jwks_breaker.clone();
            async move { auth_middleware(req, next, auth, jwks_breaker).await }
        }));
    }

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

async fn auth_middleware(
    mut req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
    auth: Arc<AuthConfig>,
    jwks_breaker: Option<CircuitBreaker>,
) -> Response {
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string());

    if let Some(token) = auth_header {
        let claims = validate_jwt(&token, &auth, jwks_breaker.as_ref()).await;
        if let Ok(claims) = claims {
            let headers = req.headers_mut();
            for mapping in &auth.identity_mappings {
                if let Some(val) = claims.get(&mapping.claim).and_then(|v| v.as_str()) {
                    if let Ok(name) = axum::http::header::HeaderName::from_bytes(mapping.header.as_bytes())
                    {
                        if let Ok(value) = axum::http::HeaderValue::from_str(val) {
                            headers.insert(name, value);
                        }
                    }
                }
            }
        }
    }

    next.run(req).await
}

async fn validate_jwt(
    token: &str,
    auth: &AuthConfig,
    jwks_breaker: Option<&CircuitBreaker>,
) -> Result<serde_json::Value, String> {
    use jsonwebtoken::decode_header;

    let header = decode_header(token).map_err(|e| format!("JWT header decode failed: {}", e))?;

    if let Some(ref secret) = auth.jwt_secret {
        let mut validation = Validation::new(header.alg);
        if let Some(ref issuer) = auth.issuer {
            validation.set_issuer(&[issuer.as_str()]);
        }
        if let Some(ref aud) = auth.audience {
            validation.set_audience(&[aud.as_str()]);
        }
        validation.validate_exp = true;

        let data = jsonwebtoken::decode::<serde_json::Value>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .map_err(|e| format!("JWT validation failed: {}", e))?;
        return Ok(data.claims);
    }

    if let Some(ref jwks_url) = auth.jwks_url {
        let keys = fetch_jwks_keys(jwks_url, jwks_breaker).await.map_err(|e| format!("JWKS fetch failed: {}", e))?;
        for key in &keys {
            let mut validation = Validation::new(header.alg);
            if let Some(ref issuer) = auth.issuer {
                validation.set_issuer(&[issuer.as_str()]);
            }
            if let Some(ref aud) = auth.audience {
                validation.set_audience(&[aud.as_str()]);
            }
            validation.validate_exp = true;

            if let Ok(data) = jsonwebtoken::decode::<serde_json::Value>(token, key, &validation) {
                return Ok(data.claims);
            }
        }
        return Err("No matching JWK key found".to_string());
    }

    Err("No jwt_secret or jwks_url configured".into())
}

async fn fetch_jwks_keys(
    url: &str,
    breaker: Option<&CircuitBreaker>,
) -> Result<Vec<DecodingKey>, String> {
    let body = match breaker {
        Some(cb) => {
            cb.call(|| async { reqwest::get(url).await })
                .await
                .map_err(|e| e.to_string())?
        }
        None => reqwest::get(url).await.map_err(|e| format!("JWKS fetch failed: {}", e))?,
    };
    let body = body.text().await.map_err(|e| format!("JWKS body read failed: {}", e))?;
    let jwk_set: jsonwebtoken::jwk::JwkSet =
        serde_json::from_str(&body).map_err(|e| format!("JWKS parse failed: {}", e))?;
    let mut keys = Vec::new();
    for jwk in &jwk_set.keys {
        if let Ok(key) = DecodingKey::from_jwk(jwk) {
            keys.push(key);
        }
    }
    Ok(keys)
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
