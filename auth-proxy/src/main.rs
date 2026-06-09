mod config;

use std::sync::Arc;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use jsonwebtoken::jwk::{JwkSet, PublicKeyUse};
use pingora::proxy::{ProxyHttp, Session};
use pingora::server::Server;
use pingora::upstreams::peer::HttpPeer;
use serde::Deserialize;
use tracing::info;

use crate::config::ProxyConfig;

#[derive(Debug, Deserialize)]
struct Claims {
    #[serde(default)]
    sub: String,
    #[serde(flatten)]
    extra: std::collections::HashMap<String, serde_json::Value>,
}

struct AuthProxy {
    config: Arc<ProxyConfig>,
    decoding_key: Option<DecodingKey>,
    jwks_keys: Vec<DecodingKey>,
}

impl AuthProxy {
    fn header_value_from_claims(claims: &Claims, claim_name: &str) -> Option<String> {
        if claim_name == "sub" {
            return Some(claims.sub.clone());
        }
        claims
            .extra
            .get(claim_name)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

#[async_trait::async_trait]
impl ProxyHttp for AuthProxy {
    type CTX = ();

    fn new_ctx(&self) {}

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut (),
    ) -> pingora::Result<Box<HttpPeer>> {
        let addr = self
            .config
            .upstream
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let addr = match addr.find('/') {
            Some(pos) => &addr[..pos],
            None => addr,
        };
        Ok(Box::new(HttpPeer::new(addr, false, "".to_string())))
    }

    async fn request_filter(&self, session: &mut Session, _ctx: &mut ()) -> pingora::Result<bool> {
        let auth_header = session
            .req_header()
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());

        let token = match auth_header {
            Some(h) if h.starts_with("Bearer ") => h.trim_start_matches("Bearer ").trim(),
            _ => {
                if self.config.require_auth {
                    info!("Missing or invalid Authorization header");
                    session.respond_error(401).await?;
                    return Ok(true);
                }
                return Ok(false);
            }
        };

        let claims = if !self.jwks_keys.is_empty() {
            let algorithm = Algorithm::RS256;
            let mut validation = Validation::new(algorithm);
            if !self.config.jwt_issuer.is_empty() {
                validation.set_issuer(&[&self.config.jwt_issuer]);
            }
            validation.validate_exp = false;
            validation.validate_aud = false;
            validation.required_spec_claims.clear();

            let mut result = None;
            for key in &self.jwks_keys {
                if let Ok(data) = decode::<Claims>(token, key, &validation) {
                    result = Some(data.claims);
                    break;
                }
            }
            match result {
                Some(c) => c,
                None => {
                    info!("JWT validation failed with all JWKS keys");
                    session.respond_error(401).await?;
                    return Ok(true);
                }
            }
        } else if let Some(ref key) = self.decoding_key {
            let mut validation = Validation::new(Algorithm::HS256);
            validation.validate_exp = false;
            validation.required_spec_claims.clear();
            match decode::<Claims>(token, key, &validation) {
                Ok(data) => data.claims,
                Err(e) => {
                    info!("JWT validation failed: {}", e);
                    session.respond_error(401).await?;
                    return Ok(true);
                }
            }
        } else {
            info!("No JWT validation keys configured");
            session.respond_error(500).await?;
            return Ok(true);
        };

        for mapping in &self.config.header_mappings {
            if let Some(val) = Self::header_value_from_claims(&claims, &mapping.claim) {
                let name = mapping.header.clone();
                let _ = session.req_header_mut().insert_header(name, val);
            }
        }

        Ok(false)
    }
}

fn fetch_jwks(url: &str) -> anyhow::Result<Vec<DecodingKey>> {
    let resp = ureq::get(url).call()?;
    let jwk_set: JwkSet = resp.into_body().read_json()?;
    let mut keys = Vec::new();
    for jwk in &jwk_set.keys {
        // Only use signing keys, skip encryption keys
        if let Some(ref use_val) = jwk.common.public_key_use {
            if !matches!(use_val, PublicKeyUse::Signature) {
                info!("Skipping JWK (kid: {:?}) with non-signature use", jwk.common.key_id);
                continue;
            }
        }
        match DecodingKey::from_jwk(jwk) {
            Ok(key) => keys.push(key),
            Err(e) => {
                info!("Skipping JWK (kid: {:?}): {}", jwk.common.key_id, e);
            }
        }
    }
    if keys.is_empty() {
        anyhow::bail!("No usable JWKS keys found from {}", url);
    }
    info!("Loaded {} JWKS keys from {}", keys.len(), url);
    Ok(keys)
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "auth_proxy=info".into()),
        )
        .init();

    let config_path =
        std::env::var("AUTH_PROXY_CONFIG").unwrap_or_else(|_| "config.yaml".to_string());
    let config = Arc::new(ProxyConfig::from_file(&config_path)?);

    info!(
        "Auth proxy starting on {} -> {}",
        config.listen_addr, config.upstream
    );

    let (decoding_key, jwks_keys) = if !config.jwt_jwks_url.is_empty() {
        let keys = fetch_jwks(&config.jwt_jwks_url)?;
        (None, keys)
    } else if !config.jwt_secret.is_empty() {
        (
            Some(DecodingKey::from_secret(config.jwt_secret.as_bytes())),
            Vec::new(),
        )
    } else {
        anyhow::bail!("Either jwt_secret or jwt_jwks_url must be configured");
    };

    let mut server = Server::new(None)?;
    server.bootstrap();

    let mut proxy_service = pingora::proxy::http_proxy_service(
        &server.configuration,
        AuthProxy {
            config: config.clone(),
            decoding_key,
            jwks_keys,
        },
    );
    proxy_service.add_tcp(&config.listen_addr);

    server.add_service(proxy_service);

    info!("Auth proxy ready");
    server.run_forever();
}
