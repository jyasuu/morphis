POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"cotton\") { mat_no name status } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M001"
jsonpath "$.data.searchMaterials[0].name" == "Premium Cotton Canvas"
jsonpath "$.data.searchMaterials[0].status" == "active"

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\") { mat_no name status } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 3

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", filter: { status: \"active\" }) { mat_no name status } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2
jsonpath "$.data.searchMaterials[*].mat_no" contains "M001"
jsonpath "$.data.searchMaterials[*].mat_no" contains "M002"

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"wool\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M002"

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"plain\") { mat_no name material_features { feature_name description } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M001"
jsonpath "$.data.searchMaterials[0].material_features" count == 3

POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"cotton\", filter: { status: \"active\" }) { mat_no name status } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M001"
jsonpath "$.data.searchMaterials[0].status" == "active"

# Paging: limit returns fewer results
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 1) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1

# Paging: limit of 2 on all results
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 2) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2

# Paging: first page with limit 2, offset 0 = items 0-1
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 2, offset: 0) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2

# Paging: second page with offset past first page
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 2, offset: 2) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1

# Paging: offset beyond total returns empty
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"\", limit: 10, offset: 100) { mat_no } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 0

# Raw ES query: filter by nested feature_name = "Eco"
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(esQuery: \"{\\\"bool\\\":{\\\"must\\\":[{\\\"term\\\":{\\\"material_features.feature_name.keyword\\\":\\\"Eco\\\"}}]}}\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M003"

# Raw ES query: filter by feature_name = "Certification"
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(esQuery: \"{\\\"bool\\\":{\\\"must\\\":[{\\\"term\\\":{\\\"material_features.feature_name.keyword\\\":\\\"Certification\\\"}}]}}\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M002"

# Raw ES query: filter by feature_name = "Construction" (all 3 have it)
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(esQuery: \"{\\\"bool\\\":{\\\"must\\\":[{\\\"term\\\":{\\\"material_features.feature_name.keyword\\\":\\\"Construction\\\"}}]}}\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 3

# Raw ES query: bool must + should (feature=Construction AND name contains Wool)
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(esQuery: \"{\\\"bool\\\":{\\\"must\\\":[{\\\"term\\\":{\\\"material_features.feature_name.keyword\\\":\\\"Construction\\\"}}],\\\"should\\\":[{\\\"match\\\":{\\\"name\\\":\\\"Wool\\\"}}],\\\"minimum_should_match\\\":1}}\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M002"

# Raw ES query: nested attribute match (attr_value = "OEKO-TEX")
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(esQuery: \"{\\\"term\\\":{\\\"material_features.feature_attributes.attr_value.keyword\\\":\\\"OEKO-TEX\\\"}}\") { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M002"

# Raw ES query: with limit
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(esQuery: \"{\\\"match_all\\\":{}}\", limit: 2) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2

# Structured nested filter: filter by feature_name
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(filter: { material_features: { feature_name: \"Eco\" } }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M003"

# Structured nested filter: Certification
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(filter: { material_features: { feature_name: \"Certification\" } }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M002"

# Structured nested filter: Construction (all 3 have it)
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(filter: { material_features: { feature_name: \"Construction\" } }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 3

# Structured deeply nested filter: attr_value = "OEKO-TEX"
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(filter: { material_features: { feature_attributes: { attr_value: \"OEKO-TEX\" } } }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M002"

# Structured nested filter combined with top-level: feature_name=Construction + status=active
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(filter: { status: \"active\", material_features: { feature_name: \"Construction\" } }) { mat_no name status } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 2
jsonpath "$.data.searchMaterials[*].mat_no" contains "M001"
jsonpath "$.data.searchMaterials[*].mat_no" contains "M002"
jsonpath "$.data.searchMaterials[*].status" contains "active"

# Structured nested filter with query text search
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ searchMaterials(query: \"wool\", filter: { material_features: { feature_name: \"Construction\" } }) { mat_no name } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.searchMaterials" count == 1
jsonpath "$.data.searchMaterials[0].mat_no" == "M002"
