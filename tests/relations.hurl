# has_many: materials -> sizes
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materials(id: \"M001\") { mat_no sizes { id size_code } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials.mat_no" == "M001"
jsonpath "$.data.materials.sizes[0].size_code" == "S"
jsonpath "$.data.materials.sizes[1].size_code" == "M"
jsonpath "$.data.materials.sizes[2].size_code" == "L"

# has_many: materials -> colorways
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materials(id: \"M001\") { mat_no colorways { colorway_code hex } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials.colorways[0].colorway_code" == "WH"
jsonpath "$.data.materials.colorways[1].colorway_code" == "BK"
jsonpath "$.data.materials.colorways[2].colorway_code" == "NV"

# belongs_to: sizes -> material
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ sizes(id: 1) { size_code material { mat_no name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.sizes.material.mat_no" == "M001"
jsonpath "$.data.sizes.material.name" == "Premium Cotton Canvas"

# belongs_to: colorways -> material
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ colorways(id: 1) { colorway_code material { mat_no name status } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.colorways.material.mat_no" == "M001"

# belongs_to: material_features -> material
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ material_features(id: 1) { feature_name material { mat_no } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.material_features.material.mat_no" == "M001"

# belongs_to: feature_attributes -> material_feature
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ feature_attributes(id: 1) { attr_name material_feature { feature_name } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.feature_attributes.material_feature.feature_name" == "Construction"

# has_many: material_features -> feature_attributes
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ material_features(id: 1) { feature_name feature_attributes { attr_name attr_value } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.material_features.feature_attributes[0].attr_name" == "weave_type"
jsonpath "$.data.material_features.feature_attributes[1].attr_name" == "thread_count"

# Deep nested: materials -> material_features -> feature_attributes
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ materials(id: \"M002\") { mat_no material_features { feature_name feature_attributes { attr_name attr_value } } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.materials.mat_no" == "M002"
jsonpath "$.data.materials.material_features[0].feature_name" == "Construction"
jsonpath "$.data.materials.material_features[0].feature_attributes[0].attr_name" == "weave_type"
jsonpath "$.data.materials.material_features[1].feature_name" == "Certification"

# Chain: feature_attributes -> material_feature -> material
POST http://localhost:4000/graphql
Content-Type: application/json
{
  "query": "{ feature_attributes(id: 1) { attr_name material_feature { feature_name material { mat_no name } } } }"
}
HTTP 200
[Asserts]
jsonpath "$.data.feature_attributes.material_feature.feature_name" == "Construction"
jsonpath "$.data.feature_attributes.material_feature.material.mat_no" == "M001"
