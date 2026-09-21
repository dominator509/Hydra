use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use llm_router::providers::anthropic::AnthropicProvider;
use llm_router::providers::deepseek::DeepSeekProvider;
use llm_router::providers::nexus::NexusModelProvider;
use llm_router::providers::openai_compat::OpenAiCompatProvider;
use llm_router::{LlmProvider, Tag};
use tokenkiller::{
    ApproxTokenizer, Contract, ProviderTag, RouteCfg, Segment, Session, Stability, StoreLedgerSink,
};
use tokio::sync::{mpsc, watch};
use tracing::{error, warn};
use uuid::Uuid;

use crate::bridge_runtime::{BridgeConformanceRuntime, BridgeLifecycleRuntime};
use crate::execution_registry::{ExecutionHandler, ExecutionRegistry, ExecutionRegistryError};
use crate::executor::{ExecuteError, Executor, PipelineMoveStageHandler};
use crate::supervisor::{TaskHealth, TaskHealthGuard};

const EXECUTION_QUEUE_CAPACITY: usize = 256;
const EXECUTION_RECOVERY_BATCH_SIZE: i64 = 64;
const EXECUTION_RECOVERY_INTERVAL: Duration = Duration::from_secs(1);
const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";

#[derive(Clone, Debug, Default)]
pub struct LlmRuntimeConfig {
    pub egress_proxy_url: Option<String>,
    pub deepseek_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_compat_base_url: Option<String>,
    pub openai_compat_model: Option<String>,
    pub nexus_model_gateway_url: Option<String>,
    pub nexus_model_gateway_model: Option<String>,
    pub nexus_model_gateway_token: Option<String>,
    pub nexus_model_gateway_private: bool,
    pub skills_path: Option<String>,
    pub skills_trust_file: Option<String>,
    pub adapters_path: Option<String>,
    pub output_budget_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeAvailability {
    Available,
    Disabled(String),
    Experimental(String),
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeComponentInventory {
    pub bridge_host: RuntimeAvailability,
    pub bridge_secrets: RuntimeAvailability,
    pub bridge_lifecycle: RuntimeAvailability,
    pub tokenkiller_router: RuntimeAvailability,
    pub data_steward: RuntimeAvailability,
    pub bridge_engineer: RuntimeAvailability,
    pub comms_transport: RuntimeAvailability,
    pub comms_draft: RuntimeAvailability,
    pub skill_discovery: RuntimeAvailability,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeBuildError {
    #[error(transparent)]
    Registry(#[from] ExecutionRegistryError),
    #[error("failed to construct Wasmtime bridge host: {0}")]
    BridgeHost(String),
    #[error("invalid LLM runtime configuration: {0}")]
    LlmConfig(String),
    #[error("invalid signed skill configuration: {0}")]
    SkillConfig(String),
}

pub struct RuntimeServices {
    pub execution_registry: ExecutionRegistry,
    pub executor: Arc<Executor>,
    pub dispatcher: Arc<ExecutorDispatcher>,
    pub bridge_host: Arc<bridge_host::BridgeHost>,
    pub bridge_lifecycle: Option<Arc<BridgeLifecycleRuntime>>,
    pub bridge_conformance: Option<Arc<BridgeConformanceRuntime>>,
    pub bridge_secrets: Arc<dyn bridge_host::SecretSource>,
    pub concierge: Arc<dyn fabric::ConciergeService>,
    pub bridge_synthesis: Option<Arc<BridgeSynthesisRuntime>>,
    pub skill_registry: Option<Arc<agents::skills::SkillRegistry>>,
    pub executor_health: TaskHealth,
    pub components: RuntimeComponentInventory,
}

struct ConciergeBuild {
    concierge: Arc<dyn fabric::ConciergeService>,
    tokenkiller_router: RuntimeAvailability,
    bridge_synthesis: Option<Arc<BridgeSynthesisRuntime>>,
}

pub struct ExecutorWorker {
    receiver: mpsc::Receiver<governor::ExecuteToken>,
    store: store::Store,
    health: TaskHealth,
}

#[derive(Clone)]
pub struct ExecutorDispatcher {
    sender: mpsc::Sender<governor::ExecuteToken>,
}

#[async_trait]
impl fabric::ExecutionDispatcher for ExecutorDispatcher {
    async fn dispatch(&self, token: governor::ExecuteToken) -> Result<(), fabric::FabricError> {
        self.sender.send(token).await.map_err(|_| {
            fabric::FabricError::CapabilityUnavailable(
                "governed execution worker is unavailable".to_owned(),
            )
        })
    }
}

impl RuntimeServices {
    pub fn build(store: store::Store) -> Result<(Self, ExecutorWorker), RuntimeBuildError> {
        Self::build_with_config(store, LlmRuntimeConfig::default())
    }

    pub fn build_with_config(
        store: store::Store,
        llm_config: LlmRuntimeConfig,
    ) -> Result<(Self, ExecutorWorker), RuntimeBuildError> {
        Self::build_with_config_and_secrets(
            store,
            llm_config,
            Arc::new(bridge_host::StaticSecretSource::default()),
            RuntimeAvailability::Disabled(
                "no persisted vault was provided to this test/default builder".to_owned(),
            ),
        )
    }

    pub fn build_with_config_and_secrets(
        store: store::Store,
        llm_config: LlmRuntimeConfig,
        bridge_secrets: Arc<dyn bridge_host::SecretSource>,
        bridge_secret_availability: RuntimeAvailability,
    ) -> Result<(Self, ExecutorWorker), RuntimeBuildError> {
        let bridge_host = Arc::new(
            bridge_host::BridgeHost::new()
                .map_err(|error| RuntimeBuildError::BridgeHost(error.to_string()))?,
        );
        let (bridge_lifecycle, bridge_lifecycle_availability, lifecycle_handlers) =
            build_bridge_lifecycle(
                &store,
                bridge_host.clone(),
                &llm_config,
                bridge_secrets.clone(),
                bridge_secret_availability.clone(),
            );
        let mut handlers: Vec<Arc<dyn ExecutionHandler>> = vec![Arc::new(PipelineMoveStageHandler)];
        handlers.extend(lifecycle_handlers);
        let execution_registry = ExecutionRegistry::new(handlers)?;
        let executor = Arc::new(Executor::with_registry(
            store.clone(),
            execution_registry.clone(),
        ));
        let executor_health = TaskHealth::default();
        let (sender, receiver) = mpsc::channel(EXECUTION_QUEUE_CAPACITY);
        let (skill_registry, skill_discovery) = load_skill_registry(&llm_config)?;
        let concierge_build = build_concierge(store.ledger.clone(), llm_config)?;
        let bridge_engineer = match concierge_build.bridge_synthesis.as_ref() {
            Some(_) => RuntimeAvailability::Experimental(
                "TOKENKILLER mapping proposals are available; bridge activation remains unavailable"
                    .to_owned(),
            ),
            None => RuntimeAvailability::Disabled(
                "no Hydra LLM provider is configured; mapping synthesis is unavailable".to_owned(),
            ),
        };
        let bridge_conformance = bridge_lifecycle
            .as_ref()
            .map(|runtime| Arc::new(BridgeConformanceRuntime::new(runtime.clone())));
        Ok((
            Self {
                execution_registry,
                executor,
                dispatcher: Arc::new(ExecutorDispatcher { sender }),
                bridge_host,
                bridge_lifecycle,
                bridge_conformance,
                bridge_secrets,
                concierge: concierge_build.concierge,
                bridge_synthesis: concierge_build.bridge_synthesis,
                skill_registry,
                executor_health: executor_health.clone(),
                components: RuntimeComponentInventory {
                    bridge_host: RuntimeAvailability::Available,
                    bridge_secrets: bridge_secret_availability,
                    bridge_lifecycle: bridge_lifecycle_availability,
                    tokenkiller_router: concierge_build.tokenkiller_router,
                    data_steward: agent_availability(
                        &agents::data_steward::DataSteward::capability(),
                    ),
                    bridge_engineer,
                    comms_transport: agent_availability(
                        &agents::comms::Comms::transport_capability(),
                    ),
                    comms_draft: agent_availability(&agents::comms::Comms::draft_capability()),
                    skill_discovery,
                },
            },
            ExecutorWorker {
                receiver,
                store,
                health: executor_health,
            },
        ))
    }
}

fn build_bridge_lifecycle(
    store: &store::Store,
    bridge_host: Arc<bridge_host::BridgeHost>,
    config: &LlmRuntimeConfig,
    bridge_secrets: Arc<dyn bridge_host::SecretSource>,
    bridge_secret_availability: RuntimeAvailability,
) -> (
    Option<Arc<BridgeLifecycleRuntime>>,
    RuntimeAvailability,
    Vec<Arc<dyn ExecutionHandler>>,
) {
    let Some(path) = config.adapters_path.as_deref() else {
        return (
            None,
            RuntimeAvailability::Unavailable("HYDRA_ADAPTERS_PATH is not configured".to_owned()),
            Vec::new(),
        );
    };
    if !matches!(bridge_secret_availability, RuntimeAvailability::Available) {
        return (
            None,
            RuntimeAvailability::Unavailable(format!(
                "bridge lifecycle requires an available secret source: {bridge_secret_availability:?}"
            )),
            Vec::new(),
        );
    }
    let root = match bridge_host::ComponentRoot::new(path) {
        Ok(root) => root,
        Err(error) => {
            return (
                None,
                RuntimeAvailability::Unavailable(format!(
                    "configured adapter root is unavailable: {error}"
                )),
                Vec::new(),
            )
        }
    };
    let egress = match bridge_host::ReqwestEgressClient::new_with_proxy(
        config.egress_proxy_url.as_deref(),
    ) {
        Ok(client) => Arc::new(client) as Arc<dyn bridge_host::EgressClient>,
        Err(_) => {
            return (
                None,
                RuntimeAvailability::Unavailable(
                    "configured bridge egress client is unavailable".to_owned(),
                ),
                Vec::new(),
            )
        }
    };
    let lifecycle = Arc::new(bridge_host::BridgeLifecycle::new(
        bridge_host,
        root,
        store.adapter_kv.clone(),
        bridge_secrets,
        egress,
    ));
    let runtime = Arc::new(BridgeLifecycleRuntime::new(store.clone(), lifecycle));
    let handlers = runtime.handlers();
    (Some(runtime), RuntimeAvailability::Available, handlers)
}

fn load_skill_registry(
    config: &LlmRuntimeConfig,
) -> Result<
    (
        Option<Arc<agents::skills::SkillRegistry>>,
        RuntimeAvailability,
    ),
    RuntimeBuildError,
> {
    match (
        config.skills_path.as_deref(),
        config.skills_trust_file.as_deref(),
    ) {
        (None, None) => Ok((
            None,
            RuntimeAvailability::Disabled("signed skill discovery is not configured".to_owned()),
        )),
        (Some(path), Some(trust_file)) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let registry = agents::skills::SkillRegistry::load(path, trust_file, now)
                .map_err(|error| RuntimeBuildError::SkillConfig(error.to_string()))?;
            let availability = if registry.is_empty() {
                RuntimeAvailability::Unavailable(
                    "no trusted signed skills were loaded; invalid packages fail closed".to_owned(),
                )
            } else {
                RuntimeAvailability::Available
            };
            Ok((Some(Arc::new(registry)), availability))
        }
        _ => Err(RuntimeBuildError::SkillConfig(
            "HYDRA_SKILLS_PATH and HYDRA_SKILLS_TRUST_FILE must be configured together".to_owned(),
        )),
    }
}

fn agent_availability(descriptor: &agents::AgentCapabilityDescriptor) -> RuntimeAvailability {
    let reason = descriptor
        .reason
        .clone()
        .unwrap_or_else(|| format!("{} did not provide an availability reason", descriptor.name));
    match descriptor.availability {
        agents::AgentCapabilityAvailability::Available => RuntimeAvailability::Available,
        agents::AgentCapabilityAvailability::Experimental => {
            RuntimeAvailability::Experimental(reason)
        }
        agents::AgentCapabilityAvailability::Unavailable => {
            RuntimeAvailability::Unavailable(reason)
        }
    }
}

impl ExecutorWorker {
    pub async fn run(mut self, executor: Arc<Executor>, mut shutdown: watch::Receiver<bool>) {
        let _health_guard: TaskHealthGuard = self.health.start();
        let clock = RuntimeClock;
        let mut recovery_tick = tokio::time::interval(EXECUTION_RECOVERY_INTERVAL);
        recovery_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                token = self.receiver.recv() => {
                    let Some(token) = token else {
                        warn!("executor dispatch channel closed; stopping worker");
                        break;
                    };
                    if let Err(error) = executor.execute(token, &clock).await {
                        if !matches!(error, ExecuteError::EnvelopeNotApproved) {
                            error!(error = %error, "governed envelope execution failed");
                        }
                    }
                }
                _ = recovery_tick.tick() => {
                    self.recover_approved(&executor, &clock).await;
                }
            }
        }
    }

    async fn recover_approved(&self, executor: &Executor, clock: &RuntimeClock) {
        let approved = match self
            .store
            .envelopes
            .list_approved_all(EXECUTION_RECOVERY_BATCH_SIZE)
            .await
        {
            Ok(approved) => approved,
            Err(error) => {
                error!(error = %error, "approved envelope recovery scan failed");
                return;
            }
        };

        for envelope in approved {
            if let Err(error) = executor
                .execute_recovered(envelope.tenant_id, envelope.envelope_id, clock)
                .await
            {
                if !matches!(error, ExecuteError::EnvelopeNotApproved) {
                    error!(
                        tenant_id = %envelope.tenant_id,
                        envelope_id = %envelope.envelope_id,
                        error = %error,
                        "durable approved envelope recovery failed"
                    );
                }
            }
        }
    }
}

struct RuntimeClock;

impl governor::Clock for RuntimeClock {
    fn now(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }
}

#[derive(Clone)]
struct TokenkillerConcierge {
    router: Arc<llm_router::Router>,
    routes: HashMap<String, RouteCfg>,
    ledger: store::LedgerRepo,
}

/// Kernel-owned BridgeEngineer runtime. It is intentionally separate from
/// the Concierge service so route contracts and availability remain explicit.
pub struct BridgeSynthesisRuntime {
    router: Arc<llm_router::Router>,
    routes: HashMap<String, RouteCfg>,
    ledger: store::LedgerRepo,
}

#[async_trait]
impl fabric::BridgeSynthesisService for BridgeSynthesisRuntime {
    fn available(&self) -> bool {
        true
    }

    fn availability_reason(&self) -> &'static str {
        "TOKENKILLER bridge_mapping route is configured"
    }

    async fn synthesize(
        &self,
        tenant_id: Uuid,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, fabric::FabricError> {
        let request: agents::bridge_engineer::BridgeSynthesisRequest =
            serde_json::from_value(input).map_err(|_| {
                fabric::FabricError::ValidationFailed(
                    "bridge synthesis input does not match the bounded contract".to_owned(),
                )
            })?;
        let session = Session::new(
            tenant_id,
            self.routes.clone(),
            Box::new(SharedRouter(self.router.clone())),
            Box::new(StoreLedgerSink::new(self.ledger.clone())),
            Box::new(ApproxTokenizer),
            Box::new(tokenkiller::SystemClock),
        );
        let draft = agents::bridge_engineer::BridgeEngineer::synthesize(&session, request)
            .await
            .map_err(|error| match error {
                agents::bridge_engineer::AgentError::InputInvalid(_)
                | agents::bridge_engineer::AgentError::MappingInvalid(_) => {
                    fabric::FabricError::ValidationFailed(
                        "bridge synthesis input or mapping failed bounded validation".to_owned(),
                    )
                }
                agents::bridge_engineer::AgentError::Tokenkiller(_) => {
                    fabric::FabricError::CapabilityUnavailable(
                        "bridge synthesis provider is unavailable".to_owned(),
                    )
                }
                _ => fabric::FabricError::CapabilityUnavailable(
                    "bridge synthesis is unavailable".to_owned(),
                ),
            })?;
        serde_json::to_value(draft).map_err(|_| {
            fabric::FabricError::Internal(
                "bridge synthesis result could not be serialized".to_owned(),
            )
        })
    }
}

#[async_trait]
impl fabric::ConciergeService for TokenkillerConcierge {
    async fn ping(
        &self,
        tenant: Uuid,
        question: &str,
    ) -> Result<fabric::ConciergePingResponse, fabric::FabricError> {
        let session = Session::new(
            tenant,
            self.routes.clone(),
            Box::new(SharedRouter(self.router.clone())),
            Box::new(StoreLedgerSink::new(self.ledger.clone())),
            Box::new(ApproxTokenizer),
            Box::new(tokenkiller::SystemClock),
        );
        let contracted = session
            .complete(
                "concierge",
                vec![Segment {
                    stability: Stability::S0,
                    text: "You are HYDRA concierge ping service.".to_owned(),
                    version: 1,
                }],
                question.to_owned(),
            )
            .await
            .map_err(|error| {
                fabric::FabricError::LlmProviderError(format!(
                    "concierge TOKENKILLER call failed: {error}"
                ))
            })?;
        Ok(fabric::ConciergePingResponse {
            answer: contracted.raw,
            route: "concierge".to_owned(),
            provider: contracted.ledger_row.provider,
            tokens_used: u32::try_from(contracted.ledger_row.out_tokens).unwrap_or(u32::MAX),
        })
    }
}

struct UnavailableConcierge;

#[async_trait]
impl fabric::ConciergeService for UnavailableConcierge {
    async fn ping(
        &self,
        _tenant: Uuid,
        _question: &str,
    ) -> Result<fabric::ConciergePingResponse, fabric::FabricError> {
        Err(fabric::FabricError::CapabilityUnavailable(
            "TOKENKILLER LLM router is not configured".to_owned(),
        ))
    }
}

struct SharedRouter(Arc<llm_router::Router>);

#[async_trait]
impl tokenkiller::Router for SharedRouter {
    async fn complete(
        &self,
        request: tokenkiller::CompletionRequest,
    ) -> Result<tokenkiller::CompletionResponse, tokenkiller::RouterError> {
        tokenkiller::Router::complete(self.0.as_ref(), request).await
    }
}

fn build_concierge(
    ledger: store::LedgerRepo,
    config: LlmRuntimeConfig,
) -> Result<ConciergeBuild, RuntimeBuildError> {
    if config.openai_compat_base_url.is_some() && config.openai_compat_model.is_none() {
        return Err(RuntimeBuildError::LlmConfig(
            "OPENAI_COMPAT_MODEL is required with OPENAI_COMPAT_BASE_URL".to_owned(),
        ));
    }

    let mut providers: Vec<Box<dyn LlmProvider>> = Vec::new();
    match (
        config.nexus_model_gateway_url.as_ref(),
        config.nexus_model_gateway_model.as_ref(),
        config.nexus_model_gateway_token.as_ref(),
    ) {
        (Some(base_url), Some(model), Some(token)) => {
            providers.push(Box::new(
                NexusModelProvider::new_with_proxy(
                    base_url.clone(),
                    Some(token.clone()),
                    model.clone(),
                    config.nexus_model_gateway_private,
                    config.egress_proxy_url.as_deref(),
                )
                .map_err(RuntimeBuildError::LlmConfig)?,
            ));
        }
        (None, None, None) => {}
        _ => {
            return Err(RuntimeBuildError::LlmConfig(
                "NEXUS model gateway requires URL, model, and a vault-loaded token".to_owned(),
            ));
        }
    }
    if let Some(api_key) = config.deepseek_api_key {
        providers.push(Box::new(
            DeepSeekProvider::new_with_proxy(
                DEEPSEEK_BASE_URL,
                Some(api_key),
                config.egress_proxy_url.as_deref(),
            )
            .map_err(RuntimeBuildError::LlmConfig)?,
        ));
    }
    if let (Some(base_url), Some(model)) =
        (config.openai_compat_base_url, config.openai_compat_model)
    {
        providers.push(Box::new(
            OpenAiCompatProvider::new_with_proxy(
                "local",
                base_url,
                None,
                model,
                vec![Tag::Private],
                config.egress_proxy_url.as_deref(),
            )
            .map_err(RuntimeBuildError::LlmConfig)?,
        ));
    }
    if let Some(api_key) = config.anthropic_api_key {
        providers.push(Box::new(
            AnthropicProvider::new_with_proxy(
                ANTHROPIC_BASE_URL,
                Some(api_key),
                config.egress_proxy_url.as_deref(),
            )
            .map_err(RuntimeBuildError::LlmConfig)?,
        ));
    }

    if providers.is_empty() {
        return Ok(ConciergeBuild {
            concierge: Arc::new(UnavailableConcierge),
            tokenkiller_router: RuntimeAvailability::Disabled(
                "no Hydra LLM provider is configured; fake providers are test-only".to_owned(),
            ),
            bridge_synthesis: None,
        });
    }

    let provider_names = providers
        .iter()
        .map(|provider| provider.name().to_owned())
        .collect::<Vec<_>>();
    let primary_provider = provider_names
        .first()
        .cloned()
        .ok_or_else(|| RuntimeBuildError::LlmConfig("provider chain is empty".to_owned()))?;
    let output_budget_bytes = if config.output_budget_bytes == 0 {
        16_384
    } else {
        config.output_budget_bytes
    };
    let provider_tags = if primary_provider == "local"
        || (primary_provider == "nexus" && config.nexus_model_gateway_private)
    {
        vec![ProviderTag::Private]
    } else {
        Vec::new()
    };
    let mapping_output_budget_bytes = output_budget_bytes.min(2_048);
    let router = Arc::new(llm_router::Router::new(
        HashMap::from([
            (
                "concierge".to_owned(),
                llm_router::RouteCfg {
                    name: "concierge".to_owned(),
                    pii: false,
                    max_tokens: 256,
                    output_budget_bytes,
                    providers: provider_names.clone(),
                    tk_exempt: false,
                },
            ),
            (
                "bridge_mapping".to_owned(),
                llm_router::RouteCfg {
                    name: "bridge_mapping".to_owned(),
                    pii: false,
                    max_tokens: 128,
                    output_budget_bytes: mapping_output_budget_bytes,
                    providers: provider_names.clone(),
                    tk_exempt: false,
                },
            ),
        ]),
        providers,
    ));
    let routes = HashMap::from([
        (
            "concierge".to_owned(),
            RouteCfg {
                provider: primary_provider.clone(),
                provider_tags: provider_tags.clone(),
                max_tokens: 256,
                output_budget_bytes,
                contract: Contract::PlainAnswer,
                pii: false,
            },
        ),
        (
            "bridge_mapping".to_owned(),
            RouteCfg {
                provider: primary_provider,
                provider_tags,
                max_tokens: 128,
                output_budget_bytes: mapping_output_budget_bytes,
                contract: Contract::MappingYaml,
                pii: false,
            },
        ),
    ]);
    let bridge_synthesis = Some(Arc::new(BridgeSynthesisRuntime {
        router: router.clone(),
        routes: routes.clone(),
        ledger: ledger.clone(),
    }));
    Ok(ConciergeBuild {
        concierge: Arc::new(TokenkillerConcierge {
            router,
            routes,
            ledger,
        }),
        tokenkiller_router: RuntimeAvailability::Available,
        bridge_synthesis,
    })
}
