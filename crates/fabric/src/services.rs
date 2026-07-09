use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::{header::AUTHORIZATION, HeaderMap};
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

use crate::auth::{AuthCtx, Role, SessionStore};
use crate::auth::jwt::{TokenClaims, TokenScope, TokenService};
use crate::error::FabricError;

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<SessionStore>,
    pub entities: Arc<dyn EntityService>,
    pub autonomy: Arc<dyn AutonomyService>,
    pub bridges: Arc<dyn BridgeService>,
    pub envelopes: Arc<dyn EnvelopeService>,
    pub tk_stats: Arc<dyn TkStatsService>,
    pub concierge: Arc<dyn ConciergeService>,
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
        }
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

    async fn approve(&self, ctx: &AuthCtx, tenant: Uuid, id: Uuid) -> Result<ActionEnvelope, FabricError>;

    async fn reject(&self, ctx: &AuthCtx, tenant: Uuid, id: Uuid) -> Result<ActionEnvelope, FabricError>;
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
    governor: Governor,
}

impl StoreEnvelopeService {
    pub fn new(store: store::Store, governor: Governor) -> Self {
        Self { store, governor }
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
        let mut envelope = ActionEnvelope {
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
            state: EnvelopeState::Proposed,
            history: Vec::new(),
        };

        match self.governor.evaluate(&envelope, &spend) {
            Decision::Block(reason) => return Err(FabricError::ConstitutionBlocked(reason)),
            Decision::SuggestOnly => {}
            Decision::Queue => {
                envelope.transition(EnvelopeState::PendingApproval, "governor", &SystemClock)?
            }
            Decision::Execute(_) => {
                envelope.transition(EnvelopeState::Approved, "governor", &SystemClock)?
            }
        }

        Ok(self.store.envelopes.save(tenant, &envelope).await?)
    }

    async fn approve(&self, ctx: &AuthCtx, tenant: Uuid, id: Uuid) -> Result<ActionEnvelope, FabricError> {
        ctx.require_role(Role::Approver)?;
        let mut envelope = self
            .store
            .envelopes
            .list(tenant, EnvelopeState::PendingApproval)
            .await?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(FabricError::NotFound)?;
        // Four-eyes: proposer cannot approve their own envelope
        let proposed_by = envelope.history.first()
            .map(|t| t.actor.as_str())
            .unwrap_or("");
        if ctx.principal == proposed_by {
            return Err(FabricError::AuthzDenied);
        }
        envelope.transition(EnvelopeState::Approved, &ctx.principal, &SystemClock)?;
        Ok(self.store.envelopes.save(tenant, &envelope).await?)
    }

    async fn reject(&self, ctx: &AuthCtx, tenant: Uuid, id: Uuid) -> Result<ActionEnvelope, FabricError> {
        ctx.require_role(Role::Approver)?;
        let mut envelope = self
            .store
            .envelopes
            .list(tenant, EnvelopeState::PendingApproval)
            .await?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(FabricError::NotFound)?;
        // Four-eyes: proposer cannot reject their own envelope
        let proposed_by = envelope.history.first()
            .map(|t| t.actor.as_str())
            .unwrap_or("");
        if ctx.principal == proposed_by {
            return Err(FabricError::AuthzDenied);
        }
        envelope.transition(EnvelopeState::Rejected, &ctx.principal, &SystemClock)?;
        Ok(self.store.envelopes.save(tenant, &envelope).await?)
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
        Self {
            envelopes: StoreEnvelopeService::new(store.clone(), governor),
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

pub fn dev_admin_actor_from_headers(headers: &HeaderMap) -> Result<&'static str, FabricError> {
    if !matches!(env::var("HYDRA_ENV").ok().as_deref(), Some("dev")) {
        return Err(FabricError::AuthzDenied);
    }

    let raw = headers
        .get(AUTHORIZATION)
        .ok_or(FabricError::AuthzDenied)?
        .to_str()
        .map_err(|_| FabricError::AuthzDenied)?;

    if raw.trim() == "Bearer hydra-dev-admin" {
        Ok("dev-admin")
    } else {
        Err(FabricError::AuthzDenied)
    }
}

/// Build an AuthCtx from HTTP request headers.
///
/// In the current dev-mode implementation, this builds a session with
/// appropriate roles from the Bearer token. The `hydra-dev-admin` token
/// gets the `Admin` role; any other Bearer token gets `Viewer`.
/// Full SessionStore lookup will be added when auth middleware is wired.
pub fn auth_ctx_from_headers(headers: &HeaderMap) -> AuthCtx {
    let tenant = tenant_from_headers(headers)
        .unwrap_or_else(|_| Uuid::parse_str("00000000-0000-0000-0000-000000000001")
            .expect("dev tenant"));

    let principal = extract_principal(headers);

    // Build a dev-mode session from the Bearer token when present.
    let session = build_dev_session(headers, tenant, &principal);

    AuthCtx {
        principal,
        tenant,
        session,
    }
}

/// Build a dev-mode session from request headers (no SessionStore lookup).
///
/// Tries JWT verification for bearer tokens that aren't the dev admin token.
fn build_dev_session(headers: &HeaderMap, tenant: Uuid, _principal: &str) -> Option<crate::auth::Session> {
    let token = extract_bearer_token(headers)?;

    // Dev admin token → Admin role
    if token == "hydra-dev-admin" {
        return Some(crate::auth::Session {
            user_id: Uuid::nil(),
            tenant_id: tenant,
            username: "admin".into(),
            roles: vec![Role::Admin],
            token: token.to_owned(),
        });
    }

    // Try JWT verification — on success build session from claims
    if let Some(jwt_session) = build_jwt_session(token, tenant) {
        return Some(jwt_session);
    }

    // Any other Bearer token → Viewer role (authenticated, minimal access)
    Some(crate::auth::Session {
        user_id: Uuid::nil(),
        tenant_id: tenant,
        username: format!("token:{token}"),
        roles: vec![Role::Viewer],
        token: token.to_owned(),
    })
}

/// Try to verify a JWT token and build a session from its claims.
#[allow(unused_variables)]
fn build_jwt_session(token_str: &str, fallback_tenant: uuid::Uuid) -> Option<crate::auth::Session> {
    let token_service = TokenService::new(b"dev-secret-key-hydra-ep-006-m4-token!".to_vec());
    let claims = token_service.verify(token_str).ok()?;
    let roles = claims_to_roles(&claims);
    Some(crate::auth::Session {
        user_id: uuid::Uuid::nil(),
        tenant_id: claims.aud,
        username: claims.sub.clone(),
        roles,
        token: token_str.into(),
    })
}

/// Map JWT token scopes to role grants.
///
/// * `admin:bridges` / `admin:autonomy` → `Admin`
/// * `approve:envelopes` → `Approver`
/// * `write:envelopes` → `Operator`
/// * Always includes `Viewer`.
fn claims_to_roles(claims: &TokenClaims) -> Vec<Role> {
    let mut roles = vec![Role::Viewer];
    for scope in TokenScope::parse_all(&claims.scope) {
        match scope {
            TokenScope::AdminBridges | TokenScope::AdminAutonomy => {
                roles.push(Role::Admin);
            }
            TokenScope::ApproveEnvelopes => {
                if !roles.contains(&Role::Approver) {
                    roles.push(Role::Approver);
                }
            }
            TokenScope::WriteEnvelopes => {
                if !roles.contains(&Role::Operator) {
                    roles.push(Role::Operator);
                }
            }
            TokenScope::ReadCdm => {
                // Viewer already covers read access
            }
        }
    }
    roles
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let auth = headers.get(AUTHORIZATION)?.to_str().ok()?;
    auth.strip_prefix("Bearer ")
}

fn extract_principal(headers: &HeaderMap) -> String {
    // Try Bearer token first (REST API)
    if let Some(auth) = headers.get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            // For now, the token IS the principal name in dev mode
            if token == "hydra-dev-admin" {
                return "user:admin".into();
            }
            return format!("token:{}", &token[..8.min(token.len())]);
        }
    }

    // Try session cookie (shell)
    if let Some(cookie) = headers.get("cookie")
        .and_then(|v| v.to_str().ok())
    {
        for pair in cookie.split(';') {
            let pair = pair.trim();
            if let Some(value) = pair.strip_prefix("hydra-session=") {
                return format!("user:{}", &value[..8.min(value.len())]);
            }
        }
    }

    "anonymous".into()
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
