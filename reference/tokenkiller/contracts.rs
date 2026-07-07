//! reference/tokenkiller/contracts.rs — per-route output contracts (SPEC-009 TK6).
//! NukeGuard bounds SIZE; contracts bound SHAPE. A full-file dump under budget is
//! still a violation. Contract failure follows the same repair-once path as a nuke.

use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub enum Contract { EnvelopeProposal, UnifiedDiff, MappingYaml, PlainAnswer }

impl Contract {
    pub fn max_bytes(self) -> usize {
        match self {
            Contract::EnvelopeProposal => 2 * 1024,
            Contract::UnifiedDiff => 8 * 1024,
            Contract::MappingYaml => 16 * 1024,
            Contract::PlainAnswer => 4 * 1024,
        }
    }
    /// One-line description embedded in repair prompts (nukeguard::repair_tail).
    pub fn summary(self) -> &'static str {
        match self {
            Contract::EnvelopeProposal => "a single JSON EnvelopeProposal object (domain, action, targets, payload, rationale, reversal, blast)",
            Contract::UnifiedDiff => "a unified diff starting with '--- ' touching only the files you were told to change",
            Contract::MappingYaml => "a YAML mapping document with top-level keys: adapter, entity, fields",
            Contract::PlainAnswer => "a short plain-text answer, no code fences",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("output exceeds contract byte cap ({got} > {cap})")]
    TooLarge { got: usize, cap: usize },
    #[error("contract shape violation: {0}")]
    Shape(String),
}

pub fn validate(c: Contract, raw: &str) -> Result<Value, ContractError> {
    let cap = c.max_bytes();
    if raw.len() > cap { return Err(ContractError::TooLarge { got: raw.len(), cap }); }
    match c {
        Contract::EnvelopeProposal => {
            let v: Value = serde_json::from_str(raw.trim())
                .map_err(|e| ContractError::Shape(format!("not JSON: {e}")))?;
            for k in ["domain", "action", "targets", "payload", "rationale", "reversal", "blast"] {
                if v.get(k).is_none() {
                    return Err(ContractError::Shape(format!("missing key '{k}'")));
                }
            }
            if v.get("payload").map(|p| p.to_string().len() > 1024).unwrap_or(false) {
                return Err(ContractError::Shape("payload itself must reference entities, not embed documents (>1KiB)".into()));
            }
            Ok(v)
        }
        Contract::UnifiedDiff => {
            let t = raw.trim_start();
            if !t.starts_with("--- ") {
                return Err(ContractError::Shape("must start with '--- '".into()));
            }
            if !t.contains("\n+++ ") || !t.contains("\n@@") {
                return Err(ContractError::Shape("missing '+++' header or '@@' hunk".into()));
            }
            Ok(Value::String(raw.to_string()))
        }
        Contract::MappingYaml => {
            // Shape gate without a YAML dep at this layer: required top-level keys at col 0.
            for k in ["adapter:", "entity:", "fields:"] {
                if !raw.lines().any(|l| l.starts_with(k)) {
                    return Err(ContractError::Shape(format!("missing top-level key '{k}'")));
                }
            }
            if raw.contains("```") {
                return Err(ContractError::Shape("no code fences in mapping output".into()));
            }
            Ok(Value::String(raw.to_string()))
        }
        Contract::PlainAnswer => {
            if raw.contains("```") {
                return Err(ContractError::Shape("no code fences in plain answers".into()));
            }
            Ok(Value::String(raw.trim().to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tk_envelope_proposal_shape() {
        let good = r#"{"domain":"pipeline","action":"move_stage","targets":["d1"],
            "payload":{"stage":"won"},"rationale":"90d idle","reversal":"Compensating",
            "blast":{"entities":1,"external_sends":0,"money_cents":0,"pii_egress":false}}"#;
        assert!(validate(Contract::EnvelopeProposal, good).is_ok());
        assert!(validate(Contract::EnvelopeProposal, "{\"domain\":\"x\"}").is_err());
    }
    #[test]
    fn tk_diff_must_be_a_diff() {
        assert!(validate(Contract::UnifiedDiff, "here is the whole file:\nfn main(){}").is_err());
        assert!(validate(Contract::UnifiedDiff, "--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-a\n+b\n").is_ok());
    }
}
