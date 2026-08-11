use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use llm_router::providers::anthropic::AnthropicProvider;
use llm_router::providers::deepseek::DeepSeekProvider;
use llm_router::providers::openai_compat::OpenAiCompatProvider;
use llm_router::{LlmProvider, Tag};
use tokenkiller::{
    ApproxTokenizer, Contract, ProviderTag, RouteCfg, Segment, Session, Stability, StoreLedgerSink,
};
use tokio::sync::{mpsc, watch};
use tracing::{error, warn};
use uuid::Uuid;

use crate::execution_registry::{ExecutionHandler, ExecutionRegistry, ExecutionRegistryError};
use crate::executor::{Executor, PipelineMoveStageHandler};

const EXECUTION_QUEUE_CAPACITY: usize = 256;
const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";

#[derive(Clone, Debug, Default)]
pub struct LlmRuntimeConfig {
    pub deepseek_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_compat_base_url: Option<String>,
    pub openai_compat_model: Option<String>,
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
    pub bridge_lifecycle: RuntimeAvailability,
    pub tokenkiller_router: RuntimeAvailability,
    pub data_steward: RuntimeAvailability,
    pub bridge_engineer: RuntimeAvailability,
    pub comms_transport: RuntimeAvailability,
    pub comms_draft: RuntimeAvailability,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeBuildError {
    #[error(transparent)]
    Registry(#[from] ExecutionRegistryError),
    #[error("failed to construct Wasmtime bridge host: {0}")]
    BridgeHost(String),
    #[error("invalid LLM runtime configuration: {0}")]
    LlmConfig(String),
}

pub struct RuntimeServices {
    pub execution_registry: ExecutionRegistry,
    pub executor: Arc<Executor>,
    pub dispatcher: Arc<ExecutorDispatcher>,
    pub bridge_host: Arc<bridge_host::BridgeHost>,
    pub concierge: Arc<dyn fabric::ConciergeService>,
    pub components: RuntimeComponentInventory,
}

pub struct ExecutorWorker {
    receiver: mpsc::Receiver<governor::ExecuteToken>,
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
        let handlers: Vec<Arc<dyn ExecutionHandler>> = vec![Arc::new(PipelineMoveStageHandler)];
        let execution_registry = ExecutionRegistry::new(handlers)?;
        let executor = Arc::new(Executor::with_registry(
            store.clone(),
            execution_registry.clone(),
        ));
        let (sender, receiver) = mpsc::channel(EXECUTION_QUEUE_CAPACITY);
        let bridge_host = Arc::new(
            bridge_host::BridgeHost::new()
                .map_err(|error| RuntimeBuildError::BridgeHost(error.to_string()))?,
        );
        let (concierge, tokenkiller_router) = build_concierge(store.ledger.clone(), llm_config)?;
        Ok((
            Self {
                execution_registry,
                executor,
                dispatcher: Arc::new(ExecutorDispatcher { sender }),
                bridge_host,
                concierge,
                components: RuntimeComponentInventory {
                    bridge_host: RuntimeAvailability::Available,
                    bridge_lifecycle: RuntimeAvailability::Unavailable(
                        "Wasmtime host construction is available, but no persisted adapter lifecycle execution handler is registered"
                            .to_owned(),
                    ),
                    tokenkiller_router,
                    data_steward: agent_availability(
                        &agents::data_steward::DataSteward::capability(),
                    ),
                    bridge_engineer: agent_availability(
                        &agents::bridge_engineer::BridgeEngineer::capability(),
                    ),
                    comms_transport: agent_availability(
                        &agents::comms::Comms::transport_capability(),
                    ),
                    comms_draft: agent_availability(
                        &agents::comms::Comms::draft_capability(),
                    ),
                },
            },
            ExecutorWorker { receiver },
        ))
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
        let clock = RuntimeClock;
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
                        error!(error = %error, "governed envelope execution failed");
                    }
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
) -> Result<(Arc<dyn fabric::ConciergeService>, RuntimeAvailability), RuntimeBuildError> {
    if config.openai_compat_base_url.is_some() && config.openai_compat_model.is_none() {
        return Err(RuntimeBuildError::LlmConfig(
            "OPENAI_COMPAT_MODEL is required with OPENAI_COMPAT_BASE_URL".to_owned(),
        ));
    }

    let mut providers: Vec<Box<dyn LlmProvider>> = Vec::new();
    if let Some(api_key) = config.deepseek_api_key {
        providers.push(Box::new(DeepSeekProvider::new(
            DEEPSEEK_BASE_URL,
            Some(api_key),
        )));
    }
    if let (Some(base_url), Some(model)) =
        (config.openai_compat_base_url, config.openai_compat_model)
    {
        providers.push(Box::new(OpenAiCompatProvider::new(
            "local",
            base_url,
            None,
            model,
            vec![Tag::Private],
        )));
    }
    if let Some(api_key) = config.anthropic_api_key {
        providers.push(Box::new(AnthropicProvider::new(
            ANTHROPIC_BASE_URL,
            Some(api_key),
        )));
    }

    if providers.is_empty() {
        return Ok((
            Arc::new(UnavailableConcierge),
            RuntimeAvailability::Disabled(
                "no Hydra LLM provider is configured; fake providers are test-only".to_owned(),
            ),
        ));
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
    let router = Arc::new(llm_router::Router::new(
        HashMap::from([(
            "concierge".to_owned(),
            llm_router::RouteCfg {
                name: "concierge".to_owned(),
                pii: false,
                max_tokens: 256,
                output_budget_bytes,
                providers: provider_names.clone(),
                tk_exempt: false,
            },
        )]),
        providers,
    ));
    let routes = HashMap::from([(
        "concierge".to_owned(),
        RouteCfg {
            provider: primary_provider.clone(),
            provider_tags: if primary_provider == "local" {
                vec![ProviderTag::Private]
            } else {
                Vec::new()
            },
            max_tokens: 256,
            output_budget_bytes,
            contract: Contract::PlainAnswer,
            pii: false,
        },
    )]);
    Ok((
        Arc::new(TokenkillerConcierge {
            router,
            routes,
            ledger,
        }),
        RuntimeAvailability::Available,
    ))
}
