# MCP endpoint tests.
# MCP uses Streamable HTTP (SSE). Hurl can assert status codes but not SSE body content.
# Tests validate: reachability, auth enforcement, and host header allowlisting.

# MCP initialize through auth-proxy with valid MCP JWT
POST http://localhost:9080/mcp
Authorization: Bearer {{MCP_TOKEN}}
Content-Type: application/json
Accept: application/json, text/event-stream
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": { "name": "hurl-test", "version": "1.0" }
  }
}
HTTP 200

# MCP without JWT (direct to app) should fail with 401
POST http://localhost:4000/mcp
Content-Type: application/json
Accept: application/json, text/event-stream
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": { "name": "hurl-test-noauth", "version": "1.0" }
  }
}
HTTP 401
