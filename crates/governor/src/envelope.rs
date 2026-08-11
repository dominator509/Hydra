use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    L0,
    L1,
    L2,
    L3,
    L4,
    L5,
}

impl Level {
    pub fn demote(self) -> Self {
        match self {
            Self::L5 => Self::L4,
            Self::L4 => Self::L3,
            Self::L3 => Self::L2,
            Self::L2 => Self::L2,
            Self::L1 => Self::L1,
            Self::L0 => Self::L0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reversal {
    Compensating,
    Snapshot,
    Irreversible,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlastRadius {
    pub entities: u32,
    pub external_sends: u32,
    pub money_cents: u64,
    pub pii_egress: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeState {
    Proposed,
    PendingApproval,
    Approved,
    Executing,
    Executed,
    Failed,
    RolledBack,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub from: EnvelopeState,
    pub to: EnvelopeState,
    pub actor: String,
    pub at_rfc3339: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_system: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_actor_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_actor_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_binding_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

impl InvocationContext {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionEnvelope {
    pub id: Uuid,
    pub tenant: Uuid,
    pub domain: String,
    pub action: String,
    pub kind: Option<String>,
    pub targets: Vec<Uuid>,
    pub payload: Value,
    pub rationale: String,
    pub reversal: Reversal,
    pub blast: BlastRadius,
    #[serde(default, skip_serializing_if = "InvocationContext::is_empty")]
    pub invocation: InvocationContext,
    pub state: EnvelopeState,
    pub history: Vec<Transition>,
}

impl ActionEnvelope {
    pub fn transition(
        &mut self,
        to: EnvelopeState,
        actor: &str,
        clock: &dyn Clock,
    ) -> Result<(), DomainError> {
        let legal = match self.state {
            EnvelopeState::Proposed => {
                matches!(
                    to,
                    EnvelopeState::PendingApproval
                        | EnvelopeState::Approved
                        | EnvelopeState::Rejected
                )
            }
            EnvelopeState::PendingApproval => {
                matches!(to, EnvelopeState::Approved | EnvelopeState::Rejected)
            }
            EnvelopeState::Approved => matches!(to, EnvelopeState::Executing),
            EnvelopeState::Executing => {
                matches!(to, EnvelopeState::Executed | EnvelopeState::Failed)
            }
            EnvelopeState::Executed => matches!(to, EnvelopeState::RolledBack),
            EnvelopeState::Failed => matches!(to, EnvelopeState::RolledBack),
            EnvelopeState::RolledBack => false,
            EnvelopeState::Rejected => false,
        };

        if !legal {
            return Err(DomainError::IllegalTransition {
                from: self.state,
                to,
            });
        }

        self.history.push(Transition {
            from: self.state,
            to,
            actor: actor.to_owned(),
            at_rfc3339: clock
                .now()
                .format(&Rfc3339)
                .expect("clock output must be formattable as rfc3339"),
        });
        self.state = to;

        Ok(())
    }
}
