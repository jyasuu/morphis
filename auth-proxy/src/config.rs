use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    pub listen_addr: String,
    pub upstream: String,
    #[serde(default)]
    pub jwt_secret: String,
    #[serde(default)]
    pub jwt_jwks_url: String,
    #[serde(default)]
    pub jwt_issuer: String,
    #[serde(default = "default_require_auth")]
    pub require_auth: bool,
    pub header_mappings: Vec<HeaderMapping>,
}

fn default_require_auth() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeaderMapping {
    pub claim: String,
    pub header: String,
}

impl ProxyConfig {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: ProxyConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
