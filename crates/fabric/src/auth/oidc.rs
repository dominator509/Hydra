use std::collections::BTreeSet;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use jsonwebtoken::jwk::{Jwk, JwkSet, KeyAlgorithm};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use super::{CorrelationContext, PrincipalContext, PrincipalType, Scope};
use crate::FabricError;

const MAX_JWKS_BYTES: usize = 1024 * 1024;
const MAX_JWKS_KEYS: usize = 64;
const INVALID_TOKEN: &str = "invalid bearer token";

#[derive(Debug, Clone)]
pub enum OidcKeySource {
    JwksUrl(String),
    PinnedPublicKey(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct NexusOidcConfig {
    pub provider: String,
    pub issuer: String,
    pub audience: String,
    pub key_source: OidcKeySource,
    pub allowed_algorithms: Vec<Algorithm>,
    pub jwks_cache_ttl: Duration,
    pub clock_skew: Duration,
    pub egress_proxy_url: Option<String>,
}

impl NexusOidcConfig {
    fn validate(&self) -> Result<(), FabricError> {
        for (name, value) in [
            ("provider", self.provider.as_str()),
            ("issuer", self.issuer.as_str()),
            ("audience", self.audience.as_str()),
        ] {
            if value.trim().is_empty()
                || (name == "provider" && !store::is_valid_external_binding_text(value))
            {
                return Err(FabricError::Internal(format!(
                    "Nexus OIDC {name} cannot be empty"
                )));
            }
        }
        if self.allowed_algorithms.is_empty()
            || self
                .allowed_algorithms
                .iter()
                .any(|algorithm| !is_asymmetric(*algorithm))
        {
            return Err(FabricError::Internal(
                "Nexus OIDC requires at least one asymmetric signing algorithm".to_owned(),
            ));
        }
        if self.jwks_cache_ttl.is_zero() {
            return Err(FabricError::Internal(
                "Nexus OIDC JWKS cache duration must be positive".to_owned(),
            ));
        }
        match &self.key_source {
            OidcKeySource::JwksUrl(url) if url.trim().is_empty() => Err(FabricError::Internal(
                "Nexus OIDC JWKS URL cannot be empty".to_owned(),
            )),
            OidcKeySource::PinnedPublicKey(key) if key.is_empty() => Err(FabricError::Internal(
                "Nexus OIDC pinned public key cannot be empty".to_owned(),
            )),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedExternalBinding {
    pub id: Uuid,
    pub hydra_tenant_id: Uuid,
    pub active: bool,
}

#[async_trait]
pub trait ExternalBindingResolver: Send + Sync {
    async fn resolve(
        &self,
        provider: &str,
        external_tenant_id: &str,
        external_business_id: &str,
    ) -> Result<Option<ResolvedExternalBinding>, FabricError>;
}

#[async_trait]
impl ExternalBindingResolver for store::ExternalBindingsRepo {
    async fn resolve(
        &self,
        provider: &str,
        external_tenant_id: &str,
        external_business_id: &str,
    ) -> Result<Option<ResolvedExternalBinding>, FabricError> {
        let binding = store::ExternalBindingsRepo::resolve(
            self,
            provider,
            external_tenant_id,
            external_business_id,
        )
        .await?;

        Ok(binding.map(|binding| ResolvedExternalBinding {
            id: binding.id,
            hydra_tenant_id: binding.hydra_tenant_id,
            active: binding.status == store::BindingStatus::Active,
        }))
    }
}

#[derive(Debug, Clone)]
struct CachedJwks {
    fetched_at: Instant,
    set: JwkSet,
}

#[derive(Clone)]
pub struct OidcAuthenticator {
    config: NexusOidcConfig,
    bindings: Arc<dyn ExternalBindingResolver>,
    client: reqwest::Client,
    jwks: Arc<RwLock<Option<CachedJwks>>>,
    refresh: Arc<Mutex<()>>,
}

impl OidcAuthenticator {
    pub fn new(
        config: NexusOidcConfig,
        bindings: Arc<dyn ExternalBindingResolver>,
    ) -> Result<Self, FabricError> {
        config.validate()?;
        let client = build_oidc_client(config.egress_proxy_url.as_deref())?;

        Ok(Self {
            config,
            bindings,
            client,
            jwks: Arc::new(RwLock::new(None)),
            refresh: Arc::new(Mutex::new(())),
        })
    }

    pub async fn authenticate(
        &self,
        authorization: Option<&str>,
        correlation: CorrelationContext,
    ) -> Result<PrincipalContext, FabricError> {
        let token = bearer_token(authorization)?;
        let header = decode_header(token).map_err(|_| authentication_failed())?;
        if !self.config.allowed_algorithms.contains(&header.alg) || !is_asymmetric(header.alg) {
            return Err(authentication_failed());
        }

        let key = self.decoding_key(header.alg, header.kid.as_deref()).await?;
        let mut validation = Validation::new(header.alg);
        validation.algorithms = self.config.allowed_algorithms.clone();
        validation.leeway = self.config.clock_skew.as_secs();
        validation.validate_nbf = true;
        validation.set_audience(&[self.config.audience.as_str()]);
        validation.set_issuer(&[self.config.issuer.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);

        let claims = decode::<ExternalClaims>(token, &key, &validation)
            .map_err(|_| authentication_failed())?
            .claims;
        self.principal_from_claims(claims, correlation).await
    }

    async fn principal_from_claims(
        &self,
        claims: ExternalClaims,
        correlation: CorrelationContext,
    ) -> Result<PrincipalContext, FabricError> {
        if claims.sub.trim().is_empty()
            || claims.iss != self.config.issuer
            || !claims.aud.contains(&self.config.audience)
            || claims.exp == 0
            || !store::is_valid_external_binding_text(&claims.external_tenant_id)
            || !store::is_valid_external_binding_text(&claims.external_business_id)
            || claims
                .jti
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
            || claims
                .delegated_by
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
            || claims
                .authentication_strength
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
        {
            return Err(authentication_failed());
        }

        let principal_type =
            PrincipalType::from_str(&claims.principal_type).map_err(|_| authentication_failed())?;
        if !matches!(
            principal_type,
            PrincipalType::Human | PrincipalType::NexusService | PrincipalType::NexusAgent
        ) {
            return Err(authentication_failed());
        }

        let scopes = parse_scopes(&claims.scope)?;
        let binding = self
            .bindings
            .resolve(
                &self.config.provider,
                &claims.external_tenant_id,
                &claims.external_business_id,
            )
            .await?
            .filter(|binding| binding.active && !binding.hydra_tenant_id.is_nil())
            .ok_or(FabricError::AuthzDenied)?;

        Ok(PrincipalContext {
            principal_id: claims.sub,
            principal_type,
            hydra_tenant_id: binding.hydra_tenant_id,
            external_provider: Some(self.config.provider.clone()),
            external_tenant_id: Some(claims.external_tenant_id),
            external_business_id: Some(claims.external_business_id),
            scopes,
            delegated_by: claims.delegated_by,
            authentication_strength: claims.authentication_strength,
            token_id: claims.jti,
            correlation,
            binding_id: Some(binding.id),
            trace: store::TraceContext::fresh(),
        })
    }

    async fn decoding_key(
        &self,
        algorithm: Algorithm,
        kid: Option<&str>,
    ) -> Result<DecodingKey, FabricError> {
        match &self.config.key_source {
            OidcKeySource::PinnedPublicKey(key) => decoding_key_from_pem(key, algorithm),
            OidcKeySource::JwksUrl(url) => {
                let kid = kid.ok_or_else(authentication_failed)?;
                let jwk = self.jwk(url, kid).await?;
                if let Some(key_algorithm) = jwk.common.key_algorithm {
                    if !key_algorithm_matches(key_algorithm, algorithm) {
                        return Err(authentication_failed());
                    }
                }
                DecodingKey::from_jwk(&jwk).map_err(|_| authentication_failed())
            }
        }
    }

    async fn jwk(&self, url: &str, kid: &str) -> Result<Jwk, FabricError> {
        if let Some(result) = self.fresh_cached_jwk(kid).await {
            return result;
        }

        let _refresh = self.refresh.lock().await;
        if let Some(result) = self.fresh_cached_jwk(kid).await {
            return result;
        }

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| authentication_failed())?
            .error_for_status()
            .map_err(|_| authentication_failed())?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_JWKS_BYTES as u64)
        {
            return Err(authentication_failed());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| authentication_failed())?;
        if bytes.len() > MAX_JWKS_BYTES {
            return Err(authentication_failed());
        }
        let set: JwkSet = serde_json::from_slice(&bytes).map_err(|_| authentication_failed())?;
        if set.keys.is_empty() || set.keys.len() > MAX_JWKS_KEYS {
            return Err(authentication_failed());
        }
        let key = set.find(kid).cloned().ok_or_else(authentication_failed)?;
        *self.jwks.write().await = Some(CachedJwks {
            fetched_at: Instant::now(),
            set,
        });
        Ok(key)
    }

    async fn fresh_cached_jwk(&self, kid: &str) -> Option<Result<Jwk, FabricError>> {
        let cache = self.jwks.read().await;
        cache.as_ref().and_then(|cached| {
            (cached.fetched_at.elapsed() < self.config.jwks_cache_ttl).then(|| {
                cached
                    .set
                    .find(kid)
                    .cloned()
                    .ok_or_else(authentication_failed)
            })
        })
    }
}

#[derive(Debug, Deserialize)]
struct ExternalClaims {
    iss: String,
    sub: String,
    aud: AudienceClaim,
    exp: u64,
    #[serde(default)]
    jti: Option<String>,
    #[serde(default)]
    scope: String,
    principal_type: String,
    external_tenant_id: String,
    external_business_id: String,
    #[serde(default)]
    delegated_by: Option<String>,
    #[serde(default, rename = "acr")]
    authentication_strength: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AudienceClaim {
    One(String),
    Many(Vec<String>),
}

impl AudienceClaim {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Self::One(value) => value == expected,
            Self::Many(values) => values.iter().any(|value| value == expected),
        }
    }
}

pub fn parse_algorithm(value: &str) -> Result<Algorithm, FabricError> {
    match value {
        "RS256" => Ok(Algorithm::RS256),
        "RS384" => Ok(Algorithm::RS384),
        "RS512" => Ok(Algorithm::RS512),
        "PS256" => Ok(Algorithm::PS256),
        "PS384" => Ok(Algorithm::PS384),
        "PS512" => Ok(Algorithm::PS512),
        "ES256" => Ok(Algorithm::ES256),
        "ES384" => Ok(Algorithm::ES384),
        "EdDSA" => Ok(Algorithm::EdDSA),
        _ => Err(FabricError::Internal(format!(
            "unsupported Nexus OIDC signing algorithm '{value}'"
        ))),
    }
}

fn parse_scopes(value: &str) -> Result<BTreeSet<Scope>, FabricError> {
    value
        .split_ascii_whitespace()
        .map(|scope| Scope::from_str(scope).map_err(|_| authentication_failed()))
        .collect()
}

fn bearer_token(authorization: Option<&str>) -> Result<&str, FabricError> {
    let authorization = authorization.ok_or_else(authentication_failed)?;
    let (scheme, token) = authorization
        .split_once(' ')
        .ok_or_else(authentication_failed)?;
    if !scheme.eq_ignore_ascii_case("Bearer") || token.trim().is_empty() || token != token.trim() {
        return Err(authentication_failed());
    }
    Ok(token)
}

fn decoding_key_from_pem(key: &[u8], algorithm: Algorithm) -> Result<DecodingKey, FabricError> {
    let result = match algorithm {
        Algorithm::RS256
        | Algorithm::RS384
        | Algorithm::RS512
        | Algorithm::PS256
        | Algorithm::PS384
        | Algorithm::PS512 => DecodingKey::from_rsa_pem(key),
        Algorithm::ES256 | Algorithm::ES384 => DecodingKey::from_ec_pem(key),
        Algorithm::EdDSA => DecodingKey::from_ed_pem(key),
        _ => return Err(authentication_failed()),
    };
    result.map_err(|_| authentication_failed())
}

fn is_asymmetric(algorithm: Algorithm) -> bool {
    matches!(
        algorithm,
        Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::PS256
            | Algorithm::PS384
            | Algorithm::PS512
            | Algorithm::ES256
            | Algorithm::ES384
            | Algorithm::EdDSA
    )
}

fn key_algorithm_matches(key_algorithm: KeyAlgorithm, algorithm: Algorithm) -> bool {
    matches!(
        (key_algorithm, algorithm),
        (KeyAlgorithm::RS256, Algorithm::RS256)
            | (KeyAlgorithm::RS384, Algorithm::RS384)
            | (KeyAlgorithm::RS512, Algorithm::RS512)
            | (KeyAlgorithm::PS256, Algorithm::PS256)
            | (KeyAlgorithm::PS384, Algorithm::PS384)
            | (KeyAlgorithm::PS512, Algorithm::PS512)
            | (KeyAlgorithm::ES256, Algorithm::ES256)
            | (KeyAlgorithm::ES384, Algorithm::ES384)
            | (KeyAlgorithm::EdDSA, Algorithm::EdDSA)
    )
}

fn authentication_failed() -> FabricError {
    FabricError::AuthnFailed(INVALID_TOKEN.to_owned())
}

fn build_oidc_client(proxy_url: Option<&str>) -> Result<reqwest::Client, FabricError> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5));
    if let Some(proxy_url) = proxy_url {
        let proxy = reqwest::Proxy::all(proxy_url)
            .map_err(|_| FabricError::Internal("invalid egress proxy URL".to_owned()))?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|_| FabricError::Internal("build OIDC client failed".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nexus_auth_rejects_symmetric_algorithm_configuration() {
        let config = NexusOidcConfig {
            provider: "nexus".to_owned(),
            issuer: "https://issuer.example".to_owned(),
            audience: "hydra".to_owned(),
            key_source: OidcKeySource::PinnedPublicKey(b"unused".to_vec()),
            allowed_algorithms: vec![Algorithm::HS256],
            jwks_cache_ttl: Duration::from_secs(300),
            clock_skew: Duration::from_secs(30),
            egress_proxy_url: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn nexus_auth_algorithm_parser_exposes_only_asymmetric_algorithms() {
        assert_eq!(parse_algorithm("RS256").ok(), Some(Algorithm::RS256));
        assert_eq!(parse_algorithm("EdDSA").ok(), Some(Algorithm::EdDSA));
        assert!(parse_algorithm("HS256").is_err());
        assert!(parse_algorithm("none").is_err());
    }

    #[test]
    fn oidc_client_rejects_malformed_proxy_without_echoing_it() {
        let error = match build_oidc_client(Some("http://user:secret@[invalid")) {
            Ok(_) => panic!("malformed proxy must fail closed"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            FabricError::Internal(message) if message == "invalid egress proxy URL"
        ));
    }

    #[test]
    fn nexus_auth_rejects_invalid_provider_binding_key() {
        let config = NexusOidcConfig {
            provider: "nexus\nprovider".to_owned(),
            issuer: "https://issuer.example".to_owned(),
            audience: "hydra".to_owned(),
            key_source: OidcKeySource::PinnedPublicKey(b"unused".to_vec()),
            allowed_algorithms: vec![Algorithm::RS256],
            jwks_cache_ttl: Duration::from_secs(300),
            clock_skew: Duration::from_secs(30),
            egress_proxy_url: None,
        };
        assert!(config.validate().is_err());
    }
}
