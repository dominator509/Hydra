//! layer L2 persistence: Postgres-backed repositories and testkit scaffolding.

pub mod adapter_kv;
pub mod autonomy;
pub mod edges;
pub mod entities;
pub mod envelopes;
pub mod events;
pub mod ledger;
pub mod testkit;

use sqlx::PgPool;
use thiserror::Error;

pub use adapter_kv::AdapterKvRepo;
pub use autonomy::{AutonomyRepo, StoredAutonomyCell};
pub use edges::EdgesRepo;
pub use entities::EntitiesRepo;
pub use envelopes::EnvelopesRepo;
pub use events::EventsRepo;
pub use ledger::{LedgerRepo, LedgerRow};
pub use testkit::TestDb;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("conflict on version {0}")]
    Conflict(u64),
    #[error("record not found")]
    NotFound,
    #[error("tenant mismatch")]
    TenantMismatch,
    #[error("schema violation at {path}: {message}")]
    SchemaViolation { path: String, message: String },
    #[error("unknown kind '{0}'")]
    UnknownKind(String),
    #[error("store invariant violated: {0}")]
    Invariant(String),
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
    pub edges: EdgesRepo,
    pub events: EventsRepo,
    pub envelopes: EnvelopesRepo,
    pub ledger: LedgerRepo,
    pub adapter_kv: AdapterKvRepo,
    pub autonomy: AutonomyRepo,
}

impl Store {
    pub fn new(pool: PgPool) -> Self {
        Self {
            entities: EntitiesRepo::new(pool.clone()),
            edges: EdgesRepo::new(pool.clone()),
            events: EventsRepo::new(pool.clone()),
            envelopes: EnvelopesRepo::new(pool.clone()),
            ledger: LedgerRepo::new(pool.clone()),
            adapter_kv: AdapterKvRepo::new(pool.clone()),
            autonomy: AutonomyRepo::new(pool.clone()),
            pool,
        }
    }
}
