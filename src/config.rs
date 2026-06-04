use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub tables: HashMap<String, TableConfig>,
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
pub struct TableConfig {
    pub table: String,
    pub columns: Vec<ColumnConfig>,
    pub primary_key: Vec<String>,
    #[serde(default)]
    pub relations: Vec<RelationConfig>,
    #[serde(default)]
    pub permissions: PermissionsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: ColumnType,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    HasMany,
    HasOne,
    BelongsTo,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PermissionsConfig {
    #[serde(default)]
    pub query: Vec<String>,
    #[serde(default)]
    pub create: Vec<String>,
    #[serde(default)]
    pub update: Vec<String>,
    #[serde(default)]
    pub delete: Vec<String>,
}

impl Config {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
