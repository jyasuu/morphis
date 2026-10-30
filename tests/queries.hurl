# Single query by string PK
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materials(id: \"M001\") { mat_no name status } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials.mat_no" == "M001"
jsonpath "$.data.materials.name" == "Premium Cotton Canvas"
jsonpath "$.data.materials.status" == "active"

# Single query by int PK
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ sizes(id: 1) { id size_code mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.sizes.id" == 1
jsonpath "$.data.sizes.size_code" == "S"
jsonpath "$.data.sizes.mat_no" == "M001"

# List query
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList[0].mat_no" == "M001"
jsonpath "$.data.materialsList[1].mat_no" == "M002"
jsonpath "$.data.materialsList[2].mat_no" == "M003"
jsonpath "$.data.materialsList[0].name" == "Premium Cotton Canvas"

# List with filter
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(filter: { status: \"discontinued\" }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList[0].mat_no" == "M003"
jsonpath "$.data.materialsList[0].name" == "Recycled Polyester"
jsonpath "$.data.materialsList" count == 1

# Colorways with nullable hex
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ colorways(id: 1) { id colorway_code name hex } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.colorways.hex" == "#FFFFFF"
jsonpath "$.data.colorways.colorway_code" == "WH"

# Material features list
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ material_featuresList { id feature_name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.material_featuresList[0].feature_name" == "Construction"
jsonpath "$.data.material_featuresList" count == 8

# Feature attributes
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ feature_attributesList { id attr_name attr_value } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.feature_attributesList[0].attr_name" == "weave_type"
jsonpath "$.data.feature_attributesList[0].attr_value" == "plain"

# Numeric filter on int column
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ feature_attributesList(filter: { feature_id: 1 }) { id attr_name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.feature_attributesList" count == 2
jsonpath "$.data.feature_attributesList[0].attr_name" == "weave_type"
jsonpath "$.data.feature_attributesList[1].attr_name" == "thread_count"

# Numeric filter on auto-increment int PK
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ sizesList(filter: { id: 1 }) { size_code } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.sizesList" count == 1
jsonpath "$.data.sizesList[0].size_code" == "S"

# List query with limit
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(limit: 2) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList" count == 2

# List query with limit + offset (page 2)
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(limit: 2, offset: 2) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList" count == 1
jsonpath "$.data.materialsList[0].mat_no" == "M003"

# Offset beyond total returns empty
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(limit: 10, offset: 100) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList" count == 0

# Nullable field: feature with null description
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ material_features(id: 5) { feature_name description } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.material_features.feature_name" == "Certification"
jsonpath "$.data.material_features.description" == null

# GraphQL introspection
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ __schema { queryType { name } types { name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.__schema.queryType.name" == "Query"

# ── where operators (SQL) ─────────────────────────────────────

# in + case-insensitive contains, combined with AND
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(where: { mat_no: { in: [\"M001\", \"M002\", \"M003\"] }, name: { contains: \"WOOL\" } }) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList" count == 1
jsonpath "$.data.materialsList[0].mat_no" == "M002"

# range comparison on a string column
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(where: { mat_no: { gte: \"M002\", lt: \"M003\" } }) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList" count == 1
jsonpath "$.data.materialsList[0].mat_no" == "M002"

# ne + the legacy equality filter still compose
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(filter: { status: \"active\" }, where: { mat_no: { ne: \"M001\" } }) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList" count == 1
jsonpath "$.data.materialsList[0].mat_no" == "M002"

# LIKE metacharacters are matched literally ("%" must not match everything)
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(where: { name: { contains: \"%\" } }) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materialsList" count == 0

# integer column comparison
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ sizesList(where: { id: { gte: 1 } }, limit: 2) { id } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.sizesList" count == 2

# empty `in` is a clear error, not silent no-op
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(where: { name: { in: [] } }) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.errors[0].message" contains "at least one value"

# ── request guardrails ────────────────────────────────────────

# negative limit is rejected
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(limit: -1) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.errors[0].message" contains "non-negative"

# an oversized limit is clamped, not an error
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materialsList(limit: 1000000) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.errors" not exists
jsonpath "$.data.materialsList" count >= 3

# nesting beyond limits.max_depth (default 10) is rejected before any SQL runs
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materials(id: \"M001\") { material_features { feature_attributes { material_feature { material { material_features { feature_attributes { material_feature { material { material_features { name } } } } } } } } } } }"
}
HTTP 200
[Asserts]
jsonpath "$.errors[0].message" contains "nested too deep"

# introspection is exempt from the depth limit (full standard shape, 7-level ofType)
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ __schema { types { name fields { name type { kind ofType { kind ofType { kind ofType { kind ofType { kind ofType { kind ofType { kind name } } } } } } } } } } }"
}
HTTP 200
[Asserts]
jsonpath "$.errors" not exists
jsonpath "$.data.__schema.types" count > 10
