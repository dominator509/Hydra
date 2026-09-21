use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fabric::services::{
    demo_governor, AppState, BridgeConformanceService, BridgeSynthesisService,
    ConciergeServiceImpl, NexusControlPlaneConfig, StoreA2aTaskService, StoreAutonomyService,
    StoreBridgeService, StoreEntityService, StoreEnvelopeService, StoreTkStatsService,
};
use fabric::{
    app, ExternalBindingResolver, FabricError, NexusOidcConfig, OidcAuthenticator, OidcKeySource,
    ResolvedExternalBinding,
};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use store::{NewA2aTask, Store, TestDb};
use time::OffsetDateTime;
use uuid::Uuid;

const ISSUER: &str = "https://issuer.nexus.test";
const AUDIENCE: &str = "hydra-api";
const KEY_ID: &str = "nexus-a2a-test-key";
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
    binding_a: Uuid,
    binding_b: Uuid,
    tenant_a: Uuid,
    tenant_b: Uuid,
}

#[async_trait]
impl ExternalBindingResolver for FakeBindingResolver {
    async fn resolve(
        &self,
        provider: &str,
        external_tenant_id: &str,
        external_business_id: &str,
    ) -> Result<Option<ResolvedExternalBinding>, FabricError> {
        if provider != "nexus" || external_tenant_id != "nexus-tenant" {
            return Ok(None);
        }
        Ok(match external_business_id {
            "business-a" => Some(ResolvedExternalBinding {
                id: self.binding_a,
                hydra_tenant_id: self.tenant_a,
                active: true,
            }),
            "business-b" => Some(ResolvedExternalBinding {
                id: self.binding_b,
                hydra_tenant_id: self.tenant_b,
                active: true,
            }),
            _ => None,
        })
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

struct FakeBridgeSynthesis {
    fail: bool,
}

#[async_trait]
impl BridgeSynthesisService for FakeBridgeSynthesis {
    fn available(&self) -> bool {
        true
    }

    fn availability_reason(&self) -> &'static str {
        "test mapping proposal service is configured"
    }

    async fn synthesize(&self, _tenant_id: Uuid, input: Value) -> Result<Value, FabricError> {
        if self.fail {
            return Err(FabricError::CapabilityUnavailable(
                "test provider failure".to_owned(),
            ));
        }
        Ok(json!({
            "adapterId": input.get("adapterId").cloned().unwrap_or_else(|| json!("memcrm")),
            "entity": "deal",
            "mapping": "adapter: memcrm\nentity: deal\nfields:\n  stage: crm.stage\n",
            "validationReport": "mapping_only: validated; activation: unavailable",
            "repaired": false,
            "provider": "fake",
            "model": "test"
        }))
    }
}

struct FakeBridgeConformance {
    fail: bool,
}

#[async_trait]
impl BridgeConformanceService for FakeBridgeConformance {
    fn available(&self) -> bool {
        true
    }

    fn availability_reason(&self) -> &'static str {
        "test bridge conformance service is configured"
    }

    async fn conform(&self, _tenant_id: Uuid, _input: Value) -> Result<Value, FabricError> {
        if self.fail {
            return Err(FabricError::CapabilityUnavailable(
                "test conformance failure".to_owned(),
            ));
        }
        Ok(json!({
            "artifact_sha256": "a".repeat(64),
            "descriptor": {
                "name": "memcrm",
                "version": "1.0.0",
                "kinds": ["party"],
                "capabilities": {
                    "read": true,
                    "write": true,
                    "incremental_sync": true,
                    "etags": true,
                    "server_side_query": false
                }
            },
            "checked_kind": "party",
            "schema_field_count": 4,
            "listed_record_count": 0,
            "changed_record_count": 0,
            "incremental_checked": true,
            "fuel_remaining": 999,
            "report": "describe:pass; probe:pass; schema:pass; list:pass; changes-since:checked"
        }))
    }
}

struct Harness {
    address: std::net::SocketAddr,
    tenant_a: Uuid,
    tenant_b: Uuid,
    store: Store,
    db: TestDb,
    server: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl Harness {
    async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with(None).await
    }

    async fn start_with_synthesis() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with(Some(false)).await
    }

    async fn start_with_failed_synthesis() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with(Some(true)).await
    }

    async fn start_with_conformance(failure: bool) -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_services(None, Some(failure)).await
    }

    async fn start_with(
        synthesis_failure: Option<bool>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_services(synthesis_failure, None).await
    }

    async fn start_with_services(
        synthesis_failure: Option<bool>,
        conformance_failure: Option<bool>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let tenant_a = Uuid::from_u128(0xaaaaaaaa_aaaa_4aaa_8aaa_aaaaaaaaaaaa);
        let tenant_b = Uuid::from_u128(0xbbbbbbbb_bbbb_4bbb_8bbb_bbbbbbbbbbbb);
        let binding_a = Uuid::from_u128(0xcccccccc_cccc_4ccc_8ccc_cccccccccccc);
        let binding_b = Uuid::from_u128(0xdddddddd_dddd_4ddd_8ddd_dddddddddddd);
        let db = TestDb::new().await?;
        let store = Store::new(db.pool.clone());
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
                binding_a,
                binding_b,
                tenant_a,
                tenant_b,
            }),
        )?;
        let mut state = AppState::new(
            Arc::new(fabric::SessionStore::new(db.pool.clone())),
            Arc::new(StoreEntityService::new(store.clone())),
            Arc::new(StoreAutonomyService::new(store.clone())),
            Arc::new(StoreBridgeService::new(store.clone(), demo_governor())),
            Arc::new(StoreEnvelopeService::new(store.clone(), demo_governor())),
            Arc::new(StoreTkStatsService::new(
                store.ledger.clone(),
                vec!["a2a".to_owned()],
            )),
            Arc::new(ConciergeServiceImpl),
        )
        .with_a2a_tasks(Arc::new(StoreA2aTaskService::new(store.clone())))
        .with_external_auth(Arc::new(authenticator))
        .with_rate_limiter(Arc::new(fabric::rate::RateLimiter::new(100, 60)))
        .with_nexus_control_plane(NexusControlPlaneConfig {
            enabled: true,
            resource: "https://hydra.test/a2a".to_owned(),
            resource_metadata_url: "https://hydra.test/.well-known/oauth-protected-resource/a2a"
                .to_owned(),
            authorization_servers: vec![ISSUER.to_owned()],
            allowed_hosts: vec!["127.0.0.1".to_owned(), "localhost".to_owned()],
            allowed_origins: vec![ORIGIN.to_owned()],
            max_request_body_bytes: 1_048_576,
        });
        if let Some(fail) = synthesis_failure {
            state = state.with_bridge_synthesis(Arc::new(FakeBridgeSynthesis { fail }));
        }
        if let Some(fail) = conformance_failure {
            state = state.with_bridge_conformance(Arc::new(FakeBridgeConformance { fail }));
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move { axum::serve(listener, app(state)).await });
        Ok(Self {
            address,
            tenant_a,
            tenant_b,
            store,
            db,
            server,
        })
    }

    fn url(&self) -> String {
        format!("http://{}/a2a", self.address)
    }

    async fn stop(self) -> Result<(), Box<dyn std::error::Error>> {
        self.server.abort();
        self.db.cleanup().await?;
        Ok(())
    }
}

#[tokio::test]
async fn bridge_synthesis_is_authenticated_proposal_only_and_idempotent(
) -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start_with_synthesis().await?;
    let client = reqwest::Client::new();
    let url = harness.url();
    let token = sign_token("business-a", "nexus-service:a2a-synthesis")?;
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "SendMessage",
        "params": {"message": {
            "messageId": "synthesis-1",
            "contextId": "synthesis-context",
            "role": "user",
            "parts": [{"kind": "text", "text": "bounded mapping"}],
            "metadata": {
                "io.hydra/workflow": "bridge-synthesis",
                "io.hydra/input": {
                    "adapterId": "memcrm",
                    "descriptor": "bounded CRM descriptor",
                    "schema": "deal.stage"
                }
            }
        }}
    });
    let card = client
        .get(format!(
            "http://{}/.well-known/agent-card.json",
            harness.address
        ))
        .send()
        .await?
        .json::<Value>()
        .await?;
    assert_eq!(
        card["skills"]
            .as_array()
            .and_then(|skills| skills
                .iter()
                .find(|skill| skill["name"] == "bridge-synthesis"))
            .and_then(|skill| skill["metadata"]["available"].as_bool()),
        Some(true)
    );

    let first = post_json(&client, &url, &token, &request, None).await?;
    assert_eq!(first["result"]["status"]["state"], "completed");
    assert_eq!(
        first["result"]["artifacts"][0]["parts"][0]["data"]["mode"],
        "mapping_proposal"
    );
    assert_eq!(
        first["result"]["artifacts"][0]["parts"][0]["data"]["proposal"]["adapterId"],
        "memcrm"
    );
    assert_eq!(
        first["result"]["metadata"]["io.hydra/correlationId"],
        "a2a-correlation"
    );

    let replay = post_json(&client, &url, &token, &request, None).await?;
    assert_eq!(replay["result"]["id"], first["result"]["id"]);
    assert_eq!(replay["result"]["status"]["state"], "completed");
    assert_eq!(
        replay["result"]["artifacts"][0]["parts"][0]["data"],
        first["result"]["artifacts"][0]["parts"][0]["data"]
    );

    harness.stop().await?;
    Ok(())
}

#[tokio::test]
async fn bridge_synthesis_provider_failure_is_durable_and_redacted(
) -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start_with_failed_synthesis().await?;
    let client = reqwest::Client::new();
    let url = harness.url();
    let token = sign_token("business-a", "nexus-service:a2a-synthesis-failure")?;
    let request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "SendMessage",
        "params": {"message": {
            "messageId": "synthesis-failure-1",
            "contextId": "synthesis-failure-context",
            "role": "user",
            "parts": [{"kind": "text", "text": "bounded mapping"}],
            "metadata": {
                "io.hydra/workflow": "bridge-synthesis",
                "io.hydra/input": {
                    "adapterId": "memcrm",
                    "descriptor": "bounded CRM descriptor",
                    "schema": "deal.stage"
                }
            }
        }}
    });
    let first = post_json(&client, &url, &token, &request, None).await?;
    assert_eq!(first["result"]["status"]["state"], "failed");
    assert_eq!(
        first["result"]["artifacts"][0]["parts"][0]["data"]["error"],
        "synthesis_failed"
    );
    assert!(!first.to_string().contains("test provider failure"));

    let replay = post_json(&client, &url, &token, &request, None).await?;
    assert_eq!(replay["result"]["id"], first["result"]["id"]);
    assert_eq!(replay["result"]["status"]["state"], "failed");
    assert_eq!(
        replay["result"]["artifacts"][0]["parts"][0]["data"],
        first["result"]["artifacts"][0]["parts"][0]["data"]
    );

    harness.stop().await?;
    Ok(())
}

#[tokio::test]
async fn bridge_conformance_is_authenticated_metadata_only_and_idempotent(
) -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start_with_conformance(false).await?;
    let client = reqwest::Client::new();
    let url = harness.url();
    let token = sign_token("business-a", "nexus-service:a2a-conformance")?;
    let request = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "SendMessage",
        "params": {"message": {
            "messageId": "conformance-1",
            "contextId": "conformance-context",
            "role": "user",
            "parts": [{"kind": "text", "text": "validate adapter"}],
            "metadata": {
                "io.hydra/workflow": "bridge-conformance",
                "io.hydra/input": {
                    "adapterId": "memcrm",
                    "kind": "party",
                    "limit": 25
                }
            }
        }}
    });
    let card = client
        .get(format!(
            "http://{}/.well-known/agent-card.json",
            harness.address
        ))
        .send()
        .await?
        .json::<Value>()
        .await?;
    assert_eq!(
        card["skills"]
            .as_array()
            .and_then(|skills| skills
                .iter()
                .find(|skill| skill["name"] == "bridge-conformance"))
            .and_then(|skill| skill["metadata"]["available"].as_bool()),
        Some(true)
    );

    let first = post_json(&client, &url, &token, &request, None).await?;
    assert_eq!(first["result"]["status"]["state"], "completed");
    assert_eq!(
        first["result"]["artifacts"][0]["parts"][0]["data"]["mode"],
        "metadata_only"
    );
    assert_eq!(
        first["result"]["artifacts"][0]["parts"][0]["data"]["conformance"]["checked_kind"],
        "party"
    );
    assert!(!first.to_string().contains("test conformance failure"));

    let replay = post_json(&client, &url, &token, &request, None).await?;
    assert_eq!(replay["result"]["id"], first["result"]["id"]);
    assert_eq!(
        replay["result"]["artifacts"][0]["parts"][0]["data"],
        first["result"]["artifacts"][0]["parts"][0]["data"]
    );

    harness.stop().await?;
    Ok(())
}

#[tokio::test]
async fn bridge_conformance_failure_is_durable_and_redacted(
) -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start_with_conformance(true).await?;
    let client = reqwest::Client::new();
    let url = harness.url();
    let token = sign_token("business-a", "nexus-service:a2a-conformance-failure")?;
    let request = send_request(
        "conformance-failure-1",
        "conformance-failure",
        "bridge-conformance",
        "adapter",
    );
    let first = post_json(&client, &url, &token, &request, None).await?;
    assert_eq!(first["result"]["status"]["state"], "failed");
    assert_eq!(
        first["result"]["artifacts"][0]["parts"][0]["data"]["error"],
        "conformance_failed"
    );
    assert!(!first.to_string().contains("test conformance failure"));

    let replay = post_json(&client, &url, &token, &request, None).await?;
    assert_eq!(replay["result"]["id"], first["result"]["id"]);
    assert_eq!(replay["result"]["status"]["state"], "failed");
    harness.stop().await?;
    Ok(())
}

#[tokio::test]
async fn a2a_workflows_are_authenticated_durable_tenant_scoped_and_resumable(
) -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::start().await?;
    let client = reqwest::Client::new();
    let url = harness.url();
    let token_a = sign_token("business-a", "nexus-service:a2a-a")?;
    let token_b = sign_token("business-b", "nexus-service:a2a-b")?;

    let anonymous = client
        .post(&url)
        .header("a2a-version", "1.0")
        .header("origin", ORIGIN)
        .json(&send_request(
            "message-1",
            "context-1",
            "migration-assessment",
            "alpha",
        ))
        .send()
        .await?;
    assert_eq!(anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);

    let request = send_request("message-1", "context-1", "migration-assessment", "alpha");
    let first = post_json(&client, &url, &token_a, &request, Some(harness.tenant_b)).await?;
    let task_id = first["result"]["id"]
        .as_str()
        .ok_or("A2A result must include a task id")?
        .parse::<Uuid>()?;
    assert_eq!(first["result"]["status"]["state"], "completed");
    assert_eq!(
        first["result"]["artifacts"][0]["parts"][0]["data"]["inputKeys"],
        json!(["adapter"])
    );
    assert_eq!(
        first["result"]["metadata"]["io.hydra/correlationId"],
        "a2a-correlation"
    );
    assert_eq!(
        first["result"]["history"]
            .as_array()
            .map(|history| history.len()),
        Some(3)
    );

    let replay = post_json(&client, &url, &token_a, &request, None).await?;
    assert_eq!(replay["result"]["id"], task_id.to_string());
    assert_eq!(replay["result"]["status"]["state"], "completed");

    let conflict = send_request(
        "message-1",
        "context-1",
        "migration-assessment",
        "different",
    );
    let conflict = post_json(&client, &url, &token_a, &conflict, None).await?;
    assert_eq!(conflict["error"]["code"], -32005);

    let unsupported = send_request("message-unsupported", "context-1", "bridge-synthesis", "x");
    let unsupported = post_json(&client, &url, &token_a, &unsupported, None).await?;
    assert_eq!(unsupported["error"]["code"], -32004);

    let cross_tenant = json_rpc(
        &client,
        &url,
        &token_b,
        json!({"jsonrpc": "2.0", "id": 8, "method": "GetTask", "params": {"id": task_id}}),
        None,
    )
    .await?;
    assert_eq!(cross_tenant["error"]["code"], -32001);

    let pending = insert_pending(
        &harness.store,
        harness.tenant_a,
        "message-resume",
        "resume-context",
    )
    .await?;
    let resumed = send_request(
        "message-resume",
        "resume-context",
        "migration-assessment",
        "resume",
    );
    let resumed = post_json(&client, &url, &token_a, &resumed, None).await?;
    assert_eq!(resumed["result"]["id"], pending.to_string());
    assert_eq!(resumed["result"]["status"]["state"], "completed");

    let cancel_id = insert_pending(
        &harness.store,
        harness.tenant_a,
        "message-cancel",
        "cancel-context",
    )
    .await?;
    let canceled = json_rpc(
        &client,
        &url,
        &token_a,
        json!({"jsonrpc": "2.0", "id": 10, "method": "CancelTask", "params": {"id": cancel_id}}),
        None,
    )
    .await?;
    assert_eq!(canceled["result"]["id"], cancel_id.to_string());
    assert_eq!(canceled["result"]["status"]["state"], "canceled");

    let listed = json_rpc(
        &client,
        &url,
        &token_a,
        json!({"jsonrpc": "2.0", "id": 11, "method": "ListTasks", "params": {"contextId": "context-1"}}),
        None,
    )
    .await?;
    assert_eq!(listed["result"]["tasks"].as_array().map(Vec::len), Some(1));

    harness.stop().await?;
    Ok(())
}

#[tokio::test]
async fn a2a_protocol_and_workflow_metadata_are_validated() -> Result<(), Box<dyn std::error::Error>>
{
    let harness = Harness::start().await?;
    let client = reqwest::Client::new();
    let url = harness.url();
    let token = sign_token("business-a", "nexus-service:a2a-protocol")?;

    let wrong_version = json_rpc_with_version(
        &client,
        &url,
        &token,
        send_request(
            "message-version",
            "context-version",
            "migration-assessment",
            "x",
        ),
        None,
        "2.0",
    )
    .await?;
    assert_eq!(wrong_version["error"]["code"], -32003);

    let invalid_role = json_rpc(
        &client,
        &url,
        &token,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "SendMessage",
            "params": {"message": {"messageId": "invalid-role", "role": "agent", "parts": [{"kind": "text", "text": "x"}], "metadata": {"io.hydra/workflow": "migration-assessment"}}}
        }),
        None,
    )
    .await?;
    assert_eq!(invalid_role["error"]["code"], -32602);

    harness.stop().await?;
    Ok(())
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    request: &Value,
    tenant_header: Option<Uuid>,
) -> Result<Value, Box<dyn std::error::Error>> {
    json_rpc(client, url, token, request.clone(), tenant_header).await
}

async fn json_rpc(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    request: Value,
    tenant_header: Option<Uuid>,
) -> Result<Value, Box<dyn std::error::Error>> {
    json_rpc_with_version(client, url, token, request, tenant_header, "1.0").await
}

async fn json_rpc_with_version(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    request: Value,
    tenant_header: Option<Uuid>,
    version: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut request_builder = client
        .post(url)
        .bearer_auth(token)
        .header("a2a-version", version)
        .header("origin", ORIGIN)
        .header("x-correlation-id", "a2a-correlation")
        .header("x-request-id", "a2a-request");
    if let Some(tenant) = tenant_header {
        request_builder = request_builder.header("x-hydra-tenant", tenant.to_string());
    }
    Ok(request_builder
        .json(&request)
        .send()
        .await?
        .json::<Value>()
        .await?)
}

fn send_request(message_id: &str, context_id: &str, workflow: &str, adapter: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "SendMessage",
        "params": {
            "message": {
                "messageId": message_id,
                "contextId": context_id,
                "role": "user",
                "parts": [{"kind": "text", "text": adapter}],
                "metadata": {
                    "io.hydra/workflow": workflow,
                    "io.hydra/input": {"adapter": adapter}
                }
            }
        }
    })
}

async fn insert_pending(
    store: &Store,
    tenant_id: Uuid,
    message_id: &str,
    context_id: &str,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let input = json!({"adapter": "resume"});
    let request_hash = request_hash("migration-assessment", context_id, &input, None);
    let resolution = store
        .a2a_tasks
        .create_or_get(NewA2aTask {
            id: Uuid::new_v4(),
            tenant_id,
            context_id: context_id.to_owned(),
            message_id: message_id.to_owned(),
            workflow: "migration-assessment".to_owned(),
            request_hash,
            requester_principal_id: "nexus-service:test".to_owned(),
            requester_principal_type: "nexus_service".to_owned(),
            correlation_id: "a2a-correlation".to_owned(),
            causation_id: None,
            objective_id: None,
            input,
            history: json!([{"state": "submitted"}]),
        })
        .await?;
    Ok(resolution.task().id)
}

fn request_hash(
    workflow: &str,
    context_id: &str,
    input: &Value,
    objective_id: Option<&str>,
) -> String {
    let canonical = json!({
        "workflow": workflow,
        "contextId": context_id,
        "input": input,
        "objectiveId": objective_id
    });
    let digest = Sha256::digest(serde_json::to_vec(&canonical).unwrap_or_default());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sign_token(business_id: &str, subject: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let now = u64::try_from(OffsetDateTime::now_utc().unix_timestamp()).unwrap_or_default();
    let claims = TestClaims {
        iss: ISSUER.to_owned(),
        sub: subject.to_owned(),
        aud: AUDIENCE.to_owned(),
        exp: now + 300,
        nbf: now.saturating_sub(5),
        iat: now,
        jti: format!("{subject}-token"),
        scope: "hydra.bridges.read".to_owned(),
        principal_type: "nexus_service".to_owned(),
        external_tenant_id: "nexus-tenant".to_owned(),
        external_business_id: business_id.to_owned(),
        acr: "urn:nexus:acr:service".to_owned(),
    };
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(KEY_ID.to_owned());
    encode(&header, &claims, &EncodingKey::from_ed_der(PRIVATE_KEY_DER))
}
