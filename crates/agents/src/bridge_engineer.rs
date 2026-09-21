//! BridgeEngineer — state machine that orchestrates adapter discovery -> wiring -> draft.
//!
//! The loop is a 7-step state machine:
//!   Discover → Introspect → Synthesize → Conform → Wire → Canary → Draft
//!
//! The historical loop still stops before activation. The bounded async
//! synthesis seam below produces only a reviewable mapping proposal through
//! TOKENKILLER; it never generates or executes adapter code.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{AgentCapabilityAvailability, AgentCapabilityDescriptor};

/// Error type for agent-level failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    #[error("discovery failed: {0}")]
    DiscoveryFailed(String),
    #[error("introspection failed: {0}")]
    IntrospectFailed(String),
    #[error("synthesis not yet implemented: {0}")]
    SynthesisNotImplemented(String),
    #[error("synthesis input is invalid: {0}")]
    InputInvalid(String),
    #[error("synthesis mapping is invalid: {0}")]
    MappingInvalid(String),
    #[error("TOKENKILLER synthesis failed: {0}")]
    Tokenkiller(String),
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

/// Bounded adapter metadata accepted by the proposal-only synthesis path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeSynthesisRequest {
    #[serde(rename = "adapterId")]
    pub adapter_id: String,
    pub descriptor: String,
    pub schema: String,
}

/// Normalized, non-executable mapping proposal returned by BridgeEngineer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeSynthesisDraft {
    #[serde(rename = "adapterId")]
    pub adapter_id: String,
    pub entity: String,
    pub mapping: String,
    #[serde(rename = "validationReport")]
    pub validation_report: String,
    pub repaired: bool,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MappingDocument {
    adapter: String,
    entity: String,
    fields: BTreeMap<String, String>,
}

const MAX_ADAPTER_ID_BYTES: usize = 128;
const MAX_DESCRIPTOR_BYTES: usize = 8 * 1024;
const MAX_SCHEMA_BYTES: usize = 16 * 1024;
const MAX_MAPPING_FIELDS: usize = 64;
const MAX_MAPPING_NAME_BYTES: usize = 128;
const MAX_MAPPING_VALUE_BYTES: usize = 256;

const STABLE_S0: &str =
    "You are HYDRA BridgeEngineer. Produce a bounded CRM bridge mapping proposal only.";
const STABLE_S1: &str = "Return YAML only with adapter, entity, and fields top-level keys. Do not emit code, secrets, URLs, or customer records.";
const STABLE_S2: &str = "Mapping proposals are review-only. Preserve Hydra CDM identity and existing adapter fields. Keep mappings bounded and reversible.";

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
    pub fn capability() -> AgentCapabilityDescriptor {
        AgentCapabilityDescriptor {
            name: "hydra.agent.bridge_engineer.deploy_proposal".to_owned(),
            availability: AgentCapabilityAvailability::Unavailable,
            envelope_only: true,
            reason: Some(
                "adapter synthesis stops at SynthesisNotImplemented; no deploy envelope is produced"
                    .to_owned(),
            ),
        }
    }

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
                LoopStep::Conform | LoopStep::Wire | LoopStep::Canary | LoopStep::Draft => {
                    // Unreachable until Synthesize is wired.
                    return Err(AgentError::Internal(format!(
                        "step {step:?} reached without synthesis"
                    )));
                }
            }
        }
    }

    /// Produce a bounded mapping proposal through the existing TOKENKILLER
    /// `bridge_mapping` route. This method deliberately stops before any
    /// conformance, wiring, activation, or CRM mutation step.
    pub async fn synthesize(
        session: &tokenkiller::Session,
        request: BridgeSynthesisRequest,
    ) -> Result<BridgeSynthesisDraft, AgentError> {
        validate_request(&request)?;

        let tail = format!(
            "adapter_id: {}\ndescriptor:\n{}\nschema:\n{}",
            request.adapter_id, request.descriptor, request.schema
        );
        let contracted = session
            .complete(
                "bridge_mapping",
                vec![
                    tokenkiller::Segment {
                        stability: tokenkiller::Stability::S0,
                        text: STABLE_S0.to_owned(),
                        version: 1,
                    },
                    tokenkiller::Segment {
                        stability: tokenkiller::Stability::S1,
                        text: STABLE_S1.to_owned(),
                        version: 1,
                    },
                    tokenkiller::Segment {
                        stability: tokenkiller::Stability::S2,
                        text: STABLE_S2.to_owned(),
                        version: 1,
                    },
                ],
                tail,
            )
            .await
            .map_err(|error| AgentError::Tokenkiller(error.to_string()))?;

        let document: MappingDocument = serde_yaml::from_str(&contracted.raw)
            .map_err(|error| AgentError::MappingInvalid(format!("YAML shape: {error}")))?;
        validate_mapping(&request.adapter_id, &document)?;
        let mapping = serde_yaml::to_string(&document)
            .map_err(|error| AgentError::MappingInvalid(format!("YAML normalization: {error}")))?
            .trim()
            .to_owned();

        let provider = safe_provenance(
            if contracted.provenance.provider.is_empty() {
                &contracted.ledger_row.provider
            } else {
                &contracted.provenance.provider
            },
            "provider",
        );
        let model = safe_provenance(&contracted.provenance.model, "model");

        Ok(BridgeSynthesisDraft {
            adapter_id: document.adapter,
            entity: document.entity,
            mapping,
            validation_report:
                "mapping_only: validated; wasm_conformance: unavailable; activation: unavailable"
                    .to_owned(),
            repaired: contracted.repaired,
            provider,
            model,
        })
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

fn validate_request(request: &BridgeSynthesisRequest) -> Result<(), AgentError> {
    validate_name("adapterId", &request.adapter_id, MAX_ADAPTER_ID_BYTES)?;
    validate_metadata("descriptor", &request.descriptor, MAX_DESCRIPTOR_BYTES)?;
    validate_metadata("schema", &request.schema, MAX_SCHEMA_BYTES)?;
    Ok(())
}

fn validate_mapping(requested_adapter: &str, document: &MappingDocument) -> Result<(), AgentError> {
    if document.adapter != requested_adapter {
        return Err(AgentError::MappingInvalid(
            "mapping adapter does not match request".to_owned(),
        ));
    }
    validate_name("adapter", &document.adapter, MAX_ADAPTER_ID_BYTES)
        .map_err(|error| AgentError::MappingInvalid(error.to_string()))?;
    validate_name("entity", &document.entity, MAX_MAPPING_NAME_BYTES)
        .map_err(|error| AgentError::MappingInvalid(error.to_string()))?;
    if document.fields.is_empty() || document.fields.len() > MAX_MAPPING_FIELDS {
        return Err(AgentError::MappingInvalid(format!(
            "fields must contain 1..={MAX_MAPPING_FIELDS} entries"
        )));
    }
    for (source, target) in &document.fields {
        validate_name("field", source, MAX_MAPPING_NAME_BYTES)
            .map_err(|error| AgentError::MappingInvalid(error.to_string()))?;
        validate_mapping_value(target)?;
    }
    Ok(())
}

fn validate_name(label: &str, value: &str, max_bytes: usize) -> Result<(), AgentError> {
    if value.is_empty() || value.len() > max_bytes || value.contains("..") {
        return Err(AgentError::InputInvalid(format!(
            "{label} must be non-empty, bounded, and traversal-free"
        )));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
    {
        return Err(AgentError::InputInvalid(format!(
            "{label} contains unsupported characters"
        )));
    }
    Ok(())
}

fn validate_metadata(label: &str, value: &str, max_bytes: usize) -> Result<(), AgentError> {
    if value.is_empty() || value.len() > max_bytes {
        return Err(AgentError::InputInvalid(format!(
            "{label} must be non-empty and at most {max_bytes} bytes"
        )));
    }
    let lower = value.to_ascii_lowercase();
    for marker in [
        "access_token",
        "api_key",
        "authorization",
        "bearer ",
        "password",
        "secret",
        "token",
        "http://",
        "https://",
        "```",
        "wasm",
        "fn ",
        "pub ",
    ] {
        if lower.contains(marker) {
            return Err(AgentError::InputInvalid(format!(
                "{label} contains forbidden metadata"
            )));
        }
    }
    Ok(())
}

fn validate_mapping_value(value: &str) -> Result<(), AgentError> {
    if value.is_empty() || value.len() > MAX_MAPPING_VALUE_BYTES {
        return Err(AgentError::MappingInvalid(
            "field mapping value is empty or oversized".to_owned(),
        ));
    }
    let lower = value.to_ascii_lowercase();
    for marker in [
        "access_token",
        "api_key",
        "authorization",
        "password",
        "secret",
        "token",
        "http://",
        "https://",
        "../",
        "..\\",
        "```",
    ] {
        if lower.contains(marker) {
            return Err(AgentError::MappingInvalid(
                "field mapping contains forbidden content".to_owned(),
            ));
        }
    }
    Ok(())
}

fn safe_provenance(value: &str, label: &str) -> String {
    if value.is_empty() || value.len() > 128 {
        return "unknown".to_owned();
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:/".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("{label}-redacted")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    use super::*;
    use async_trait::async_trait;

    struct MemoryLedger;

    #[async_trait]
    impl tokenkiller::LedgerSink for MemoryLedger {
        async fn record(
            &self,
            _row: &tokenkiller::LedgerRow,
        ) -> Result<(), tokenkiller::LedgerError> {
            Ok(())
        }
    }

    struct FakeRouter {
        responses:
            Mutex<VecDeque<Result<tokenkiller::CompletionResponse, tokenkiller::RouterError>>>,
    }

    #[async_trait]
    impl tokenkiller::Router for FakeRouter {
        async fn complete(
            &self,
            _request: tokenkiller::CompletionRequest,
        ) -> Result<tokenkiller::CompletionResponse, tokenkiller::RouterError> {
            self.responses
                .lock()
                .expect("fake router lock should not be poisoned")
                .pop_front()
                .expect("fake router response should be queued")
        }
    }

    fn session(
        response: Result<tokenkiller::CompletionResponse, tokenkiller::RouterError>,
    ) -> tokenkiller::Session {
        tokenkiller::Session::new(
            uuid::Uuid::nil(),
            HashMap::from([(
                "bridge_mapping".to_owned(),
                tokenkiller::RouteCfg {
                    provider: "fake".to_owned(),
                    provider_tags: vec![tokenkiller::ProviderTag::Private],
                    max_tokens: 128,
                    output_budget_bytes: 2048,
                    contract: tokenkiller::Contract::MappingYaml,
                    pii: false,
                },
            )]),
            Box::new(FakeRouter {
                responses: Mutex::new(VecDeque::from([response])),
            }),
            Box::new(MemoryLedger),
            Box::new(tokenkiller::ApproxTokenizer),
            Box::new(tokenkiller::SystemClock),
        )
    }

    fn request() -> BridgeSynthesisRequest {
        BridgeSynthesisRequest {
            adapter_id: "memcrm".to_owned(),
            descriptor: "bounded CRM adapter descriptor".to_owned(),
            schema: "deal.stage and deal.owner fields".to_owned(),
        }
    }

    fn response(raw: &str) -> tokenkiller::CompletionResponse {
        tokenkiller::CompletionResponse {
            provider: "fake".to_owned(),
            chunks: vec![raw.to_owned()],
            provenance: tokenkiller::ProviderProvenance {
                provider: "fake".to_owned(),
                model: "mapping-v1".to_owned(),
                privacy: tokenkiller::ProviderPrivacy::Private,
                ..Default::default()
            },
            ..Default::default()
        }
    }

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
        assert!(matches!(result, Err(AgentError::DiscoveryFailed(_))));
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

    #[tokio::test]
    async fn synthesis_normalizes_a_mapping_without_executable_artifacts() {
        let session = session(Ok(response(
            "adapter: memcrm\nentity: deal\nfields:\n  stage: crm.stage\n  owner: crm.owner\n",
        )));
        let draft = BridgeEngineer::synthesize(&session, request())
            .await
            .expect("valid mapping should synthesize");
        assert_eq!(draft.adapter_id, "memcrm");
        assert_eq!(draft.entity, "deal");
        assert!(draft.mapping.contains("stage: crm.stage"));
        assert!(draft.validation_report.contains("activation: unavailable"));
        assert!(!draft.mapping.contains("wasm"));
        assert_eq!(draft.provider, "fake");
    }

    #[tokio::test]
    async fn synthesis_rejects_secret_shaped_input_before_provider_call() {
        let session = session(Err(tokenkiller::RouterError {
            message: "provider should not be called".to_owned(),
        }));
        let mut request = request();
        request.schema = "access_token: forbidden".to_owned();
        let result = BridgeEngineer::synthesize(&session, request).await;
        assert!(matches!(result, Err(AgentError::InputInvalid(_))));
    }

    #[tokio::test]
    async fn synthesis_rejects_adapter_identity_mismatch() {
        let session = session(Ok(response(
            "adapter: other\nentity: deal\nfields:\n  stage: crm.stage\n",
        )));
        let result = BridgeEngineer::synthesize(&session, request()).await;
        assert!(matches!(result, Err(AgentError::MappingInvalid(_))));
    }

    #[tokio::test]
    async fn synthesis_propagates_provider_failure_without_artifact() {
        let session = session(Err(tokenkiller::RouterError {
            message: "fake provider unavailable".to_owned(),
        }));
        let result = BridgeEngineer::synthesize(&session, request()).await;
        assert!(matches!(result, Err(AgentError::Tokenkiller(_))));
    }
}
