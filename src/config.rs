use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub tables: HashMap<String, TableConfig>,
    #[serde(default)]
    pub elasticsearch: Option<ElasticsearchConfig>,
    #[serde(default)]
    pub search_indexes: Vec<SearchIndexConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ElasticsearchConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchIndexConfig {
    pub name: String,
    pub index: String,
    #[serde(rename = "type")]
    pub graphql_type: String,
    pub searchable_fields: Vec<String>,
    #[serde(default)]
    pub join_fields: Vec<SearchJoinConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchJoinConfig {
    pub name: String,
    pub index_field: String,
    pub table: String,
    pub local_field: String,
    pub foreign_field: String,
    #[serde(default)]
    pub local_fields: Vec<String>,
    #[serde(default)]
    pub foreign_fields: Vec<String>,
    pub searchable_fields: Vec<String>,
    #[serde(default)]
    pub join_fields: Vec<SearchJoinConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    4000
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RowFilterConfig {
    ColumnFilter {
        column: String,
        from_header: String,
        #[serde(default = "default_auto_set")]
        auto_set: bool,
    },
    SubqueryFilter {
        from_header: String,
        columns: Vec<String>,
        match_columns: Vec<String>,
        from_source: String,
        user_column: String,
        #[serde(default)]
        cache_ttl_secs: Option<u64>,
    },
}

impl RowFilterConfig {
    pub fn header_name(&self) -> &str {
        match self {
            RowFilterConfig::ColumnFilter { from_header, .. } => from_header,
            RowFilterConfig::SubqueryFilter { from_header, .. } => from_header,
        }
    }

    #[allow(dead_code)]
    pub fn column(&self) -> Option<&str> {
        match self {
            RowFilterConfig::ColumnFilter { column, .. } => Some(column),
            RowFilterConfig::SubqueryFilter { .. } => None,
        }
    }

    pub fn is_auto_set(&self) -> bool {
        match self {
            RowFilterConfig::ColumnFilter { auto_set, .. } => *auto_set,
            RowFilterConfig::SubqueryFilter { .. } => false,
        }
    }
}

fn default_auto_set() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct PermissionCacheEntry {
    pub values: Vec<serde_json::Value>,
    pub expires_at: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct PermissionCache {
    store: std::collections::HashMap<String, PermissionCacheEntry>,
}

impl PermissionCache {
    pub fn new() -> Self {
        Self {
            store: std::collections::HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<Vec<serde_json::Value>> {
        self.store.get(key).and_then(|entry| {
            if std::time::Instant::now() < entry.expires_at {
                Some(entry.values.clone())
            } else {
                None
            }
        })
    }

    pub fn set(&mut self, key: String, values: Vec<serde_json::Value>, ttl: std::time::Duration) {
        self.store.insert(key, PermissionCacheEntry {
            expires_at: std::time::Instant::now() + ttl,
            values,
        });
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableConfig {
    pub table: String,
    pub columns: Vec<ColumnConfig>,
    pub primary_key: Vec<String>,
    #[serde(default)]
    pub relations: Vec<RelationConfig>,
    #[serde(default)]
    pub row_filters: Vec<RowFilterConfig>,
    #[serde(default)]
    pub crud: CrudConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CrudConfig {
    pub create: bool,
    pub read: bool,
    pub update: bool,
    pub delete: bool,
}

impl Default for CrudConfig {
    fn default() -> Self {
        Self { create: true, read: true, update: true, delete: true }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: ColumnType,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    #[allow(dead_code)]
    pub unique: bool,
    #[serde(default)]
    pub auto_increment: bool,
    #[serde(default)]
    #[allow(dead_code)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnType {
    Int,
    Int64,
    Float,
    Boolean,
    String,
    Text,
    Uuid,
    DateTime,
    Date,
    Json,
}

impl std::fmt::Display for ColumnType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColumnType::Int => write!(f, "Int"),
            ColumnType::Int64 => write!(f, "Int64"),
            ColumnType::Float => write!(f, "Float"),
            ColumnType::Boolean => write!(f, "Boolean"),
            ColumnType::String => write!(f, "String"),
            ColumnType::Text => write!(f, "Text"),
            ColumnType::Uuid => write!(f, "UUID"),
            ColumnType::DateTime => write!(f, "DateTime"),
            ColumnType::Date => write!(f, "Date"),
            ColumnType::Json => write!(f, "JSON"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub rel_type: RelationType,
    pub table: String,
    pub local_field: String,
    pub foreign_field: String,
    #[serde(default)]
    pub local_fields: Vec<String>,
    #[serde(default)]
    pub foreign_fields: Vec<String>,
}

impl RelationConfig {
    pub fn field_pairs(&self) -> Vec<(&str, &str)> {
        if !self.local_fields.is_empty() && !self.foreign_fields.is_empty() {
            self.local_fields.iter().zip(self.foreign_fields.iter())
                .map(|(l, f)| (l.as_str(), f.as_str()))
                .collect()
        } else {
            vec![(self.local_field.as_str(), self.foreign_field.as_str())]
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    HasMany,
    HasOne,
    BelongsTo,
}

impl Config {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "127.0.0.1"
  port: 8080
tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
        nullable: false
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.database.url, "postgres://localhost/test");
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.tables.len(), 1);
        assert!(config.elasticsearch.is_none());
        assert!(config.search_indexes.is_empty());
    }

    #[test]
    fn test_table_config_columns() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
        nullable: false
        unique: true
        auto_increment: true
      - name: name
        type: string
        nullable: false
      - name: description
        type: text
        nullable: true
      - name: price
        type: float
      - name: active
        type: boolean
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let table = config.tables.get("items").unwrap();
        assert_eq!(table.columns.len(), 5);

        let id_col = &table.columns[0];
        assert_eq!(id_col.name, "id");
        assert!(matches!(id_col.col_type, ColumnType::Int));
        assert!(!id_col.nullable);
        assert!(id_col.unique);
        assert!(id_col.auto_increment);

        let desc_col = &table.columns[2];
        assert!(desc_col.nullable);

        let price_col = &table.columns[3];
        assert!(matches!(price_col.col_type, ColumnType::Float));
        assert!(!price_col.nullable);
    }

    #[test]
    fn test_column_type_display() {
        assert_eq!(ColumnType::Int.to_string(), "Int");
        assert_eq!(ColumnType::Int64.to_string(), "Int64");
        assert_eq!(ColumnType::Float.to_string(), "Float");
        assert_eq!(ColumnType::Boolean.to_string(), "Boolean");
        assert_eq!(ColumnType::String.to_string(), "String");
        assert_eq!(ColumnType::Text.to_string(), "Text");
        assert_eq!(ColumnType::Uuid.to_string(), "UUID");
        assert_eq!(ColumnType::DateTime.to_string(), "DateTime");
        assert_eq!(ColumnType::Date.to_string(), "Date");
        assert_eq!(ColumnType::Json.to_string(), "JSON");
    }

    #[test]
    fn test_relation_types() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  parents:
    table: parents
    primary_key: [id]
    columns:
      - name: id
        type: int
    relations:
      - name: children
        type: has_many
        table: children
        local_field: id
        foreign_field: parent_id
  children:
    table: children
    primary_key: [id]
    columns:
      - name: id
        type: int
      - name: parent_id
        type: int
    relations:
      - name: parent
        type: belongs_to
        table: parents
        local_field: parent_id
        foreign_field: id
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();

        let parent = config.tables.get("parents").unwrap();
        assert_eq!(parent.relations.len(), 1);
        assert!(matches!(parent.relations[0].rel_type, RelationType::HasMany));
        assert_eq!(parent.relations[0].table, "children");

        let child = config.tables.get("children").unwrap();
        assert_eq!(child.relations.len(), 1);
        assert!(matches!(child.relations[0].rel_type, RelationType::BelongsTo));
        assert_eq!(child.relations[0].table, "parents");
    }

    #[test]
    fn test_default_values() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  t:
    table: t
    primary_key: [id]
    columns:
      - name: id
        type: int
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.database.max_connections, 10);
        assert_eq!(config.server.port, 4000);
    }

    #[test]
    fn test_search_indexes() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
search_indexes:
  - name: items_search
    index: items
    type: items
    searchable_fields: [id, name]
    join_fields:
      - name: tags
        index_field: tags
        table: item_tags
        local_field: id
        foreign_field: item_id
        searchable_fields: [tag_name]
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.search_indexes.len(), 1);
        let idx = &config.search_indexes[0];
        assert_eq!(idx.name, "items_search");
        assert_eq!(idx.searchable_fields, vec!["id", "name"]);
        assert_eq!(idx.join_fields.len(), 1);
        assert_eq!(idx.join_fields[0].name, "tags");
    }

    #[test]
    fn test_no_tables_error() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables: {}
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.tables.is_empty());
    }

    #[test]
    fn test_row_filters_parsed() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
      - name: tenant_id
        type: string
    row_filters:
      - column: tenant_id
        from_header: X-Tenant-ID
        auto_set: true
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let table = config.tables.get("items").unwrap();
        assert_eq!(table.row_filters.len(), 1);
        let rf = &table.row_filters[0];
        assert_eq!(rf.column(), Some("tenant_id"));
        assert_eq!(rf.header_name(), "X-Tenant-ID");
        assert!(rf.is_auto_set());
    }

    #[test]
    fn test_row_filters_default_auto_set() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
      - name: tenant_id
        type: string
    row_filters:
      - column: tenant_id
        from_header: X-Tenant-ID
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.tables.get("items").unwrap().row_filters[0].is_auto_set());
    }

    #[test]
    fn test_row_filters_explicit_disable_auto_set() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
      - name: tenant_id
        type: string
    row_filters:
      - column: tenant_id
        from_header: X-Tenant-ID
        auto_set: false
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.tables.get("items").unwrap().row_filters[0].is_auto_set());
    }

    #[test]
    fn test_row_filters_empty_by_default() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.tables.get("items").unwrap().row_filters.is_empty());
    }

    #[test]
    fn test_subquery_row_filter() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  items:
    table: items
    primary_key: [id]
    columns:
      - name: id
        type: int
    row_filters:
      - type: subquery
        from_header: X-User-ID
        columns: [tenant_id, region]
        match_columns: [tenant_id, region]
        from_source: user_permissions
        user_column: user_id
        cache_ttl_secs: 60
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let rf = &config.tables.get("items").unwrap().row_filters[0];
        match rf {
            RowFilterConfig::SubqueryFilter { columns, match_columns, from_source, user_column, from_header, .. } => {
                assert_eq!(columns, &vec!["tenant_id".to_string(), "region".to_string()]);
                assert_eq!(match_columns, &vec!["tenant_id".to_string(), "region".to_string()]);
                assert_eq!(from_source, "user_permissions");
                assert_eq!(user_column, "user_id");
                assert_eq!(from_header, "X-User-ID");
            }
            _ => panic!("expected SubqueryFilter variant"),
        }
        assert_eq!(rf.header_name(), "X-User-ID");
        assert!(!rf.is_auto_set());
        assert_eq!(rf.column(), None);
    }

    #[test]
    fn test_composite_pk_rejected_at_runtime_not_parse() {
        let yaml = r#"
database:
  url: "postgres://localhost/test"
server:
  host: "0.0.0.0"
tables:
  t:
    table: t
    primary_key: [pk1, pk2]
    columns:
      - name: pk1
        type: string
      - name: pk2
        type: string
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let table = config.tables.get("t").unwrap();
        assert_eq!(table.primary_key.len(), 2);
    }
}
