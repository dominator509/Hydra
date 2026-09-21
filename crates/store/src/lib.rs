//! layer L2 persistence: Postgres-backed repositories and testkit scaffolding.

pub mod a2a_tasks;
pub mod adapter_kv;
pub mod approvals;
pub mod autonomy;
pub mod bridge_adapters;
pub mod bridge_schedules;
pub mod bridge_sync;
pub mod edges;
pub mod entities;
pub mod envelopes;
pub mod events;
pub mod execution_receipts;
pub mod external_bindings;
pub mod idempotency;
pub mod ledger;
pub mod operators;
pub mod outbox;
pub mod rate_limits;
pub mod sessions;
pub mod tenant_data;
pub mod testkit;
pub mod trace_context;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use thiserror::Error;

pub use a2a_tasks::{A2aTask, A2aTaskResolution, A2aTaskTransition, A2aTasksRepo, NewA2aTask};
pub use adapter_kv::AdapterKvRepo;
pub use approvals::{ApprovalAssertion, ApprovalDecision, ApprovalsRepo, NewApprovalAssertion};
pub use autonomy::{AutonomyFreeze, AutonomyRepo, StoredAutonomyCell};
pub use bridge_adapters::{
    BridgeAdapterRecord, BridgeAdapterState, BridgeAdapterTransition,
    BridgeAdapterTransitionRecord, BridgeAdaptersRepo, NewBridgeAdapter,
};
pub use bridge_schedules::{
    BridgeScheduleState, BridgeSchedulesRepo, BridgeSyncSchedule, BridgeSyncScheduleCompletion,
    BridgeSyncScheduleLease, NewBridgeSyncSchedule, MAX_INTERVAL_SECONDS, MAX_PAGE_LIMIT,
    MIN_INTERVAL_SECONDS, MIN_PAGE_LIMIT,
};
pub use bridge_sync::{
    BridgeSyncApply, BridgeSyncApplyError, BridgeSyncChange, BridgeSyncConflict,
    BridgeSyncOperation, BridgeSyncRepo, BridgeSyncRun, BridgeSyncRunStatus, BridgeSyncState,
    NewBridgeSyncRun,
};
pub use edges::EdgesRepo;
pub use entities::EntitiesRepo;
pub use envelopes::EnvelopesRepo;
pub use events::{EventProvenance, EventsRepo};
pub use execution_receipts::{
    ExecutionOutcome, ExecutionReceipt, ExecutionReceiptsRepo, NewExecutionReceipt,
};
pub use external_bindings::{
    is_valid_external_binding_text, BindingStatus, ExternalBindingsRepo, ExternalTenantBinding,
    NewExternalTenantBinding, EXTERNAL_BINDING_TEXT_MAX_LENGTH,
};
pub use idempotency::{
    IdempotencyRecord, IdempotencyRepo, IdempotencyResolution, NewIdempotencyRecord,
};
pub use ledger::{LedgerRepo, LedgerRow};
pub use operators::{validate_role, NewOperatorUser, OperatorRepo, OperatorUser};
pub use outbox::{OutboxClaimBatch, OutboxFailureDisposition, OutboxRecord, OutboxRepo};
pub use rate_limits::{RateLimitDecision, RateLimitsRepo};
pub use sessions::{AuthUserRecord, SessionRecord, SessionRepo};
pub use tenant_data::{
    ExportEdge, ExportEntity, ExportEvent, RetentionMetric, RetentionPreview, TenantDataExport,
    TenantDataRepo, MAX_EXPORT_RECORDS, TENANT_DATA_SCHEMA_VERSION,
};
pub use testkit::{TestDb, MIGRATOR};
pub use trace_context::TraceContext;

/// Run all pending SQL migrations on the given pool.
/// Equivalent to `sqlx migrate run` — applies all migrations from the
/// `migrations/` directory that have not yet been applied.
pub async fn run_migrations(pool: &PgPool) -> Result<(), StoreError> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("conflict on version {0}")]
    Conflict(u64),
    #[error("record not found")]
    NotFound,
    #[error("tenant mismatch")]
    TenantMismatch,
    #[error("idempotency key conflicts with a different request")]
    IdempotencyConflict,
    #[error("approval assertion denied")]
    ApprovalDenied,
    #[error("outbox relay claim is no longer owned")]
    OutboxClaimLost,
    #[error("schema violation at {path}: {message}")]
    SchemaViolation { path: String, message: String },
    #[error("unknown kind '{0}'")]
    UnknownKind(String),
    #[error("store invariant violated: {0}")]
    Invariant(String),
    #[error(transparent)]
    EventContract(#[from] cdm::EventContractError),
    #[error(transparent)]
    Governor(#[from] governor::DomainError),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<cdm::DomainError> for StoreError {
    fn from(value: cdm::DomainError) -> Self {
        match value {
            cdm::DomainError::UnknownKind(kind) => Self::UnknownKind(kind),
            cdm::DomainError::SchemaViolation { path, message } => {
                Self::SchemaViolation { path, message }
            }
            cdm::DomainError::InvalidSchema { kind, message } => {
                Self::Invariant(format!("invalid schema for kind '{kind}': {message}"))
            }
        }
    }
}

#[derive(Clone)]
pub struct Store {
    pub pool: PgPool,
    pub entities: EntitiesRepo,
    pub approvals: ApprovalsRepo,
    pub edges: EdgesRepo,
    pub events: EventsRepo,
    pub external_bindings: ExternalBindingsRepo,
    pub idempotency: IdempotencyRepo,
    pub envelopes: EnvelopesRepo,
    pub execution_receipts: ExecutionReceiptsRepo,
    pub ledger: LedgerRepo,
    pub outbox: OutboxRepo,
    pub operators: OperatorRepo,
    pub rate_limits: RateLimitsRepo,
    pub sessions: SessionRepo,
    pub adapter_kv: AdapterKvRepo,
    pub autonomy: AutonomyRepo,
    pub a2a_tasks: A2aTasksRepo,
    pub bridge_adapters: BridgeAdaptersRepo,
    pub bridge_sync: BridgeSyncRepo,
    pub bridge_schedules: BridgeSchedulesRepo,
    pub tenant_data: TenantDataRepo,
}

impl Store {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, StoreError> {
        if database_url.trim().is_empty() {
            return Err(StoreError::Invariant(
                "DATABASE_URL must not be empty".to_owned(),
            ));
        }
        if max_connections == 0 {
            return Err(StoreError::Invariant(
                "Store connection pool must allow at least one connection".to_owned(),
            ));
        }
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await?;
        Ok(Self::new(pool))
    }

    pub fn new(pool: PgPool) -> Self {
        Self {
            approvals: ApprovalsRepo::new(pool.clone()),
            entities: EntitiesRepo::new(pool.clone()),
            edges: EdgesRepo::new(pool.clone()),
            events: EventsRepo::new(pool.clone()),
            external_bindings: ExternalBindingsRepo::new(pool.clone()),
            idempotency: IdempotencyRepo::new(pool.clone()),
            envelopes: EnvelopesRepo::new(pool.clone()),
            execution_receipts: ExecutionReceiptsRepo::new(pool.clone()),
            ledger: LedgerRepo::new(pool.clone()),
            outbox: OutboxRepo::new(pool.clone()),
            operators: OperatorRepo::new(pool.clone()),
            rate_limits: RateLimitsRepo::new(pool.clone()),
            sessions: SessionRepo::new(pool.clone()),
            adapter_kv: AdapterKvRepo::new(pool.clone()),
            autonomy: AutonomyRepo::new(pool.clone()),
            a2a_tasks: A2aTasksRepo::new(pool.clone()),
            bridge_adapters: BridgeAdaptersRepo::new(pool.clone()),
            bridge_sync: BridgeSyncRepo::new(pool.clone()),
            bridge_schedules: BridgeSchedulesRepo::new(pool.clone()),
            tenant_data: TenantDataRepo::new(pool.clone()),
            pool,
        }
    }

    pub async fn migrate(&self) -> Result<(), StoreError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    pub async fn health_check(&self) -> Result<(), StoreError> {
        let _: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&self.pool).await?;
        Ok(())
    }
}
