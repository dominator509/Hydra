//! reference/governor.rs — deterministic Autonomy Governor (SPEC-001 B2/B3/B5).
//! L1 domain: NO async, NO IO, NO network deps. Copy into crates/governor and split into modules.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level { L0, L1, L2, L3, L4, L5 }

impl Level {
    /// Irreversible actions execute one level more conservatively (SPEC-001 B2).
    /// L0/L1 are already human-driven; demotion is identity there.
    pub fn demote(self) -> Level {
        match self {
            Level::L5 => Level::L4,
            Level::L4 => Level::L3,
            Level::L3 => Level::L2,
            Level::L2 => Level::L2, // queue-for-approval is the floor for autonomous intent
            Level::L1 => Level::L1,
            Level::L0 => Level::L0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reversal { Compensating, Snapshot, Irreversible }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlastRadius {
    pub entities: u32,
    pub external_sends: u32,
    pub money_cents: u64,
    pub pii_egress: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeState {
    Proposed, PendingApproval, Approved, Executing,
    Executed, Failed, RolledBack, Rejected,
}

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("illegal transition {from:?} -> {to:?}")]
    IllegalTransition { from: EnvelopeState, to: EnvelopeState },
    #[error("policy matrix ambiguous for {0}")]
    PolicyResolutionAmbiguous(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionEnvelope {
    pub id: uuid::Uuid,
    pub tenant: uuid::Uuid,
    pub domain: String,          // e.g. "comms", "pipeline", "data", "bridges"
    pub action: String,          // e.g. "send_email", "merge_parties"
    pub kind: Option<String>,    // entity kind specialization
    pub targets: Vec<uuid::Uuid>,
    pub payload: serde_json::Value,
    pub rationale: String,       // model-written; NEVER parsed for control flow
    pub reversal: Reversal,
    pub blast: BlastRadius,
    pub state: EnvelopeState,
    pub history: Vec<Transition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: EnvelopeState,
    pub to: EnvelopeState,
    pub actor: String,
    pub at_rfc3339: String,
}

/// Injected clock keeps the machine pure & testable (SPEC-001 B3).
pub trait Clock { fn now_rfc3339(&self) -> String; }

impl ActionEnvelope {
    /// Transition table, exhaustive over the CURRENT state (no wildcard on `from`):
    /// each arm's `matches!` enumerates the complete legal target set for that state.
    /// Adding a new EnvelopeState forces a compile error here — the compiler is the spec.
    pub fn transition(
        &mut self,
        to: EnvelopeState,
        actor: &str,
        clock: &dyn Clock,
    ) -> Result<(), DomainError> {
        use EnvelopeState::*;
        let ok = match self.state {
            Proposed        => matches!(to, PendingApproval | Approved | Rejected),
            PendingApproval => matches!(to, Approved | Rejected),
            Approved        => matches!(to, Executing),
            Executing       => matches!(to, Executed | Failed),
            Executed        => matches!(to, RolledBack), // compensating action ran
            Failed          => matches!(to, RolledBack), // cleanup after partial effects
            RolledBack      => false,                    // terminal
            Rejected        => false,                    // terminal
        };
        if !ok {
            return Err(DomainError::IllegalTransition { from: self.state, to });
        }
        self.history.push(Transition {
            from: self.state, to, actor: actor.to_string(), at_rfc3339: clock.now_rfc3339(),
        });
        self.state = to;
        Ok(())
    }
}

// ---------- Policy matrix ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell { pub level: Level, pub batch_max: Option<u32> }

/// Key specificity: (domain, Some(action), Some(kind)) > (domain, Some(action), None) > (domain, None, None).
#[derive(Debug, Default, Clone)]
pub struct PolicyMatrix {
    cells: HashMap<(String, Option<String>, Option<String>), Cell>,
}

impl PolicyMatrix {
    /// Load-time duplicate detection makes runtime resolution unambiguous (SPEC-001 error PolicyResolutionAmbiguous).
    pub fn insert(&mut self, domain: &str, action: Option<&str>, kind: Option<&str>, cell: Cell)
        -> Result<(), DomainError>
    {
        let k = (domain.into(), action.map(Into::into), kind.map(Into::into));
        if self.cells.insert(k.clone(), cell).is_some() {
            return Err(DomainError::PolicyResolutionAmbiguous(format!("{k:?}")));
        }
        Ok(())
    }
    pub fn resolve(&self, domain: &str, action: &str, kind: Option<&str>) -> Cell {
        let probes = [
            (domain.to_string(), Some(action.to_string()), kind.map(str::to_string)),
            (domain.to_string(), Some(action.to_string()), None),
            (domain.to_string(), None, None),
        ];
        for p in probes {
            if let Some(c) = self.cells.get(&p) { return c.clone(); }
        }
        Cell { level: Level::L1, batch_max: None } // safe default: suggest-only
    }
}

// ---------- Constitution ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constitution {
    pub monthly_spend_cap_cents: u64,
    pub pii_egress_allowlist: Vec<String>, // provider tags allowed to receive PII intent
    pub blast_entities_ceiling: u32,       // above this ⇒ clamp to ≤L3
    pub blast_sends_ceiling: u32,
    pub blast_money_ceiling_cents: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct SpendSnapshot { pub month_to_date_cents: u64 }

// ---------- Decision + sealed execute token ----------

mod sealed {
    /// ExecuteToken can only be constructed inside governor::evaluate.
    /// The kernel executor REQUIRES this token — code cannot forge an Execute path (INV-1).
    #[derive(Debug)]
    pub struct ExecuteToken { pub(super) envelope_id: uuid::Uuid }
    impl ExecuteToken { pub fn envelope_id(&self) -> uuid::Uuid { self.envelope_id } }
}
pub use sealed::ExecuteToken;

#[derive(Debug)]
pub enum Decision {
    Block(String),
    SuggestOnly,
    Queue,
    Execute(ExecuteToken),
}

pub struct Governor { pub matrix: PolicyMatrix, pub constitution: Constitution }

impl Governor {
    /// Pure, deterministic, <5ms p99 target (SPEC-001). Order matters and is normative:
    /// constitution → resolve → irreversible demotion → blast ceiling clamp → map.
    pub fn evaluate(&self, e: &ActionEnvelope, spend: &SpendSnapshot) -> Decision {
        // 1. Constitution (hard rules override even L5)
        if spend.month_to_date_cents >= self.constitution.monthly_spend_cap_cents {
            return Decision::Block("constitution: monthly spend cap reached".into());
        }
        if e.blast.pii_egress
            && !self.constitution.pii_egress_allowlist.iter().any(|t| t == "private")
        {
            return Decision::Block("constitution: pii egress not allow-listed".into());
        }
        if e.action == "hard_delete" {
            return Decision::Block("constitution: hard_delete is forbidden".into());
        }
        // 2. Cell resolution
        let cell = self.matrix.resolve(&e.domain, &e.action, e.kind.as_deref());
        let mut level = cell.level;
        // 3. Irreversible demotion
        if matches!(e.reversal, Reversal::Irreversible) { level = level.demote(); }
        // 4. Blast ceiling clamp (never above L3 when blast is large)
        let big = e.blast.entities > self.constitution.blast_entities_ceiling
            || e.blast.external_sends > self.constitution.blast_sends_ceiling
            || e.blast.money_cents > self.constitution.blast_money_ceiling_cents;
        if big && level > Level::L3 { level = Level::L3; }
        // 5. Map level → decision
        match level {
            Level::L0 => Decision::Block("cell is manual-only (L0)".into()),
            Level::L1 => Decision::SuggestOnly,
            Level::L2 | Level::L3 => Decision::Queue, // L3 batching handled by queue consumer via cell.batch_max
            Level::L4 | Level::L5 => Decision::Execute(sealed::ExecuteToken { envelope_id: e.id }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct FixedClock;
    impl Clock for FixedClock { fn now_rfc3339(&self) -> String { "2026-01-01T00:00:00Z".into() } }

    fn envl(reversal: Reversal, blast: BlastRadius) -> ActionEnvelope {
        ActionEnvelope {
            id: uuid::Uuid::nil(), tenant: uuid::Uuid::nil(),
            domain: "pipeline".into(), action: "move_stage".into(), kind: Some("deal".into()),
            targets: vec![], payload: serde_json::json!({}), rationale: String::new(),
            reversal, blast, state: EnvelopeState::Proposed, history: vec![],
        }
    }

    #[test]
    fn irreversible_demotes_and_blast_clamps() {
        let mut m = PolicyMatrix::default();
        m.insert("pipeline", None, None, Cell { level: Level::L5, batch_max: None }).unwrap();
        let g = Governor { matrix: m, constitution: Constitution {
            monthly_spend_cap_cents: 10_000, pii_egress_allowlist: vec!["private".into()],
            blast_entities_ceiling: 50, blast_sends_ceiling: 10, blast_money_ceiling_cents: 5_000 } };
        let spend = SpendSnapshot { month_to_date_cents: 0 };
        // L5 + irreversible ⇒ L4 ⇒ still Execute
        assert!(matches!(g.evaluate(&envl(Reversal::Irreversible, BlastRadius::default()), &spend), Decision::Execute(_)));
        // L5 + big blast ⇒ clamp L3 ⇒ Queue
        let big = BlastRadius { entities: 500, ..Default::default() };
        assert!(matches!(g.evaluate(&envl(Reversal::Compensating, big), &spend), Decision::Queue));
    }

    #[test]
    fn transition_table_rejects_illegal() {
        let mut e = envl(Reversal::Compensating, BlastRadius::default());
        assert!(e.transition(EnvelopeState::Executing, "x", &FixedClock).is_err());
        e.transition(EnvelopeState::Approved, "gov", &FixedClock).unwrap();
        e.transition(EnvelopeState::Executing, "exec", &FixedClock).unwrap();
        e.transition(EnvelopeState::Executed, "exec", &FixedClock).unwrap();
        assert_eq!(e.history.len(), 3);
    }
}
