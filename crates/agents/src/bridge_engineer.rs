//! BridgeEngineer — state machine that orchestrates adapter discovery → wiring → draft.
//!
//! The loop is a 7-step state machine:
//!   Discover → Introspect → Synthesize → Conform → Wire → Canary → Draft
//!
//! Synthesize is the only step that would require LLM-mediated code generation
//! (via TK route `bridge_codegen`). It is a deliberate placeholder that returns
//! an error — proving the orchestration works even without real synthesis.

use serde::{Deserialize, Serialize};

/// Error type for agent-level failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    #[error("discovery failed: {0}")]
    DiscoveryFailed(String),
    #[error("introspection failed: {0}")]
    IntrospectFailed(String),
    #[error("synthesis not yet implemented: {0}")]
    SynthesisNotImplemented(String),
    #[error("conformance check failed: {0}")]
    ConformanceFailed(String),
    #[error("wiring step failed: {0}")]
    WiringFailed(String),
    #[error("canary check failed: {0}")]
    CanaryFailed(String),
    #[error("internal: {0}")]
    Internal(String),
}

/// The output of the BridgeEngineer loop: enough metadata to propose a bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeDraft {
    /// SHA-256 of the compiled adapter Wasm.
    pub wasm_sha: String,
    /// Summary of adapter conformance properties exercised.
    pub conformance_report: String,
    /// Wiring transforms derived during the loop (serialized).
    pub wiring: String,
    /// Identifier of the adapter that was discovered.
    pub adapter_id: String,
}

/// Enum of the seven steps in the engineering loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopStep {
    Discover,
    Introspect,
    Synthesize,
    Conform,
    Wire,
    Canary,
    Draft,
}

/// Orchestrator that drives adapter discovery → wiring → draft generation.
pub struct BridgeEngineer;

impl BridgeEngineer {
    /// Run the full engineering loop for the given adapter target.
    ///
    /// Each step is driven by the returned `LoopStep` sequence. The `Synthesize`
    /// step currently raises `SynthesisNotImplemented` — this is intentional,
    /// as real synthesis requires the `bridge_codegen` route which is pending.
    pub fn run(_target: &str) -> Result<EnvelopeDraft, AgentError> {
        let mut step = LoopStep::Discover;
        let mut adapter_id = String::new();
        let mut _descriptor = String::new();
        let mut _schema = String::new();

        loop {
            match step {
                LoopStep::Discover => {
                    if _target.is_empty() {
                        return Err(AgentError::DiscoveryFailed("target is empty".into()));
                    }
                    adapter_id = _target.to_owned();
                    _descriptor = format!("discovered adapter: {_target}");
                    step = LoopStep::Introspect;
                }
                LoopStep::Introspect => {
                    if _descriptor.is_empty() {
                        return Err(AgentError::IntrospectFailed(
                            "no descriptor from discovery".into(),
                        ));
                    }
                    _schema = format!("schema for {adapter_id}");
                    step = LoopStep::Synthesize;
                }
                LoopStep::Synthesize => {
                    // Honest placeholder — real synthesis needs LLM bridge_codegen.
                    return Err(AgentError::SynthesisNotImplemented(
                        "LLM synthesis not yet implemented; bridge_codegen route is pending".into(),
                    ));
                }
                LoopStep::Conform
                | LoopStep::Wire
                | LoopStep::Canary
                | LoopStep::Draft => {
                    // Unreachable until Synthesize is wired.
                    return Err(AgentError::Internal(format!(
                        "step {step:?} reached without synthesis"
                    )));
                }
            }
        }
    }

    /// Return the canonical ordered list of steps (for test assertions).
    pub fn steps() -> Vec<LoopStep> {
        vec![
            LoopStep::Discover,
            LoopStep::Introspect,
            LoopStep::Synthesize,
            LoopStep::Conform,
            LoopStep::Wire,
            LoopStep::Canary,
            LoopStep::Draft,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steps_order() {
        let steps = BridgeEngineer::steps();
        assert_eq!(steps.len(), 7);
        assert_eq!(steps[0], LoopStep::Discover);
        assert_eq!(steps[1], LoopStep::Introspect);
        assert_eq!(steps[2], LoopStep::Synthesize);
        assert_eq!(steps[3], LoopStep::Conform);
        assert_eq!(steps[4], LoopStep::Wire);
        assert_eq!(steps[5], LoopStep::Canary);
        assert_eq!(steps[6], LoopStep::Draft);
    }

    #[test]
    fn test_discover_empty_target() {
        let result = BridgeEngineer::run("");
        assert!(matches!(
            result,
            Err(AgentError::DiscoveryFailed(_))
        ));
    }

    #[test]
    fn test_synthesis_placeholder_error() {
        let result = BridgeEngineer::run("memcrm");
        assert!(matches!(
            result,
            Err(AgentError::SynthesisNotImplemented(_))
        ));
        if let Err(AgentError::SynthesisNotImplemented(msg)) = result {
            assert!(
                msg.contains("LLM synthesis"),
                "expected synthesis placeholder message, got: {msg}"
            );
        }
    }

    #[test]
    fn test_run_reaches_synthesis() {
        // Verify the loop progresses past Discover and Introspect before hitting synthesis.
        let result = BridgeEngineer::run("some-adapter");
        assert!(
            matches!(result, Err(AgentError::SynthesisNotImplemented(_))),
            "expected synthesis error, got: {result:?}"
        );
    }
}
