# Row-level security integration tests
# Prerequisites:
#   materials table must have a tenant_id column (nullable text)

# Create material with X-Tenant-ID — auto_set: true should set tenant_id
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
{
  "query": "mutation { createMaterials(input: { mat_no: \"RLTEST1\", name: \"Row Filter Test Alpha\", status: \"active\" }) { mat_no name tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createMaterials.mat_no" == "RLTEST1"
jsonpath "$.data.createMaterials.tenant_id" == "tenant-alpha"

# Create another with different tenant
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-beta
{
  "query": "mutation { createMaterials(input: { mat_no: \"RLTEST2\", name: \"Row Filter Test Beta\", status: \"active\" }) { mat_no name tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createMaterials.mat_no" == "RLTEST2"
jsonpath "$.data.createMaterials.tenant_id" == "tenant-beta"

# List with tenant-alpha — should see only alpha's material
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
{
  "query": "{ materialsList { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList[0].mat_no" == "RLTEST1"
jsonpath "$.data.materialsList[0].tenant_id" == "tenant-alpha"
jsonpath "$.data.materialsList" count == 1

# List with tenant-beta — should see only beta's material
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-beta
{
  "query": "{ materialsList { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList[0].mat_no" == "RLTEST2"
jsonpath "$.data.materialsList[0].tenant_id" == "tenant-beta"
jsonpath "$.data.materialsList" count == 1

# Single query with matching tenant
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
{
  "query": "{ materials(id: \"RLTEST1\") { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials.mat_no" == "RLTEST1"
jsonpath "$.data.materials.tenant_id" == "tenant-alpha"

# Single query with non-matching tenant — should be filtered out
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-beta
{
  "query": "{ materials(id: \"RLTEST1\") { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials" == null

# List without headers — no filters applied, all rows visible
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList[*].mat_no" contains "RLTEST1"
jsonpath "$.data.materialsList[*].mat_no" contains "RLTEST2"

# === Subquery row filter tests ===
# materials table also has a subquery filter:
# (tenant_id) IN (SELECT tenant_id FROM materials WHERE tenant_id = $N)
# Both filters must pass (AND).

# Both X-Tenant-ID and X-User-ID match — should pass both filters
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
X-User-ID: tenant-alpha
{
  "query": "{ materialsList { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList[0].mat_no" == "RLTEST1"
jsonpath "$.data.materialsList" count == 1

# Column filter matches but subquery doesn't — should be empty
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
X-User-ID: tenant-beta
{
  "query": "{ materialsList { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList" count == 0

# Both headers match beta's material
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-beta
X-User-ID: tenant-beta
{
  "query": "{ materialsList { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList[0].mat_no" == "RLTEST2"
jsonpath "$.data.materialsList" count == 1

# Single query with both headers matching
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
X-User-ID: tenant-alpha
{
  "query": "{ materials(id: \"RLTEST1\") { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials.mat_no" == "RLTEST1"

# Single query: column passes, subquery fails
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
X-User-ID: tenant-beta
{
  "query": "{ materials(id: \"RLTEST1\") { mat_no tenant_id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials" == null

# Update with both headers matching
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
X-User-ID: tenant-alpha
{
  "query": "mutation { updateMaterials(id: \"RLTEST1\", input: { name: \"Alpha via Both Filters\" }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.updateMaterials.mat_no" == "RLTEST1"
jsonpath "$.data.updateMaterials.name" == "Alpha via Both Filters"

# Update: column passes, subquery fails
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
X-User-ID: tenant-beta
{
  "query": "mutation { updateMaterials(id: \"RLTEST1\", input: { name: \"Should Not Work\" }) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data" == null
jsonpath "$.errors" count > 0

# Update with matching tenant
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
{
  "query": "mutation { updateMaterials(id: \"RLTEST1\", input: { name: \"Alpha via Column Filter\" }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.updateMaterials.mat_no" == "RLTEST1"
jsonpath "$.data.updateMaterials.name" == "Alpha via Column Filter"

# Update with wrong tenant — should fail (no row matching PK + filter)
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-beta
{
  "query": "mutation { updateMaterials(id: \"RLTEST1\", input: { name: \"Should Not Work\" }) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data" == null
jsonpath "$.errors" count > 0

# Delete with wrong tenant — should fail
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-beta
{
  "query": "mutation { deleteMaterials(id: \"RLTEST1\") { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data" == null
jsonpath "$.errors" count > 0

# === ES row filter tests ===
# Note: ES uses dynamic mapping; term queries on text fields require
# single-token values (hyphens are split by the standard analyzer).
# These docs are created directly in ES (no PG sync needed).

# Create ES doc with tenant_id "alpha"
POST http://localhost:9200/materials/_doc/ES-TEST-A
Content-Type: application/json
{
  "mat_no": "ES-TEST-A",
  "name": "ES Row Filter Alpha",
  "status": "active",
  "tenant_id": "alpha"
}
HTTP 201

# Create ES doc with tenant_id "beta"
POST http://localhost:9200/materials/_doc/ES-TEST-B
Content-Type: application/json
{
  "mat_no": "ES-TEST-B",
  "name": "ES Row Filter Beta",
  "status": "active",
  "tenant_id": "beta"
}
HTTP 201

# Refresh ES index so docs are searchable
POST http://localhost:9200/materials/_refresh
HTTP 200

# Search with X-Tenant-ID: alpha — only ES-TEST-A
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: alpha
{
  "query": "{ searchMaterials(query: \"ES Row Filter\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "ES-TEST-A"

# Search with X-Tenant-ID: beta — only ES-TEST-B
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: beta
{
  "query": "{ searchMaterials(query: \"ES Row Filter\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "ES-TEST-B"

# Search without headers — both ES docs visible
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"ES Row Filter\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2

# Clean up ES test docs
DELETE http://localhost:9200/materials/_doc/ES-TEST-A
HTTP 200
DELETE http://localhost:9200/materials/_doc/ES-TEST-B
HTTP 200

# Clean up PG test data
POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-alpha
{
  "query": "mutation { deleteMaterials(id: \"RLTEST1\") { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteMaterials.mat_no" == "RLTEST1"

POST http://localhost:4000/graphql
Content-Type: application/json
X-Tenant-ID: tenant-beta
{
  "query": "mutation { deleteMaterials(id: \"RLTEST2\") { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteMaterials.mat_no" == "RLTEST2"

# === RBAC via permission lookup table ===
# protected_data uses (region) IN (SELECT region FROM user_permissions WHERE user_id = $N)
# Pure RBAC: no tenant_id column on target table, access controlled entirely by user_permissions

# Create permission: user-a can access region "us" and "eu"
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createUserPermissions(input: { user_id: \"user-a\", tenant_id: \"-\", region: \"us\" }) { id user_id region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createUserPermissions.user_id" == "user-a"

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createUserPermissions(input: { user_id: \"user-a\", tenant_id: \"-\", region: \"eu\" }) { id user_id region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createUserPermissions.user_id" == "user-a"

# Create permission: user-b can access region "us" only
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createUserPermissions(input: { user_id: \"user-b\", tenant_id: \"-\", region: \"us\" }) { id user_id region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createUserPermissions.user_id" == "user-b"

# Create protected_data entries (no auto_set — all fields from input)
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createProtectedData(input: { id: \"PDATA-001\", name: \"Protected US\", region: \"us\" }) { id name region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createProtectedData.id" == "PDATA-001"
jsonpath "$.data.createProtectedData.region" == "us"

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createProtectedData(input: { id: \"PDATA-002\", name: \"Protected EU\", region: \"eu\" }) { id name region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createProtectedData.id" == "PDATA-002"
jsonpath "$.data.createProtectedData.region" == "eu"

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createProtectedData(input: { id: \"PDATA-003\", name: \"Protected US 2\", region: \"us\" }) { id name region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createProtectedData.id" == "PDATA-003"
jsonpath "$.data.createProtectedData.region" == "us"

# user-a can see both us and eu records
POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-a
{
  "query": "{ protectedDataList { id name region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.protectedDataList" count == 3
jsonpath "$.data.protectedDataList[*].id" contains "PDATA-001"
jsonpath "$.data.protectedDataList[*].id" contains "PDATA-002"
jsonpath "$.data.protectedDataList[*].id" contains "PDATA-003"

# user-b can only see us records
POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-b
{
  "query": "{ protectedDataList { id name region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.protectedDataList" count == 2
jsonpath "$.data.protectedDataList[*].id" contains "PDATA-001"
jsonpath "$.data.protectedDataList[*].id" contains "PDATA-003"

# user-a can combine client filter with row permissions
POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-a
{
  "query": "{ protectedDataList(filter: { region: \"eu\" }) { id name region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.protectedDataList" count == 1
jsonpath "$.data.protectedDataList[0].id" == "PDATA-002"

# Single query with matching permissions
POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-a
{
  "query": "{ protectedData(id: \"PDATA-001\") { id name region } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.protectedData.id" == "PDATA-001"

# Single query without matching permissions
POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-b
{
  "query": "{ protectedData(id: \"PDATA-002\") { id name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.protectedData" == null

# Update with matching permissions
POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-a
{
  "query": "mutation { updateProtectedData(id: \"PDATA-001\", input: { name: \"Updated US\" }) { id name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.updateProtectedData.id" == "PDATA-001"
jsonpath "$.data.updateProtectedData.name" == "Updated US"

# Update with non-matching permissions
POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-b
{
  "query": "mutation { updateProtectedData(id: \"PDATA-002\", input: { name: \"Should Not Work\" }) { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data" == null
jsonpath "$.errors" count > 0

# Delete with non-matching permissions
POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-b
{
  "query": "mutation { deleteProtectedData(id: \"PDATA-002\") { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data" == null
jsonpath "$.errors" count > 0

# Clean up protected_data
POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-a
{
  "query": "mutation { deleteProtectedData(id: \"PDATA-001\") { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteProtectedData.id" == "PDATA-001"

POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-a
{
  "query": "mutation { deleteProtectedData(id: \"PDATA-002\") { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteProtectedData.id" == "PDATA-002"

POST http://localhost:4000/graphql
Content-Type: application/json
X-User-ID: user-b
{
  "query": "mutation { deleteProtectedData(id: \"PDATA-003\") { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteProtectedData.id" == "PDATA-003"

# Clean up user_permissions
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "mutation { deleteUserPermissions(id: 1) { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteUserPermissions.id" == "1"

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "mutation { deleteUserPermissions(id: 2) { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteUserPermissions.id" == "2"

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "mutation { deleteUserPermissions(id: 3) { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteUserPermissions.id" == "3"
