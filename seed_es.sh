#!/usr/bin/env bash
set -euo pipefail

ES_URL="${ES_URL:-http://localhost:9200}"
ES_AUTH="${ES_AUTH:--u elastic:morphis_es_pass}"
INDEX="${1:-materials}"

echo "=== Creating index: $INDEX ==="
curl -s $ES_AUTH -X PUT "$ES_URL/$INDEX" -H "Content-Type: application/json" -d '{
  "settings": { "number_of_shards": 1, "number_of_replicas": 0 }
}' | python3 -m json.tool

echo ""
echo "=== Indexing documents ==="

# Material M001 - Premium Cotton Canvas
curl -s $ES_AUTH -X POST "$ES_URL/$INDEX/_doc/M001" -H "Content-Type: application/json" -d '{
  "mat_no": "M001",
  "name": "Premium Cotton Canvas",
  "status": "active",
  "material_features": [
    {
      "feature_name": "Construction",
      "description": "Plain weave",
      "feature_attributes": [
        { "attr_name": "weave_type", "attr_value": "plain" },
        { "attr_name": "thread_count", "attr_value": "120" }
      ]
    },
    {
      "feature_name": "Care",
      "description": "Standard care instructions",
      "feature_attributes": [
        { "attr_name": "wash", "attr_value": "30°C" },
        { "attr_name": "bleach", "attr_value": "No" }
      ]
    }
  ]
}' | python3 -m json.tool

# Material M002 - Merino Wool Blend
curl -s $ES_AUTH -X POST "$ES_URL/$INDEX/_doc/M002" -H "Content-Type: application/json" -d '{
  "mat_no": "M002",
  "name": "Merino Wool Blend",
  "status": "active",
  "material_features": [
    {
      "feature_name": "Construction",
      "description": "Knitted",
      "feature_attributes": [
        { "attr_name": "weave_type", "attr_value": "knit" },
        { "attr_name": "weight", "attr_value": "180 gsm" }
      ]
    },
    {
      "feature_name": "Certification",
      "description": null,
      "feature_attributes": [
        { "attr_name": "standard", "attr_value": "OEKO-TEX" },
        { "attr_name": "class", "attr_value": "I" }
      ]
    }
  ]
}' | python3 -m json.tool

# Material M003 - Recycled Polyester
curl -s $ES_AUTH -X POST "$ES_URL/$INDEX/_doc/M003" -H "Content-Type: application/json" -d '{
  "mat_no": "M003",
  "name": "Recycled Polyester",
  "status": "discontinued",
  "material_features": [
    {
      "feature_name": "Construction",
      "description": "Twist",
      "feature_attributes": [
        { "attr_name": "weave_type", "attr_value": "twist" },
        { "attr_name": "weight", "attr_value": "150 gsm" }
      ]
    },
    {
      "feature_name": "Eco",
      "description": "Recycled materials",
      "feature_attributes": [
        { "attr_name": "recycled_content", "attr_value": "100%" },
        { "attr_name": "certification", "attr_value": "GRS" }
      ]
    }
  ]
}' | python3 -m json.tool

echo ""
echo "=== Refreshing index ==="
curl -s $ES_AUTH -X POST "$ES_URL/$INDEX/_refresh" | python3 -m json.tool

echo ""
echo "=== Verifying count ==="
curl -s $ES_AUTH "$ES_URL/$INDEX/_count" | python3 -m json.tool

echo ""
echo "=== Done ==="
