---
name: morphis-config
description: Pure YAML reference for morphis configuration — main app and auth-proxy config files, all fields with defaults, Docker wiring, and common editing tasks. No code required.
---

# Morphis Configuration Reference

All configuration is YAML. Two independent config files: one for the main app, one for the auth-proxy.

## Quick start (zero to running)

### Prerequisites

- Docker + Docker Compose v2
- The `morphis:local` Docker image (pre-built or built from source)

### Step 1: Create project structure

```
project/
├── config.yaml              # local dev config
├── config.docker.yaml       # Docker config (mounted into container)
├── docker-compose.yml
└── db/
    └── init.sql             # postgres table definitions
```

### Step 2: Create `db/init.sql`

```sql
CREATE TABLE IF NOT EXISTS items (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    status VARCHAR(50) DEFAULT 'active'
);

INSERT INTO items (name, status) VALUES
    ('Widget A', 'active'),
    ('Widget B', 'active');
```

### Step 3: Create `config.yaml`

```yaml
database:
  url: "postgres://postgres:postgres@localhost:5432/morphis"
  max_connections: 10

server:
  host: "0.0.0.0"
  port: 4000

auth:
  enabled: false

tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
        nullable: false
        auto_increment: true
      - name: name
        type: string
        nullable: false
      - name: status
        type: string
        nullable: false
```

### Step 4: Create `config.docker.yaml`

Same as config.yaml but with Docker hostnames:

```yaml
database:
  url: "postgres://postgres:postgres@db:5432/morphis"
  max_connections: 10

server:
  host: "0.0.0.0"
  port: 4000

auth:
  enabled: false

tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
        nullable: false
        auto_increment: true
      - name: name
        type: string
        nullable: false
      - name: status
        type: string
        nullable: false
```

### Step 5: Create `docker-compose.yml`

```yaml
services:
  db:
    image: postgres:16
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: morphis
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5
    volumes:
      - ./db/init.sql:/docker-entrypoint-initdb.d/01-init.sql

  app:
    image: ghcr.io/jyasuu/morphis:main
    environment:
      MORPHIS_CONFIG: /app/config.yaml
    ports:
      - "4000:4000"
    volumes:
      - ./config.docker.yaml:/app/config.yaml
    depends_on:
      db:
        condition: service_healthy
```

### Step 6: Start

```bash
docker compose up -d
```

### Step 7: Verify

```bash
# Health check
curl http://localhost:4000/

# GraphQL query
curl -s -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ itemsList { id name status } }"}'
```

Expected output:
```json
{"data":{"itemsList":[{"id":1,"name":"Widget A","status":"active"},{"id":2,"name":"Widget B","status":"active"}]}}
```

### Key rules

- `tables:` cannot be empty — Morphis crashes with `SchemaError("Object \"Query\" must define one or more fields")`
- Column type `date_time` uses underscore (not `datetime`)
- The `tables` map key becomes the GraphQL type name: `items` → `itemsList`, `createItems`, `deleteItems`
- Docker config uses service names as hostnames (`db` not `localhost`)

| File | Service | Env var override |
|---|---|---|
| `config.yaml` | Main app (local dev) | `MORPHIS_CONFIG` |
| `config.docker.yaml` | Main app (Docker) | `MORPHIS_CONFIG` |
| `auth-proxy/config.yaml` | Auth-proxy (local dev) | `AUTH_PROXY_CONFIG` |
| `auth-proxy/config.docker.yaml` | Auth-proxy (Docker) | `AUTH_PROXY_CONFIG` |

In Docker, `docker-compose.yml` mounts the `config.docker.yaml` files into the container at `/app/config.yaml` and sets the env var to point there.

---

## Config files

| File | Service | Env var override |
|---|---|---|
| `config.yaml` | Main app (local dev) | `MORPHIS_CONFIG` |
| `config.docker.yaml` | Main app (Docker) | `MORPHIS_CONFIG` |
| `auth-proxy/config.yaml` | Auth-proxy (local dev) | `AUTH_PROXY_CONFIG` |
| `auth-proxy/config.docker.yaml` | Auth-proxy (Docker) | `AUTH_PROXY_CONFIG` |

In Docker, `docker-compose.yml` mounts the `config.docker.yaml` files into the container at `/app/config.yaml` and sets the env var to point there.

---

## Minimal docker-compose.yml

```yaml
services:
  db:
    image: postgres:16
    command: ["postgres", "-c", "wal_level=logical"]
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: morphis
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5
    volumes:
      - ./db/init.sql:/docker-entrypoint-initdb.d/01-init.sql

  app:
    image: morphis:local
    environment:
      MORPHIS_CONFIG: /app/config.yaml
    ports:
      - "4000:4000"
    volumes:
      - ./config.docker.yaml:/app/config.yaml
    depends_on:
      db:
        condition: service_healthy
```

Start with:
```bash
docker compose up -d
```

**Notes:**
- `db/init.sql` creates the postgres tables that `tables:` in config references
- Port 4000 is the GraphQL API + MCP endpoint
- For local dev without Docker, point `config.yaml` database.url to `localhost:5432` and run `morphis` directly

---

## Main app config (`config.yaml`)

### database (required)

```yaml
database:
  url: "postgres://user:pass@localhost:5432/morphis"  # required
  max_connections: 10                                   # default: 10
```

### server (required)

```yaml
server:
  host: "0.0.0.0"   # default: "0.0.0.0"
  port: 4000          # default: 4000
```

### auth (optional — remove entire section to disable)

```yaml
auth:
  enabled: false                   # default: false
  jwt_secret: "your-secret"        # HS256 shared secret
  jwks_url: "http://..."          # JWKS endpoint for RS256
  issuer: "http://..."            # optional — JWT issuer validation
  audience: "api"                  # optional — JWT audience validation
  identity_mappings:               # JWT claim → upstream header
    - claim: sub
      header: X-User-ID
    - claim: tenant_id
      header: X-Tenant-ID
    - claim: role
      header: X-Role
```

### elasticsearch (optional)

```yaml
elasticsearch:
  url: "http://elastic:morphis_es_pass@localhost:9200"
```

### mcp (optional)

```yaml
mcp:
  enabled: true                    # default: true
  server_name: morphis-mcp
  server_description: "MCP server for morphis"
  prompts:
    system: "You are a database assistant..."
    query_guidance: "WORKFLOW: discover_tables → graphql_schema → graphql"
  auth:                            # optional MCP-specific auth (separate from top-level auth)
    enabled: true
    jwt_secret: "mcp-secret"
    jwks_url: "http://..."
    issuer: "..."
    audience: "..."
    identity_mappings:
      - claim: sub
        header: X-User-ID
```

### circuit_breakers (optional — all fields have defaults)

```yaml
circuit_breakers:
  es:
    failure_threshold: 5           # default: 5
    reset_timeout_secs: 30         # default: 30
    half_open_max_requests: 3      # default: 3
  jwks:
    failure_threshold: 3           # default: 3
    reset_timeout_secs: 60         # default: 60
    half_open_max_requests: 1      # default: 1
```

### tables (required — map key = GraphQL type name)

Each table key becomes the GraphQL type name. Example: key `materials` produces `materialsList` query, `createMaterials` mutation.

```yaml
tables:
  materials:                       # ← GraphQL type name
    table: materials               # ← actual postgres table name
    primary_key: [mat_no]          # required, single column only
    prompt: "Description shown to MCP tools"
    crud:                          # optional — all default to true
      create: true
      read: true
      update: true
      delete: true
    common_queries:                # optional — shown by MCP discover_tables
      - description: "Find materials by feature"
        tool: query                # query | query_by_related | search
        params: { table: "materials" }
```

#### Column fields

```yaml
    columns:
      - name: mat_no               # column name in postgres
        type: string               # required: int | int64 | float | boolean | string | text | uuid | date_time | date | json
        nullable: false            # default: false
        unique: false              # default: false
        auto_increment: false      # default: false
        default: "'active'"        # optional — SQL expression as string
        prompt: "Unique material identifier"
        examples: ["M001", "M002"]
```

**Important:** The type `date_time` uses an underscore (not `datetime`).

#### Row filters (optional — row-level security)

Two variants, distinguished by which fields are present:

**Column filter** — injects header value directly into a column:
```yaml
    row_filters:
      - column: tenant_id
        from_header: X-Tenant-ID
        auto_set: true             # default: true — auto-populate on INSERT/UPDATE
```

**Subquery filter** — looks up allowed values from another table:
```yaml
    row_filters:
      - type: subquery
        from_header: X-User-ID
        columns: [tenant_id]       # columns to filter on THIS table
        match_columns: [tenant_id] # must match columns 1:1
        from_source: user_permissions  # table to query for allowed values
        user_column: user_id      # column in from_source matching the header value
        cache_ttl_secs: 30         # optional — cache subquery results in seconds
```

#### Relations (optional)

```yaml
    relations:
      - name: sizes                # GraphQL field name
        type: has_many             # has_many | has_one | belongs_to
        table: sizes               # target table key (must exist in tables map)
        local_field: mat_no        # column on THIS table
        foreign_field: mat_no      # column on target table
```

Composite key variant:
```yaml
      - name: parent
        type: belongs_to
        table: parents
        local_fields: [pk1, pk2]
        foreign_fields: [fk1, fk2]
```

### search_indexes (optional — Elasticsearch sync)

```yaml
search_indexes:
  - name: materials_search         # index name in ES
    index: materials               # postgres table to index
    type: materials                # GraphQL type name
    searchable_fields: [mat_no, name, status]
    join_fields:                   # optional — nested join for related data
      - name: features
        index_field: material_features  # ES field name
        table: material_features   # postgres table
        local_field: mat_no
        foreign_field: mat_no
        searchable_fields: [feature_name, description]
        join_fields: []            # supports nesting
```

---

## Auth-proxy config (`auth-proxy/config.yaml`)

```yaml
listen_addr: "0.0.0.0:9080"       # required
upstream: "http://app:4000"        # required — morphis app URL
jwt_secret: "shared-secret"        # HS256 fallback secret
jwt_jwks_url: "http://keycloak:8080/realms/morphis/protocol/openid-connect/certs"
jwt_issuer: ""                     # optional — empty skips issuer validation
require_auth: true                 # default: true — reject unauthenticated requests
header_mappings:                   # required — JWT claim → upstream header
  - claim: sub
    header: X-User-ID
  - claim: tenant_id
    header: X-Tenant-ID
  - claim: role
    header: X-Role
```

JWT validation order: RS256 (JWKS) first, HS256 (`jwt_secret`) fallback. If both fail → 401.

---

## Common tasks

### Change server port

Edit `config.yaml`:
```yaml
server:
  port: 3001
```

### Disable auth

Remove the `auth` section entirely, or set `auth.enabled: false`.

### Add a new table

1. Add the postgres table to `db/init.sql`
2. Add seed data to `db/seed.sql` (optional)
3. Add the table config under `tables:` in `config.yaml` and `config.docker.yaml`

### Add row-level security to a table

Add `row_filters` entries. Column filters are simpler (direct header injection). Subquery filters require a permissions table that maps users to allowed values.

### Add a relation between tables

Add to the table's `relations` list. The `name` becomes the GraphQL field name. Both sides should be defined (parent has `has_many`, child has `belongs_to`).

### Change auth-proxy JWT behavior

- Add `jwt_jwks_url` for RS256 (Keycloak OIDC)
- Keep `jwt_secret` for HS256 fallback (credentials mode)
- Set `jwt_issuer: ""` to skip issuer validation (when hostname differs internal vs external)
- Set `require_auth: false` to allow unauthenticated requests through

### Add a new column to an existing table

Add the column entry under the table's `columns:` list. The column name must match the postgres column. Rebuild after.

### Add Elasticsearch indexing for a table

Add a `search_indexes` entry with the table's searchable fields. The `pgx-listen` service in docker-compose.yml handles syncing via Postgres logical replication.
