use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contract {
    EnvelopeProposal,
    UnifiedDiff,
    MappingYaml,
    PlainAnswer,
}

impl Contract {
    pub fn max_bytes(self) -> usize {
        match self {
            Self::EnvelopeProposal => 2 * 1024,
            Self::UnifiedDiff => 8 * 1024,
            Self::MappingYaml => 16 * 1024,
            Self::PlainAnswer => 4 * 1024,
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::EnvelopeProposal => {
                "a single JSON EnvelopeProposal object (domain, action, targets, payload, rationale, reversal, blast)"
            }
            Self::UnifiedDiff => {
                "a unified diff starting with '--- ' touching only the files you were told to change"
            }
            Self::MappingYaml => {
                "a YAML mapping document with top-level keys: adapter, entity, fields"
            }
            Self::PlainAnswer => "a short plain-text answer, no code fences",
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

pub fn validate(contract: Contract, raw: &str) -> Result<Value, ContractError> {
    let cap = contract.max_bytes();
    if raw.len() > cap {
        return Err(ContractError::TooLarge {
            got: raw.len(),
            cap,
        });
    }

    match contract {
        Contract::EnvelopeProposal => {
            let value: Value = serde_json::from_str(raw.trim())
                .map_err(|error| ContractError::Shape(format!("not JSON: {error}")))?;
            for key in [
                "domain",
                "action",
                "targets",
                "payload",
                "rationale",
                "reversal",
                "blast",
            ] {
                if value.get(key).is_none() {
                    return Err(ContractError::Shape(format!("missing key '{key}'")));
                }
            }

            if value
                .get("payload")
                .map(|payload| payload.to_string().len() > 1024)
                .unwrap_or(false)
            {
                return Err(ContractError::Shape(
                    "payload itself must reference entities, not embed documents (>1KiB)".into(),
                ));
            }

            Ok(value)
        }
        Contract::UnifiedDiff => {
            let trimmed = raw.trim_start();
            if !trimmed.starts_with("--- ") {
                return Err(ContractError::Shape("must start with '--- '".into()));
            }
            if !trimmed.contains("\n+++ ") || !trimmed.contains("\n@@") {
                return Err(ContractError::Shape(
                    "missing '+++' header or '@@' hunk".into(),
                ));
            }
            Ok(Value::String(raw.to_owned()))
        }
        Contract::MappingYaml => {
            for key in ["adapter:", "entity:", "fields:"] {
                if !raw.lines().any(|line| line.starts_with(key)) {
                    return Err(ContractError::Shape(format!(
                        "missing top-level key '{key}'"
                    )));
                }
            }
            if raw.contains("```") {
                return Err(ContractError::Shape(
                    "no code fences in mapping output".into(),
                ));
            }
            Ok(Value::String(raw.to_owned()))
        }
        Contract::PlainAnswer => {
            if raw.contains("```") {
                return Err(ContractError::Shape(
                    "no code fences in plain answers".into(),
                ));
            }
            Ok(Value::String(raw.trim().to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tk_envelope_proposal_shape() {
        let good = r#"{"domain":"pipeline","action":"move_stage","targets":["d1"],"payload":{"stage":"won"},"rationale":"90d idle","reversal":"Compensating","blast":{"entities":1,"external_sends":0,"money_cents":0,"pii_egress":false}}"#;
        assert!(validate(Contract::EnvelopeProposal, good).is_ok());
        assert!(validate(Contract::EnvelopeProposal, r#"{"domain":"x"}"#).is_err());
    }

    #[test]
    fn tk_diff_must_be_a_diff() {
        assert!(validate(Contract::UnifiedDiff, "here is the whole file").is_err());
        assert!(validate(
            Contract::UnifiedDiff,
            "--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-a\n+b\n",
        )
        .is_ok());
    }
}
