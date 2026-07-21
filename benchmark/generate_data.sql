-- Bulk data generator for benchmark tests.
-- Generates N benchmark materials with sizes, colorways, features, and attributes.
-- Run: PGPASSWORD=postgres psql -h db -U postgres -d morphis -f generate_data.sql
--
-- Number of materials to generate. Pass via CLI:
--   psql -v num=10000 -f generate_data.sql
-- Python script passes this via -v automatically. Default: 5000

\t on
\echo '=== Generating benchmark data ==='

-- Generate materials
INSERT INTO materials (mat_no, name, status, tenant_id)
SELECT
  'BM-' || LPAD(i::text, 7, '0'),
  CASE (i % 5)
    WHEN 0 THEN 'Premium '
    WHEN 1 THEN 'Standard '
    WHEN 2 THEN 'Economy '
    WHEN 3 THEN 'Pro '
    ELSE 'Elite '
  END || 'Benchmark Material ' || i,
  CASE
    WHEN i % 7 = 0 THEN 'discontinued'
    WHEN i % 13 = 0 THEN 'inactive'
    ELSE 'active'
  END,
  CASE WHEN i % 10 = 0 THEN 'tenant-b' ELSE 'tenant-a' END
FROM generate_series(1, :num) AS i
ON CONFLICT (mat_no) DO NOTHING;

\echo '  materials done'

-- Generate sizes (5 per material)
INSERT INTO sizes (size_code, mat_no, name)
SELECT
  s.size_code,
  'BM-' || LPAD(i::text, 7, '0'),
  s.name
FROM generate_series(1, :num) AS i
CROSS JOIN (
  VALUES ('XS', 'Extra Small'), ('S', 'Small'),
         ('M', 'Medium'), ('L', 'Large'), ('XL', 'Extra Large')
) AS s(size_code, name)
ON CONFLICT DO NOTHING;

\echo '  sizes done'

-- Generate colorways (3 per material)
INSERT INTO colorways (colorway_code, mat_no, name, hex)
SELECT
  'CW-' || LPAD(i::text, 7, '0') || c.suffix,
  'BM-' || LPAD(i::text, 7, '0'),
  c.name,
  c.hex
FROM generate_series(1, :num) AS i
CROSS JOIN (
  VALUES ('-W', 'White', '#FFFFFF'),
         ('-B', 'Black', '#000000'),
         ('-N', 'Navy', '#000080')
) AS c(suffix, name, hex)
ON CONFLICT DO NOTHING;

\echo '  colorways done'

-- Generate material_features (3 per material, one INSERT per feature batch)
INSERT INTO material_features (mat_no, feature_name, description)
SELECT
  'BM-' || LPAD(i::text, 7, '0'),
  f.feature_name,
  f.description
FROM generate_series(1, :num) AS i
CROSS JOIN LATERAL (
  VALUES
    ('Construction', 'Construction details for BM-' || LPAD(i::text, 7, '0')),
    ('Care', 'Care instructions for BM-' || LPAD(i::text, 7, '0')),
    ('Water Resistant', 'Water resistance info for BM-' || LPAD(i::text, 7, '0'))
) AS f(feature_name, description)
WHERE f.feature_name IS NOT NULL
ON CONFLICT DO NOTHING;

\echo '  material_features done'

-- Generate feature_attributes (2 per feature)
INSERT INTO feature_attributes (feature_id, attr_name, attr_value)
SELECT
  mf.id,
  a.attr_name,
  a.attr_value
FROM material_features mf
CROSS JOIN LATERAL (
  VALUES
    ('type', mf.feature_name || '_type_for_' || mf.mat_no),
    ('value', 'standard_value_for_' || mf.mat_no)
) AS a(attr_name, attr_value)
WHERE mf.mat_no LIKE 'BM-%'
  AND NOT EXISTS (
    SELECT 1 FROM feature_attributes fa WHERE fa.feature_id = mf.id
  )
ON CONFLICT DO NOTHING;

\echo '  feature_attributes done'

-- Show stats
\echo ''
\echo '=== Generation stats ==='
SELECT 'materials' AS entity, count(*)::text AS count FROM materials WHERE mat_no LIKE 'BM-%'
UNION ALL
SELECT 'sizes', count(*)::text FROM sizes WHERE mat_no LIKE 'BM-%'
UNION ALL
SELECT 'colorways', count(*)::text FROM colorways WHERE mat_no LIKE 'BM-%'
UNION ALL
SELECT 'material_features', count(*)::text FROM material_features WHERE mat_no LIKE 'BM-%'
UNION ALL
SELECT 'feature_attributes', count(*)::text FROM feature_attributes fa WHERE EXISTS (
  SELECT 1 FROM material_features mf WHERE mf.id = fa.feature_id AND mf.mat_no LIKE 'BM-%'
);
