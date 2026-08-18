POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"cotton\") { score node { mat_no name status } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].score" exists
jsonpath "$.data.searchMaterials[0].score" isFloat
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M001"
jsonpath "$.data.searchMaterials[0].node.name" == "Premium Cotton Canvas"
jsonpath "$.data.searchMaterials[0].node.status" == "active"

POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\") { node { mat_no name status } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 3

# eq operator: filter by single status
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { status: { eq: \"active\" } }) { node { mat_no name status } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2
jsonpath "$.data.searchMaterials[*].node.mat_no" contains "M001"
jsonpath "$.data.searchMaterials[*].node.mat_no" contains "M002"

# ne operator: exclude active materials
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { status: { ne: \"active\" } }) { node { mat_no name status } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M003"
jsonpath "$.data.searchMaterials[0].node.status" == "discontinued"

# in operator: match multiple statuses (OR)
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { status: { in: [\"active\", \"discontinued\"] } }) { node { mat_no name status } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 3

# in operator: match subset of statuses
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { status: { in: [\"discontinued\"] } }) { node { mat_no name status } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M003"

# contains operator: substring match on name
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { name: { contains: \"Cotton\" } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M001"

# starts_with operator
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { name: { starts_with: \"Merino\" } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M002"

# ends_with operator
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { name: { ends_with: \"Polyester\" } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M003"

# Multiple operators on same field: name contains AND status eq
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { name: { contains: \"Wool\" }, status: { eq: \"active\" } }) { node { mat_no name status } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M002"

# Nested filter: eq on feature_name
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { material_features: { feature_name: { eq: \"Eco\" } } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M003"

# Nested filter: in on feature_name (OR) — Construction + Eco
# M001 has Construction+Care, M002 has Construction+Certification, M003 has Construction+Eco
# All 3 have Construction, so all match
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { material_features: { feature_name: { in: [\"Eco\", \"Construction\"] } } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 3

# Nested filter: in on feature_name — Certification only (only M002)
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { material_features: { feature_name: { in: [\"Certification\"] } } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M002"

# Nested filter: all on feature_name (AND) — material must have BOTH Construction AND Eco
# M001: Construction+Care (no Eco) → no match
# M002: Construction+Certification (no Eco) → no match
# M003: Construction+Eco → match
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { material_features: { feature_name: { all: [\"Construction\", \"Eco\"] } } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M003"

# Nested filter: all on feature_name — Construction AND Certification
# Only M002 has both
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { material_features: { feature_name: { all: [\"Construction\", \"Certification\"] } } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M002"

# Deeply nested filter: eq on attr_value
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { material_features: { feature_attributes: { attr_value: { eq: \"OEKO-TEX\" } } } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M002"

# Combined: nested eq + top-level eq
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { status: { eq: \"active\" }, material_features: { feature_name: { eq: \"Construction\" } } }) { node { mat_no name status } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2
jsonpath "$.data.searchMaterials[*].node.mat_no" contains "M001"
jsonpath "$.data.searchMaterials[*].node.mat_no" contains "M002"
jsonpath "$.data.searchMaterials[*].node.status" contains "active"

# Combined: text query + operator filter
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"wool\", filter: { material_features: { feature_name: { eq: \"Construction\" } } }) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M002"

POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"wool\") { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M002"

POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"plain\") { node { mat_no name material_features { feature_name description } } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M001"
jsonpath "$.data.searchMaterials[0].node.material_features" count == 3

# Nested relations on a search hit resolve through the batched relation loader:
# sizes, colorways, and material_features -> feature_attributes must all return
# full child data (N+1 regression: previously one SQL query per hit per level).
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"plain\") { node { mat_no sizes { size_code } colorways { colorway_code } material_features { feature_name feature_attributes { attr_name attr_value } } } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M001"
jsonpath "$.data.searchMaterials[0].node.sizes" count == 3
jsonpath "$.data.searchMaterials[0].node.sizes[*].size_code" contains "S"
jsonpath "$.data.searchMaterials[0].node.sizes[*].size_code" contains "M"
jsonpath "$.data.searchMaterials[0].node.sizes[*].size_code" contains "L"
jsonpath "$.data.searchMaterials[0].node.colorways" count == 3
jsonpath "$.data.searchMaterials[0].node.material_features" count == 3
jsonpath "$.data.searchMaterials[0].node.material_features[0].feature_name" == "Construction"
jsonpath "$.data.searchMaterials[0].node.material_features[0].feature_attributes[0].attr_name" == "weave_type"
jsonpath "$.data.searchMaterials[0].node.material_features[0].feature_attributes[0].attr_value" == "plain"

# Paging: limit returns fewer results
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 1) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1

# queryOperator: AND requires every term to match — "cotton canvas" only in M001
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"cotton canvas\", queryOperator: AND) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].node.mat_no" == "M001"

# queryOperator: OR matches any term — "wool canvas" is M001 (canvas) + M002 (wool)
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"wool canvas\", queryOperator: OR) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2
jsonpath "$.data.searchMaterials[*].node.mat_no" contains "M001"
jsonpath "$.data.searchMaterials[*].node.mat_no" contains "M002"

# queryOperator: AND with no document holding both terms returns nothing
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"wool canvas\", queryOperator: AND) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 0

# default queryOperator is OR — "wool canvas" matches either term
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"wool canvas\") { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2
jsonpath "$.data.searchMaterials[*].node.mat_no" contains "M001"
jsonpath "$.data.searchMaterials[*].node.mat_no" contains "M002"

# queryOperator ignored when query is empty
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", queryOperator: AND) { node { mat_no } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 3

# Paging: limit of 2 on all results
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 2) { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2

# Paging: first page with limit 2, offset 0 = items 0-1
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 2, offset: 0) { node { mat_no } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2

# Paging: second page with offset past first page
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 2, offset: 2) { node { mat_no } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1

# Paging: offset beyond total returns empty
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 10, offset: 100) { node { mat_no } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 0

# esQuery is disabled with pgroonga backend — should return an error
POST http://app:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(esQuery: \"{\\\"bool\\\":{\\\"must\\\":[{\\\"term\\\":{\\\"material_features.feature_name.keyword\\\":\\\"Eco\\\"}}]}}\") { node { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data" == null
jsonpath "$.errors[0].message" contains "esQuery is not supported"
