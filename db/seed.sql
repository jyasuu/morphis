INSERT INTO materials (mat_no, name, status) VALUES
  ('M001', 'Premium Cotton Canvas', 'active'),
  ('M002', 'Merino Wool Blend', 'active'),
  ('M003', 'Recycled Polyester', 'discontinued');

INSERT INTO sizes (size_code, mat_no, name) VALUES
  ('S', 'M001', 'Small'),
  ('M', 'M001', 'Medium'),
  ('L', 'M001', 'Large');

INSERT INTO colorways (colorway_code, mat_no, name, hex) VALUES
  ('WH', 'M001', 'White', '#FFFFFF'),
  ('BK', 'M001', 'Black', '#000000'),
  ('NV', 'M001', 'Navy', '#000080');

INSERT INTO material_features (mat_no, feature_name, description) VALUES
  ('M001', 'Construction', 'Plain weave'),
  ('M001', 'Care', 'Standard care instructions'),
  ('M001', 'Water Resistant', 'Durable water-repellent finish suitable for outdoor use'),
  ('M002', 'Construction', 'Knitted'),
  ('M002', 'Certification', NULL),
  ('M002', 'Water Resistant', 'Naturally water-repellent lanolin-rich wool'),
  ('M003', 'Construction', 'Twist'),
  ('M003', 'Eco', 'Recycled materials');

INSERT INTO feature_attributes (feature_id, attr_name, attr_value) VALUES
  (1, 'weave_type', 'plain'),
  (1, 'thread_count', '120'),
  (2, 'wash', '30°C'),
  (2, 'bleach', 'No'),
  (3, 'type', 'DWR'),
  (3, 'durability', 'high'),
  (4, 'weave_type', 'knit'),
  (4, 'weight', '180 gsm'),
  (5, 'standard', 'OEKO-TEX'),
  (5, 'class', 'I'),
  (6, 'type', 'lanolin'),
  (7, 'weave_type', 'twist'),
  (7, 'weight', '150 gsm'),
  (8, 'recycled_content', '100%'),
  (8, 'certification', 'GRS');
