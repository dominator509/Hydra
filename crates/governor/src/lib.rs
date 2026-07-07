//! layer L1 deterministic autonomy governor and envelope machine.

mod constitution;
mod decision;
mod envelope;
mod policy;

use thiserror::Error;

pub use constitution::{Constitution, Rule, SpendSnapshot};
pub use decision::{Decision, ExecuteToken};
pub use envelope::{
    ActionEnvelope, BlastRadius, Clock, EnvelopeState, Level, Reversal, Transition,
};
pub use policy::{Cell, PolicyMatrix};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    #[error("illegal transition {from:?} -> {to:?}")]
    IllegalTransition {
        from: EnvelopeState,
        to: EnvelopeState,
    },
    #[error("policy matrix ambiguous for {0}")]
    PolicyResolutionAmbiguous(String),
}

pub struct Governor {
    pub matrix: PolicyMatrix,
    pub constitution: Constitution,
}

impl Governor {
    pub fn evaluate(&self, envelope: &ActionEnvelope, spend: &SpendSnapshot) -> Decision {
        if let Err(rule) = self.constitution.check(envelope, spend) {
            return Decision::Block(rule.to_string());
        }

        let cell =
            self.matrix
                .resolve(&envelope.domain, &envelope.action, envelope.kind.as_deref());
        let mut level = cell.level;

        if matches!(envelope.reversal, Reversal::Irreversible) {
            level = level.demote();
        }

        let exceeds_blast_ceiling = envelope.blast.entities
            > self.constitution.blast_entities_ceiling
            || envelope.blast.external_sends > self.constitution.blast_sends_ceiling
            || envelope.blast.money_cents > self.constitution.blast_money_ceiling_cents;
        if exceeds_blast_ceiling && level > Level::L3 {
            level = Level::L3;
        }

        match level {
            Level::L0 => Decision::Block("cell is manual-only (L0)".to_owned()),
            Level::L1 => Decision::SuggestOnly,
            Level::L2 | Level::L3 => Decision::Queue,
            Level::L4 | Level::L5 => Decision::Execute(ExecuteToken::new(envelope.id)),
        }
    }
}
