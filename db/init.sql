CREATE TABLE IF NOT EXISTS materials (
    mat_no TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    tenant_id TEXT
);

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
