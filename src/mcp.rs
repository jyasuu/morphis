use std::sync::Arc;
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

use crate::config::{ColumnType, Config, MCPAuthConfig, RowFilterConfig};
use crate::schema::apply_row_filters;
use crate::schema::AppContext;
use crate::schema::Identity;
use crate::schema::db;

tokio::task_local! {
    pub static MCP_IDENTITY: Identity;
}

// ── Shared state for auth middleware ────────────────────────────

#[derive(Clone)]
pub struct MCPState {
    pub auth_config: Option<Arc<MCPAuthConfig>>,
    #[allow(dead_code)]
    pub app_context: Arc<AppContext>,
    #[allow(dead_code)]
    pub config: Arc<Config>,
}

// ── MCP Server ─────────────────────────────────────────────────

pub struct MorphisMCPServer {
    config: Arc<Config>,
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

    fn identity(&self) -> Identity {
        MCP_IDENTITY.try_with(|id| id.clone()).unwrap_or_default()
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

    /// Discover all available tables with their schemas, prompts, and search indexes.
    #[tool(description = "Discover available tables, their columns, types, prompts, and search indexes. Always call this first to understand what data is available.")]
    async fn discover_tables(&self) -> Result<CallToolResult, McpError> {
        let mut tables = serde_json::Map::new();
        for name in self.config.tables.keys() {
            if let Some(info) = self.col_info(name) {
                let mut obj = serde_json::Map::new();
                obj.insert("db_table".into(), serde_json::json!(info.db_table));
                obj.insert("prompt".into(), serde_json::json!(info.prompt));
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

    /// Query records from a table using structured filters.
    #[tool(description = "Query records from a table with optional filters, ordering, and pagination. Use discover_tables first to see available tables and columns.")]
    async fn query(
        &self,
        Parameters(args): Parameters<QueryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let table_name = &args.table;
        let table_cfg = self.config.tables.get(table_name).ok_or_else(|| {
            McpError::invalid_params(
                format!("Table '{}' not found. Use discover_tables to see available tables.", table_name),
                None::<serde_json::Value>,
            )
        })?;

        let col_names: Vec<String> = table_cfg.columns.iter().map(|c| c.name.clone()).collect();
        let row_filters = table_cfg.row_filters.clone();

        let mut sql = format!(
            "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT * FROM {}",
            table_cfg.table
        );
        let mut params: Vec<String> = Vec::new();

        if let Some(ref filters) = args.filters {
            if let Some(obj) = filters.as_object() {
                let mut clauses = Vec::new();
                build_filter_clauses(obj, &col_names, &mut clauses, &mut params);
                if !clauses.is_empty() {
                    sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
                }
            }
        }

        let identity = self.identity();
        apply_row_filters(&mut sql, &mut params, &identity, &row_filters);

        if let Some(ref order) = args.order_by {
            let sanitized: Vec<String> = order
                .split(',')
                .filter_map(|seg| {
                    let seg = seg.trim().to_string();
                    if seg.is_empty() {
                        return None;
                    }
                    let parts: Vec<&str> = seg.split_whitespace().collect();
                    match parts.as_slice() {
                        [col] if col_names.contains(&col.to_string()) => Some(seg),
                        [col, dir]
                            if col_names.contains(&col.to_string())
                                && matches!(*dir, "ASC" | "DESC" | "asc" | "desc") =>
                        {
                            Some(seg)
                        }
                        _ => None,
                    }
                })
                .collect();
            if !sanitized.is_empty() {
                sql.push_str(&format!(" ORDER BY {}", sanitized.join(", ")));
            }
        }

        let limit = args.limit.unwrap_or(50).min(1000);
        sql.push_str(&format!(" LIMIT {}", limit));

        if let Some(offset) = args.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }
        sql.push_str(") t");

        let rows = db::fetch_many(&self.app_context.pool, &sql, &params)
            .await
            .map_err(|e| {
                McpError::internal_error(
                    format!("Query failed: {:?}", e),
                    None::<serde_json::Value>,
                )
            })?;

        let result = serde_json::json!({
            "table": table_name,
            "count": rows.len(),
            "rows": rows,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(
                    format!("Failed to serialize response: {}", e),
                    None::<serde_json::Value>,
                ))?
        )]))
    }

    /// Get a single record by its primary key.
    #[tool(description = "Get a single record by its primary key value.")]
    async fn get(
        &self,
        Parameters(args): Parameters<GetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let table_name = &args.table;
        let table_cfg = self.config.tables.get(table_name).ok_or_else(|| {
            McpError::invalid_params(
                format!("Table '{}' not found.", table_name),
                None::<serde_json::Value>,
            )
        })?;

        let pks = &table_cfg.primary_key;
        let mut where_clauses = Vec::new();
        let mut params = Vec::new();

        if pks.len() == 1 {
            let pk = &pks[0];
            let is_int = table_cfg
                .columns
                .iter()
                .any(|c| c.name == *pk && matches!(c.col_type, ColumnType::Int | ColumnType::Int64));
            let val = pk_value_to_string(&args.id, is_int)?;
            let cast = if is_int { "::int" } else { "" };
            where_clauses.push(format!("{} = ${}{}", pk, 1, cast));
            params.push(val);
        } else {
            let vals = match &args.id {
                serde_json::Value::Array(arr) => arr.clone(),
                _ => {
                    return Err(McpError::invalid_params(
                        "Composite primary key requires an array of values",
                        None::<serde_json::Value>,
                    ))
                }
            };
            if vals.len() != pks.len() {
                return Err(McpError::invalid_params(
                    format!(
                        "Expected {} values for primary key, got {}",
                        pks.len(),
                        vals.len()
                    ),
                    None::<serde_json::Value>,
                ));
            }
            for (i, (pk, val)) in pks.iter().zip(vals.iter()).enumerate() {
                let is_int = table_cfg.columns.iter().any(|c| {
                    c.name == *pk && matches!(c.col_type, ColumnType::Int | ColumnType::Int64)
                });
                let val_str = pk_value_to_string(val, is_int)?;
                let cast = if is_int { "::int" } else { "" };
                where_clauses.push(format!("{} = ${}{}", pk, i + 1, cast));
                params.push(val_str);
            }
        }

        let mut sql = format!(
            "SELECT row_to_json(t)::text FROM (SELECT * FROM {} WHERE {}",
            table_cfg.table,
            where_clauses.join(" AND ")
        );
        let identity = self.identity();
        apply_row_filters(&mut sql, &mut params, &identity, &table_cfg.row_filters);
        sql.push_str(" LIMIT 1) t");

        match db::fetch_one(&self.app_context.pool, &sql, &params)
            .await
            .map_err(|e| {
                McpError::internal_error(
                    format!("Query failed: {:?}", e),
                    None::<serde_json::Value>,
                )
            })? {
            Some(row) => {
                let result = serde_json::json!({ "row": row });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .map_err(|e| McpError::internal_error(
                            format!("Failed to serialize response: {}", e),
                            None::<serde_json::Value>,
                        ))?
                )]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                serde_json::json!({ "message": "Record not found" }).to_string(),
            )])),
        }
    }

    /// Full-text search across a search index.
    #[tool(description = "Full-text search across indexed fields using Elasticsearch. Use discover_tables to see available search indexes.")]
    async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let index_cfg = self
            .config
            .search_indexes
            .iter()
            .find(|si| si.name == args.index)
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!(
                        "Search index '{}' not found. Use discover_tables to see available indexes.",
                        args.index
                    ),
                    None::<serde_json::Value>,
                )
            })?;

        let all_searchable = collect_searchable_fields(index_cfg);
        let identity = self.identity();

        let row_filters = self
            .config
            .tables
            .get(&index_cfg.graphql_type)
            .map(|t| t.row_filters.clone())
            .unwrap_or_default();

        let mut must_clauses: Vec<serde_json::Value> = Vec::new();
        let filter_clauses = build_es_row_filters_mcp(
            &self.app_context,
            &identity,
            &row_filters,
        )
        .await
        .map_err(|e| {
            McpError::internal_error(e.to_string(), None::<serde_json::Value>)
        })?;
        must_clauses.extend(filter_clauses);

        let mut bool_body = serde_json::json!({
            "must": must_clauses
        });
        if !args.query.is_empty() {
            bool_body["should"] = serde_json::json!([{
                "multi_match": {
                    "query": args.query,
                    "fields": all_searchable,
                    "type": "cross_fields"
                }
            }]);
            bool_body["minimum_should_match"] = serde_json::json!(1);
        }

        let limit = args.limit.unwrap_or(50).min(1000) as usize;
        let offset = args.offset.unwrap_or(0) as usize;

        let mut es_query = serde_json::json!({
            "query": { "bool": bool_body },
            "size": limit,
        });
        if offset > 0 {
            es_query["from"] = serde_json::json!(offset);
        }

        let (es_client, es_url) = match (&self.app_context.es_client, &self.app_context.es_url) {
            (Some(c), Some(u)) => (c.clone(), u.clone()),
            _ => {
                return Err(McpError::internal_error(
                    "Elasticsearch is not configured",
                    None::<serde_json::Value>,
                ))
            }
        };

        let url = format!(
            "{}/{}/_search",
            es_url.trim_end_matches('/'),
            index_cfg.index
        );

        let resp = es_client
            .post(&url)
            .json(&es_query)
            .send()
            .await
            .map_err(|e| {
                McpError::internal_error(
                    format!("ES request failed: {}", e),
                    None::<serde_json::Value>,
                )
            })?;

        let body: serde_json::Value = resp.json().await.map_err(|e| {
            McpError::internal_error(
                format!("ES parse failed: {}", e),
                None::<serde_json::Value>,
            )
        })?;

        let hits = body["hits"]["hits"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let mut results = Vec::new();
        for hit in hits {
            let source = hit
                .get("_source")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let enriched =
                es_enrich_source(source, index_cfg, &self.app_context.pool).await;
            results.push(enriched);
        }

        let output = serde_json::json!({
            "index": args.index,
            "query": args.query,
            "total": body["hits"]["total"]["value"].as_i64().unwrap_or(0),
            "results": results,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(
                    format!("Failed to serialize response: {}", e),
                    None::<serde_json::Value>,
                ))?
        )]))
    }

    /// Find parent records based on conditions in a related table.
    /// E.g. find materials with feature "Water Resistant" by querying table="materials", related="material_features".
    #[tool(description = "Find parent records based on filters applied to a related (child/joined) table. Uses the relation name from discover_tables. Example: query_by_related(table='materials', related='material_features', filters={'feature_name': 'Water Resistant'}) to find all materials with that feature.")]
    async fn query_by_related(
        &self,
        Parameters(args): Parameters<QueryByRelatedArgs>,
    ) -> Result<CallToolResult, McpError> {
        let table_name = &args.table;
        let table_cfg = self.config.tables.get(table_name).ok_or_else(|| {
            McpError::invalid_params(
                format!("Table '{}' not found.", table_name),
                None::<serde_json::Value>,
            )
        })?;

        let rel = table_cfg.relations.iter().find(|r| r.name == args.related).ok_or_else(|| {
            McpError::invalid_params(
                format!("Relation '{}' not found on table '{}'. Available: {}",
                    args.related, table_name,
                    table_cfg.relations.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", ")),
                None::<serde_json::Value>,
            )
        })?;

        let related_cfg = self.config.tables.get(&rel.table).ok_or_else(|| {
            McpError::internal_error(
                format!("Related table '{}' not found in config", rel.table),
                None::<serde_json::Value>,
            )
        })?;
        let related_col_names: Vec<String> = related_cfg.columns.iter().map(|c| c.name.clone()).collect();
        let rel_pk = table_cfg.primary_key.first().ok_or_else(|| {
            McpError::internal_error(
                format!("Table '{}' has no primary key defined", table_name),
                None::<serde_json::Value>,
            )
        })?;

        let mut sub_params: Vec<String> = Vec::new();
        let mut sub_where = String::new();
        if let Some(ref filters) = args.filters {
            if let Some(obj) = filters.as_object() {
                let mut clauses = Vec::new();
                build_filter_clauses(obj, &related_col_names, &mut clauses, &mut sub_params);
                if !clauses.is_empty() {
                    sub_where = format!(" WHERE {}", clauses.join(" AND "));
                }
            }
        }

        let identity = self.identity();
        let related_row_filters = related_cfg.row_filters.clone();
        let mut rel_sql = format!(
            "SELECT {} FROM {}",
            rel.foreign_field, rel.table
        );
        rel_sql.push_str(&sub_where);
        apply_row_filters(&mut rel_sql, &mut sub_params, &identity, &related_row_filters);

        let limit = args.limit.unwrap_or(50).min(1000);
        let offset = args.offset.unwrap_or(0);

        let row_filters = table_cfg.row_filters.clone();
        let mut sql = format!(
            "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT * FROM {} WHERE {} IN ({}",
            table_cfg.table, rel_pk, rel_sql,
        );
        let mut params: Vec<String> = sub_params;

        sql.push_str(")");
        apply_row_filters(&mut sql, &mut params, &identity, &row_filters);
        sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));
        sql.push_str(") t");

        let rows = db::fetch_many(&self.app_context.pool, &sql, &params)
            .await
            .map_err(|e| {
                McpError::internal_error(
                    format!("Query by related failed: {:?}", e),
                    None::<serde_json::Value>,
                )
            })?;

        let result = serde_json::json!({
            "table": table_name,
            "related": args.related,
            "count": rows.len(),
            "rows": rows,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(
                    format!("Failed to serialize response: {}", e),
                    None::<serde_json::Value>,
                ))?
        )]))
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
                 query to fetch records with filters, get to retrieve by primary key, \
                 and search for full-text search."
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
pub struct QueryArgs {
    /// Name of the table to query (use discover_tables to see available tables)
    pub table: String,
    /// Optional filters as key-value pairs.
    /// Supports operators: __gt, __gte, __lt, __lte, __ne, __contains, __startswith, __endswith
    /// Nested OR conditions: {"OR": [{"status": "active"}, {"status": "draft"}]}
    pub filters: Option<serde_json::Value>,
    /// Optional ordering, e.g. "name ASC", "created_at DESC"
    pub order_by: Option<String>,
    /// Maximum number of records (default: 50, max: 1000)
    pub limit: Option<i32>,
    /// Number of records to skip (default: 0)
    pub offset: Option<i32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetArgs {
    /// Name of the table
    pub table: String,
    /// Primary key value (single value for single-column PK, array for composite PK)
    pub id: serde_json::Value,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Name of the search index (use discover_tables to see available indexes)
    pub index: String,
    /// Search query text for full-text search
    pub query: String,
    /// Maximum number of results (default: 50, max: 1000)
    pub limit: Option<i32>,
    /// Number of results to skip (default: 0)
    pub offset: Option<i32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QueryByRelatedArgs {
    /// Name of the parent table to query (e.g. "materials")
    pub table: String,
    /// Name of the relation to join through (e.g. "material_features" for "materials with feature X")
    pub related: String,
    /// Optional filters on the related table as key-value pairs.
    /// Supports operators: __gt, __gte, __lt, __lte, __ne, __contains, __startswith, __endswith
    pub filters: Option<serde_json::Value>,
    /// Maximum number of parent records (default: 50, max: 1000)
    pub limit: Option<i32>,
    /// Number of records to skip (default: 0)
    pub offset: Option<i32>,
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
                    match validate_jwt(&token, auth_cfg).await {
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
                            Identity::default()
                        }
                    }
                }
                None => {
                    tracing::debug!("MCP request without Bearer token (will use empty identity)");
                    Identity::default()
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
        let jwks = fetch_jwks(jwks_url).await?;
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

async fn fetch_jwks(url: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let body = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("JWKS fetch failed: {}", e))?
        .text()
        .await
        .map_err(|e| format!("JWKS body read failed: {}", e))?;
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
    let app_context = Arc::new(AppContext {
        pool,
        es_client,
        es_url,
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

    let mcp_state = MCPState {
        auth_config,
        app_context: app_context.clone(),
        config: config.clone(),
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

fn build_filter_clauses(
    obj: &serde_json::Map<String, serde_json::Value>,
    allowed: &[String],
    clauses: &mut Vec<String>,
    params: &mut Vec<String>,
) {
    for (key, val) in obj {
        if val.is_null() {
            continue;
        }

        // Handle OR conditions
        if key == "OR" {
            if let Some(arr) = val.as_array() {
                let mut or_clauses = Vec::new();
                for item in arr {
                    if let Some(sub_obj) = item.as_object() {
                        let mut sub_clauses = Vec::new();
                        build_filter_clauses(sub_obj, allowed, &mut sub_clauses, params);
                        if !sub_clauses.is_empty() {
                            or_clauses.push(format!("({})", sub_clauses.join(" AND ")));
                        }
                    }
                }
                if !or_clauses.is_empty() {
                    clauses.push(or_clauses.join(" OR "));
                }
            }
            continue;
        }

        // Extract field name and operator
        let (field, op) = if let Some(idx) = key.rfind("__") {
            let field_name = &key[..idx];
            let operator = &key[idx + 2..];
            // Check if operator is valid
            match operator {
                "gt" | "gte" | "lt" | "lte" | "ne" | "contains" | "startswith" | "endswith" => {
                    (field_name.to_string(), operator)
                }
                _ => continue,
            }
        } else {
            (key.clone(), "eq")
        };

        // Validate field is allowed
        if !allowed.contains(&field) {
            continue;
        }

        let val_str = json_val_to_string(val);
        let param_idx = params.len() + 1;

        match op {
            "eq" => {
                clauses.push(format!("{} = ${}", field, param_idx));
                params.push(val_str);
            }
            "ne" => {
                clauses.push(format!("{} <> ${}", field, param_idx));
                params.push(val_str);
            }
            "gt" => {
                clauses.push(format!("{} > ${}", field, param_idx));
                params.push(val_str);
            }
            "gte" => {
                clauses.push(format!("{} >= ${}", field, param_idx));
                params.push(val_str);
            }
            "lt" => {
                clauses.push(format!("{} < ${}", field, param_idx));
                params.push(val_str);
            }
            "lte" => {
                clauses.push(format!("{} <= ${}", field, param_idx));
                params.push(val_str);
            }
            "contains" => {
                clauses.push(format!("{} LIKE ${}", field, param_idx));
                params.push(format!("%{}%", val_str));
            }
            "startswith" => {
                clauses.push(format!("{} LIKE ${}", field, param_idx));
                params.push(format!("{}%", val_str));
            }
            "endswith" => {
                clauses.push(format!("{} LIKE ${}", field, param_idx));
                params.push(format!("%{}", val_str));
            }
            _ => {}
        }
    }
}

fn json_val_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => v.to_string(),
    }
}

fn pk_value_to_string(v: &serde_json::Value, is_int: bool) -> Result<String, McpError> {
    if is_int {
        v.as_i64()
            .map(|n| n.to_string())
            .or_else(|| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!("Expected integer primary key value, got: {}", v),
                    None::<serde_json::Value>,
                )
            })
    } else {
        v.as_str()
            .map(|s| s.to_string())
            .or_else(|| v.as_i64().map(|n| n.to_string()))
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!("Expected string primary key value, got: {}", v),
                    None::<serde_json::Value>,
                )
            })
    }
}

// ── ES helpers (adapted from schema/search.rs) ──────────────────

fn collect_searchable_fields(cfg: &crate::config::SearchIndexConfig) -> Vec<String> {
    let mut fields = cfg.searchable_fields.clone();
    for jf in &cfg.join_fields {
        for f in &jf.searchable_fields {
            fields.push(format!("{}.{}", jf.index_field, f));
        }
        for nested in &jf.join_fields {
            for f in &nested.searchable_fields {
                fields.push(format!("{}.{}.{}", jf.index_field, nested.index_field, f));
            }
        }
    }
    fields
}

async fn build_es_row_filters_mcp(
    app_ctx: &AppContext,
    identity: &Identity,
    row_filters: &[RowFilterConfig],
) -> Result<Vec<serde_json::Value>, String> {
    let mut clauses = Vec::new();
    for rf in row_filters {
        let Some(val) = identity.header_value(rf.header_name()) else {
            continue;
        };
        match rf {
            RowFilterConfig::ColumnFilter { column, .. } => {
                clauses.push(serde_json::json!({
                    "term": { column: val }
                }));
            }
            RowFilterConfig::SubqueryFilter {
                columns,
                match_columns,
                from_source,
                user_column,
                cache_ttl_secs,
                ..
            } => {
                let cache_key = format!("{}:{}:{}", from_source, user_column, val);
                let ttl = std::time::Duration::from_secs(cache_ttl_secs.unwrap_or(60));
                let cols: Vec<String> = match_columns.clone();
                let rows = {
                    let cached = {
                        let mut cache = app_ctx.permission_cache.lock().await;
                        cache.get(&cache_key)
                    };
                    if let Some(cached) = cached {
                        cached
                    } else {
                        let sql = format!(
                            "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT DISTINCT {} FROM {} WHERE {} = $1) t",
                            cols.join(", "),
                            from_source,
                            user_column,
                        );
                        let result = db::fetch_joined_rows(&app_ctx.pool, &sql, val).await.map_err(|e| format!("{:?}", e))?;
                        let mut cache = app_ctx.permission_cache.lock().await;
                        cache.set(cache_key, result.clone(), ttl);
                        result
                    }
                };
                if rows.is_empty() {
                    let false_clause = serde_json::json!({
                        "term": { "__no_match": "__impossible" }
                    });
                    clauses.push(false_clause);
                } else {
                    let mut should = Vec::new();
                    for row in &rows {
                        let mut must = Vec::new();
                        if let Some(obj) = row.as_object() {
                            for (col_idx, col) in columns.iter().enumerate() {
                                if let Some(mcol) = match_columns.get(col_idx)
                                    && let Some(v) = obj.get(mcol.as_str())
                                {
                                    must.push(serde_json::json!({ "term": { col: v } }));
                                }
                            }
                        }
                        if !must.is_empty() {
                            should.push(serde_json::json!({ "bool": { "must": must } }));
                        }
                    }
                    if !should.is_empty() {
                        clauses.push(serde_json::json!({
                            "bool": { "should": should, "minimum_should_match": 1 }
                        }));
                    }
                }
            }
        }
    }
    Ok(clauses)
}

async fn es_enrich_nested(
    source: serde_json::Value,
    join_fields: &[crate::config::SearchJoinConfig],
    pool: &Pool<Postgres>,
) -> serde_json::Value {
    if join_fields.is_empty() {
        return source;
    }
    let mut enriched = match source {
        serde_json::Value::Object(m) => m,
        other => return other,
    };
    for jf in join_fields {
        let local_val = enriched
            .get(&jf.local_field)
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_default();
        if local_val.is_empty() {
            continue;
        }
        let sql = format!(
            "SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM (SELECT * FROM {} WHERE {} = $1) t",
            jf.table, jf.foreign_field
        );
        let rows = db::fetch_joined_rows(pool, &sql, &local_val)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("ES enrichment query failed: {:?}", e);
                vec![]
            });
        let enriched_rows: Vec<serde_json::Value> =
            futures::future::join_all(rows.into_iter().map(|item| {
                let jf_clone = jf.clone();
                async move { es_enrich_nested(item, &jf_clone.join_fields, pool).await }
            }))
            .await;
        enriched.insert(
            jf.index_field.clone(),
            serde_json::Value::Array(enriched_rows),
        );
    }
    serde_json::Value::Object(enriched)
}

async fn es_enrich_source(
    source: serde_json::Value,
    cfg: &crate::config::SearchIndexConfig,
    pool: &Pool<Postgres>,
) -> serde_json::Value {
    es_enrich_nested(source, &cfg.join_fields, pool).await
}
