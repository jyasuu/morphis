#!/bin/sh
# Seed database tables needed for auth-proxy integration test
set -e

DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-morphis}"
DB_USER="${DB_USER:-postgres}"
DB_PASS="${DB_PASS:-postgres}"

PGPASSWORD="$DB_PASS" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" <<SQL
CREATE TABLE IF NOT EXISTS user_permissions (
  id SERIAL PRIMARY KEY,
  user_id VARCHAR(100) NOT NULL,
  tenant_id VARCHAR(100) NOT NULL,
  region VARCHAR(100) NOT NULL
);

CREATE TABLE IF NOT EXISTS protected_data (
  id VARCHAR(100) PRIMARY KEY,
  name VARCHAR(100) NOT NULL,
  value VARCHAR(100),
  region VARCHAR(100)
);

INSERT INTO user_permissions (user_id, tenant_id, region) VALUES
  ('admin', 'default', 'main')
ON CONFLICT DO NOTHING;

INSERT INTO protected_data (id, name, value, region) VALUES
  ('PD001', 'Test Secret', 'sensitive-value', 'main')
ON CONFLICT DO NOTHING;

ALTER TABLE materials ADD COLUMN IF NOT EXISTS tenant_id VARCHAR(100);
UPDATE materials SET tenant_id = 'default' WHERE tenant_id IS NULL;
SQL

echo "Auth chain DB tables seeded"
