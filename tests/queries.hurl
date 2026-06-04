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
jsonpath "$.data.material_featuresList" count == 6

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
