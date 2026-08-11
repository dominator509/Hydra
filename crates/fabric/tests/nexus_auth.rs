use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use fabric::{
    CorrelationContext, ExternalBindingResolver, FabricError, NexusOidcConfig, OidcAuthenticator,
    OidcKeySource, PrincipalType, ResolvedExternalBinding, Scope,
};
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

const ISSUER: &str = "https://issuer.nexus.test";
const AUDIENCE: &str = "hydra-api";
const KEY_ID: &str = "nexus-test-key";
const PUBLIC_KEY_PEM: &[u8] = br#"-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAA6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=
-----END PUBLIC KEY-----
"#;
const PRIVATE_KEY_DER: &[u8] = &[
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
const OTHER_PRIVATE_KEY_DER: &[u8] = &[
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

#[derive(Clone)]
struct FakeBindingResolver {
    external_tenant_id: String,
    external_business_id: String,
    binding: Option<ResolvedExternalBinding>,
}

#[async_trait]
impl ExternalBindingResolver for FakeBindingResolver {
    async fn resolve(
        &self,
        provider: &str,
        external_tenant_id: &str,
        external_business_id: &str,
    ) -> Result<Option<ResolvedExternalBinding>, FabricError> {
        if provider == "nexus"
            && external_tenant_id == self.external_tenant_id
            && external_business_id == self.external_business_id
        {
            Ok(self.binding)
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct TestClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
    scope: String,
    principal_type: String,
    external_tenant_id: String,
    external_business_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    delegated_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    acr: Option<String>,
}

#[tokio::test]
async fn nexus_auth_anonymous_request_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let authenticator = pinned_authenticator(true)?;
    let result = authenticator.authenticate(None, correlation()).await;
    assert!(matches!(result, Err(FabricError::AuthnFailed(_))));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_expired_token_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let authenticator = pinned_authenticator(true)?;
    let mut claims = valid_claims();
    claims.exp = now().saturating_sub(120);
    claims.iat = claims.exp.saturating_sub(60);
    claims.nbf = claims.iat;

    let result = authenticator
        .authenticate(
            Some(&bearer(sign(&claims, PRIVATE_KEY_DER)?)),
            correlation(),
        )
        .await;
    assert!(matches!(result, Err(FabricError::AuthnFailed(_))));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_future_nbf_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let authenticator = pinned_authenticator(true)?;
    let mut claims = valid_claims();
    claims.nbf = now() + 300;

    let result = authenticator
        .authenticate(
            Some(&bearer(sign(&claims, PRIVATE_KEY_DER)?)),
            correlation(),
        )
        .await;
    assert!(matches!(result, Err(FabricError::AuthnFailed(_))));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_wrong_issuer_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let authenticator = pinned_authenticator(true)?;
    let mut claims = valid_claims();
    claims.iss = "https://attacker.invalid".to_owned();

    let result = authenticator
        .authenticate(
            Some(&bearer(sign(&claims, PRIVATE_KEY_DER)?)),
            correlation(),
        )
        .await;
    assert!(matches!(result, Err(FabricError::AuthnFailed(_))));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_wrong_audience_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let authenticator = pinned_authenticator(true)?;
    let mut claims = valid_claims();
    claims.aud = "some-other-resource".to_owned();

    let result = authenticator
        .authenticate(
            Some(&bearer(sign(&claims, PRIVATE_KEY_DER)?)),
            correlation(),
        )
        .await;
    assert!(matches!(result, Err(FabricError::AuthnFailed(_))));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_invalid_signature_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let authenticator = pinned_authenticator(true)?;
    let token = sign(&valid_claims(), OTHER_PRIVATE_KEY_DER)?;
    let result = authenticator
        .authenticate(Some(&bearer(token)), correlation())
        .await;
    assert!(matches!(result, Err(FabricError::AuthnFailed(_))));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_disabled_business_binding_is_rejected() -> Result<(), Box<dyn std::error::Error>>
{
    let authenticator = pinned_authenticator(false)?;
    let result = authenticator
        .authenticate(
            Some(&bearer(sign(&valid_claims(), PRIVATE_KEY_DER)?)),
            correlation(),
        )
        .await;
    assert!(matches!(result, Err(FabricError::AuthzDenied)));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_cross_business_access_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let authenticator = pinned_authenticator(true)?;
    let mut claims = valid_claims();
    claims.external_business_id = "business-b".to_owned();

    let result = authenticator
        .authenticate(
            Some(&bearer(sign(&claims, PRIVATE_KEY_DER)?)),
            correlation(),
        )
        .await;
    assert!(matches!(result, Err(FabricError::AuthzDenied)));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_internal_principal_claim_is_rejected() -> Result<(), Box<dyn std::error::Error>>
{
    let authenticator = pinned_authenticator(true)?;
    let mut claims = valid_claims();
    claims.principal_type = "hydra_internal_agent".to_owned();

    let result = authenticator
        .authenticate(
            Some(&bearer(sign(&claims, PRIVATE_KEY_DER)?)),
            correlation(),
        )
        .await;
    assert!(matches!(result, Err(FabricError::AuthnFailed(_))));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_unknown_scope_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let authenticator = pinned_authenticator(true)?;
    let mut claims = valid_claims();
    claims.scope.push_str(" hydra.unknown");

    let result = authenticator
        .authenticate(
            Some(&bearer(sign(&claims, PRIVATE_KEY_DER)?)),
            correlation(),
        )
        .await;
    assert!(matches!(result, Err(FabricError::AuthnFailed(_))));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_valid_service_is_bound_only_to_hydra_mapping(
) -> Result<(), Box<dyn std::error::Error>> {
    let hydra_tenant_id = Uuid::new_v4();
    let binding_id = Uuid::new_v4();
    let authenticator = authenticator(
        OidcKeySource::PinnedPublicKey(PUBLIC_KEY_PEM.to_vec()),
        FakeBindingResolver {
            external_tenant_id: "nexus-tenant-a".to_owned(),
            external_business_id: "business-a".to_owned(),
            binding: Some(ResolvedExternalBinding {
                id: binding_id,
                hydra_tenant_id,
                active: true,
            }),
        },
    )?;

    let principal = authenticator
        .authenticate(
            Some(&bearer(sign(&valid_claims(), PRIVATE_KEY_DER)?)),
            correlation(),
        )
        .await?;
    assert_eq!(principal.principal_type, PrincipalType::NexusService);
    assert_eq!(principal.hydra_tenant_id, hydra_tenant_id);
    assert_eq!(principal.binding_id, Some(binding_id));
    assert_eq!(
        principal.external_business_id.as_deref(),
        Some("business-a")
    );
    assert!(principal.scopes.contains(&Scope::CapabilitiesRead));
    assert!(principal.scopes.contains(&Scope::CrmRead));
    assert_eq!(principal.token_id.as_deref(), Some("token-1"));
    Ok(())
}

#[tokio::test]
async fn nexus_auth_jwks_are_cached_between_valid_requests(
) -> Result<(), Box<dyn std::error::Error>> {
    let jwk: Jwk = serde_json::from_value(serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": "A6EHv_POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg",
        "alg": "EdDSA",
        "kid": KEY_ID
    }))?;
    let requests = Arc::new(AtomicUsize::new(0));
    let state = JwksState {
        set: JwkSet { keys: vec![jwk] },
        requests: requests.clone(),
    };
    let app = Router::new()
        .route("/jwks", get(serve_jwks))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    let authenticator = authenticator(
        OidcKeySource::JwksUrl(format!("http://{address}/jwks")),
        active_resolver(),
    )?;
    let authorization = bearer(sign(&valid_claims(), PRIVATE_KEY_DER)?);
    authenticator
        .authenticate(Some(&authorization), correlation())
        .await?;
    authenticator
        .authenticate(Some(&authorization), correlation())
        .await?;

    assert_eq!(requests.load(Ordering::SeqCst), 1);
    server.abort();
    Ok(())
}

#[derive(Clone)]
struct JwksState {
    set: JwkSet,
    requests: Arc<AtomicUsize>,
}

async fn serve_jwks(State(state): State<JwksState>) -> Json<JwkSet> {
    state.requests.fetch_add(1, Ordering::SeqCst);
    Json(state.set)
}

fn pinned_authenticator(active: bool) -> Result<OidcAuthenticator, FabricError> {
    authenticator(
        OidcKeySource::PinnedPublicKey(PUBLIC_KEY_PEM.to_vec()),
        FakeBindingResolver {
            external_tenant_id: "nexus-tenant-a".to_owned(),
            external_business_id: "business-a".to_owned(),
            binding: Some(ResolvedExternalBinding {
                id: Uuid::new_v4(),
                hydra_tenant_id: Uuid::new_v4(),
                active,
            }),
        },
    )
}

fn active_resolver() -> FakeBindingResolver {
    FakeBindingResolver {
        external_tenant_id: "nexus-tenant-a".to_owned(),
        external_business_id: "business-a".to_owned(),
        binding: Some(ResolvedExternalBinding {
            id: Uuid::new_v4(),
            hydra_tenant_id: Uuid::new_v4(),
            active: true,
        }),
    }
}

fn authenticator(
    key_source: OidcKeySource,
    resolver: FakeBindingResolver,
) -> Result<OidcAuthenticator, FabricError> {
    OidcAuthenticator::new(
        NexusOidcConfig {
            provider: "nexus".to_owned(),
            issuer: ISSUER.to_owned(),
            audience: AUDIENCE.to_owned(),
            key_source,
            allowed_algorithms: vec![Algorithm::EdDSA],
            jwks_cache_ttl: Duration::from_secs(300),
            clock_skew: Duration::from_secs(5),
        },
        Arc::new(resolver),
    )
}

fn valid_claims() -> TestClaims {
    let now = now();
    TestClaims {
        iss: ISSUER.to_owned(),
        sub: "nexus-service:crm-orchestrator".to_owned(),
        aud: AUDIENCE.to_owned(),
        exp: now + 300,
        nbf: now.saturating_sub(5),
        iat: now,
        jti: "token-1".to_owned(),
        scope: "hydra.capabilities.read hydra.crm.read".to_owned(),
        principal_type: "nexus_service".to_owned(),
        external_tenant_id: "nexus-tenant-a".to_owned(),
        external_business_id: "business-a".to_owned(),
        delegated_by: None,
        acr: Some("urn:nexus:acr:service".to_owned()),
    }
}

fn sign(claims: &TestClaims, private_key: &[u8]) -> Result<String, jsonwebtoken::errors::Error> {
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(KEY_ID.to_owned());
    encode(&header, claims, &EncodingKey::from_ed_der(private_key))
}

fn bearer(token: String) -> String {
    format!("Bearer {token}")
}

fn correlation() -> CorrelationContext {
    CorrelationContext {
        request_id: Uuid::new_v4().to_string(),
        correlation_id: "correlation-1".to_owned(),
        causation_id: None,
    }
}

fn now() -> u64 {
    u64::try_from(OffsetDateTime::now_utc().unix_timestamp()).unwrap_or_default()
}
