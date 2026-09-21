use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::Scope;

pub const RUNTIME_PIPELINE_MOVE_STAGE_DEAL: &str = "execution-handler:pipeline/move_stage/deal";
pub const RUNTIME_TENANT_SCOPED_ENVELOPE_GET: &str = "envelope-store:tenant-scoped-get";
pub const RUNTIME_BRIDGE_DEPLOY_ADAPTER: &str = "execution-handler:bridges/deploy_adapter/*";
pub const RUNTIME_BRIDGE_PAUSE_ADAPTER: &str = "execution-handler:bridges/pause_adapter/*";
pub const RUNTIME_BRIDGE_RESUME_ADAPTER: &str = "execution-handler:bridges/resume_adapter/*";
pub const RUNTIME_BRIDGE_SYNC_ADAPTER: &str = "execution-handler:bridges/sync_adapter/*";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCategory {
    Query,
    Command,
    Workflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    ReadOnly,
    Low,
    Moderate,
    High,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReversalSemantics {
    NotApplicable,
    Reversible,
    RequiresCompensation,
    Irreversible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencySemantics {
    NotApplicable,
    NaturallyIdempotent,
    RequiredKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Synchronous,
    Asynchronous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernorBinding {
    pub domain: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    pub name: String,
    pub version: String,
    pub category: CapabilityCategory,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub required_scopes: Vec<Scope>,
    pub risk_class: RiskClass,
    pub reversal: ReversalSemantics,
    pub idempotency: IdempotencySemantics,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governor_binding: Option<GovernorBinding>,
    pub execution_mode: ExecutionMode,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_runtime_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deprecated_aliases: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CapabilityRegistryError {
    #[error("invalid capability descriptor '{0}'")]
    InvalidDescriptor(String),
    #[error("duplicate capability or alias '{0}'")]
    DuplicateName(String),
}

#[derive(Debug, Clone)]
pub struct CapabilityRegistry {
    descriptors: BTreeMap<String, CapabilityDescriptor>,
    aliases: BTreeMap<String, String>,
}

impl CapabilityRegistry {
    pub fn new(
        descriptors: impl IntoIterator<Item = CapabilityDescriptor>,
    ) -> Result<Self, CapabilityRegistryError> {
        let mut by_name = BTreeMap::new();
        for descriptor in descriptors {
            validate_descriptor(&descriptor)?;
            let name = descriptor.name.clone();
            if by_name.insert(name.clone(), descriptor).is_some() {
                return Err(CapabilityRegistryError::DuplicateName(name));
            }
        }

        let canonical_names = by_name.keys().cloned().collect::<BTreeSet<_>>();
        let mut aliases = BTreeMap::new();
        for descriptor in by_name.values() {
            for alias in &descriptor.deprecated_aliases {
                if canonical_names.contains(alias)
                    || aliases
                        .insert(alias.clone(), descriptor.name.clone())
                        .is_some()
                {
                    return Err(CapabilityRegistryError::DuplicateName(alias.clone()));
                }
            }
        }

        Ok(Self {
            descriptors: by_name,
            aliases,
        })
    }

    pub fn nexus_v1() -> Result<Self, CapabilityRegistryError> {
        Self::new(nexus_v1_descriptors())
    }

    pub fn nexus_v1_with_runtime_capabilities<I, S>(
        runtime_capabilities: I,
    ) -> Result<Self, CapabilityRegistryError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let runtime_capabilities = runtime_capabilities
            .into_iter()
            .map(Into::into)
            .collect::<BTreeSet<_>>();
        let descriptors = nexus_v1_descriptors().into_iter().map(|mut descriptor| {
            if !descriptor.available
                && !descriptor.required_runtime_capabilities.is_empty()
                && descriptor
                    .required_runtime_capabilities
                    .iter()
                    .all(|required| runtime_capabilities.contains(required))
            {
                descriptor.available = true;
                descriptor.unavailable_reason = None;
            }
            descriptor
        });
        Self::new(descriptors)
    }

    pub fn resolve(&self, name: &str) -> Option<(&CapabilityDescriptor, bool)> {
        if let Some(descriptor) = self.descriptors.get(name) {
            return Some((descriptor, false));
        }
        self.aliases
            .get(name)
            .and_then(|canonical| self.descriptors.get(canonical))
            .map(|descriptor| (descriptor, true))
    }

    pub fn get(&self, name: &str) -> Option<&CapabilityDescriptor> {
        self.resolve(name).map(|(descriptor, _)| descriptor)
    }

    pub fn descriptors(&self) -> Vec<&CapabilityDescriptor> {
        self.descriptors.values().collect()
    }

    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        match Self::nexus_v1() {
            Ok(registry) => registry,
            Err(error) => panic!("static capability registry is invalid: {error}"),
        }
    }
}

fn validate_descriptor(descriptor: &CapabilityDescriptor) -> Result<(), CapabilityRegistryError> {
    if descriptor.name.trim().is_empty()
        || descriptor.version.trim().is_empty()
        || descriptor.description.trim().is_empty()
        || descriptor.required_scopes.is_empty()
        || !descriptor.input_schema.is_object()
        || !descriptor.output_schema.is_object()
        || (descriptor.available && descriptor.unavailable_reason.is_some())
        || (!descriptor.available
            && descriptor
                .unavailable_reason
                .as_ref()
                .is_none_or(|reason| reason.trim().is_empty()))
        || descriptor
            .deprecated_aliases
            .iter()
            .any(|alias| alias.trim().is_empty() || alias == &descriptor.name)
    {
        return Err(CapabilityRegistryError::InvalidDescriptor(
            descriptor.name.clone(),
        ));
    }
    Ok(())
}

fn nexus_v1_descriptors() -> Vec<CapabilityDescriptor> {
    vec![
        capability(
            "hydra.capabilities.list",
            CapabilityCategory::Query,
            "List Hydra's canonical Nexus interoperability capabilities and current availability.",
            empty_object_schema(),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["capabilities"],
                "properties": { "capabilities": { "type": "array", "items": { "type": "object" } } }
            }),
            vec![Scope::CapabilitiesRead],
            RiskClass::ReadOnly,
            ReversalSemantics::NotApplicable,
            IdempotencySemantics::NaturallyIdempotent,
            None,
            ExecutionMode::Synchronous,
            true,
            None,
            vec![],
            vec![],
        ),
        capability(
            "hydra.crm.context",
            CapabilityCategory::Query,
            "Return a compact deterministic CRM context projection for the bound business.",
            empty_object_schema(),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["tenant", "pipeline", "pending_approvals", "bridge_health", "capability_availability"],
                "properties": {
                    "tenant": { "type": "object" },
                    "pipeline": { "type": "object" },
                    "pending_approvals": { "type": "integer", "minimum": 0 },
                    "bridge_health": { "type": "array", "items": { "type": "object" } },
                    "capability_availability": { "type": "array", "items": { "type": "object" } }
                }
            }),
            vec![Scope::CrmContextRead],
            RiskClass::ReadOnly,
            ReversalSemantics::NotApplicable,
            IdempotencySemantics::NaturallyIdempotent,
            None,
            ExecutionMode::Synchronous,
            true,
            None,
            vec![],
            vec![],
        ),
        capability(
            "hydra.crm.get",
            CapabilityCategory::Query,
            "Get one canonical Hydra CDM entity by kind and identifier.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["kind", "entity_id"],
                "properties": {
                    "kind": { "type": "string", "minLength": 1 },
                    "entity_id": { "type": "string", "format": "uuid" }
                }
            }),
            entity_output_schema(),
            vec![Scope::CrmRead],
            RiskClass::ReadOnly,
            ReversalSemantics::NotApplicable,
            IdempotencySemantics::NaturallyIdempotent,
            None,
            ExecutionMode::Synchronous,
            true,
            None,
            vec![],
            vec!["hydra.get_entity"],
        ),
        capability(
            "hydra.crm.pipeline_summary",
            CapabilityCategory::Query,
            "Summarize canonical deal counts and value by pipeline stage.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": { "pipeline_id": { "type": ["string", "null"] } }
            }),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["total_deals", "stages"],
                "properties": {
                    "total_deals": { "type": "integer", "minimum": 0 },
                    "stages": { "type": "array", "items": { "type": "object" } }
                }
            }),
            vec![Scope::CrmRead],
            RiskClass::ReadOnly,
            ReversalSemantics::NotApplicable,
            IdempotencySemantics::NaturallyIdempotent,
            None,
            ExecutionMode::Synchronous,
            true,
            None,
            vec![],
            vec!["hydra.pipeline_stats"],
        ),
        capability(
            "hydra.crm.propose_action",
            CapabilityCategory::Command,
            "Propose a governed canonical deal stage change; this never mutates a CRM record directly.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["deal_id", "stage", "rationale", "idempotency_key"],
                "properties": {
                    "deal_id": { "type": "string", "format": "uuid" },
                    "stage": { "type": "string", "minLength": 1 },
                    "rationale": { "type": "string", "minLength": 1, "maxLength": 2000 },
                    "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 200 },
                    "objective_id": { "type": ["string", "null"], "minLength": 1, "maxLength": 200 },
                    "task_id": { "type": ["string", "null"], "minLength": 1, "maxLength": 200 }
                }
            }),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["envelope_id", "state", "decision"],
                "properties": {
                    "envelope_id": { "type": "string", "format": "uuid" },
                    "state": { "type": "string" },
                    "decision": { "type": "string" }
                }
            }),
            vec![Scope::CrmPropose],
            RiskClass::Moderate,
            ReversalSemantics::RequiresCompensation,
            IdempotencySemantics::RequiredKey,
            Some(GovernorBinding {
                domain: "pipeline".to_owned(),
                action: "move_stage".to_owned(),
                kind: Some("deal".to_owned()),
            }),
            ExecutionMode::Asynchronous,
            false,
            Some("idempotent external proposal path is implemented in EP-013"),
            vec![RUNTIME_PIPELINE_MOVE_STAGE_DEAL],
            vec!["hydra.propose_envelope"],
        ),
        capability(
            "hydra.crm.search",
            CapabilityCategory::Query,
            "Search canonical Hydra CDM entities using a bounded provider-neutral query.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["query"],
                "properties": {
                    "query": { "type": "string", "minLength": 1, "maxLength": 500 },
                    "kind": { "type": ["string", "null"] },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                }
            }),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["items"],
                "properties": { "items": { "type": "array", "items": entity_output_schema() } }
            }),
            vec![Scope::CrmRead],
            RiskClass::ReadOnly,
            ReversalSemantics::NotApplicable,
            IdempotencySemantics::NaturallyIdempotent,
            None,
            ExecutionMode::Synchronous,
            true,
            None,
            vec![],
            vec!["hydra.search_entities"],
        ),
        capability(
            "hydra.bridges.deploy",
            CapabilityCategory::Command,
            "Propose governed activation of a prebuilt, digest-pinned Hydra bridge component.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["adapter_id", "wiring_ref", "grant", "rationale", "idempotency_key"],
                "properties": {
                    "adapter_id": { "type": "string", "minLength": 1, "maxLength": 128 },
                    "wiring_ref": { "type": "string", "minLength": 1, "maxLength": 256 },
                    "grant": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["origins", "secret_names", "fuel"],
                        "properties": {
                            "origins": { "type": "array", "items": { "type": "string" } },
                            "secret_names": { "type": "array", "items": { "type": "string" } },
                            "dsn_name": { "type": ["string", "null"] },
                            "fuel": { "type": "integer", "minimum": 1 }
                        }
                    },
                    "rationale": { "type": "string", "minLength": 1, "maxLength": 2000 },
                    "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 200 },
                    "config": { "type": "object" }
                }
            }),
            governed_receipt_schema(),
            vec![Scope::BridgesAdmin],
            RiskClass::Moderate,
            ReversalSemantics::RequiresCompensation,
            IdempotencySemantics::RequiredKey,
            Some(GovernorBinding {
                domain: "bridges".to_owned(),
                action: "deploy_adapter".to_owned(),
                kind: None,
            }),
            ExecutionMode::Asynchronous,
            false,
            Some("prebuilt bridge lifecycle runtime is not configured"),
            vec![RUNTIME_BRIDGE_DEPLOY_ADAPTER],
            vec![],
        ),
        capability(
            "hydra.bridges.pause",
            CapabilityCategory::Command,
            "Propose a governed pause of an active Hydra bridge adapter.",
            bridge_lifecycle_input_schema(),
            governed_receipt_schema(),
            vec![Scope::BridgesAdmin],
            RiskClass::Low,
            ReversalSemantics::Reversible,
            IdempotencySemantics::RequiredKey,
            Some(GovernorBinding {
                domain: "bridges".to_owned(),
                action: "pause_adapter".to_owned(),
                kind: None,
            }),
            ExecutionMode::Asynchronous,
            false,
            Some("prebuilt bridge lifecycle runtime is not configured"),
            vec![RUNTIME_BRIDGE_PAUSE_ADAPTER],
            vec![],
        ),
        capability(
            "hydra.bridges.resume",
            CapabilityCategory::Command,
            "Propose a governed resume of a paused Hydra bridge adapter after re-probing it.",
            bridge_lifecycle_input_schema(),
            governed_receipt_schema(),
            vec![Scope::BridgesAdmin],
            RiskClass::Low,
            ReversalSemantics::Reversible,
            IdempotencySemantics::RequiredKey,
            Some(GovernorBinding {
                domain: "bridges".to_owned(),
                action: "resume_adapter".to_owned(),
                kind: None,
            }),
            ExecutionMode::Asynchronous,
            false,
            Some("prebuilt bridge lifecycle runtime is not configured"),
            vec![RUNTIME_BRIDGE_RESUME_ADAPTER],
            vec![],
        ),
        capability(
            "hydra.bridges.sync",
            CapabilityCategory::Command,
            "Propose a governed incremental synchronization page from an active Hydra bridge into canonical CRM data.",
            bridge_sync_input_schema(),
            governed_receipt_schema(),
            vec![Scope::BridgesAdmin],
            RiskClass::Moderate,
            ReversalSemantics::RequiresCompensation,
            IdempotencySemantics::RequiredKey,
            Some(GovernorBinding {
                domain: "bridges".to_owned(),
                action: "sync_adapter".to_owned(),
                kind: None,
            }),
            ExecutionMode::Asynchronous,
            false,
            Some("prebuilt bridge lifecycle runtime is not configured"),
            vec![RUNTIME_BRIDGE_SYNC_ADAPTER],
            vec![],
        ),
        capability(
            "hydra.crm.timeline",
            CapabilityCategory::Query,
            "Return a bounded canonical event timeline for an entity or the bound business.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "entity_id": { "type": ["string", "null"], "format": "uuid" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                }
            }),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["events"],
                "properties": { "events": { "type": "array", "items": { "type": "object" } } }
            }),
            vec![Scope::CrmRead],
            RiskClass::ReadOnly,
            ReversalSemantics::NotApplicable,
            IdempotencySemantics::NaturallyIdempotent,
            None,
            ExecutionMode::Synchronous,
            false,
            Some("tenant-scoped canonical timeline service is implemented with EP-014 events"),
            vec!["event-contract:v1"],
            vec![],
        ),
        capability(
            "hydra.envelopes.get",
            CapabilityCategory::Query,
            "Get one tenant-scoped governed ActionEnvelope projection.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["envelope_id"],
                "properties": { "envelope_id": { "type": "string", "format": "uuid" } }
            }),
            json!({ "type": "object" }),
            vec![Scope::EnvelopesRead],
            RiskClass::ReadOnly,
            ReversalSemantics::NotApplicable,
            IdempotencySemantics::NaturallyIdempotent,
            None,
            ExecutionMode::Synchronous,
            false,
            Some("tenant-scoped envelope lookup is implemented in EP-013"),
            vec![RUNTIME_TENANT_SCOPED_ENVELOPE_GET],
            vec![],
        ),
        capability(
            "hydra.envelopes.list",
            CapabilityCategory::Query,
            "List bounded tenant-scoped governed ActionEnvelope projections.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "state": { "type": "string", "default": "pending_approval" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                }
            }),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["envelopes"],
                "properties": { "envelopes": { "type": "array", "items": { "type": "object" } } }
            }),
            vec![Scope::EnvelopesRead],
            RiskClass::ReadOnly,
            ReversalSemantics::NotApplicable,
            IdempotencySemantics::NaturallyIdempotent,
            None,
            ExecutionMode::Synchronous,
            true,
            None,
            vec![],
            vec!["hydra.list_pending"],
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn capability(
    name: &str,
    category: CapabilityCategory,
    description: &str,
    input_schema: Value,
    output_schema: Value,
    required_scopes: Vec<Scope>,
    risk_class: RiskClass,
    reversal: ReversalSemantics,
    idempotency: IdempotencySemantics,
    governor_binding: Option<GovernorBinding>,
    execution_mode: ExecutionMode,
    available: bool,
    unavailable_reason: Option<&str>,
    required_runtime_capabilities: Vec<&str>,
    deprecated_aliases: Vec<&str>,
) -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        category,
        description: description.to_owned(),
        input_schema,
        output_schema,
        required_scopes,
        risk_class,
        reversal,
        idempotency,
        governor_binding,
        execution_mode,
        available,
        unavailable_reason: unavailable_reason.map(str::to_owned),
        required_runtime_capabilities: required_runtime_capabilities
            .into_iter()
            .map(str::to_owned)
            .collect(),
        deprecated_aliases: deprecated_aliases.into_iter().map(str::to_owned).collect(),
    }
}

fn empty_object_schema() -> Value {
    json!({ "type": "object", "additionalProperties": false })
}

fn bridge_lifecycle_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["adapter_id", "rationale", "idempotency_key"],
        "properties": {
            "adapter_id": { "type": "string", "minLength": 1, "maxLength": 128 },
            "rationale": { "type": "string", "minLength": 1, "maxLength": 2000 },
            "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 200 }
        }
    })
}

fn bridge_sync_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["adapter_id", "kind", "limit", "rationale", "idempotency_key"],
        "properties": {
            "adapter_id": { "type": "string", "minLength": 1, "maxLength": 128 },
            "kind": { "type": "string", "minLength": 1, "maxLength": 128 },
            "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
            "rationale": { "type": "string", "minLength": 1, "maxLength": 2000 },
            "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 200 }
        }
    })
}

fn governed_receipt_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["envelope_id", "state", "decision"],
        "properties": {
            "envelope_id": { "type": "string", "format": "uuid" },
            "state": { "type": "string" },
            "decision": { "type": "string" }
        }
    })
}

fn entity_output_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "kind", "tenant", "body", "origin", "version"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "kind": { "type": "string" },
            "tenant": { "type": "string", "format": "uuid" },
            "body": { "type": "object" },
            "origin": { "type": "string" },
            "origin_ref": { "type": ["string", "null"] },
            "version": { "type": "integer", "minimum": 1 }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_registry_is_deterministic_and_complete() {
        let registry = CapabilityRegistry::nexus_v1().expect("static registry should validate");
        let names = registry
            .descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "hydra.bridges.deploy",
                "hydra.bridges.pause",
                "hydra.bridges.resume",
                "hydra.bridges.sync",
                "hydra.capabilities.list",
                "hydra.crm.context",
                "hydra.crm.get",
                "hydra.crm.pipeline_summary",
                "hydra.crm.propose_action",
                "hydra.crm.search",
                "hydra.crm.timeline",
                "hydra.envelopes.get",
                "hydra.envelopes.list",
            ]
        );
    }

    #[test]
    fn capability_registry_resolves_only_safe_deprecated_aliases() {
        let registry = CapabilityRegistry::default();
        let Some((descriptor, deprecated)) = registry.resolve("hydra.search_entities") else {
            panic!("safe search alias should resolve");
        };
        assert!(deprecated);
        assert_eq!(descriptor.name, "hydra.crm.search");
        assert!(registry.resolve("hydra.approve").is_none());
        assert!(registry.resolve("hydra.tk_stats").is_none());
    }

    #[test]
    fn capability_registry_rejects_duplicate_canonical_names() {
        let mut descriptor = CapabilityRegistry::default().descriptors()[0].clone();
        descriptor.deprecated_aliases.clear();
        let result = CapabilityRegistry::new([descriptor.clone(), descriptor]);
        assert!(matches!(
            result,
            Err(CapabilityRegistryError::DuplicateName(_))
        ));
    }

    #[test]
    fn capability_registry_advertises_unavailable_reason_truthfully() {
        let registry = CapabilityRegistry::default();
        let proposal = registry
            .get("hydra.crm.propose_action")
            .expect("proposal descriptor exists");
        assert!(!proposal.available);
        assert!(proposal.unavailable_reason.is_some());
        assert_eq!(proposal.idempotency, IdempotencySemantics::RequiredKey);

        for name in [
            "hydra.bridges.deploy",
            "hydra.bridges.pause",
            "hydra.bridges.resume",
            "hydra.bridges.sync",
        ] {
            assert!(registry.get(name).is_some());
            assert!(
                !registry
                    .get(name)
                    .expect("bridge capability exists")
                    .available
            );
        }
    }
}
