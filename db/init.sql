CREATE EXTENSION IF NOT EXISTS pgroonga;

CREATE TABLE IF NOT EXISTS materials (
    mat_no TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    tenant_id TEXT,
    search_doc JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_materials_search_doc
    ON materials USING pgroonga (search_doc)
    WITH (tokenizer = 'TokenBigram');

CREATE TABLE IF NOT EXISTS sizes (
    id SERIAL PRIMARY KEY,
    size_code TEXT NOT NULL,
    mat_no TEXT NOT NULL REFERENCES materials(mat_no),
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS colorways (
    id SERIAL PRIMARY KEY,
    colorway_code TEXT NOT NULL,
    mat_no TEXT NOT NULL REFERENCES materials(mat_no),
    name TEXT NOT NULL,
    hex TEXT
);

CREATE TABLE IF NOT EXISTS material_features (
    id SERIAL PRIMARY KEY,
    mat_no TEXT NOT NULL REFERENCES materials(mat_no),
    feature_name TEXT NOT NULL,
    description TEXT
);

CREATE TABLE IF NOT EXISTS feature_attributes (
    id SERIAL PRIMARY KEY,
    feature_id INTEGER NOT NULL REFERENCES material_features(id),
    attr_name TEXT NOT NULL,
    attr_value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_permissions (
    id SERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    region TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS protected_data (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    value TEXT,
    region TEXT
);

CREATE OR REPLACE FUNCTION rebuild_search_doc(p_mat_no TEXT)
RETURNS VOID AS $$
BEGIN
  PERFORM set_config('app.rebuilding_search_doc', 'true', false);
  UPDATE materials SET search_doc = (
    SELECT jsonb_build_object(
      'mat_no', m.mat_no,
      'name', m.name,
      'status', m.status,
      'tenant_id', m.tenant_id,
      'sizes', (
        SELECT COALESCE(jsonb_agg(jsonb_build_object(
          'id', s.id, 'size_code', s.size_code, 'mat_no', s.mat_no, 'name', s.name
        )), '[]'::jsonb)
        FROM sizes s WHERE s.mat_no = m.mat_no
      ),
      'colorways', (
        SELECT COALESCE(jsonb_agg(jsonb_build_object(
          'id', c.id, 'colorway_code', c.colorway_code, 'mat_no', c.mat_no,
          'name', c.name, 'hex', c.hex
        )), '[]'::jsonb)
        FROM colorways c WHERE c.mat_no = m.mat_no
      ),
      'material_features', (
        SELECT COALESCE(jsonb_agg(jsonb_build_object(
          'id', mf.id, 'feature_id', mf.id, 'mat_no', mf.mat_no,
          'feature_name', mf.feature_name, 'description', mf.description,
          'feature_attributes', (
            SELECT COALESCE(jsonb_agg(jsonb_build_object(
              'id', fa.id, 'feature_id', fa.feature_id,
              'attr_name', fa.attr_name, 'attr_value', fa.attr_value
            )), '[]'::jsonb)
            FROM feature_attributes fa WHERE fa.feature_id = mf.id
          )
        )), '[]'::jsonb)
        FROM material_features mf WHERE mf.mat_no = m.mat_no
      )
    )
    FROM materials m WHERE m.mat_no = p_mat_no
  )
  WHERE materials.mat_no = p_mat_no;
  PERFORM set_config('app.rebuilding_search_doc', 'false', false);
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION rebuild_search_doc_on_child_change()
RETURNS trigger AS $$
DECLARE
  mat_no_val TEXT;
BEGIN
  IF current_setting('app.rebuilding_search_doc', true) = 'true' THEN
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
  END IF;
  IF TG_TABLE_NAME = 'feature_attributes' THEN
    SELECT mat_no INTO mat_no_val
      FROM material_features
      WHERE id = (CASE WHEN TG_OP = 'DELETE' THEN OLD.feature_id ELSE NEW.feature_id END);
  ELSE
    IF TG_OP = 'DELETE' THEN
      mat_no_val := OLD.mat_no;
    ELSE
      mat_no_val := NEW.mat_no;
    END IF;
  END IF;

  IF mat_no_val IS NOT NULL THEN
    PERFORM rebuild_search_doc(mat_no_val);
  END IF;

  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION rebuild_search_doc_on_material_change()
RETURNS trigger AS $$
DECLARE
  mat_no_val TEXT;
BEGIN
  IF current_setting('app.rebuilding_search_doc', true) = 'true' THEN
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
  END IF;
  IF TG_OP = 'DELETE' THEN
    mat_no_val := OLD.mat_no;
  ELSE
    mat_no_val := NEW.mat_no;
  END IF;

  PERFORM rebuild_search_doc(mat_no_val);

  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS search_doc_material_change_trigger ON materials;
CREATE TRIGGER search_doc_material_change_trigger
AFTER INSERT OR UPDATE OR DELETE ON materials
FOR EACH ROW EXECUTE FUNCTION rebuild_search_doc_on_material_change();

DROP TRIGGER IF EXISTS search_doc_sizes_change_trigger ON sizes;
CREATE TRIGGER search_doc_sizes_change_trigger
AFTER INSERT OR UPDATE OR DELETE ON sizes
FOR EACH ROW EXECUTE FUNCTION rebuild_search_doc_on_child_change();

DROP TRIGGER IF EXISTS search_doc_colorways_change_trigger ON colorways;
CREATE TRIGGER search_doc_colorways_change_trigger
AFTER INSERT OR UPDATE OR DELETE ON colorways
FOR EACH ROW EXECUTE FUNCTION rebuild_search_doc_on_child_change();

DROP TRIGGER IF EXISTS search_doc_material_features_change_trigger ON material_features;
CREATE TRIGGER search_doc_material_features_change_trigger
AFTER INSERT OR UPDATE OR DELETE ON material_features
FOR EACH ROW EXECUTE FUNCTION rebuild_search_doc_on_child_change();

DROP TRIGGER IF EXISTS search_doc_feature_attributes_change_trigger ON feature_attributes;
CREATE TRIGGER search_doc_feature_attributes_change_trigger
AFTER INSERT OR UPDATE OR DELETE ON feature_attributes
FOR EACH ROW EXECUTE FUNCTION rebuild_search_doc_on_child_change();
