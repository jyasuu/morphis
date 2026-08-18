# Create material with string PK
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createMaterials(input: { mat_no: \"HTEST\", name: \"Hurl Test\", status: \"active\" }) { mat_no name status } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createMaterials.mat_no" == "HTEST"
jsonpath "$.data.createMaterials.name" == "Hurl Test"
jsonpath "$.data.createMaterials.status" == "active"

# Update material
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { updateMaterials(id: \"HTEST\", input: { name: \"Hurl Updated\" }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.updateMaterials.mat_no" == "HTEST"
jsonpath "$.data.updateMaterials.name" == "Hurl Updated"

# Verify update via query
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ materials(id: \"HTEST\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials.name" == "Hurl Updated"

# Create size with auto-increment int PK
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createSizes(input: { size_code: \"HL\", mat_no: \"M001\", name: \"Hurl Large\" }) { id size_code mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.createSizes.size_code" == "HL"
jsonpath "$.data.createSizes.mat_no" == "M001"
jsonpath "$.data.createSizes.id" > 0

# Update size with int PK
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { updateSizes(id: 1, input: { name: \"Updated Small\" }) { id name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.updateSizes.id" == 1
jsonpath "$.data.updateSizes.name" == "Updated Small"

# Restore size name
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { updateSizes(id: 1, input: { name: \"Small\" }) { id name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.updateSizes.name" == "Small"

# Create colorway with explicit null for a nullable column
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createColorways(input: { colorway_code: \"NT\", mat_no: \"M001\", name: \"Null Test\", hex: null }) { id colorway_code hex } }"
}
HTTP 200
[Captures]
colorway_id: jsonpath "$.data.createColorways.id"
[Asserts]
jsonpath "$.data.createColorways.colorway_code" == "NT"
jsonpath "$.data.createColorways.hex" == null

# Update created colorway: set a value on the nullable column
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { updateColorways(id: {{colorway_id}}, input: { hex: \"#ABCDEF\" }) { hex } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.updateColorways.hex" == "#ABCDEF"

# Update created colorway: clear the nullable column back to null
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { updateColorways(id: {{colorway_id}}, input: { hex: null }) { hex } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.updateColorways.hex" == null

# Cleanup created colorway
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { deleteColorways(id: {{colorway_id}}) { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteColorways.id" == {{colorway_id}}

# Delete material
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { deleteMaterials(id: \"HTEST\") { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.deleteMaterials.mat_no" == "HTEST"

# Verify deletion
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ materials(id: \"HTEST\") { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials" == null

# Error: duplicate PK should fail
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { createMaterials(input: { mat_no: \"M001\", name: \"Duplicate\", status: \"active\" }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data" == null
jsonpath "$.errors" count > 0

# Error: update non-existent ID should return null
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "mutation { updateMaterials(id: \"NONEXISTENT\", input: { name: \"Ghost\" }) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data" == null
