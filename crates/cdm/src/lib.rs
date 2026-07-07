//! layer L1 domain types, kind registry, and identity proposals.

mod identity;
mod kinds;
mod schema;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

pub use identity::{proposals, MergeProposal, PartyView};
pub use kinds::{builtin_kind_names, builtin_kind_schemas};
pub use schema::KindRegistry;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    #[error("unknown kind '{0}'")]
    UnknownKind(String),
    #[error("schema for kind '{kind}' failed to compile: {message}")]
    InvalidSchema { kind: String, message: String },
    #[error("schema violation at {path}: {message}")]
    SchemaViolation { path: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: Uuid,
    pub kind: String,
    pub tenant: Uuid,
    pub body: Value,
    pub origin: String,
    pub origin_ref: Option<String>,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub src: Uuid,
    pub rel: String,
    pub dst: Uuid,
}
