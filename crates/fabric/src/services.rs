use std::sync::Arc;

use async_trait::async_trait;
use axum::http::HeaderMap;
use governor::{
    ActionEnvelope, BlastRadius, Cell, Clock, Constitution, Decision, EnvelopeState, Governor,
    Level, PolicyMatrix, Reversal, SpendSnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::FabricError;

#[derive(Clone)]
pub struct AppState {
    pub envelopes: Arc<dyn EnvelopeService>,
    pub tk_stats: Arc<dyn TkStatsService>,
}

impl AppState {
    pub fn new(envelopes: Arc<dyn EnvelopeService>, tk_stats: Arc<dyn TkStatsService>) -> Self {
        Self {
            envelopes,
            tk_stats,
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

    async fn approve(&self, tenant: Uuid, id: Uuid) -> Result<ActionEnvelope, FabricError>;

    async fn reject(&self, tenant: Uuid, id: Uuid) -> Result<ActionEnvelope, FabricError>;
}

#[async_trait]
pub trait EntityService: Send + Sync {}

#[async_trait]
pub trait AutonomyService: Send + Sync {}

#[async_trait]
pub trait BridgeService: Send + Sync {}

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

    async fn approve(&self, tenant: Uuid, id: Uuid) -> Result<ActionEnvelope, FabricError> {
        let mut envelope = self
            .store
            .envelopes
            .list(tenant, EnvelopeState::PendingApproval)
            .await?
            .into_iter()
            .find(|envelope| envelope.id == id)
            .ok_or(FabricError::NotFound)?;
        envelope.transition(EnvelopeState::Approved, "approver", &SystemClock)?;
        Ok(self.store.envelopes.save(tenant, &envelope).await?)
    }

    async fn reject(&self, tenant: Uuid, id: Uuid) -> Result<ActionEnvelope, FabricError> {
        let mut envelope = self
            .store
            .envelopes
            .list(tenant, EnvelopeState::PendingApproval)
            .await?
            .into_iter()
            .find(|envelope| envelope.id == id)
            .ok_or(FabricError::NotFound)?;
        envelope.transition(EnvelopeState::Rejected, "approver", &SystemClock)?;
        Ok(self.store.envelopes.save(tenant, &envelope).await?)
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
