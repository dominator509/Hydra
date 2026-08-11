//! Comms — templated notification dispatch for tenant-facing messages.
//!
//! Currently supports draft_email only. The actual send pathway is stubbed
//! pending SMTP/SES integration; this module focuses on template rendering.

use std::collections::HashMap;

use crate::bridge_engineer::AgentError;
use crate::{AgentCapabilityAvailability, AgentCapabilityDescriptor};

/// Lightweight notification dispatch.
///
/// Each method is self-contained and testable. No real transport is
/// needed for the draft step — the output is a rendered string.
pub struct Comms;

impl Comms {
    pub fn draft_capability() -> AgentCapabilityDescriptor {
        AgentCapabilityDescriptor {
            name: "hydra.agent.comms.draft".to_owned(),
            availability: AgentCapabilityAvailability::Available,
            envelope_only: false,
            reason: Some("renders a local string draft; it does not deliver messages".to_owned()),
        }
    }

    pub fn transport_capability() -> AgentCapabilityDescriptor {
        AgentCapabilityDescriptor {
            name: "hydra.agent.comms.send".to_owned(),
            availability: AgentCapabilityAvailability::Unavailable,
            envelope_only: true,
            reason: Some("no SMTP, SES, or other delivery transport is implemented".to_owned()),
        }
    }

    /// Draft an email by interpolating `vars` into the named `template`.
    ///
    /// Templates use `{{key}}` placeholders. A leading/trailing whitespace
    /// trimmed version of each var value is substituted.
    ///
    /// Returns the interpolated string.
    pub fn draft_email(
        _tenant: &str,
        template: &str,
        vars: &HashMap<String, String>,
    ) -> Result<String, AgentError> {
        let mut result = template.to_owned();

        for (key, value) in vars {
            let placeholder = format!("{{{{{key}}}}}");
            result = result.replace(&placeholder, value.trim());
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_draft_email_simple() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "Alice".into());
        vars.insert("adapter".into(), "SuiteCRM".into());

        let result = Comms::draft_email(
            "tenant-1",
            "Hello {{name}}, the {{adapter}} bridge is ready.",
            &vars,
        )
        .expect("email draft should render");

        assert_eq!(result, "Hello Alice, the SuiteCRM bridge is ready.");
    }

    #[test]
    fn test_draft_email_missing_var_retains_placeholder() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "Bob".into());

        let result =
            Comms::draft_email("tenant-1", "Hello {{name}}, your code is {{code}}.", &vars)
                .expect("email draft should preserve missing placeholders");

        assert_eq!(result, "Hello Bob, your code is {{code}}.");
    }

    #[test]
    fn test_draft_email_empty_template() {
        let vars = HashMap::new();
        let result =
            Comms::draft_email("tenant-1", "", &vars).expect("empty template should render");
        assert_eq!(result, "");
    }

    #[test]
    fn test_draft_email_trimming() {
        let mut vars = HashMap::new();
        vars.insert("val".into(), "  hello  ".into());

        let result =
            Comms::draft_email("t", "x{{val}}y", &vars).expect("trimmed value should render");
        assert_eq!(result, "xhelloy");
    }
}
