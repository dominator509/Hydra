use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use cdm::Entity;
use fabric::{
    AppState, AuthorizationService, NexusControlPlaneConfig, NexusOidcConfig, OidcAuthenticator,
    OidcKeySource, Scope,
};
use governor::{Constitution, Level};
use hydra_kernel::event_status::EventRuntimeStatusService;
use hydra_kernel::event_stream::{
    EventStreamConfig, JetStreamEventPublisher, HYDRA_EVENT_STREAM_NAME,
};
use hydra_kernel::policy_provider::PersistedGovernorProvider;
use hydra_kernel::relay::{run_with_health, RelayHealth};
use hydra_kernel::runtime_services::RuntimeServices;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use store::{NewExternalTenantBinding, Store, TestDb};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::fake_event_consumer::FakeEventConsumer;
use super::fake_mcp_client::FakeMcpClient;

pub type HarnessError = Box<dyn std::error::Error + Send + Sync>;

const AUDIENCE: &str = "hydra-api";
const KEY_ID: &str = "fake-nexus-e2e-key";
const NEXUS_ORIGIN: &str = "https://nexus.test";
const PRIVATE_KEY_DER: &[u8] = &[
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];

#[derive(Clone)]
struct JwksState {
    document: Value,
    requests: Arc<AtomicUsize>,
}

async fn serve_jwks(State(state): State<JwksState>) -> Json<Value> {
    state.requests.fetch_add(1, Ordering::SeqCst);
    Json(state.document)
}

pub struct FakeNexusIssuer {
    issuer: String,
    jwks_url: String,
    requests: Arc<AtomicUsize>,
    server: JoinHandle<Result<(), std::io::Error>>,
}

struct TokenRequest<'a> {
    principal_id: &'a str,
    principal_type: &'a str,
    external_tenant_id: &'a str,
    external_business_id: &'a str,
    scopes: &'a [Scope],
    delegated_by: Option<&'a str>,
    authentication_strength: Option<&'a str>,
}

impl FakeNexusIssuer {
    async fn start() -> Result<Self, HarnessError> {
        let requests = Arc::new(AtomicUsize::new(0));
        let state = JwksState {
            document: json!({
                "keys": [{
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "x": "A6EHv_POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg",
                    "alg": "EdDSA",
                    "kid": KEY_ID,
                    "use": "sig"
                }]
            }),
            requests: requests.clone(),
        };
        let app = Router::new()
            .route("/.well-known/jwks.json", get(serve_jwks))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let issuer = format!("http://{address}");
        Ok(Self {
            jwks_url: format!("{issuer}/.well-known/jwks.json"),
            issuer,
            requests,
            server,
        })
    }

    fn issue(&self, request: TokenRequest<'_>) -> Result<String, HarnessError> {
        let now = u64::try_from(time::OffsetDateTime::now_utc().unix_timestamp())?;
        let claims = json!({
            "iss": self.issuer,
            "sub": request.principal_id,
            "aud": AUDIENCE,
            "exp": now + 300,
            "nbf": now.saturating_sub(5),
            "iat": now,
            "jti": format!("fake-nexus-{}", Uuid::new_v4()),
            "scope": request.scopes.iter().map(|scope| scope.as_str()).collect::<Vec<_>>().join(" "),
            "principal_type": request.principal_type,
            "external_tenant_id": request.external_tenant_id,
            "external_business_id": request.external_business_id,
            "delegated_by": request.delegated_by,
            "acr": request.authentication_strength
        });
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(KEY_ID.to_owned());
        Ok(encode(
            &header,
            &claims,
            &EncodingKey::from_ed_der(PRIVATE_KEY_DER),
        )?)
    }

    fn jwks_requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    async fn shutdown(self) {
        self.server.abort();
        let _ = self.server.await;
    }
}

#[derive(Clone)]
pub struct FakeNexusTokens {
    pub service: String,
    pub agent: String,
    pub human_approver: String,
    pub other_business: String,
}

#[derive(Clone)]
pub struct FakeNexusRestClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

impl FakeNexusRestClient {
    fn new(base_url: &str, token: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.into(),
        }
    }

    pub async fn get_response(
        &self,
        path: &str,
        correlation_id: &str,
    ) -> Result<reqwest::Response, reqwest::Error> {
        self.client
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .header("x-request-id", format!("request-{correlation_id}"))
            .header("x-correlation-id", correlation_id)
            .send()
            .await
    }

    pub async fn get_json(&self, path: &str, correlation_id: &str) -> Result<Value, HarnessError> {
        response_json(self.get_response(path, correlation_id).await?).await
    }

    pub async fn post_json(
        &self,
        path: &str,
        body: &Value,
        correlation_id: &str,
        causation_id: Option<&str>,
    ) -> Result<Value, HarnessError> {
        let mut request = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .header("x-request-id", format!("request-{correlation_id}"))
            .header("x-correlation-id", correlation_id)
            .json(body);
        if let Some(causation_id) = causation_id {
            request = request.header("x-causation-id", causation_id);
        }
        response_json(request.send().await?).await
    }
}

async fn response_json(response: reqwest::Response) -> Result<Value, HarnessError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(format!("fake Nexus HTTP request failed with {status}: {body}").into());
    }
    Ok(serde_json::from_str(&body)?)
}

pub struct FakeNexusHarness {
    db: Option<TestDb>,
    pub store: Store,
    pub hydra_tenant_id: Uuid,
    pub other_hydra_tenant_id: Uuid,
    pub binding_id: Uuid,
    pub external_tenant_id: String,
    pub external_business_id: String,
    pub other_external_business_id: String,
    pub deal: Entity,
    pub tokens: FakeNexusTokens,
    pub objective_id: String,
    pub task_id: String,
    pub correlation_id: String,
    pub causation_id: String,
    base_url: String,
    event_context: async_nats::jetstream::Context,
    stream_name: String,
    shutdown: watch::Sender<bool>,
    executor_task: JoinHandle<()>,
    relay_task: JoinHandle<()>,
    server: JoinHandle<Result<(), std::io::Error>>,
    issuer: FakeNexusIssuer,
}

impl FakeNexusHarness {
    pub async fn start() -> Result<Self, HarnessError> {
        let db = TestDb::new().await?;
        let store = Store::new(db.pool.clone());
        let hydra_tenant_id = Uuid::new_v4();
        let other_hydra_tenant_id = Uuid::new_v4();
        let external_tenant_id = format!("nexus-tenant-{}", Uuid::new_v4().simple());
        let external_business_id = format!("business-{}", Uuid::new_v4().simple());
        let other_external_business_id = format!("business-{}", Uuid::new_v4().simple());
        let binding = store
            .external_bindings
            .create(NewExternalTenantBinding {
                provider: "nexus".to_owned(),
                external_tenant_id: external_tenant_id.clone(),
                external_business_id: external_business_id.clone(),
                hydra_tenant_id,
            })
            .await?;
        store
            .external_bindings
            .create(NewExternalTenantBinding {
                provider: "nexus".to_owned(),
                external_tenant_id: external_tenant_id.clone(),
                external_business_id: other_external_business_id.clone(),
                hydra_tenant_id: other_hydra_tenant_id,
            })
            .await?;

        store
            .autonomy
            .upsert_cell(
                hydra_tenant_id,
                "pipeline",
                "move_stage",
                Some("deal"),
                Level::L2,
                &json!({"batch_max": 25}),
            )
            .await?;
        let deal = store
            .entities
            .upsert(
                hydra_tenant_id,
                Entity {
                    id: Uuid::new_v4(),
                    kind: "deal".to_owned(),
                    tenant: hydra_tenant_id,
                    body: json!({
                        "title": "Hydra Nexus E2E Renewal",
                        "stage_id": "discovery"
                    }),
                    origin: "native".to_owned(),
                    origin_ref: Some("fake-nexus:e2e-deal".to_owned()),
                    version: 1,
                },
            )
            .await?;

        let issuer = FakeNexusIssuer::start().await?;
        let nats_url =
            std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".to_owned());
        let nats = async_nats::connect(nats_url).await?;
        let event_context = async_nats::jetstream::new(nats.clone());
        let publisher =
            JetStreamEventPublisher::bootstrap(nats, EventStreamConfig::nexus_v1()).await?;
        let relay_health = RelayHealth::default();
        let event_status: Arc<dyn fabric::EventStatusService> = Arc::new(
            EventRuntimeStatusService::new(publisher.clone(), relay_health.clone()),
        );

        let constitution = Constitution {
            monthly_spend_cap_cents: 50_000,
            pii_egress_allowlist: vec!["private".to_owned()],
            blast_entities_ceiling: 250,
            blast_sends_ceiling: 50,
            blast_money_ceiling_cents: 250_000,
        };
        let governor_provider: Arc<dyn fabric::GovernorProvider> = Arc::new(
            PersistedGovernorProvider::new(store.autonomy.clone(), constitution),
        );
        let (runtime, worker) = RuntimeServices::build(store.clone())?;
        let authorization = Arc::new(AuthorizationService::new(["mfa".to_owned()]));
        let mut runtime_capabilities = runtime.execution_registry.runtime_capabilities();
        runtime_capabilities
            .insert(fabric::capabilities::RUNTIME_TENANT_SCOPED_ENVELOPE_GET.to_owned());
        let capabilities =
            fabric::CapabilityRegistry::nexus_v1_with_runtime_capabilities(runtime_capabilities)?;
        let external_auth = Arc::new(OidcAuthenticator::new(
            NexusOidcConfig {
                provider: "nexus".to_owned(),
                issuer: issuer.issuer.clone(),
                audience: AUDIENCE.to_owned(),
                key_source: OidcKeySource::JwksUrl(issuer.jwks_url.clone()),
                allowed_algorithms: vec![Algorithm::EdDSA],
                jwks_cache_ttl: Duration::from_secs(300),
                clock_skew: Duration::from_secs(5),
            },
            Arc::new(store.external_bindings.clone()),
        )?);

        let envelope_service: Arc<dyn fabric::EnvelopeService> = Arc::new(
            fabric::StoreEnvelopeService::with_governor_provider(
                store.clone(),
                governor_provider.clone(),
            )
            .with_execution_dispatcher(runtime.dispatcher.clone())
            .with_authorization(authorization.clone()),
        );
        let state = AppState::new(
            Arc::new(fabric::SessionStore::new(store.pool.clone())),
            Arc::new(fabric::StoreEntityService::new(store.clone())),
            Arc::new(fabric::StoreAutonomyService::new(store.clone())),
            Arc::new(fabric::StoreBridgeService::with_runtime(
                store.clone(),
                governor_provider,
                runtime.dispatcher.clone(),
            )),
            envelope_service,
            Arc::new(fabric::StoreTkStatsService::new(
                store.ledger.clone(),
                vec!["concierge".to_owned()],
            )),
            runtime.concierge.clone(),
        )
        .with_authorization(authorization)
        .with_capabilities(Arc::new(capabilities))
        .with_external_auth(external_auth)
        .with_rate_limiter(Arc::new(fabric::rate::RateLimiter::new(1_000, 60)))
        .with_event_status(event_status);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let base_url = format!("http://{address}");
        let state = state.with_nexus_control_plane(NexusControlPlaneConfig {
            enabled: true,
            resource: format!("{base_url}/mcp"),
            resource_metadata_url: format!("{base_url}/.well-known/oauth-protected-resource/mcp"),
            authorization_servers: vec![issuer.issuer.clone()],
            allowed_hosts: vec!["127.0.0.1".to_owned(), "localhost".to_owned()],
            allowed_origins: vec![NEXUS_ORIGIN.to_owned()],
            max_request_body_bytes: 1_048_576,
        });

        let (shutdown, executor_shutdown) = watch::channel(false);
        let relay_shutdown = shutdown.subscribe();
        let executor = runtime.executor.clone();
        let executor_task = tokio::spawn(worker.run(executor, executor_shutdown));
        let relay_task = tokio::spawn(run_with_health(
            relay_shutdown,
            store.outbox.clone(),
            Arc::new(publisher),
            relay_health,
        ));
        let server = tokio::spawn(async move { axum::serve(listener, fabric::app(state)).await });

        let service_scopes = [
            Scope::CapabilitiesRead,
            Scope::CrmRead,
            Scope::CrmContextRead,
            Scope::EnvelopesRead,
        ];
        let agent_scopes = [
            Scope::CapabilitiesRead,
            Scope::CrmRead,
            Scope::CrmPropose,
            Scope::EnvelopesRead,
        ];
        let human_scopes = [Scope::CapabilitiesRead, Scope::EnvelopesApprove];
        let tokens = FakeNexusTokens {
            service: issuer.issue(TokenRequest {
                principal_id: "nexus-service:crm-orchestrator",
                principal_type: "nexus_service",
                external_tenant_id: &external_tenant_id,
                external_business_id: &external_business_id,
                scopes: &service_scopes,
                delegated_by: None,
                authentication_strength: Some("service"),
            })?,
            agent: issuer.issue(TokenRequest {
                principal_id: "nexus-agent:revenue",
                principal_type: "nexus_agent",
                external_tenant_id: &external_tenant_id,
                external_business_id: &external_business_id,
                scopes: &agent_scopes,
                delegated_by: None,
                authentication_strength: Some("agent"),
            })?,
            human_approver: issuer.issue(TokenRequest {
                principal_id: "human:revenue-owner",
                principal_type: "human",
                external_tenant_id: &external_tenant_id,
                external_business_id: &external_business_id,
                scopes: &human_scopes,
                delegated_by: Some("nexus:user-session"),
                authentication_strength: Some("mfa"),
            })?,
            other_business: issuer.issue(TokenRequest {
                principal_id: "nexus-service:other-business",
                principal_type: "nexus_service",
                external_tenant_id: &external_tenant_id,
                external_business_id: &other_external_business_id,
                scopes: &service_scopes,
                delegated_by: None,
                authentication_strength: Some("service"),
            })?,
        };

        Ok(Self {
            db: Some(db),
            store,
            hydra_tenant_id,
            other_hydra_tenant_id,
            binding_id: binding.id,
            external_tenant_id,
            external_business_id,
            other_external_business_id,
            deal,
            tokens,
            objective_id: format!("objective-{}", Uuid::new_v4()),
            task_id: format!("task-{}", Uuid::new_v4()),
            correlation_id: format!("correlation-{}", Uuid::new_v4()),
            causation_id: format!("causation-{}", Uuid::new_v4()),
            base_url,
            event_context,
            stream_name: HYDRA_EVENT_STREAM_NAME.to_owned(),
            shutdown,
            executor_task,
            relay_task,
            server,
            issuer,
        })
    }

    pub fn service_client(&self) -> FakeNexusRestClient {
        FakeNexusRestClient::new(&self.base_url, self.tokens.service.clone())
    }

    pub fn human_client(&self) -> FakeNexusRestClient {
        FakeNexusRestClient::new(&self.base_url, self.tokens.human_approver.clone())
    }

    pub fn other_business_client(&self) -> FakeNexusRestClient {
        FakeNexusRestClient::new(&self.base_url, self.tokens.other_business.clone())
    }

    pub fn agent_mcp_client(&self) -> FakeMcpClient {
        FakeMcpClient::new(&self.base_url, self.tokens.agent.clone())
    }

    pub fn other_business_mcp_client(&self) -> FakeMcpClient {
        FakeMcpClient::new(&self.base_url, self.tokens.other_business.clone())
    }

    pub async fn event_consumer(
        &self,
        subject: &str,
    ) -> Result<FakeEventConsumer, super::fake_nexus_consumer::FakeNexusConsumerError> {
        FakeEventConsumer::connect(
            self.event_context.clone(),
            self.stream_name.clone(),
            subject,
        )
        .await
    }

    pub fn jwks_requests(&self) -> usize {
        self.issuer.jwks_requests()
    }

    pub async fn shutdown(mut self) -> Result<(), HarnessError> {
        let _ = self.shutdown.send(true);
        self.executor_task.await?;
        self.relay_task.await?;
        self.server.abort();
        let _ = self.server.await;
        self.issuer.shutdown().await;
        drop(self.store);
        let db = self.db.take().ok_or("fake Nexus test database missing")?;
        db.cleanup().await?;
        Ok(())
    }
}
