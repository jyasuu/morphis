# Auth-proxy integration test with Keycloak JWT
# Requires: --variable TOKEN=<keycloak-access-token>

# Health endpoint through auth-proxy with valid token
GET http://localhost:9080/health
Authorization: Bearer {{TOKEN}}
HTTP 200

# GraphQL query through auth-proxy with valid token
POST http://localhost:9080/graphql
Authorization: Bearer {{TOKEN}}
Content-Type: application/json
{
  "query": "{ materialsList { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList" isCollection

# Missing Authorization header -> 401
GET http://localhost:9080/health
HTTP 401
