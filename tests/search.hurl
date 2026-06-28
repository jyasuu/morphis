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
jsonpath "$.data.searchMaterials[0].material_features" count == 2

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
