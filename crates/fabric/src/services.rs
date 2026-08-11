use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::HeaderMap;
use cdm::Entity;
use governor::{
    ActionEnvelope, BlastRadius, Cell, Clock, Constitution, Decision, EnvelopeState, Governor,
    Level, PolicyMatrix, Reversal, SpendSnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use time::OffsetDateTime;
use tokenkiller::{
    ApproxTokenizer, CacheUsage, CompletionRequest, CompletionResponse, Contract, LedgerRow,
    LedgerSink, ProviderTag, RouteCfg, Router as TkRouter, RouterError, Segment, Session,
    Stability,
};
use uuid::Uuid;

use crate::auth::{
    AuthCtx, AuthorizationService, OidcAuthenticator, PrincipalContext, PrincipalType, Role,
    SessionStore,
};
use crate::capabilities::{CapabilityDescriptor, CapabilityRegistry, IdempotencySemantics};
use crate::error::FabricError;
use crate::rate::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<SessionStore>,
    pub entities: Arc<dyn EntityService>,
    pub autonomy: Arc<dyn AutonomyService>,
    pub bridges: Arc<dyn BridgeService>,
    pub envelopes: Arc<dyn EnvelopeService>,
    pub tk_stats: Arc<dyn TkStatsService>,
    pub concierge: Arc<dyn ConciergeService>,
    pub authorization: Arc<AuthorizationService>,
    pub capabilities: Arc<CapabilityRegistry>,
    pub external_auth: Option<Arc<OidcAuthenticator>>,
    pub rate_limiter: Arc<RateLimiter>,
    pub nexus_control_plane: Arc<NexusControlPlaneConfig>,
    pub event_status: Arc<dyn EventStatusService>,
    pub allow_development_identity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct EventInfrastructureStatus {
    pub available: bool,
    pub contract_version: Option<String>,
    pub stream: Option<String>,
    pub jetstream_acknowledged_publish: bool,
    pub relay_operational: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[async_trait]
pub trait EventStatusService: Send + Sync {
    async fn status(&self) -> Result<EventInfrastructureStatus, FabricError>;
}

struct UnavailableEventStatusService;

#[async_trait]
impl EventStatusService for UnavailableEventStatusService {
    async fn status(&self) -> Result<EventInfrastructureStatus, FabricError> {
        Ok(EventInfrastructureStatus {
            available: false,
            contract_version: None,
            stream: None,
            jetstream_acknowledged_publish: false,
            relay_operational: false,
            reason: Some("canonical event infrastructure is not configured".to_owned()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NexusControlPlaneConfig {
    pub enabled: bool,
    pub resource: String,
    pub resource_metadata_url: String,
    pub authorization_servers: Vec<String>,
    pub allowed_hosts: Vec<String>,
    pub allowed_origins: Vec<String>,
    pub max_request_body_bytes: usize,
}

impl Default for NexusControlPlaneConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            resource: "http://localhost/mcp".to_owned(),
            resource_metadata_url: "http://localhost/.well-known/oauth-protected-resource"
                .to_owned(),
            authorization_servers: Vec::new(),
            allowed_hosts: vec![
                "localhost".to_owned(),
                "127.0.0.1".to_owned(),
                "::1".to_owned(),
            ],
            allowed_origins: Vec::new(),
            max_request_body_bytes: 1_048_576,
        }
    }
}

impl AppState {
    pub fn new(
        auth: Arc<SessionStore>,
        entities: Arc<dyn EntityService>,
        autonomy: Arc<dyn AutonomyService>,
        bridges: Arc<dyn BridgeService>,
        envelopes: Arc<dyn EnvelopeService>,
        tk_stats: Arc<dyn TkStatsService>,
        concierge: Arc<dyn ConciergeService>,
    ) -> Self {
        Self {
            auth,
            entities,
            autonomy,
            bridges,
            envelopes,
            tk_stats,
            concierge,
            authorization: Arc::new(AuthorizationService::default()),
            capabilities: Arc::new(CapabilityRegistry::default()),
            external_auth: None,
            rate_limiter: Arc::new(RateLimiter::new(60, 60)),
            nexus_control_plane: Arc::new(NexusControlPlaneConfig::default()),
            event_status: Arc::new(UnavailableEventStatusService),
            allow_development_identity: false,
        }
    }

    pub fn with_authorization(mut self, authorization: Arc<AuthorizationService>) -> Self {
        self.authorization = authorization;
        self
    }

    pub fn with_capabilities(mut self, capabilities: Arc<CapabilityRegistry>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_external_auth(mut self, external_auth: Arc<OidcAuthenticator>) -> Self {
        self.external_auth = Some(external_auth);
        self
    }

    pub fn with_rate_limiter(mut self, rate_limiter: Arc<RateLimiter>) -> Self {
        self.rate_limiter = rate_limiter;
        self
    }

    pub fn with_nexus_control_plane(mut self, config: NexusControlPlaneConfig) -> Self {
        self.nexus_control_plane = Arc::new(config);
        self
    }

    pub fn with_event_status(mut self, event_status: Arc<dyn EventStatusService>) -> Self {
        self.event_status = event_status;
        self
    }

    pub fn with_development_identity(mut self, allowed: bool) -> Self {
        self.allow_development_identity = allowed;
        self
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnvelopeCreateRequest {
    pub domain: String,
    pub action: String,
    pub kind: Option<String>,
    pub targets: Vec<Uuid>,
    pub payload: Value,
    pub rationale: String,
    pub reversal: Reversal,
    pub blast: BlastRadiusDto,
}

#[derive(Debug, Clone)]
pub struct GovernedExternalProposal {
    pub request: EnvelopeCreateRequest,
    pub idempotency_key: String,
    pub request_hash: String,
    pub objective_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeApprovalDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EnvelopeApprovalRequest {
    pub decision: EnvelopeApprovalDecision,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EnvelopeApprovalReceipt {
    pub approval_id: Uuid,
    pub envelope_id: Uuid,
    pub state: EnvelopeState,
    pub decision: EnvelopeApprovalDecision,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlastRadiusDto {
    pub entities: u32,
    pub external_sends: u32,
    pub money_cents: u64,
    pub pii_egress: bool,
}

impl From<BlastRadiusDto> for BlastRadius {
    fn from(value: BlastRadiusDto) -> Self {
        Self {
            entities: value.entities,
            external_sends: value.external_sends,
            money_cents: value.money_cents,
            pii_egress: value.pii_egress,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct TkRouteStat {
    pub route: String,
    pub hit_ratio: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct TkWindowStats {
    pub window: String,
    pub routes: Vec<TkRouteStat>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EntityDeleteResponse {
    pub id: Uuid,
    pub kind: String,
    pub version: u64,
    pub deleted: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AutonomyCellDto {
    pub domain: String,
    pub action: String,
    pub kind: Option<String>,
    pub level: String,
    pub cfg: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BridgeGrantDto {
    pub origins: Vec<String>,
    pub secret_names: Vec<String>,
    pub dsn_name: Option<String>,
    pub fuel: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BridgeRegisterRequest {
    pub adapter_id: String,
    pub wiring_ref: String,
    pub rationale: String,
    pub grant: BridgeGrantDto,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BridgeStatusDto {
    pub adapter_id: String,
    pub state: String,
    pub envelope_id: Option<Uuid>,
    pub envelope_state: Option<String>,
    pub wiring_ref: Option<String>,
}

#[async_trait]
pub trait EnvelopeService: Send + Sync {
    async fn list(
        &self,
        tenant: Uuid,
        state: EnvelopeState,
    ) -> Result<Vec<ActionEnvelope>, FabricError>;

    async fn propose(
        &self,
        tenant: Uuid,
        request: EnvelopeCreateRequest,
    ) -> Result<ActionEnvelope, FabricError>;

    async fn get(&self, _tenant: Uuid, _id: Uuid) -> Result<ActionEnvelope, FabricError> {
        Err(FabricError::CapabilityUnavailable(
            "tenant-scoped envelope lookup is not wired".to_owned(),
        ))
    }

    async fn propose_external(
        &self,
        _principal: &PrincipalContext,
        _capability: &CapabilityDescriptor,
        _proposal: GovernedExternalProposal,
    ) -> Result<ActionEnvelope, FabricError> {
        Err(FabricError::CapabilityUnavailable(
            "governed external proposal service is not wired".to_owned(),
        ))
    }

    async fn decide_external_approval(
        &self,
        _principal: &PrincipalContext,
        _id: Uuid,
        _request: EnvelopeApprovalRequest,
    ) -> Result<EnvelopeApprovalReceipt, FabricError> {
        Err(FabricError::CapabilityUnavailable(
            "external approval service is not wired".to_owned(),
        ))
    }

    async fn approve(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        id: Uuid,
    ) -> Result<ActionEnvelope, FabricError>;

    async fn reject(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        id: Uuid,
    ) -> Result<ActionEnvelope, FabricError>;
}

#[async_trait]
pub trait GovernorProvider: Send + Sync {
    async fn governor(&self, tenant: Uuid) -> Result<Arc<Governor>, FabricError>;
}

#[async_trait]
pub trait ExecutionDispatcher: Send + Sync {
    async fn dispatch(&self, token: governor::ExecuteToken) -> Result<(), FabricError>;
}

struct StaticGovernorProvider {
    governor: Arc<Governor>,
}

#[async_trait]
impl GovernorProvider for StaticGovernorProvider {
    async fn governor(&self, _tenant: Uuid) -> Result<Arc<Governor>, FabricError> {
        Ok(self.governor.clone())
    }
}

#[async_trait]
pub trait EntityService: Send + Sync {
    async fn list(
        &self,
        tenant: Uuid,
        kind: &str,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> Result<Vec<Entity>, FabricError>;

    async fn get(&self, tenant: Uuid, kind: &str, id: Uuid) -> Result<Entity, FabricError>;

    async fn create(&self, tenant: Uuid, kind: &str, body: Value) -> Result<Entity, FabricError>;

    async fn patch(
        &self,
        tenant: Uuid,
        kind: &str,
        id: Uuid,
        expected_version: u64,
        patch: Value,
    ) -> Result<Entity, FabricError>;

    async fn delete(
        &self,
        tenant: Uuid,
        kind: &str,
        id: Uuid,
    ) -> Result<EntityDeleteResponse, FabricError>;
}

#[async_trait]
pub trait AutonomyService: Send + Sync {
    async fn list(&self, tenant: Uuid) -> Result<Vec<AutonomyCellDto>, FabricError>;

    async fn replace(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        actor: &str,
        cells: Vec<AutonomyCellDto>,
    ) -> Result<Vec<AutonomyCellDto>, FabricError>;
}

#[async_trait]
pub trait BridgeService: Send + Sync {
    async fn register(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        actor: &str,
        request: BridgeRegisterRequest,
    ) -> Result<ActionEnvelope, FabricError>;

    async fn status(&self, tenant: Uuid, adapter_id: &str) -> Result<BridgeStatusDto, FabricError>;

    async fn pause(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        actor: &str,
        adapter_id: &str,
    ) -> Result<BridgeStatusDto, FabricError>;

    async fn resume(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        actor: &str,
        adapter_id: &str,
    ) -> Result<BridgeStatusDto, FabricError>;
}

#[async_trait]
pub trait TkStatsService: Send + Sync {
    async fn window(&self, window: &str) -> Result<TkWindowStats, FabricError>;
}

pub struct StoreEnvelopeService {
    store: store::Store,
    governors: Arc<dyn GovernorProvider>,
    dispatcher: Option<Arc<dyn ExecutionDispatcher>>,
    authorization: Arc<AuthorizationService>,
}

impl StoreEnvelopeService {
    pub fn new(store: store::Store, governor: Governor) -> Self {
        Self::with_governor_provider(
            store,
            Arc::new(StaticGovernorProvider {
                governor: Arc::new(governor),
            }),
        )
    }

    pub fn with_governor_provider(
        store: store::Store,
        governors: Arc<dyn GovernorProvider>,
    ) -> Self {
        Self {
            store,
            governors,
            dispatcher: None,
            authorization: Arc::new(AuthorizationService::default()),
        }
    }

    pub fn with_execution_dispatcher(mut self, dispatcher: Arc<dyn ExecutionDispatcher>) -> Self {
        self.dispatcher = Some(dispatcher);
        self
    }

    pub fn with_authorization(mut self, authorization: Arc<AuthorizationService>) -> Self {
        self.authorization = authorization;
        self
    }

    async fn token_after_human_approval(
        &self,
        envelope: &ActionEnvelope,
    ) -> Result<governor::ExecuteToken, FabricError> {
        let spend = SpendSnapshot {
            month_to_date_cents: self
                .store
                .ledger
                .month_to_date_cents(envelope.tenant, month_start())
                .await?,
        };
        let governor = self.governors.governor(envelope.tenant).await?;
        match governor.authorize_after_human_approval(envelope, &spend) {
            Decision::Execute(token) => Ok(token),
            Decision::Block(reason) => Err(FabricError::ConstitutionBlocked(reason)),
            Decision::SuggestOnly | Decision::Queue => Err(FabricError::Internal(
                "post-approval Governor returned a non-terminal decision".to_owned(),
            )),
        }
    }
}

#[async_trait]
impl EnvelopeService for StoreEnvelopeService {
    async fn list(
        &self,
        tenant: Uuid,
        state: EnvelopeState,
    ) -> Result<Vec<ActionEnvelope>, FabricError> {
        Ok(self.store.envelopes.list(tenant, state).await?)
    }

    async fn propose(
        &self,
        tenant: Uuid,
        request: EnvelopeCreateRequest,
    ) -> Result<ActionEnvelope, FabricError> {
        validate_request(&request)?;
        let spend = SpendSnapshot {
            month_to_date_cents: self
                .store
                .ledger
                .month_to_date_cents(tenant, month_start())
                .await
                .map_err(FabricError::from)?,
        };
        let envelope = ActionEnvelope {
            id: Uuid::new_v4(),
            tenant,
            domain: request.domain,
            action: request.action,
            kind: request.kind,
            targets: request.targets,
            payload: request.payload,
            rationale: request.rationale,
            reversal: request.reversal,
            blast: request.blast.into(),
            invocation: governor::InvocationContext::default(),
            state: EnvelopeState::Proposed,
            history: Vec::new(),
        };

        let governor = self.governors.governor(tenant).await?;
        let decision = governor.evaluate(&envelope, &spend);
        if let Decision::Block(reason) = &decision {
            return Err(FabricError::ConstitutionBlocked(reason.clone()));
        }
        let mut execute_token = None;
        let mut envelope = self.store.envelopes.save(tenant, &envelope).await?;
        match decision {
            Decision::Block(_) => unreachable!("blocked decisions return before persistence"),
            Decision::SuggestOnly => {}
            Decision::Queue => {
                envelope = self
                    .store
                    .envelopes
                    .transition(
                        tenant,
                        envelope.id,
                        EnvelopeState::PendingApproval,
                        "governor",
                        &SystemClock,
                    )
                    .await?
            }
            Decision::Execute(token) => {
                envelope = self
                    .store
                    .envelopes
                    .transition(
                        tenant,
                        envelope.id,
                        EnvelopeState::Approved,
                        "governor",
                        &SystemClock,
                    )
                    .await?;
                execute_token = Some(token);
            }
        }

        if let (Some(dispatcher), Some(token)) = (&self.dispatcher, execute_token) {
            dispatcher.dispatch(token).await?;
        }

        Ok(envelope)
    }

    async fn get(&self, tenant: Uuid, id: Uuid) -> Result<ActionEnvelope, FabricError> {
        Ok(self.store.envelopes.get(tenant, id).await?)
    }

    async fn propose_external(
        &self,
        principal: &PrincipalContext,
        capability: &CapabilityDescriptor,
        proposal: GovernedExternalProposal,
    ) -> Result<ActionEnvelope, FabricError> {
        validate_external_proposal(principal, capability, &proposal)?;
        let tenant = principal.hydra_tenant_id;
        let spend = SpendSnapshot {
            month_to_date_cents: self
                .store
                .ledger
                .month_to_date_cents(tenant, month_start())
                .await?,
        };
        let mut envelope = ActionEnvelope {
            id: Uuid::new_v4(),
            tenant,
            domain: proposal.request.domain,
            action: proposal.request.action,
            kind: proposal.request.kind,
            targets: proposal.request.targets,
            payload: proposal.request.payload,
            rationale: proposal.request.rationale,
            reversal: proposal.request.reversal,
            blast: proposal.request.blast.into(),
            invocation: governor::InvocationContext {
                request_id: Some(principal.correlation.request_id.clone()),
                correlation_id: Some(principal.correlation.correlation_id.clone()),
                causation_id: principal.correlation.causation_id.clone(),
                origin_system: principal.external_provider.clone(),
                external_actor_id: Some(principal.principal_id.clone()),
                external_actor_type: Some(principal_type_name(principal.principal_type).to_owned()),
                external_binding_id: principal.binding_id,
                objective_id: proposal.objective_id,
                task_id: proposal.task_id,
                approval_id: None,
                idempotency_key: Some(proposal.idempotency_key.clone()),
            },
            state: EnvelopeState::Proposed,
            history: Vec::new(),
        };

        let governor = self.governors.governor(tenant).await?;
        let mut execute_token = None;
        match governor.evaluate(&envelope, &spend) {
            Decision::Block(reason) => return Err(FabricError::ConstitutionBlocked(reason)),
            Decision::SuggestOnly => {}
            Decision::Queue => {
                envelope.transition(EnvelopeState::PendingApproval, "governor", &SystemClock)?
            }
            Decision::Execute(token) => {
                envelope.transition(EnvelopeState::Approved, "governor", &SystemClock)?;
                execute_token = Some(token);
            }
        }

        let resolution = self
            .store
            .idempotency
            .resolve_or_create_envelope_with_trace(
                store::NewIdempotencyRecord {
                    tenant_id: tenant,
                    origin_system: principal
                        .external_provider
                        .clone()
                        .ok_or(FabricError::AuthzDenied)?,
                    idempotency_key: proposal.idempotency_key,
                    capability: capability.name.clone(),
                    request_hash: proposal.request_hash,
                    envelope_id: envelope.id,
                },
                &envelope,
                Some(&principal.trace),
            )
            .await?;

        if matches!(&resolution, store::IdempotencyResolution::Recorded(_)) {
            if let (Some(dispatcher), Some(token)) = (&self.dispatcher, execute_token) {
                dispatcher.dispatch(token).await?;
            }
        }

        Ok(self
            .store
            .envelopes
            .get(tenant, resolution.record().envelope_id)
            .await?)
    }

    async fn decide_external_approval(
        &self,
        principal: &PrincipalContext,
        id: Uuid,
        request: EnvelopeApprovalRequest,
    ) -> Result<EnvelopeApprovalReceipt, FabricError> {
        let tenant = principal.hydra_tenant_id;
        let envelope = self.store.envelopes.get(tenant, id).await?;
        let proposer = envelope
            .invocation
            .external_actor_id
            .as_deref()
            .ok_or(FabricError::AuthzDenied)?;
        self.authorization
            .authorize_external_approval(principal, tenant, proposer)?;
        let execute_token = if request.decision == EnvelopeApprovalDecision::Approve {
            Some(self.token_after_human_approval(&envelope).await?)
        } else {
            None
        };
        let assertion_id = Uuid::new_v4();
        let assertion = store::NewApprovalAssertion {
            id: assertion_id,
            tenant_id: tenant,
            envelope_id: id,
            human_actor_id: principal.principal_id.clone(),
            delegated_by: principal
                .delegated_by
                .clone()
                .ok_or(FabricError::AuthzDenied)?,
            authentication_strength: principal
                .authentication_strength
                .clone()
                .ok_or(FabricError::AuthzDenied)?,
            request_id: Some(principal.correlation.request_id.clone()),
            correlation_id: Some(principal.correlation.correlation_id.clone()),
            objective_id: envelope.invocation.objective_id.clone(),
            task_id: envelope.invocation.task_id.clone(),
            decision: match request.decision {
                EnvelopeApprovalDecision::Approve => store::ApprovalDecision::Approved,
                EnvelopeApprovalDecision::Reject => store::ApprovalDecision::Rejected,
            },
            comment: request.comment,
        };
        let (_, envelope) = self
            .store
            .approvals
            .create_and_transition_with_trace(
                assertion,
                &principal.principal_id,
                &SystemClock,
                Some(&principal.trace),
            )
            .await?;
        if let (Some(dispatcher), Some(token)) = (&self.dispatcher, execute_token) {
            dispatcher.dispatch(token).await?;
        }
        Ok(EnvelopeApprovalReceipt {
            approval_id: assertion_id,
            envelope_id: envelope.id,
            state: envelope.state,
            decision: request.decision,
        })
    }

    async fn approve(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        id: Uuid,
    ) -> Result<ActionEnvelope, FabricError> {
        ctx.require_role(Role::Approver)?;
        let envelope = self.store.envelopes.get(tenant, id).await?;
        // Four-eyes: proposer cannot approve their own envelope
        let proposed_by = envelope
            .history
            .first()
            .map(|t| t.actor.as_str())
            .unwrap_or("");
        if ctx.principal == proposed_by {
            return Err(FabricError::AuthzDenied);
        }
        let token = self.token_after_human_approval(&envelope).await?;
        let (_, envelope) = self
            .store
            .approvals
            .create_and_transition(
                store::NewApprovalAssertion {
                    id: Uuid::new_v4(),
                    tenant_id: tenant,
                    envelope_id: id,
                    human_actor_id: ctx.principal.clone(),
                    delegated_by: "hydra-local-auth".to_owned(),
                    authentication_strength: "local-session".to_owned(),
                    request_id: None,
                    correlation_id: envelope.invocation.correlation_id.clone(),
                    objective_id: envelope.invocation.objective_id.clone(),
                    task_id: envelope.invocation.task_id.clone(),
                    decision: store::ApprovalDecision::Approved,
                    comment: None,
                },
                &ctx.principal,
                &SystemClock,
            )
            .await?;
        if let Some(dispatcher) = &self.dispatcher {
            dispatcher.dispatch(token).await?;
        }
        Ok(envelope)
    }

    async fn reject(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        id: Uuid,
    ) -> Result<ActionEnvelope, FabricError> {
        ctx.require_role(Role::Approver)?;
        let envelope = self.store.envelopes.get(tenant, id).await?;
        // Four-eyes: proposer cannot reject their own envelope
        let proposed_by = envelope
            .history
            .first()
            .map(|t| t.actor.as_str())
            .unwrap_or("");
        if ctx.principal == proposed_by {
            return Err(FabricError::AuthzDenied);
        }
        let (_, envelope) = self
            .store
            .approvals
            .create_and_transition(
                store::NewApprovalAssertion {
                    id: Uuid::new_v4(),
                    tenant_id: tenant,
                    envelope_id: id,
                    human_actor_id: ctx.principal.clone(),
                    delegated_by: "hydra-local-auth".to_owned(),
                    authentication_strength: "local-session".to_owned(),
                    request_id: None,
                    correlation_id: envelope.invocation.correlation_id.clone(),
                    objective_id: envelope.invocation.objective_id.clone(),
                    task_id: envelope.invocation.task_id.clone(),
                    decision: store::ApprovalDecision::Rejected,
                    comment: None,
                },
                &ctx.principal,
                &SystemClock,
            )
            .await?;
        Ok(envelope)
    }
}

pub struct StoreEntityService {
    store: store::Store,
}

impl StoreEntityService {
    pub fn new(store: store::Store) -> Self {
        Self { store }
    }
}

#[async_trait]
impl EntityService for StoreEntityService {
    async fn list(
        &self,
        tenant: Uuid,
        kind: &str,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> Result<Vec<Entity>, FabricError> {
        Ok(self
            .store
            .entities
            .list(tenant, kind, cursor, i64::from(limit))
            .await?)
    }

    async fn get(&self, tenant: Uuid, kind: &str, id: Uuid) -> Result<Entity, FabricError> {
        let entity = self.store.entities.get(tenant, id).await?;
        ensure_kind(&entity, kind)?;
        Ok(entity)
    }

    async fn create(&self, tenant: Uuid, kind: &str, body: Value) -> Result<Entity, FabricError> {
        Ok(self
            .store
            .entities
            .upsert(
                tenant,
                Entity {
                    id: Uuid::new_v4(),
                    kind: kind.to_owned(),
                    tenant,
                    body,
                    origin: "native".into(),
                    origin_ref: None,
                    version: 1,
                },
            )
            .await?)
    }

    async fn patch(
        &self,
        tenant: Uuid,
        kind: &str,
        id: Uuid,
        expected_version: u64,
        patch: Value,
    ) -> Result<Entity, FabricError> {
        let entity = self.store.entities.get(tenant, id).await?;
        ensure_kind(&entity, kind)?;
        if entity.version != expected_version {
            return Err(FabricError::VersionConflict);
        }

        let mut body = entity.body.clone();
        apply_merge_patch(&mut body, patch);
        Ok(self
            .store
            .entities
            .upsert(
                tenant,
                Entity {
                    id: entity.id,
                    kind: entity.kind,
                    tenant: entity.tenant,
                    body,
                    origin: entity.origin,
                    origin_ref: entity.origin_ref,
                    version: entity.version + 1,
                },
            )
            .await?)
    }

    async fn delete(
        &self,
        tenant: Uuid,
        kind: &str,
        id: Uuid,
    ) -> Result<EntityDeleteResponse, FabricError> {
        let entity = self.store.entities.get(tenant, id).await?;
        ensure_kind(&entity, kind)?;
        self.store.entities.soft_delete(tenant, id).await?;
        Ok(EntityDeleteResponse {
            id,
            kind: entity.kind,
            version: entity.version + 1,
            deleted: true,
        })
    }
}

pub struct StoreAutonomyService {
    store: store::Store,
}

impl StoreAutonomyService {
    pub fn new(store: store::Store) -> Self {
        Self { store }
    }
}

#[async_trait]
impl AutonomyService for StoreAutonomyService {
    async fn list(&self, tenant: Uuid) -> Result<Vec<AutonomyCellDto>, FabricError> {
        let cells = self.store.autonomy.list(tenant).await?;
        cells.into_iter().map(dto_from_stored_cell).collect()
    }

    async fn replace(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        actor: &str,
        cells: Vec<AutonomyCellDto>,
    ) -> Result<Vec<AutonomyCellDto>, FabricError> {
        ctx.require_role(Role::Admin)?;
        validate_autonomy_cells(&cells)?;
        let stored = cells
            .iter()
            .map(stored_cell_from_dto)
            .collect::<Result<Vec<_>, _>>()?;
        self.store
            .autonomy
            .replace_cells(tenant, actor, &stored)
            .await?;
        self.list(tenant).await
    }
}

pub struct StoreBridgeService {
    store: store::Store,
    envelopes: StoreEnvelopeService,
}

impl StoreBridgeService {
    pub fn new(store: store::Store, governor: Governor) -> Self {
        Self::with_governor_provider(
            store,
            Arc::new(StaticGovernorProvider {
                governor: Arc::new(governor),
            }),
        )
    }

    pub fn with_governor_provider(
        store: store::Store,
        governors: Arc<dyn GovernorProvider>,
    ) -> Self {
        Self {
            envelopes: StoreEnvelopeService::with_governor_provider(store.clone(), governors),
            store,
        }
    }

    pub fn with_runtime(
        store: store::Store,
        governors: Arc<dyn GovernorProvider>,
        dispatcher: Arc<dyn ExecutionDispatcher>,
    ) -> Self {
        Self {
            envelopes: StoreEnvelopeService::with_governor_provider(store.clone(), governors)
                .with_execution_dispatcher(dispatcher),
            store,
        }
    }

    async fn is_paused(&self, tenant: Uuid, adapter_id: &str) -> Result<bool, FabricError> {
        let scoped = scoped_bridge_key(tenant, adapter_id);
        Ok(matches!(
            self.store
                .adapter_kv
                .get(&scoped, "paused")
                .await?
                .as_deref(),
            Some("true")
        ))
    }

    async fn find_bridge_envelope(
        &self,
        tenant: Uuid,
        adapter_id: &str,
        states: &[EnvelopeState],
    ) -> Result<Option<ActionEnvelope>, FabricError> {
        for state in states {
            let envelopes = self.store.envelopes.list(tenant, *state).await?;
            if let Some(envelope) = envelopes
                .into_iter()
                .find(|envelope| is_bridge_envelope(envelope, adapter_id))
            {
                return Ok(Some(envelope));
            }
        }

        Ok(None)
    }

    async fn current_status(
        &self,
        tenant: Uuid,
        adapter_id: &str,
    ) -> Result<BridgeStatusDto, FabricError> {
        let paused = self.is_paused(tenant, adapter_id).await?;

        if let Some(envelope) = self
            .find_bridge_envelope(
                tenant,
                adapter_id,
                &[
                    EnvelopeState::PendingApproval,
                    EnvelopeState::Approved,
                    EnvelopeState::Executing,
                    EnvelopeState::Proposed,
                ],
            )
            .await?
        {
            return Ok(bridge_status_dto(
                adapter_id,
                if paused { "paused" } else { "queued" },
                &envelope,
            ));
        }

        if let Some(envelope) = self
            .find_bridge_envelope(tenant, adapter_id, &[EnvelopeState::Executed])
            .await?
        {
            return Ok(bridge_status_dto(
                adapter_id,
                if paused { "paused" } else { "active" },
                &envelope,
            ));
        }

        if let Some(envelope) = self
            .find_bridge_envelope(
                tenant,
                adapter_id,
                &[
                    EnvelopeState::Failed,
                    EnvelopeState::RolledBack,
                    EnvelopeState::Rejected,
                ],
            )
            .await?
        {
            return Ok(bridge_status_dto(
                adapter_id,
                if paused { "paused" } else { "inactive" },
                &envelope,
            ));
        }

        Err(FabricError::NotFound)
    }
}

#[async_trait]
impl BridgeService for StoreBridgeService {
    async fn register(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        _actor: &str,
        request: BridgeRegisterRequest,
    ) -> Result<ActionEnvelope, FabricError> {
        ctx.require_role(Role::Admin)?;
        validate_bridge_request(&request)?;

        self.envelopes
            .propose(
                tenant,
                EnvelopeCreateRequest {
                    domain: "bridges".into(),
                    action: "deploy_adapter".into(),
                    kind: None,
                    targets: vec![bridge_target(tenant, &request.adapter_id)],
                    payload: bridge_payload(&request),
                    rationale: request.rationale.clone(),
                    reversal: Reversal::Compensating,
                    blast: BlastRadiusDto {
                        entities: 1,
                        external_sends: 0,
                        money_cents: 0,
                        pii_egress: false,
                    },
                },
            )
            .await
    }

    async fn status(&self, tenant: Uuid, adapter_id: &str) -> Result<BridgeStatusDto, FabricError> {
        self.current_status(tenant, adapter_id).await
    }

    async fn pause(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        _actor: &str,
        adapter_id: &str,
    ) -> Result<BridgeStatusDto, FabricError> {
        ctx.require_role(Role::Admin)?;
        let _ = self.current_status(tenant, adapter_id).await?;
        let scoped = scoped_bridge_key(tenant, adapter_id);
        self.store.adapter_kv.set(&scoped, "paused", "true").await?;
        self.current_status(tenant, adapter_id).await
    }

    async fn resume(
        &self,
        ctx: &AuthCtx,
        tenant: Uuid,
        _actor: &str,
        adapter_id: &str,
    ) -> Result<BridgeStatusDto, FabricError> {
        ctx.require_role(Role::Admin)?;
        let _ = self.current_status(tenant, adapter_id).await?;
        let scoped = scoped_bridge_key(tenant, adapter_id);
        self.store
            .adapter_kv
            .set(&scoped, "paused", "false")
            .await?;
        self.current_status(tenant, adapter_id).await
    }
}

pub struct StoreTkStatsService {
    ledger: store::LedgerRepo,
    routes: Vec<String>,
}

impl StoreTkStatsService {
    pub fn new(ledger: store::LedgerRepo, routes: Vec<String>) -> Self {
        Self { ledger, routes }
    }
}

#[async_trait]
impl TkStatsService for StoreTkStatsService {
    async fn window(&self, window: &str) -> Result<TkWindowStats, FabricError> {
        let since = window_start(window)?;
        let mut routes = Vec::with_capacity(self.routes.len());
        for route in &self.routes {
            routes.push(TkRouteStat {
                route: route.clone(),
                hit_ratio: self.ledger.route_ratio(route, since).await?,
            });
        }
        Ok(TkWindowStats {
            window: window.to_owned(),
            routes,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConciergePingResponse {
    pub answer: String,
    pub route: String,
    pub provider: String,
    pub tokens_used: u32,
}

#[async_trait]
pub trait ConciergeService: Send + Sync {
    async fn ping(
        &self,
        tenant: Uuid,
        question: &str,
    ) -> Result<ConciergePingResponse, FabricError>;
}

pub struct ConciergeServiceImpl;

#[async_trait]
impl ConciergeService for ConciergeServiceImpl {
    async fn ping(
        &self,
        tenant: Uuid,
        question: &str,
    ) -> Result<ConciergePingResponse, FabricError> {
        let mut routes = HashMap::new();
        routes.insert(
            "concierge".into(),
            RouteCfg {
                provider: "test".into(),
                provider_tags: vec![ProviderTag::Private],
                max_tokens: 256,
                output_budget_bytes: 4096,
                contract: Contract::PlainAnswer,
                pii: false,
            },
        );

        let segments = vec![Segment {
            stability: Stability::S0,
            text: "You are HYDRA concierge ping service.".into(),
            version: 1,
        }];

        let session = Session::new(
            tenant,
            routes,
            Box::new(PingRouter),
            Box::new(MemoryLedger::default()),
            Box::new(ApproxTokenizer),
            Box::new(tokenkiller::SystemClock),
        );

        let contracted = session
            .complete("concierge", segments, question.to_owned())
            .await
            .map_err(|error| {
                FabricError::LlmProviderError(format!("concierge tk error: {error}"))
            })?;

        Ok(ConciergePingResponse {
            answer: contracted.raw,
            route: "concierge".into(),
            provider: contracted.ledger_row.provider,
            tokens_used: contracted.ledger_row.out_tokens as u32,
        })
    }
}

struct PingRouter;

#[async_trait]
impl TkRouter for PingRouter {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, RouterError> {
        let tail = request.prompt.tail_bytes;
        Ok(CompletionResponse {
            provider: "test".into(),
            chunks: vec![format!("Pong: {tail}")],
            usage: CacheUsage::default(),
            out_tokens: 7,
            cost_cents: 0,
        })
    }
}

#[derive(Default)]
struct MemoryLedger {
    rows: Mutex<Vec<LedgerRow>>,
}

#[async_trait]
impl LedgerSink for MemoryLedger {
    async fn record(&self, row: &LedgerRow) -> Result<(), tokenkiller::LedgerError> {
        self.rows
            .lock()
            .expect("memory ledger lock should not be poisoned")
            .push(row.clone());
        Ok(())
    }
}

pub fn tenant_from_headers(headers: &HeaderMap) -> Result<Uuid, FabricError> {
    let raw = headers
        .get("x-hydra-tenant")
        .ok_or_else(|| FabricError::ValidationFailed("missing x-hydra-tenant header".into()))?
        .to_str()
        .map_err(|_| FabricError::ValidationFailed("x-hydra-tenant must be utf-8".into()))?;
    Uuid::parse_str(raw)
        .map_err(|error| FabricError::ValidationFailed(format!("invalid tenant uuid: {error}")))
}

fn validate_request(request: &EnvelopeCreateRequest) -> Result<(), FabricError> {
    if request.domain.trim().is_empty() {
        return Err(FabricError::ValidationFailed(
            "domain must not be empty".into(),
        ));
    }
    if request.action.trim().is_empty() {
        return Err(FabricError::ValidationFailed(
            "action must not be empty".into(),
        ));
    }
    if request.targets.is_empty() {
        return Err(FabricError::ValidationFailed(
            "targets must contain at least one entity id".into(),
        ));
    }
    if request.rationale.trim().is_empty() {
        return Err(FabricError::ValidationFailed(
            "rationale must not be empty".into(),
        ));
    }
    Ok(())
}

fn validate_external_proposal(
    principal: &PrincipalContext,
    capability: &CapabilityDescriptor,
    proposal: &GovernedExternalProposal,
) -> Result<(), FabricError> {
    validate_request(&proposal.request)?;
    let Some(binding) = capability.governor_binding.as_ref() else {
        return Err(FabricError::CapabilityUnavailable(
            "command capability has no Governor binding".to_owned(),
        ));
    };
    let optional_ids_valid = [
        proposal.objective_id.as_deref(),
        proposal.task_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .all(|value| !value.trim().is_empty() && value.len() <= 200);
    if !principal.is_external()
        || principal.hydra_tenant_id.is_nil()
        || principal
            .external_provider
            .as_deref()
            .is_none_or(str::is_empty)
        || principal
            .external_tenant_id
            .as_deref()
            .is_none_or(str::is_empty)
        || principal
            .external_business_id
            .as_deref()
            .is_none_or(str::is_empty)
        || principal.binding_id.is_none()
        || !capability.available
        || capability.idempotency != IdempotencySemantics::RequiredKey
        || binding.domain != proposal.request.domain
        || binding.action != proposal.request.action
        || binding.kind != proposal.request.kind
        || proposal.idempotency_key.trim().is_empty()
        || proposal.idempotency_key.len() > 200
        || proposal.request_hash.len() != 64
        || !proposal
            .request_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || !optional_ids_valid
    {
        return Err(FabricError::AuthzDenied);
    }
    Ok(())
}

fn principal_type_name(principal_type: PrincipalType) -> &'static str {
    match principal_type {
        PrincipalType::Human => "human",
        PrincipalType::NexusService => "nexus_service",
        PrincipalType::NexusAgent => "nexus_agent",
        PrincipalType::HydraInternalAgent => "hydra_internal_agent",
        PrincipalType::LocalHydraUser => "local_hydra_user",
    }
}

fn validate_autonomy_cells(cells: &[AutonomyCellDto]) -> Result<(), FabricError> {
    for cell in cells {
        if cell.domain.trim().is_empty() {
            return Err(FabricError::ValidationFailed(
                "autonomy cell domain must not be empty".into(),
            ));
        }
        if cell.action.trim().is_empty() {
            return Err(FabricError::ValidationFailed(
                "autonomy cell action must not be empty".into(),
            ));
        }
    }
    Ok(())
}

fn validate_bridge_request(request: &BridgeRegisterRequest) -> Result<(), FabricError> {
    if request.adapter_id.trim().is_empty() {
        return Err(FabricError::ValidationFailed(
            "bridge adapter_id must not be empty".into(),
        ));
    }
    if request.wiring_ref.trim().is_empty() {
        return Err(FabricError::ValidationFailed(
            "bridge wiring_ref must not be empty".into(),
        ));
    }
    if request.grant.fuel == 0 {
        return Err(FabricError::ValidationFailed(
            "bridge grant fuel must be greater than zero".into(),
        ));
    }
    Ok(())
}

fn ensure_kind(entity: &Entity, kind: &str) -> Result<(), FabricError> {
    if entity.kind == kind {
        Ok(())
    } else {
        Err(FabricError::NotFound)
    }
}

fn apply_merge_patch(target: &mut Value, patch: Value) {
    match patch {
        Value::Object(patch_map) => {
            if !target.is_object() {
                *target = Value::Object(Map::new());
            }
            let Some(target_map) = target.as_object_mut() else {
                panic!("target should be an object after normalization");
            };
            for (key, value) in patch_map {
                if value.is_null() {
                    target_map.remove(&key);
                    continue;
                }

                match target_map.get_mut(&key) {
                    Some(existing) => apply_merge_patch(existing, value),
                    None => {
                        target_map.insert(key, value);
                    }
                }
            }
        }
        other => *target = other,
    }
}

fn bridge_payload(request: &BridgeRegisterRequest) -> Value {
    json!({
        "adapter_id": request.adapter_id,
        "wiring_ref": request.wiring_ref,
        "grant": request.grant,
    })
}

fn bridge_target(_tenant: Uuid, _adapter_id: &str) -> Uuid {
    Uuid::new_v4()
}

fn scoped_bridge_key(tenant: Uuid, adapter_id: &str) -> String {
    format!("{tenant}:{adapter_id}")
}

fn is_bridge_envelope(envelope: &ActionEnvelope, adapter_id: &str) -> bool {
    envelope.domain == "bridges"
        && envelope.action == "deploy_adapter"
        && envelope.payload.get("adapter_id").and_then(Value::as_str) == Some(adapter_id)
}

fn bridge_status_dto(adapter_id: &str, state: &str, envelope: &ActionEnvelope) -> BridgeStatusDto {
    BridgeStatusDto {
        adapter_id: adapter_id.to_owned(),
        state: state.to_owned(),
        envelope_id: Some(envelope.id),
        envelope_state: Some(envelope_state_name(envelope.state).to_owned()),
        wiring_ref: envelope
            .payload
            .get("wiring_ref")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

fn envelope_state_name(state: EnvelopeState) -> &'static str {
    match state {
        EnvelopeState::Proposed => "Proposed",
        EnvelopeState::PendingApproval => "PendingApproval",
        EnvelopeState::Approved => "Approved",
        EnvelopeState::Executing => "Executing",
        EnvelopeState::Executed => "Executed",
        EnvelopeState::Failed => "Failed",
        EnvelopeState::RolledBack => "RolledBack",
        EnvelopeState::Rejected => "Rejected",
    }
}

fn dto_from_stored_cell(cell: store::StoredAutonomyCell) -> Result<AutonomyCellDto, FabricError> {
    Ok(AutonomyCellDto {
        domain: cell.domain,
        action: cell.action,
        kind: cell.kind,
        level: level_name(cell.level).to_owned(),
        cfg: cell.cfg,
    })
}

fn stored_cell_from_dto(cell: &AutonomyCellDto) -> Result<store::StoredAutonomyCell, FabricError> {
    Ok(store::StoredAutonomyCell {
        domain: cell.domain.clone(),
        action: cell.action.clone(),
        kind: cell.kind.clone(),
        level: parse_level_name(&cell.level)?,
        cfg: cell.cfg.clone(),
    })
}

fn level_name(level: governor::Level) -> &'static str {
    match level {
        governor::Level::L0 => "L0",
        governor::Level::L1 => "L1",
        governor::Level::L2 => "L2",
        governor::Level::L3 => "L3",
        governor::Level::L4 => "L4",
        governor::Level::L5 => "L5",
    }
}

fn parse_level_name(level: &str) -> Result<governor::Level, FabricError> {
    match level {
        "L0" => Ok(governor::Level::L0),
        "L1" => Ok(governor::Level::L1),
        "L2" => Ok(governor::Level::L2),
        "L3" => Ok(governor::Level::L3),
        "L4" => Ok(governor::Level::L4),
        "L5" => Ok(governor::Level::L5),
        other => Err(FabricError::ValidationFailed(format!(
            "unknown autonomy level '{other}'"
        ))),
    }
}

fn month_start() -> OffsetDateTime {
    let now = OffsetDateTime::now_utc();
    let now = if let Ok(value) = now.replace_day(1) {
        value
    } else {
        panic!("all months have a first day")
    };
    let now = if let Ok(value) = now.replace_hour(0) {
        value
    } else {
        panic!("midnight hour should always be valid")
    };
    let now = if let Ok(value) = now.replace_minute(0) {
        value
    } else {
        panic!("minute zero should always be valid")
    };
    if let Ok(value) = now.replace_second(0) {
        value
    } else {
        panic!("second zero should always be valid")
    }
}

fn window_start(window: &str) -> Result<OffsetDateTime, FabricError> {
    let now = OffsetDateTime::now_utc();
    match window {
        "1h" => Ok(now - time::Duration::hours(1)),
        "24h" => Ok(now - time::Duration::hours(24)),
        "7d" => Ok(now - time::Duration::days(7)),
        other => Err(FabricError::ValidationFailed(format!(
            "unsupported tk ledger window '{other}'"
        ))),
    }
}

struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

impl From<governor::DomainError> for FabricError {
    fn from(value: governor::DomainError) -> Self {
        FabricError::Internal(value.to_string())
    }
}

pub fn demo_governor() -> Governor {
    let mut matrix = PolicyMatrix::default();
    if let Err(error) = matrix.insert(
        "bridges",
        Some("deploy_adapter"),
        None,
        Cell {
            level: Level::L2,
            batch_max: Some(1),
        },
    ) {
        panic!("demo policy insert should succeed: {error}");
    }
    if let Err(error) = matrix.insert(
        "pipeline",
        Some("move_stage"),
        Some("deal"),
        Cell {
            level: Level::L4,
            batch_max: Some(25),
        },
    ) {
        panic!("demo policy insert should succeed: {error}");
    }

    Governor {
        matrix,
        constitution: Constitution {
            monthly_spend_cap_cents: 50_000,
            pii_egress_allowlist: vec!["private".into()],
            blast_entities_ceiling: 250,
            blast_sends_ceiling: 50,
            blast_money_ceiling_cents: 250_000,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concierge_ping_exercises_tk_path() -> Result<(), FabricError> {
        let service = ConciergeServiceImpl;
        let tenant = Uuid::new_v4();
        let question = "what is the status of deal 42?";

        let response = service.ping(tenant, question).await?;

        assert_eq!(response.route, "concierge");
        assert_eq!(response.provider, "test");
        assert!(
            response.answer.contains("deal 42"),
            "answer should echo the question; got: {}",
            response.answer
        );
        assert!(
            response.tokens_used > 0,
            "tokens_used should report positive output tokens"
        );
        Ok(())
    }

    #[tokio::test]
    async fn concierge_ping_contract_plain_answer_no_fences() -> Result<(), FabricError> {
        let service = ConciergeServiceImpl;
        let tenant = Uuid::new_v4();

        let response = service.ping(tenant, "code fence test").await?;

        // PlainAnswer contract rejects code fences — our fake router doesn't emit them
        assert!(!response.answer.contains("```"));
        assert!(!response.answer.contains("```"));
        assert_eq!(response.provider, "test");
        Ok(())
    }
}
