use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceContext {
    pub traceparent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
}

impl TraceContext {
    pub fn new(
        traceparent: impl Into<String>,
        tracestate: Option<String>,
    ) -> Result<Self, StoreError> {
        let context = Self {
            traceparent: traceparent.into(),
            tracestate,
        };
        context.validate()?;
        Ok(context)
    }

    pub fn fresh() -> Self {
        let trace_id = Uuid::new_v4().simple().to_string();
        Self {
            traceparent: format!("00-{trace_id}-{}-01", fresh_span_id()),
            tracestate: None,
        }
    }

    pub fn child(&self) -> Self {
        let parts = self.traceparent.split('-').collect::<Vec<_>>();
        if parts.len() != 4 {
            return Self::fresh();
        }
        Self {
            traceparent: format!("{}-{}-{}-{}", parts[0], parts[1], fresh_span_id(), parts[3]),
            tracestate: self.tracestate.clone(),
        }
    }

    pub fn trace_id(&self) -> &str {
        self.traceparent
            .split('-')
            .nth(1)
            .unwrap_or("00000000000000000000000000000000")
    }

    pub fn validate(&self) -> Result<(), StoreError> {
        let parts = self.traceparent.split('-').collect::<Vec<_>>();
        let valid = parts.len() == 4
            && parts[0] == "00"
            && valid_hex(parts[1], 32)
            && parts[1] != "00000000000000000000000000000000"
            && valid_hex(parts[2], 16)
            && parts[2] != "0000000000000000"
            && valid_hex(parts[3], 2);
        if !valid {
            return Err(StoreError::Invariant(
                "invalid W3C traceparent carrier".to_owned(),
            ));
        }
        if let Some(tracestate) = self.tracestate.as_deref() {
            let members = tracestate.split(',').collect::<Vec<_>>();
            if tracestate.is_empty()
                || tracestate.len() > 512
                || members.len() > 32
                || members.iter().any(|member| {
                    member.trim().is_empty()
                        || member.len() > 256
                        || !member.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
                })
            {
                return Err(StoreError::Invariant(
                    "invalid W3C tracestate carrier".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::fresh()
    }
}

fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn fresh_span_id() -> String {
    Uuid::new_v4().simple().to_string()[..16].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_context_child_preserves_trace_and_changes_span() {
        let parent = TraceContext::new(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            Some("vendor=value".to_owned()),
        )
        .expect("valid trace context");
        let child = parent.child();

        assert_eq!(child.trace_id(), parent.trace_id());
        assert_ne!(child.traceparent, parent.traceparent);
        assert_eq!(child.tracestate, parent.tracestate);
        child.validate().expect("valid child trace context");
    }

    #[test]
    fn trace_context_rejects_zero_or_noncanonical_identifiers() {
        for invalid in [
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
        ] {
            assert!(TraceContext::new(invalid, None).is_err());
        }
    }
}
