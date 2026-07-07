use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::ActionEnvelope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constitution {
    pub monthly_spend_cap_cents: u64,
    pub pii_egress_allowlist: Vec<String>,
    pub blast_entities_ceiling: u32,
    pub blast_sends_ceiling: u32,
    pub blast_money_ceiling_cents: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpendSnapshot {
    pub month_to_date_cents: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Rule {
    #[error("constitution: monthly spend cap reached")]
    MonthlySpendCapReached,
    #[error("constitution: pii egress not allow-listed")]
    PiiEgressNotAllowListed,
    #[error("constitution: hard_delete is forbidden")]
    HardDeleteForbidden,
}

impl Constitution {
    pub fn check(&self, envelope: &ActionEnvelope, spend: &SpendSnapshot) -> Result<(), Rule> {
        if spend.month_to_date_cents >= self.monthly_spend_cap_cents {
            return Err(Rule::MonthlySpendCapReached);
        }

        if envelope.blast.pii_egress
            && !self
                .pii_egress_allowlist
                .iter()
                .any(|entry| entry == "private")
        {
            return Err(Rule::PiiEgressNotAllowListed);
        }

        if envelope.action == "hard_delete" || payload_hard_delete(&envelope.payload) {
            return Err(Rule::HardDeleteForbidden);
        }

        Ok(())
    }
}

fn payload_hard_delete(payload: &Value) -> bool {
    payload
        .get("hard_delete")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}
