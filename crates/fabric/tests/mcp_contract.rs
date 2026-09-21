use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fabric::services::{
    AppState, BridgeRegisterRequest, BridgeService, BridgeStatusDto, ConciergeServiceImpl,
    EntityDeleteResponse, EntityService, EnvelopeCreateRequest, EnvelopeService,
    NexusControlPlaneConfig, StoreAutonomyService, StoreTkStatsService,
};
use fabric::{
    app, AuthCtx, ExternalBindingResolver, FabricError, NexusOidcConfig, OidcAuthenticator,
    OidcKeySource, ResolvedExternalBinding,
};
use governor::{ActionEnvelope, EnvelopeState};
use jsonschema::{Draft, JSONSchema};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;
use uuid::Uuid;

const ISSUER: &str = "https://issuer.nexus.test";
const AUDIENCE: &str = "hydra-api";
const KEY_ID: &str = "nexus-contract-key";
const ORIGIN: &str = "https://nexus.test";
const PUBLIC_KEY_PEM: &[u8] = br#"-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAA6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=
-----END PUBLIC KEY-----
"#;
const PRIVATE_KEY_DER: &[u8] = &[
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];

#[derive(Clone)]
struct FakeBindingResolver {
    binding_id: Uuid,
    hydra_tenant_id: Uuid,
}

#[async_trait]
impl ExternalBindingResolver for FakeBindingResolver {
    async fn resolve(
        &self,
        provider: &str,
        external_tenant_id: &str,
        external_business_id: &str,
    ) -> Result<Option<ResolvedExternalBinding>, FabricError> {
        Ok((provider == "nexus"
            && external_tenant_id == "nexus-tenant-a"
            && external_business_id == "business-a")
            .then_some(ResolvedExternalBinding {
                id: self.binding_id,
                hydra_tenant_id: self.hydra_tenant_id,
                active: true,
            }))
    }
}

#[derive(Clone)]
struct FakeEntityService {
    entities: Arc<Vec<cdm::Entity>>,
}

#[async_trait]
impl EntityService for FakeEntityService {
    async fn list(
        &self,
        tenant: Uuid,
        kind: &str,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> Result<Vec<cdm::Entity>, FabricError> {
        Ok(self
            .entities
            .iter()
            .filter(|entity| entity.tenant == tenant && entity.kind == kind)
            .filter(|entity| cursor.is_none_or(|cursor| entity.id > cursor))
            .take(usize::from(limit))
            .cloned()
            .collect())
    }

    async fn get(&self, tenant: Uuid, kind: &str, id: Uuid) -> Result<cdm::Entity, FabricError> {
        self.entities
            .iter()
            .find(|entity| entity.tenant == tenant && entity.kind == kind && entity.id == id)
            .cloned()
            .ok_or(FabricError::NotFound)
    }

    async fn create(
        &self,
        _tenant: Uuid,
        _kind: &str,
        _body: Value,
    ) -> Result<cdm::Entity, FabricError> {
        Err(FabricError::AuthzDenied)
    }

    async fn patch(
        &self,
        _tenant: Uuid,
        _kind: &str,
        _id: Uuid,
        _expected_version: u64,
        _patch: Value,
    ) -> Result<cdm::Entity, FabricError> {
        Err(FabricError::AuthzDenied)
    }

    async fn delete(
        &self,
        _tenant: Uuid,
        _kind: &str,
        _id: Uuid,
    ) -> Result<EntityDeleteResponse, FabricError> {
        Err(FabricError::AuthzDenied)
    }
}

struct FakeEnvelopeService;

struct FakeBridgeService;

#[async_trait]
impl BridgeService for FakeBridgeService {
    async fn register(
        &self,
        _ctx: &AuthCtx,
        _tenant: Uuid,
        _actor: &str,
        _request: BridgeRegisterRequest,
    ) -> Result<ActionEnvelope, FabricError> {
        Err(FabricError::AuthzDenied)
    }

    async fn status(
        &self,
        _tenant: Uuid,
        _adapter_id: &str,
    ) -> Result<BridgeStatusDto, FabricError> {
        Err(FabricError::NotFound)
    }

    async fn list_status(&self, _tenant: Uuid) -> Result<Vec<BridgeStatusDto>, FabricError> {
        Ok(Vec::new())
    }

    async fn pause(
        &self,
        _ctx: &AuthCtx,
        _tenant: Uuid,
        _actor: &str,
        _adapter_id: &str,
    ) -> Result<BridgeStatusDto, FabricError> {
        Err(FabricError::AuthzDenied)
    }

    async fn resume(
        &self,
        _ctx: &AuthCtx,
        _tenant: Uuid,
        _actor: &str,
        _adapter_id: &str,
    ) -> Result<BridgeStatusDto, FabricError> {
        Err(FabricError::AuthzDenied)
    }
}

#[async_trait]
impl EnvelopeService for FakeEnvelopeService {
    async fn list(
        &self,
        _tenant: Uuid,
        _state: EnvelopeState,
    ) -> Result<Vec<ActionEnvelope>, FabricError> {
        Ok(Vec::new())
    }

    async fn propose(
        &self,
        _tenant: Uuid,
        _request: EnvelopeCreateRequest,
    ) -> Result<ActionEnvelope, FabricError> {
        Err(FabricError::AuthzDenied)
    }

    async fn approve(
        &self,
        _ctx: &AuthCtx,
        _tenant: Uuid,
        _id: Uuid,
    ) -> Result<ActionEnvelope, FabricError> {
        Err(FabricError::AuthzDenied)
    }

    async fn reject(
        &self,
        _ctx: &AuthCtx,
        _tenant: Uuid,
        _id: Uuid,
    ) -> Result<ActionEnvelope, FabricError> {
        Err(FabricError::AuthzDenied)
    }
}

#[derive(Debug, Serialize)]
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
    acr: String,
}

struct Harness {
    address: std::net::SocketAddr,
    token: String,
    tenant_a: Uuid,
    tenant_b: Uuid,
    binding_id: Uuid,
    server: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl Harness {
    async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_rate_limit(60).await
    }

    async fn start_with_rate_limit(max_requests: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let tenant_a = Uuid::from_u128(0xaaaaaaaa_aaaa_4aaa_8aaa_aaaaaaaaaaaa);
        let tenant_b = Uuid::from_u128(0xbbbbbbbb_bbbb_4bbb_8bbb_bbbbbbbbbbbb);
        let binding_id = Uuid::from_u128(0xcccccccc_cccc_4ccc_8ccc_cccccccccccc);
        let entities = vec![
            entity(
                1,
                tenant_a,
                "party",
                json!({"display_name": "First nonmatching account"}),
            ),
            entity(
                2,
                tenant_a,
                "party",
                json!({"display_name": "Needle Account"}),
            ),
            entity(
                3,
                tenant_a,
                "deal",
                json!({
                    "title": "Needle Expansion",
                    "pipeline_id": "pipeline-main",
                    "stage_id": "qualified",
                    "amount_cents": 125_000
                }),
            ),
            entity(
                4,
                tenant_b,
                "party",
                json!({"display_name": "Needle from another business"}),
            ),
        ];

        let pool = PgPoolOptions::new().connect_lazy("postgres://hydra:hydra@127.0.0.1:1/hydra")?;
        let store = store::Store::new(pool.clone());
        let authenticator = OidcAuthenticator::new(
            NexusOidcConfig {
                provider: "nexus".to_owned(),
                issuer: ISSUER.to_owned(),
                audience: AUDIENCE.to_owned(),
                key_source: OidcKeySource::PinnedPublicKey(PUBLIC_KEY_PEM.to_vec()),
                allowed_algorithms: vec![Algorithm::EdDSA],
                jwks_cache_ttl: Duration::from_secs(300),
                clock_skew: Duration::from_secs(5),
                egress_proxy_url: None,
            },
            Arc::new(FakeBindingResolver {
                binding_id,
                hydra_tenant_id: tenant_a,
            }),
        )?;
        let state = AppState::new(
            Arc::new(fabric::SessionStore::new(pool)),
            Arc::new(FakeEntityService {
                entities: Arc::new(entities),
            }),
            Arc::new(StoreAutonomyService::new(store.clone())),
            Arc::new(FakeBridgeService),
            Arc::new(FakeEnvelopeService),
            Arc::new(StoreTkStatsService::new(
                store.ledger.clone(),
                vec!["concierge".to_owned()],
            )),
            Arc::new(ConciergeServiceImpl),
        )
        .with_external_auth(Arc::new(authenticator))
        .with_rate_limiter(Arc::new(fabric::rate::RateLimiter::new(max_requests, 60)))
        .with_nexus_control_plane(NexusControlPlaneConfig {
            enabled: true,
            resource: "https://hydra.test/mcp".to_owned(),
            resource_metadata_url: "https://hydra.test/.well-known/oauth-protected-resource/mcp"
                .to_owned(),
            authorization_servers: vec![ISSUER.to_owned()],
            allowed_hosts: vec!["127.0.0.1".to_owned(), "localhost".to_owned()],
            allowed_origins: vec![ORIGIN.to_owned()],
            max_request_body_bytes: 1_048_576,
        });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move { axum::serve(listener, app(state)).await });

        Ok(Self {
            address,
            token: sign_token()?,
            tenant_a,
            tenant_b,
            binding_id,
            server,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    fn stop(self) {
        self.server.abort();
    }
}

#[tokio::test]
async fn mcp_contract_external_rate_limit_is_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start_with_rate_limit(1).await?;
    let client = reqwest::Client::new();
    let first = client
        .get(harness.url("/v1/nexus/capabilities"))
        .bearer_auth(&harness.token)
        .send()
        .await?;
    assert_eq!(first.status(), reqwest::StatusCode::OK);

    let limited = client
        .get(harness.url("/v1/nexus/capabilities"))
        .bearer_auth(&harness.token)
        .send()
        .await?;
    assert_eq!(limited.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers().contains_key(reqwest::header::RETRY_AFTER));

    harness.stop();
    Ok(())
}

#[tokio::test]
async fn mcp_contract_token_endpoint_is_not_an_issuer() -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start().await?;
    let response = reqwest::Client::new()
        .post(harness.url("/oauth/token"))
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;

    assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert!(!body.contains("access_token"));
    assert!(!body.contains("signing_secret"));

    harness.stop();
    Ok(())
}

#[tokio::test]
async fn mcp_contract_authenticates_negotiates_and_enforces_origin(
) -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start().await?;
    let client = reqwest::Client::new();

    let metadata = client
        .get(harness.url("/.well-known/oauth-protected-resource/mcp"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    assert_eq!(metadata["resource"], "https://hydra.test/mcp");
    assert_eq!(metadata["authorization_servers"], json!([ISSUER]));
    assert!(metadata["scopes_supported"]
        .as_array()
        .is_some_and(|scopes| scopes.contains(&json!("hydra.capabilities.read"))));

    let initialize = initialize_request();
    let anonymous = mcp_post(&client, &harness, None, ORIGIN, &initialize).await?;
    assert_eq!(anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);
    let challenge = anonymous
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(challenge.contains("resource_metadata="));
    assert!(challenge.contains("scope=\"hydra.capabilities.read\""));

    let wrong_origin = mcp_post(
        &client,
        &harness,
        Some(&harness.token),
        "https://attacker.invalid",
        &initialize,
    )
    .await?;
    assert_eq!(wrong_origin.status(), reqwest::StatusCode::FORBIDDEN);

    let initialized = json_response(
        mcp_post(&client, &harness, Some(&harness.token), ORIGIN, &initialize).await?,
    )
    .await?;
    assert_eq!(
        initialized["result"]["protocolVersion"],
        fabric::mcp::MCP_PROTOCOL_VERSION
    );

    let get = client
        .get(harness.url("/mcp"))
        .bearer_auth(&harness.token)
        .header(reqwest::header::ORIGIN, ORIGIN)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .send()
        .await?;
    assert!(
        get.status() == reqwest::StatusCode::OK
            || get.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED,
        "authenticated MCP GET must be SSE or explicit 405, got {}",
        get.status()
    );

    harness.stop();
    Ok(())
}

#[tokio::test]
async fn mcp_contract_tools_are_stable_tenant_bound_and_schema_valid(
) -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start().await?;
    let client = reqwest::Client::new();

    let listed = json_response(
        mcp_post(
            &client,
            &harness,
            Some(&harness.token),
            ORIGIN,
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        )
        .await?,
    )
    .await?;
    let tools = listed["result"]["tools"]
        .as_array()
        .expect("MCP tools/list must return tools");
    let expected_names = [
        "hydra.bridges.deploy",
        "hydra.bridges.pause",
        "hydra.bridges.resume",
        "hydra.bridges.sync",
        "hydra.capabilities.list",
        "hydra.crm.context",
        "hydra.crm.get",
        "hydra.crm.pipeline_summary",
        "hydra.crm.propose_action",
        "hydra.crm.search",
        "hydra.crm.timeline",
        "hydra.envelopes.get",
        "hydra.envelopes.list",
    ];
    assert_eq!(
        tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>(),
        expected_names
    );
    assert!(!tools.iter().any(|tool| tool["name"] == "hydra.approve"));

    let local_schema = fabric::mcp::tool_schema();
    assert_eq!(listed["result"]["tools"], local_schema["tools"]);
    let schema_snapshot = json!({
        "protocol": "mcp",
        "version": fabric::mcp::MCP_PROTOCOL_VERSION,
        "tools": listed["result"]["tools"]
    });
    let digest = sha256_json(&schema_snapshot)?;
    println!("mcp schema snapshot sha256: {digest}");
    assert_eq!(
        digest,
        include_str!("fixtures/mcp-tools-2025-11-25.sha256").trim()
    );

    let search = json_response(
        mcp_post_with_tenant_header(
            &client,
            &harness,
            &harness.token,
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "hydra.crm.search",
                    "arguments": {"query": "Needle", "kind": "party", "limit": 10},
                    "_meta": {"x-hydra-tenant": harness.tenant_b}
                }
            }),
            harness.tenant_b,
        )
        .await?,
    )
    .await?;
    let structured = &search["result"]["structuredContent"];
    let items = structured["items"]
        .as_array()
        .expect("search structuredContent must contain items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["tenant"], harness.tenant_a.to_string());
    assert_eq!(items[0]["body"]["display_name"], "Needle Account");

    let search_tool = tools
        .iter()
        .find(|tool| tool["name"] == "hydra.crm.search")
        .expect("search tool descriptor");
    let validator = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&search_tool["outputSchema"])
        .map_err(|error| format!("compile MCP output schema: {error}"))?;
    if let Err(errors) = validator.validate(structured) {
        let messages = errors.map(|error| error.to_string()).collect::<Vec<_>>();
        panic!("structuredContent did not match outputSchema: {messages:?}");
    }

    let alias = json_response(
        mcp_post(
            &client,
            &harness,
            Some(&harness.token),
            ORIGIN,
            &json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "hydra.search_entities",
                    "arguments": {"query": "Needle", "kind": "party"}
                }
            }),
        )
        .await?,
    )
    .await?;
    assert_eq!(alias["result"]["_meta"]["io.hydra/deprecated-alias"], true);
    assert_eq!(
        alias["result"]["structuredContent"]["items"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let proposal = json_response(
        mcp_post(
            &client,
            &harness,
            Some(&harness.token),
            ORIGIN,
            &json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "hydra.crm.propose_action",
                    "arguments": {
                        "deal_id": Uuid::new_v4(),
                        "stage": "won",
                        "rationale": "contract test",
                        "idempotency_key": "contract-key"
                    }
                }
            }),
        )
        .await?,
    )
    .await?;
    assert_eq!(proposal["result"]["isError"], true);
    assert_eq!(
        proposal["result"]["structuredContent"]["error"]["code"],
        "authz_denied"
    );

    harness.stop();
    Ok(())
}

#[tokio::test]
async fn mcp_contract_rest_facade_uses_authenticated_binding(
) -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start().await?;
    let client = reqwest::Client::new();

    let context_response = client
        .get(harness.url("/v1/nexus/context"))
        .bearer_auth(&harness.token)
        .header("x-hydra-tenant", harness.tenant_b.to_string())
        .header("x-request-id", "request-contract-1")
        .header("x-correlation-id", "objective-contract-1")
        .send()
        .await?
        .error_for_status()?;
    assert_eq!(
        context_response
            .headers()
            .get("x-correlation-id")
            .and_then(|value| value.to_str().ok()),
        Some("objective-contract-1")
    );
    let context = context_response.json::<Value>().await?;
    assert_eq!(
        context["tenant"]["hydra_tenant_id"],
        harness.tenant_a.to_string()
    );
    assert_eq!(
        context["tenant"]["binding_id"],
        harness.binding_id.to_string()
    );
    assert_eq!(context["pipeline"]["total_deals"], 1);

    let capabilities = client
        .get(harness.url("/v1/nexus/capabilities"))
        .bearer_auth(&harness.token)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    assert_eq!(
        capabilities["capabilities"].as_array().map(Vec::len),
        Some(13)
    );

    let binding = client
        .get(harness.url("/v1/nexus/bindings"))
        .bearer_auth(&harness.token)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    assert_eq!(binding["bindings"][0]["id"], harness.binding_id.to_string());

    let legacy_direct_write = client
        .post(harness.url("/v1/entities/party"))
        .bearer_auth(&harness.token)
        .json(&json!({"display_name": "must not write"}))
        .send()
        .await?;
    assert_eq!(
        legacy_direct_write.status(),
        reqwest::StatusCode::FORBIDDEN,
        "an external bearer cannot use the local CRUD route as a direct mutation path"
    );

    harness.stop();
    Ok(())
}

#[tokio::test]
async fn mcp_contract_rest_facade_rejects_oversized_requests(
) -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start().await?;
    let client = reqwest::Client::new();
    let oversized = format!(
        "{{\"deal_id\":\"{}\",\"stage\":\"won\",\"rationale\":\"{}\",\"idempotency_key\":\"contract-key\"}}",
        Uuid::new_v4(),
        "x".repeat(1_048_576)
    );

    let response = client
        .post(harness.url("/v1/nexus/proposals/stage-change"))
        .bearer_auth(&harness.token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(oversized)
        .send()
        .await?;

    assert_eq!(response.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);
    harness.stop();
    Ok(())
}

fn initialize_request() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": fabric::mcp::MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "fake-nexus", "version": "1.0.0"}
        }
    })
}

async fn mcp_post(
    client: &reqwest::Client,
    harness: &Harness,
    token: Option<&str>,
    origin: &str,
    body: &Value,
) -> Result<reqwest::Response, reqwest::Error> {
    let request = client
        .post(harness.url("/mcp"))
        .header(reqwest::header::ORIGIN, origin)
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .header("mcp-protocol-version", fabric::mcp::MCP_PROTOCOL_VERSION)
        .json(body);
    match token {
        Some(token) => request.bearer_auth(token).send().await,
        None => request.send().await,
    }
}

async fn mcp_post_with_tenant_header(
    client: &reqwest::Client,
    harness: &Harness,
    token: &str,
    body: &Value,
    tenant: Uuid,
) -> Result<reqwest::Response, reqwest::Error> {
    client
        .post(harness.url("/mcp"))
        .bearer_auth(token)
        .header(reqwest::header::ORIGIN, ORIGIN)
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .header("mcp-protocol-version", fabric::mcp::MCP_PROTOCOL_VERSION)
        .header("x-hydra-tenant", tenant.to_string())
        .json(body)
        .send()
        .await
}

async fn json_response(response: reqwest::Response) -> Result<Value, Box<dyn std::error::Error>> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(format!("unexpected MCP HTTP status {status}: {body}").into());
    }
    serde_json::from_str(&body)
        .map_err(|error| format!("invalid MCP JSON response ({error}): {body}").into())
}

fn entity(number: u128, tenant: Uuid, kind: &str, body: Value) -> cdm::Entity {
    cdm::Entity {
        id: Uuid::from_u128(number),
        kind: kind.to_owned(),
        tenant,
        body,
        origin: "hydra-contract-fixture".to_owned(),
        origin_ref: Some(format!("fixture:{number}")),
        version: 1,
    }
}

fn sign_token() -> Result<String, jsonwebtoken::errors::Error> {
    let now = u64::try_from(OffsetDateTime::now_utc().unix_timestamp()).unwrap_or_default();
    let claims = TestClaims {
        iss: ISSUER.to_owned(),
        sub: "nexus-service:contract".to_owned(),
        aud: AUDIENCE.to_owned(),
        exp: now + 300,
        nbf: now.saturating_sub(5),
        iat: now,
        jti: "mcp-contract-token".to_owned(),
        scope: [
            "hydra.capabilities.read",
            "hydra.crm.read",
            "hydra.crm.context.read",
            "hydra.envelopes.read",
        ]
        .join(" "),
        principal_type: "nexus_service".to_owned(),
        external_tenant_id: "nexus-tenant-a".to_owned(),
        external_business_id: "business-a".to_owned(),
        acr: "urn:nexus:acr:service".to_owned(),
    };
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(KEY_ID.to_owned());
    encode(&header, &claims, &EncodingKey::from_ed_der(PRIVATE_KEY_DER))
}

fn sha256_json(value: &Value) -> Result<String, serde_json::Error> {
    let digest = Sha256::digest(serde_json::to_vec(value)?);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}
