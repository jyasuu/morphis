use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use axum::http::{header, HeaderValue, Request};
use axum::middleware;
use axum::response::Response;
use jsonwebtoken::{DecodingKey, Validation};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{tool, tool_router, ErrorData as McpError, RoleServer, ServerHandler};
use sqlx::{Pool, Postgres};

use crate::circuit_breaker::CircuitBreaker;
use crate::config::{Config, MCPAuthConfig};
use crate::schema::AppContext;
use crate::schema::Identity;

tokio::task_local! {
    pub static MCP_IDENTITY: Identity;
}

/// Cache for `graphql_schema` results — schema is static for the server lifetime.
static SCHEMA_CACHE: OnceLock<String> = OnceLock::new();

// ── Shared state for auth middleware ────────────────────────────

#[derive(Clone)]
pub struct MCPState {
    pub auth_config: Option<Arc<MCPAuthConfig>>,
    #[allow(dead_code)]
    pub app_context: Arc<AppContext>,
    #[allow(dead_code)]
    pub config: Arc<Config>,
    pub jwks_circuit_breaker: Option<CircuitBreaker>,
}

// ── MCP Server ─────────────────────────────────────────────────

pub struct MorphisMCPServer {
    config: Arc<Config>,
    #[allow(dead_code)]
    app_context: Arc<AppContext>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl MorphisMCPServer {
    pub fn new(config: Arc<Config>, app_context: Arc<AppContext>) -> Self {
        Self {
            config,
            app_context,
            tool_router: Self::tool_router(),
        }
    }

    fn col_info(&self, table_name: &str) -> Option<TableSchema> {
        let cfg = self.config.tables.get(table_name)?;
        let mut columns = Vec::new();
        for col in &cfg.columns {
            columns.push(ColumnSchema {
                name: col.name.clone(),
                col_type: col.col_type.to_string(),
                nullable: col.nullable,
                prompt: col.prompt.clone(),
                examples: col.examples.clone(),
                is_pk: cfg.primary_key.contains(&col.name),
            });
        }
        let search_indexes: Vec<String> = self
            .config
            .search_indexes
            .iter()
            .filter(|si| si.graphql_type == *table_name)
            .map(|si| si.name.clone())
            .collect();
        let relations: Vec<RelationSchema> = cfg
            .relations
            .iter()
            .map(|r| RelationSchema {
                name: r.name.clone(),
                rel_type: format!("{:?}", r.rel_type).to_lowercase(),
                table: r.table.clone(),
                local_field: r.local_field.clone(),
                foreign_field: r.foreign_field.clone(),
            })
            .collect();
        Some(TableSchema {
            db_table: cfg.table.clone(),
            prompt: cfg.prompt.clone(),
            columns,
            search_indexes,
            common_queries: cfg.common_queries.clone(),
            relations,
        })
    }

    /// Discover available tables with progressive detail.
    /// Call with no args (or detail: false) for a lightweight overview — table names, prompts,
    /// relations, and search indexes. Call with detail: true to get full column info
    /// (types, prompts, examples, nullable, primary key flags).
    #[tool(description = "Discover available tables, their prompts, relations, and search indexes. Always call this first. Pass detail:true to also get full column types, prompts, and examples for every table.")]
    async fn discover_tables(
        &self,
        Parameters(args): Parameters<DiscoverTablesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut tables = serde_json::Map::new();
        for name in self.config.tables.keys() {
            if let Some(info) = self.col_info(name) {
                let mut obj = serde_json::Map::new();
                obj.insert("db_table".into(), serde_json::json!(info.db_table));
                obj.insert("prompt".into(), serde_json::json!(info.prompt));

                if args.detail {
                    let cols: Vec<serde_json::Value> = info
                        .columns
                        .iter()
                        .map(|c| {
                            let mut m = serde_json::Map::new();
                            m.insert("name".into(), serde_json::json!(c.name));
                            m.insert("type".into(), serde_json::json!(c.col_type));
                            m.insert("nullable".into(), serde_json::json!(c.nullable));
                            m.insert("primary_key".into(), serde_json::json!(c.is_pk));
                            if let Some(p) = &c.prompt {
                                m.insert("prompt".into(), serde_json::json!(p));
                            }
                            if let Some(ex) = &c.examples {
                                m.insert("examples".into(), serde_json::json!(ex));
                            }
                            serde_json::Value::Object(m)
                        })
                        .collect();
                    obj.insert("columns".into(), serde_json::Value::Array(cols));
                } else {
                    let col_names: Vec<String> = info.columns.iter().map(|c| c.name.clone()).collect();
                    obj.insert("columns".into(), serde_json::json!(col_names));
                }

                obj.insert(
                    "search_indexes".into(),
                    serde_json::json!(info.search_indexes),
                );
                let rels: Vec<serde_json::Value> = info
                    .relations
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "name": r.name,
                            "type": r.rel_type,
                            "table": r.table,
                            "local_field": r.local_field,
                            "foreign_field": r.foreign_field,
                        })
                    })
                    .collect();
                if !rels.is_empty() {
                    obj.insert("relations".into(), serde_json::Value::Array(rels));
                }
                let cqs: Vec<serde_json::Value> = info
                    .common_queries
                    .iter()
                    .map(|cq| {
                        serde_json::json!({
                            "description": cq.description,
                            "tool": cq.tool,
                            "params": cq.params,
                        })
                    })
                    .collect();
                if !cqs.is_empty() {
                    obj.insert("common_queries".into(), serde_json::Value::Array(cqs));
                }
                tables.insert(name.clone(), serde_json::Value::Object(obj));
            }
        }
        let mut result = serde_json::Map::new();
        result.insert("tables".into(), serde_json::Value::Object(tables));
        let system_prompt = self
            .config
            .mcp
            .as_ref()
            .and_then(|m| m.prompts.as_ref())
            .and_then(|p| p.system.clone());
        let query_guidance = self
            .config
            .mcp
            .as_ref()
            .and_then(|m| m.prompts.as_ref())
            .and_then(|p| p.query_guidance.clone());
        if let Some(sp) = system_prompt {
            result.insert("system_prompt".into(), serde_json::json!(sp));
        }
        if let Some(qg) = query_guidance {
            result.insert("query_guidance".into(), serde_json::json!(qg));
        }
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(
                    format!("Failed to serialize response: {}", e),
                    None::<serde_json::Value>,
                ))?
        )]))
    }

    /// Execute a GraphQL query against the built-in endpoint.
    /// Supports nested relations, filtering, ordering, pagination, and mutations.
    ///
    /// Examples:
    ///   { materials(limit: 3) { mat_no name status } }
    ///   { materials(id: "M001") { mat_no name sizes { size_code } colorways { hex } } }
    ///   { materialsList(filter: { status: "active" }) { mat_no name material_features { feature_name } } }
    ///   mutation { createMaterials(input: { mat_no: "NEW01", name: "New", status: "active" }) { mat_no } }
    #[tool(description = "Execute any GraphQL query against the API. Supports nested relations, filtering, pagination, and mutations. Example: { materialsList(limit: 3) { mat_no name sizes { size_code } } }")]
    async fn graphql(
        &self,
        Parameters(args): Parameters<GraphqlArgs>,
    ) -> Result<CallToolResult, McpError> {
        let url = format!(
            "http://localhost:{}/graphql",
            self.config.server.port
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                McpError::internal_error(
                    format!("Failed to create HTTP client: {}", e),
                    None::<serde_json::Value>,
                )
            })?;
        let mut body = serde_json::json!({ "query": args.query });
        if let Some(vars) = args.variables {
            body["variables"] = vars;
        }

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                McpError::internal_error(
                    format!("GraphQL request failed: {}", e),
                    None::<serde_json::Value>,
                )
            })?;

        let text = resp.text().await.map_err(|e| {
            McpError::internal_error(
                format!("Failed to read GraphQL response: {}", e),
                None::<serde_json::Value>,
            )
        })?;

        // Surface GraphQL errors as tool errors so the LLM gets clear feedback
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(errors) = parsed.get("errors") {
                return Err(McpError::internal_error(
                    format!("GraphQL errors: {}", serde_json::to_string_pretty(errors).unwrap_or_default()),
                    None::<serde_json::Value>,
                ));
            }
            let formatted = serde_json::to_string_pretty(&parsed).unwrap_or(text);
            Ok(CallToolResult::success(vec![Content::text(formatted)]))
        } else {
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
    }

    /// Introspect the GraphQL schema and return every available query with its arguments, return type, and nested fields.
    /// Use this to learn the exact query names, filter inputs, and relation fields before calling graphql.
    /// Returns JSON with query name, description, arguments (name/type/description), return type, and nested fields.
    /// Example response for a query: { "query": "materialsList", "arguments": [ { "name": "filter", "type": "MaterialsFilterInput" }, ... ], "return_type": "[Materials!]!", "nested_fields": ["mat_no", "name", "sizes", ...] }
    #[tool(description = "Get the GraphQL schema: all query names, filter arguments, return types, and nested fields. Call this before graphql to learn the exact query syntax.")]
    async fn graphql_schema(&self) -> Result<CallToolResult, McpError> {
        // Return cached result — schema is static at runtime
        if let Some(cached) = SCHEMA_CACHE.get() {
            return Ok(CallToolResult::success(vec![Content::text(cached.clone())]));
        }

        let url = format!(
            "http://localhost:{}/graphql",
            self.config.server.port
        );
        let introspect_query = r#"
        {
          __schema {
            queryType {
              fields {
                name
                description
                args {
                  name
                  description
                  type { name kind ofType { name kind ofType { name kind } } }
                }
                type { name kind ofType { name kind ofType { name kind ofType { name kind } } } }
              }
            }
            types { name kind fields { name } }
          }
        }
        "#;

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .json(&serde_json::json!({ "query": introspect_query }))
            .send()
            .await
            .map_err(|e| {
                McpError::internal_error(
                    format!("Introspection request failed: {}", e),
                    None::<serde_json::Value>,
                )
            })?;

        let data: serde_json::Value = resp.json().await.map_err(|e| {
            McpError::internal_error(
                format!("Failed to parse introspection response: {}", e),
                None::<serde_json::Value>,
            )
        })?;

        let fields = data["data"]["__schema"]["queryType"]["fields"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let mut type_fields: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        if let Some(types) = data["data"]["__schema"]["types"].as_array() {
            for t in types {
                let tname = t["name"].as_str().unwrap_or("");
                if tname.starts_with("__") { continue; }
                let names: Vec<String> = t["fields"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|f| f["name"].as_str().map(|s| s.to_string()))
                            .filter(|n| !n.starts_with("__"))
                            .collect()
                    })
                    .unwrap_or_default();
                type_fields.insert(tname.to_string(), names);
            }
        }

        let mut result = Vec::new();
        for field in &fields {
            let name = field["name"].as_str().unwrap_or("");
            let desc = field["description"].as_str().unwrap_or("").to_string();
            let type_name = extract_type_name(field);

            let args: Vec<serde_json::Value> = field["args"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .map(|a| {
                            let aname = a["name"].as_str().unwrap_or("");
                            let atype = extract_type_name(a);
                            let adesc = a["description"].as_str().unwrap_or("");
                            serde_json::json!({
                                "name": aname,
                                "type": atype,
                                "description": adesc,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let type_name_clean = type_name.trim_end_matches('!').trim_start_matches('[').trim_end_matches(']').trim_end_matches('!').to_string();
            let nested: Vec<String> = type_fields.get(&type_name_clean)
                .cloned()
                .unwrap_or_default();

            result.push(serde_json::json!({
                "query": name,
                "description": if desc.is_empty() { serde_json::Value::Null } else { serde_json::json!(desc) },
                "return_type": type_name,
                "arguments": args,
                "nested_fields": if nested.is_empty() { serde_json::Value::Null } else { serde_json::json!(nested) },
            }));
        }

        let output = serde_json::json!({
            "graphql_queries": result,
            "note": "Use these query names and arguments in the graphql tool. Nested fields can be included in the selection set.",
        });

        let schema_json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(
                format!("Failed to format schema: {}", e),
                None::<serde_json::Value>,
            ))?;

        // Cache for subsequent calls — schema is static at runtime
        let _ = SCHEMA_CACHE.set(schema_json.clone());

        Ok(CallToolResult::success(vec![Content::text(schema_json)]))
    }
}

impl ServerHandler for MorphisMCPServer {
    fn get_info(&self) -> ServerInfo {
        let cfg = self.config.mcp.as_ref();
        let instructions = cfg
            .and_then(|m| {
                m.prompts
                    .as_ref()
                    .and_then(|p| p.query_guidance.clone())
            })
            .unwrap_or_else(|| {
                "Morphis Data MCP Server. Use discover_tables to explore available tables, \
                 graphql_schema to learn the GraphQL query syntax, \
                 and graphql to execute queries with nested relations."
                    .to_string()
            });
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(instructions)
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if self.get_tool(&request.name).is_none() {
            return Err(McpError::invalid_params(
                format!("Tool '{}' not found", request.name),
                None::<serde_json::Value>,
            ));
        }
        let tcc = ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }
}

// ── Parameter Structs ───────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DiscoverTablesArgs {
    /// When true, includes full column details (types, prompts, examples, nullable, primary key flags).
    /// When false (default), returns overview with column names only — call with detail:true to drill in.
    #[serde(default)]
    pub detail: bool,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GraphqlArgs {
    /// GraphQL query string. Supports nested relations.
    /// Example: { materialsList(limit: 3) { mat_no name status sizes { size_code } colorways { hex } } }
    pub query: String,
    /// Optional variables for the query
    #[serde(default)]
    pub variables: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct RelationSchema {
    name: String,
    rel_type: String,
    table: String,
    local_field: String,
    foreign_field: String,
}

#[derive(Debug, Clone)]
struct TableSchema {
    db_table: String,
    prompt: Option<String>,
    columns: Vec<ColumnSchema>,
    search_indexes: Vec<String>,
    common_queries: Vec<crate::config::CommonQueryConfig>,
    relations: Vec<RelationSchema>,
}

#[derive(Debug, Clone)]
struct ColumnSchema {
    name: String,
    col_type: String,
    nullable: bool,
    prompt: Option<String>,
    examples: Option<Vec<String>>,
    is_pk: bool,
}

// ── Auth Middleware ─────────────────────────────────────────────

async fn mcp_auth_middleware(
    axum::extract::State(state): axum::extract::State<MCPState>,
    mut req: Request<axum::body::Body>,
    next: middleware::Next,
) -> Response {
    let identity = match &state.auth_config {
        Some(auth_cfg) if auth_cfg.enabled => {
            let token = req
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(|s| s.to_string());

            match token {
                Some(token) => {
                    match validate_jwt(&token, auth_cfg, state.jwks_circuit_breaker.as_ref()).await {
                        Ok(claims) => {
                            let mut headers = std::collections::HashMap::new();
                            for mapping in &auth_cfg.identity_mappings {
                                if let Some(val) = claims.get(&mapping.claim) {
                                    if let Some(s) = val.as_str() {
                                        headers.insert(mapping.header.to_lowercase(), s.to_string());
                                    } else {
                                        headers.insert(mapping.header.to_lowercase(), val.to_string());
                                    }
                                }
                            }
                            Identity::from_raw(headers)
                        }
                        Err(e) => {
                            tracing::warn!("MCP JWT validation failed: {}", e);
                            return Response::builder()
                                .status(401)
                                .body(axum::body::Body::from("Unauthorized"))
                                .unwrap();
                        }
                    }
                }
                None => {
                    tracing::warn!("MCP request without Bearer token (rejected)");
                    return Response::builder()
                        .status(401)
                        .body(axum::body::Body::from("Unauthorized"))
                        .unwrap();
                }
            }
        }
        _ => {
            let headers = req
                .headers()
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .to_str()
                        .ok()
                        .map(|v| (name.as_str().to_lowercase(), v.to_string()))
                })
                .collect();
            Identity::from_raw(headers)
        }
    };

    MCP_IDENTITY
        .scope(identity, async move {
            // Ensure MCP requests have proper Accept header
            if req.uri().path().starts_with("/mcp") && !req.headers().contains_key(header::ACCEPT) {
                req.headers_mut().insert(
                    header::ACCEPT,
                    HeaderValue::from_static("application/json, text/event-stream"),
                );
            }
            next.run(req).await
        })
        .await
}

async fn validate_jwt(
    token: &str,
    auth: &MCPAuthConfig,
    jwks_breaker: Option<&CircuitBreaker>,
) -> Result<serde_json::Value, String> {
    use jsonwebtoken::decode_header;

    let header = decode_header(token).map_err(|e| format!("JWT header decode failed: {}", e))?;
    let kid = header.kid.clone();

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
        Ok(data.claims)
    } else if let Some(ref jwks_url) = auth.jwks_url {
        let jwks = fetch_jwks(jwks_url, jwks_breaker).await?;
        let key = find_key(&jwks, kid.as_deref())
            .ok_or_else(|| "No matching JWK key found".to_string())?;

        let decoding_key = jwk_to_decoding_key(key)?;
        let mut validation = Validation::new(header.alg);
        if let Some(ref issuer) = auth.issuer {
            validation.set_issuer(&[issuer.as_str()]);
        }
        if let Some(ref aud) = auth.audience {
            validation.set_audience(&[aud.as_str()]);
        }
        validation.validate_exp = true;

        let data = jsonwebtoken::decode::<serde_json::Value>(token, &decoding_key, &validation)
            .map_err(|e| format!("JWT validation failed: {}", e))?;
        Ok(data.claims)
    } else {
        Err("No jwt_secret or jwks_url configured".into())
    }
}

async fn fetch_jwks(
    url: &str,
    breaker: Option<&CircuitBreaker>,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let body = match breaker {
        Some(cb) => cb
            .call(|| async { client.get(url).send().await })
            .await
            .map_err(|e| e.to_string())?,
        None => client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("JWKS fetch failed: {}", e))?,
    };
    let body = body.text().await.map_err(|e| format!("JWKS body read failed: {}", e))?;
    serde_json::from_str(&body).map_err(|e| format!("JWKS parse failed: {}", e))
}

fn find_key<'a>(
    jwks: &'a serde_json::Value,
    kid: Option<&str>,
) -> Option<&'a serde_json::Value> {
    let keys = jwks["keys"].as_array()?;
    if let Some(kid) = kid {
        keys.iter().find(|k| k["kid"].as_str() == Some(kid))
    } else {
        keys.first()
    }
}

fn jwk_to_decoding_key(
    jwk: &serde_json::Value,
) -> Result<DecodingKey, String> {
    let kty = jwk["kty"].as_str().unwrap_or("");
    match kty {
        "RSA" => {
            let n = jwk["n"].as_str().ok_or("Missing RSA modulus 'n'")?;
            let e = jwk["e"].as_str().ok_or("Missing RSA exponent 'e'")?;
            Ok(DecodingKey::from_rsa_components(n, e).map_err(|e| format!("RSA key error: {}", e))?)
        }
        "EC" => {
            let x = jwk["x"].as_str().ok_or("Missing EC x")?;
            let y = jwk["y"].as_str().ok_or("Missing EC y")?;
            Ok(DecodingKey::from_ec_components(x, y).map_err(|e| format!("EC key error: {}", e))?)
        }
        "oct" => {
            let k = jwk["k"]
                .as_str()
                .ok_or("Missing symmetric key 'k'")?;
            use base64::Engine;
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(k)
                .map_err(|e| format!("Base64 decode failed: {}", e))?;
            Ok(DecodingKey::from_secret(&bytes))
        }
        _ => Err(format!("Unsupported key type: {}", kty)),
    }
}

// ── Axum Router Builder ────────────────────────────────────────

pub fn build_mcp_router(
    config: Arc<Config>,
    pool: Pool<Postgres>,
) -> Option<axum::Router> {
    let mcp_cfg = config.mcp.as_ref()?;
    if !mcp_cfg.enabled {
        return None;
    }

    let es_client = config
        .elasticsearch
        .as_ref()
        .map(|_| reqwest::Client::new());
    let es_url = config.elasticsearch.as_ref().map(|c| c.url.clone());
    let es_circuit_breaker = config
        .elasticsearch
        .as_ref()
        .map(|_| CircuitBreaker::new(config.circuit_breakers.es.to_circuit_breaker_config()));
    let app_context = Arc::new(AppContext {
        pool,
        es_client,
        es_url,
        es_circuit_breaker,
        permission_cache: Arc::new(tokio::sync::Mutex::new(
            crate::config::PermissionCache::new(),
        )),
    });

    let auth_config = mcp_cfg
        .auth
        .as_ref()
        .filter(|a| a.enabled)
        .cloned()
        .map(Arc::new);

    let jwks_circuit_breaker = auth_config
        .as_ref()
        .and_then(|a| a.jwks_url.as_ref())
        .map(|_| CircuitBreaker::new(config.circuit_breakers.jwks.to_circuit_breaker_config()));

    let mcp_state = MCPState {
        auth_config,
        app_context: app_context.clone(),
        config: config.clone(),
        jwks_circuit_breaker,
    };

    let service = StreamableHttpService::new(
        {
            let config = config.clone();
            let app_context = app_context.clone();
            move || Ok(MorphisMCPServer::new(config.clone(), app_context.clone()))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_stateful_mode(true)
            .with_sse_keep_alive(Some(Duration::from_secs(15)))
            .with_allowed_hosts(["localhost", "127.0.0.1", "0.0.0.0"]),
    );

    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(mcp_state, mcp_auth_middleware));

    tracing::info!("MCP server enabled at /mcp (Streamable HTTP)");

    Some(router)
}

// ── Filter Parsing ──────────────────────────────────────────────

// ── GraphQL introspection helpers ────────────────────────────────

fn extract_type_name(field: &serde_json::Value) -> String {
    let t = &field["type"];
    let kind = t["kind"].as_str().unwrap_or("");
    if kind == "NON_NULL" {
        if let Some(of) = t["ofType"].as_object() {
            let inner_kind = of.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            if inner_kind == "LIST" {
                let inner_name = resolve_named_type(&t["ofType"]["ofType"]);
                format!("[{}]!", inner_name)
            } else {
                let name = of.get("name").and_then(|n| n.as_str()).unwrap_or("");
                format!("{}!", name)
            }
        } else {
            "unknown".to_string()
        }
    } else if kind == "LIST" {
        let inner_name = resolve_named_type(&t["ofType"]);
        format!("[{}]", inner_name)
    } else {
        t.get("name").and_then(|n| n.as_str()).unwrap_or("unknown").to_string()
    }
}

fn resolve_named_type(t: &serde_json::Value) -> String {
    if let Some(name) = t["name"].as_str() {
        if !name.is_empty() { return name.to_string(); }
    }
    if let Some(of) = t["ofType"].as_object() {
        return resolve_named_type(&serde_json::Value::Object(of.clone()));
    }
    "unknown".to_string()
}

