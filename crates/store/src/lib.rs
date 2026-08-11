//! layer L2 persistence: Postgres-backed repositories and testkit scaffolding.

pub mod adapter_kv;
pub mod approvals;
pub mod autonomy;
pub mod edges;
pub mod entities;
pub mod envelopes;
pub mod events;
pub mod execution_receipts;
pub mod external_bindings;
pub mod idempotency;
pub mod ledger;
pub mod outbox;
pub mod testkit;
pub mod trace_context;

use sqlx::PgPool;
use thiserror::Error;

pub use adapter_kv::AdapterKvRepo;
pub use approvals::{ApprovalAssertion, ApprovalDecision, ApprovalsRepo, NewApprovalAssertion};
pub use autonomy::{AutonomyRepo, StoredAutonomyCell};
pub use edges::EdgesRepo;
pub use entities::EntitiesRepo;
pub use envelopes::EnvelopesRepo;
pub use events::{EventProvenance, EventsRepo};
pub use execution_receipts::{
    ExecutionOutcome, ExecutionReceipt, ExecutionReceiptsRepo, NewExecutionReceipt,
};
pub use external_bindings::{
    BindingStatus, ExternalBindingsRepo, ExternalTenantBinding, NewExternalTenantBinding,
};
pub use idempotency::{
    IdempotencyRecord, IdempotencyRepo, IdempotencyResolution, NewIdempotencyRecord,
};
pub use ledger::{LedgerRepo, LedgerRow};
pub use outbox::{OutboxClaimBatch, OutboxFailureDisposition, OutboxRecord, OutboxRepo};
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
    pub adapter_kv: AdapterKvRepo,
    pub autonomy: AutonomyRepo,
}

impl Store {
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
            adapter_kv: AdapterKvRepo::new(pool.clone()),
            autonomy: AutonomyRepo::new(pool.clone()),
            pool,
        }
    }
}
