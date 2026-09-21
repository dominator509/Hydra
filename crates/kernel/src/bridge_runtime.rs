use std::sync::Arc;

use async_trait::async_trait;
use bridge_host::{
    BridgeDescriptor, BridgeLifecycle, ConformanceRequest, FullRelistRequest, Grant, ProbeRequest,
    SyncRequest,
};
use governor::{ActionEnvelope, Reversal};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::execution_registry::{
    ExecutionContext, ExecutionHandler, ExecutionHandlerDescriptor, ExecutionRegistryError,
    HandlerReceipt,
};

const DEPLOY_CAPABILITY: &str = "hydra.bridges.deploy";
const PAUSE_CAPABILITY: &str = "hydra.bridges.pause";
const RESUME_CAPABILITY: &str = "hydra.bridges.resume";
const SYNC_CAPABILITY: &str = "hydra.bridges.sync";

#[derive(Clone)]
pub struct BridgeLifecycleRuntime {
    pub store: store::Store,
    pub lifecycle: Arc<BridgeLifecycle>,
}

impl BridgeLifecycleRuntime {
    pub fn new(store: store::Store, lifecycle: Arc<BridgeLifecycle>) -> Self {
        Self { store, lifecycle }
    }

    pub fn handlers(self: &Arc<Self>) -> Vec<Arc<dyn ExecutionHandler>> {
        vec![
            Arc::new(DeployAdapterHandler {
                runtime: self.clone(),
            }),
            Arc::new(PauseAdapterHandler {
                runtime: self.clone(),
            }),
            Arc::new(ResumeAdapterHandler {
                runtime: self.clone(),
            }),
            Arc::new(SyncAdapterHandler {
                runtime: self.clone(),
            }),
        ]
    }
}

#[derive(Clone)]
pub struct BridgeConformanceRuntime {
    runtime: Arc<BridgeLifecycleRuntime>,
}

impl BridgeConformanceRuntime {
    pub fn new(runtime: Arc<BridgeLifecycleRuntime>) -> Self {
        Self { runtime }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConformanceInput {
    #[serde(rename = "adapterId")]
    adapter_id: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

#[async_trait]
impl fabric::BridgeConformanceService for BridgeConformanceRuntime {
    fn available(&self) -> bool {
        true
    }

    fn availability_reason(&self) -> &'static str {
        "configured tenant-scoped BridgeHost conformance runtime is available"
    }

    async fn conform(
        &self,
        tenant_id: uuid::Uuid,
        input: Value,
    ) -> Result<Value, fabric::FabricError> {
        let input: ConformanceInput = serde_json::from_value(input).map_err(|_| {
            fabric::FabricError::ValidationFailed(
                "bridge conformance input does not match the bounded contract".to_owned(),
            )
        })?;
        if input.adapter_id.trim().is_empty()
            || input.adapter_id.len() > 128
            || input.adapter_id.chars().any(char::is_control)
        {
            return Err(fabric::FabricError::ValidationFailed(
                "bridge conformance adapterId is invalid".to_owned(),
            ));
        }
        let Some(record) = self
            .runtime
            .store
            .bridge_adapters
            .get(tenant_id, &input.adapter_id)
            .await
            .map_err(|_| {
                fabric::FabricError::CapabilityUnavailable(
                    "bridge conformance runtime is unavailable".to_owned(),
                )
            })?
        else {
            return Err(fabric::FabricError::CapabilityUnavailable(
                "bridge conformance runtime is unavailable".to_owned(),
            ));
        };
        if record.state != store::BridgeAdapterState::Active {
            return Err(fabric::FabricError::CapabilityUnavailable(
                "bridge conformance runtime is unavailable".to_owned(),
            ));
        }
        let grant = grant_from_config(&input.adapter_id, &record.grant_config).map_err(|_| {
            fabric::FabricError::CapabilityUnavailable(
                "bridge conformance runtime is unavailable".to_owned(),
            )
        })?;
        let config_json = serde_json::to_string(&record.config).map_err(|_| {
            fabric::FabricError::CapabilityUnavailable(
                "bridge conformance runtime is unavailable".to_owned(),
            )
        })?;
        let result = self
            .runtime
            .lifecycle
            .conformance(ConformanceRequest {
                tenant_id,
                adapter_id: &input.adapter_id,
                component_ref: &record.component_ref,
                expected_sha256: Some(&record.component_sha256),
                grant,
                config_json: &config_json,
                kind: input.kind.as_deref(),
                limit: input.limit.unwrap_or(25),
            })
            .await
            .map_err(|_| {
                fabric::FabricError::CapabilityUnavailable("bridge conformance failed".to_owned())
            })?;
        serde_json::to_value(result).map_err(|_| {
            fabric::FabricError::Internal(
                "bridge conformance result serialization failed".to_owned(),
            )
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantPayload {
    origins: Vec<String>,
    secret_names: Vec<String>,
    dsn_name: Option<String>,
    fuel: u64,
}

fn adapter_id(payload: &Value) -> Result<&str, ExecutionRegistryError> {
    payload
        .get("adapter_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ExecutionRegistryError::InvalidPayload("missing adapter_id".to_owned()))
}

fn grant_from_payload(adapter_id: &str, payload: &Value) -> Result<Grant, ExecutionRegistryError> {
    let grant = payload
        .get("grant")
        .cloned()
        .ok_or_else(|| ExecutionRegistryError::InvalidPayload("missing grant".to_owned()))?;
    let grant: GrantPayload = serde_json::from_value(grant).map_err(|error| {
        ExecutionRegistryError::InvalidPayload(format!("invalid grant: {error}"))
    })?;
    Ok(Grant {
        adapter_id: adapter_id.to_owned(),
        origins: grant.origins,
        secret_names: grant.secret_names,
        dsn_name: grant.dsn_name,
        fuel: grant.fuel,
    })
}

fn grant_config(grant: &Grant) -> Value {
    json!({
        "origins": grant.origins,
        "secret_names": grant.secret_names,
        "dsn_name": grant.dsn_name,
        "fuel": grant.fuel,
    })
}

fn grant_from_config(adapter_id: &str, config: &Value) -> Result<Grant, ExecutionRegistryError> {
    let grant: GrantPayload = serde_json::from_value(config.clone()).map_err(|error| {
        ExecutionRegistryError::InvalidPayload(format!("invalid persisted grant: {error}"))
    })?;
    Ok(Grant {
        adapter_id: adapter_id.to_owned(),
        origins: grant.origins,
        secret_names: grant.secret_names,
        dsn_name: grant.dsn_name,
        fuel: grant.fuel,
    })
}

fn config_json(payload: &Value) -> Result<String, ExecutionRegistryError> {
    let config = payload.get("config").cloned().unwrap_or_else(|| json!({}));
    if !config.is_object() {
        return Err(ExecutionRegistryError::InvalidPayload(
            "config must be an object".to_owned(),
        ));
    }
    serde_json::to_string(&config).map_err(|error| {
        ExecutionRegistryError::InvalidPayload(format!("serialize config: {error}"))
    })
}

fn handler_error(error: impl std::fmt::Display) -> ExecutionRegistryError {
    ExecutionRegistryError::Handler(bounded_error(error))
}

fn bounded_error(error: impl std::fmt::Display) -> String {
    error
        .to_string()
        .chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect::<String>()
}

fn deploy_schema() -> Value {
    json!({
        "type": "object",
        "required": ["adapter_id", "wiring_ref", "grant"],
        "properties": {
            "adapter_id": { "type": "string", "minLength": 1, "maxLength": 128 },
            "wiring_ref": { "type": "string", "minLength": 1, "maxLength": 256 },
            "grant": {
                "type": "object",
                "required": ["origins", "secret_names", "fuel"],
                "properties": {
                    "origins": { "type": "array", "items": { "type": "string" } },
                    "secret_names": { "type": "array", "items": { "type": "string" } },
                    "dsn_name": { "type": ["string", "null"] },
                    "fuel": { "type": "integer", "minimum": 1 }
                },
                "additionalProperties": false
            },
            "config": { "type": "object" }
        },
        "additionalProperties": false
    })
}

fn lifecycle_schema() -> Value {
    json!({
        "type": "object",
        "required": ["adapter_id"],
        "properties": {
            "adapter_id": { "type": "string", "minLength": 1, "maxLength": 128 }
        },
        "additionalProperties": false
    })
}

fn sync_schema() -> Value {
    json!({
        "type": "object",
        "required": ["adapter_id", "kind", "limit"],
        "properties": {
            "adapter_id": { "type": "string", "minLength": 1, "maxLength": 128 },
            "kind": { "type": "string", "minLength": 1, "maxLength": 128 },
            "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
        },
        "additionalProperties": false
    })
}

fn sync_kind(payload: &Value) -> Result<&str, ExecutionRegistryError> {
    payload
        .get("kind")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ExecutionRegistryError::InvalidPayload("missing sync kind".to_owned()))
}

fn sync_limit(payload: &Value) -> Result<u32, ExecutionRegistryError> {
    let limit = payload
        .get("limit")
        .and_then(Value::as_u64)
        .ok_or_else(|| ExecutionRegistryError::InvalidPayload("missing sync limit".to_owned()))?;
    u32::try_from(limit)
        .ok()
        .filter(|value| (1..=100).contains(value))
        .ok_or_else(|| {
            ExecutionRegistryError::InvalidPayload("sync limit must be 1-100".to_owned())
        })
}

fn descriptor(
    capability_name: &str,
    action: &str,
    payload_schema: Value,
    risk_class: fabric::RiskClass,
    reversal: Reversal,
) -> ExecutionHandlerDescriptor {
    ExecutionHandlerDescriptor {
        capability_name: capability_name.to_owned(),
        domain: "bridges".to_owned(),
        action: action.to_owned(),
        kind: None,
        payload_schema,
        required_target_types: Vec::new(),
        risk_class,
        reversal,
        supports_compensation: false,
    }
}

fn receipt(
    envelope: &ActionEnvelope,
    record: &store::BridgeAdapterRecord,
    state: &str,
    extra: Value,
) -> HandlerReceipt {
    HandlerReceipt {
        envelope_id: envelope.id,
        affected_targets: vec![record.id],
        details: json!({
            "adapter_id": record.adapter_id,
            "registry_id": record.id,
            "component_sha256": record.component_sha256,
            "state": state,
            "details": extra,
        }),
    }
}

struct DeployAdapterHandler {
    runtime: Arc<BridgeLifecycleRuntime>,
}

#[async_trait]
impl ExecutionHandler for DeployAdapterHandler {
    fn descriptor(&self) -> ExecutionHandlerDescriptor {
        descriptor(
            DEPLOY_CAPABILITY,
            "deploy_adapter",
            deploy_schema(),
            fabric::RiskClass::Moderate,
            Reversal::Compensating,
        )
    }

    async fn execute(
        &self,
        _context: &ExecutionContext,
        envelope: &ActionEnvelope,
    ) -> Result<HandlerReceipt, ExecutionRegistryError> {
        let adapter_id = adapter_id(&envelope.payload)?.to_owned();
        let component_ref = envelope
            .payload
            .get("wiring_ref")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ExecutionRegistryError::InvalidPayload("missing wiring_ref".to_owned())
            })?;
        let grant = grant_from_payload(&adapter_id, &envelope.payload)?;
        let grant_config = grant_config(&grant);
        let config_json = config_json(&envelope.payload)?;
        let artifact = self
            .runtime
            .lifecycle
            .load_artifact(component_ref, None)
            .map_err(handler_error)?;

        let existing = self
            .runtime
            .store
            .bridge_adapters
            .get(envelope.tenant, &adapter_id)
            .await?;
        let mut record = if let Some(record) = existing {
            if record.component_ref != component_ref
                || record.component_sha256 != artifact.sha256
                || record.grant_config != grant_config
            {
                return Err(handler_error(
                    "adapter identity is already bound to a different component or grant",
                ));
            }
            match record.state {
                store::BridgeAdapterState::Active => {
                    return Ok(receipt(
                        envelope,
                        &record,
                        store::BridgeAdapterState::Active.as_str(),
                        json!({ "idempotent": true }),
                    ));
                }
                store::BridgeAdapterState::Paused => {
                    return Err(handler_error(
                        "adapter is paused; use the governed resume action",
                    ));
                }
                store::BridgeAdapterState::Activating => {
                    return Err(handler_error("adapter activation is already in progress"));
                }
                store::BridgeAdapterState::Inactive | store::BridgeAdapterState::Failed => record,
            }
        } else {
            self.runtime
                .store
                .bridge_adapters
                .create(store::NewBridgeAdapter {
                    tenant_id: envelope.tenant,
                    adapter_id: adapter_id.clone(),
                    component_ref: component_ref.to_owned(),
                    component_sha256: artifact.sha256.clone(),
                    grant_config: grant_config.clone(),
                    config: serde_json::from_str(&config_json).map_err(handler_error)?,
                })
                .await?
        };

        record = self
            .runtime
            .store
            .bridge_adapters
            .transition(store::BridgeAdapterTransition {
                tenant_id: envelope.tenant,
                adapter_id: adapter_id.clone(),
                expected_revision: record.revision,
                expected_state: record.state,
                new_state: store::BridgeAdapterState::Activating,
                descriptor: None,
                last_error: None,
                event: json!({ "event": "activation_started", "envelope_id": envelope.id }),
            })
            .await?;

        // Probe and conformance are separate host instances so activation
        // validates the complete read-side contract without sharing adapter
        // state across checks.
        let conformance_grant = grant.clone();
        let probe = self
            .runtime
            .lifecycle
            .probe(ProbeRequest {
                tenant_id: envelope.tenant,
                adapter_id: &adapter_id,
                component_ref,
                expected_sha256: Some(artifact.sha256.as_str()),
                grant,
                config_json: &config_json,
            })
            .await;
        let probe = match probe {
            Ok(probe) => probe,
            Err(error) => {
                let message = error.to_string();
                if let Err(persist_error) = self
                    .runtime
                    .store
                    .bridge_adapters
                    .transition(store::BridgeAdapterTransition {
                        tenant_id: envelope.tenant,
                        adapter_id: adapter_id.clone(),
                        expected_revision: record.revision,
                        expected_state: store::BridgeAdapterState::Activating,
                        new_state: store::BridgeAdapterState::Failed,
                        descriptor: None,
                        last_error: Some(message.clone()),
                        event: json!({
                            "event": "activation_failed",
                            "envelope_id": envelope.id,
                            "error": message,
                        }),
                    })
                    .await
                {
                    return Err(ExecutionRegistryError::Handler(format!(
                        "bridge probe failed: {message}; failed to persist failed state: {persist_error}"
                    )));
                }
                return Err(handler_error(error));
            }
        };
        let conformance = self
            .runtime
            .lifecycle
            .conformance(ConformanceRequest {
                tenant_id: envelope.tenant,
                adapter_id: &adapter_id,
                component_ref,
                expected_sha256: Some(artifact.sha256.as_str()),
                grant: conformance_grant,
                config_json: &config_json,
                kind: None,
                limit: 25,
            })
            .await;
        let conformance = match conformance {
            Ok(conformance) => conformance,
            Err(error) => {
                let message = bounded_error(&error);
                if let Err(persist_error) = self
                    .runtime
                    .store
                    .bridge_adapters
                    .transition(store::BridgeAdapterTransition {
                        tenant_id: envelope.tenant,
                        adapter_id: adapter_id.clone(),
                        expected_revision: record.revision,
                        expected_state: store::BridgeAdapterState::Activating,
                        new_state: store::BridgeAdapterState::Failed,
                        descriptor: Some(
                            serde_json::to_value(&probe.descriptor).map_err(handler_error)?,
                        ),
                        last_error: Some(message.clone()),
                        event: json!({
                            "event": "activation_conformance_failed",
                            "envelope_id": envelope.id,
                            "error": message,
                        }),
                    })
                    .await
                {
                    return Err(ExecutionRegistryError::Handler(format!(
                        "bridge conformance failed: {message}; failed to persist failed state: {persist_error}"
                    )));
                }
                return Err(handler_error(error));
            }
        };
        record = self
            .runtime
            .store
            .bridge_adapters
            .transition(store::BridgeAdapterTransition {
                tenant_id: envelope.tenant,
                adapter_id,
                expected_revision: record.revision,
                expected_state: store::BridgeAdapterState::Activating,
                new_state: store::BridgeAdapterState::Active,
                descriptor: Some(serde_json::to_value(&probe.descriptor).map_err(handler_error)?),
                last_error: None,
                event: json!({
                    "event": "activation_succeeded",
                    "envelope_id": envelope.id,
                    "fuel_remaining": probe.fuel_remaining,
                    "conformance": {
                        "checked_kind": conformance.checked_kind,
                        "schema_field_count": conformance.schema_field_count,
                        "listed_record_count": conformance.listed_record_count,
                        "changed_record_count": conformance.changed_record_count,
                        "incremental_checked": conformance.incremental_checked,
                    },
                }),
            })
            .await?;
        Ok(receipt(
            envelope,
            &record,
            store::BridgeAdapterState::Active.as_str(),
            json!({
                "descriptor": probe.descriptor,
                "fuel_remaining": probe.fuel_remaining,
                "conformance": {
                    "checked_kind": conformance.checked_kind,
                    "schema_field_count": conformance.schema_field_count,
                    "listed_record_count": conformance.listed_record_count,
                    "changed_record_count": conformance.changed_record_count,
                    "incremental_checked": conformance.incremental_checked,
                    "report": conformance.report,
                },
            }),
        ))
    }

    async fn verify(
        &self,
        _context: &ExecutionContext,
        envelope: &ActionEnvelope,
        receipt: &HandlerReceipt,
    ) -> Result<bool, ExecutionRegistryError> {
        let adapter_id = adapter_id(&envelope.payload)?;
        let Some(record) = self
            .runtime
            .store
            .bridge_adapters
            .get(envelope.tenant, adapter_id)
            .await?
        else {
            return Ok(false);
        };
        Ok(record.state == store::BridgeAdapterState::Active
            && receipt.affected_targets == vec![record.id]
            && receipt
                .details
                .get("component_sha256")
                .and_then(Value::as_str)
                == Some(record.component_sha256.as_str()))
    }
}

struct PauseAdapterHandler {
    runtime: Arc<BridgeLifecycleRuntime>,
}

#[async_trait]
impl ExecutionHandler for PauseAdapterHandler {
    fn descriptor(&self) -> ExecutionHandlerDescriptor {
        descriptor(
            PAUSE_CAPABILITY,
            "pause_adapter",
            lifecycle_schema(),
            fabric::RiskClass::Low,
            Reversal::Snapshot,
        )
    }

    async fn execute(
        &self,
        _context: &ExecutionContext,
        envelope: &ActionEnvelope,
    ) -> Result<HandlerReceipt, ExecutionRegistryError> {
        let adapter_id = adapter_id(&envelope.payload)?.to_owned();
        let Some(record) = self
            .runtime
            .store
            .bridge_adapters
            .get(envelope.tenant, &adapter_id)
            .await?
        else {
            return Err(store::StoreError::NotFound.into());
        };
        if record.state == store::BridgeAdapterState::Paused {
            return Ok(receipt(
                envelope,
                &record,
                store::BridgeAdapterState::Paused.as_str(),
                json!({ "idempotent": true }),
            ));
        }
        if record.state != store::BridgeAdapterState::Active {
            return Err(handler_error("only an active adapter can be paused"));
        }
        let paused = self
            .runtime
            .store
            .bridge_adapters
            .transition(store::BridgeAdapterTransition {
                tenant_id: envelope.tenant,
                adapter_id,
                expected_revision: record.revision,
                expected_state: store::BridgeAdapterState::Active,
                new_state: store::BridgeAdapterState::Paused,
                descriptor: None,
                last_error: None,
                event: json!({ "event": "paused", "envelope_id": envelope.id }),
            })
            .await?;
        Ok(receipt(
            envelope,
            &paused,
            store::BridgeAdapterState::Paused.as_str(),
            json!({}),
        ))
    }

    async fn verify(
        &self,
        _context: &ExecutionContext,
        envelope: &ActionEnvelope,
        receipt: &HandlerReceipt,
    ) -> Result<bool, ExecutionRegistryError> {
        verify_state(
            &self.runtime,
            envelope,
            receipt,
            store::BridgeAdapterState::Paused,
        )
        .await
    }
}

struct ResumeAdapterHandler {
    runtime: Arc<BridgeLifecycleRuntime>,
}

struct SyncAdapterHandler {
    runtime: Arc<BridgeLifecycleRuntime>,
}

#[async_trait]
impl ExecutionHandler for SyncAdapterHandler {
    fn descriptor(&self) -> ExecutionHandlerDescriptor {
        descriptor(
            SYNC_CAPABILITY,
            "sync_adapter",
            sync_schema(),
            fabric::RiskClass::Moderate,
            Reversal::Compensating,
        )
    }

    async fn execute(
        &self,
        context: &ExecutionContext,
        envelope: &ActionEnvelope,
    ) -> Result<HandlerReceipt, ExecutionRegistryError> {
        let adapter_id = adapter_id(&envelope.payload)?.to_owned();
        let kind = sync_kind(&envelope.payload)?.to_owned();
        let limit = sync_limit(&envelope.payload)?;
        let Some(record) = self
            .runtime
            .store
            .bridge_adapters
            .get(envelope.tenant, &adapter_id)
            .await?
        else {
            return Err(store::StoreError::NotFound.into());
        };
        if record.state != store::BridgeAdapterState::Active {
            return Err(handler_error("only an active adapter can synchronize"));
        }
        let descriptor: BridgeDescriptor = record
            .descriptor
            .clone()
            .ok_or_else(|| handler_error("active adapter has no persisted descriptor"))
            .and_then(|value| {
                serde_json::from_value(value).map_err(|error| {
                    handler_error(format!("invalid persisted descriptor: {error}"))
                })
            })?;
        if !descriptor.capabilities.read
            || !descriptor.kinds.iter().any(|candidate| candidate == &kind)
        {
            return Err(handler_error(
                "adapter does not advertise read capability for this kind",
            ));
        }
        let grant = grant_from_config(&adapter_id, &record.grant_config)?;
        let config_json = serde_json::to_string(&record.config)
            .map_err(|error| handler_error(format!("serialize adapter config: {error}")))?;
        let run = self
            .runtime
            .store
            .bridge_sync
            .start(store::NewBridgeSyncRun {
                tenant_id: envelope.tenant,
                adapter_id: adapter_id.clone(),
                kind: kind.clone(),
                correlation_id: envelope.invocation.correlation_id.clone(),
                causation_id: envelope.invocation.causation_id.clone(),
                envelope_id: Some(envelope.id),
            })
            .await?;
        let provenance = store::EventProvenance::for_bridge_envelope(
            envelope,
            &adapter_id,
            context.trace_context.clone(),
        );
        let (applied, strategy, fuel_remaining, mode_details) =
            if descriptor.capabilities.incremental_sync {
                let page = match self
                    .runtime
                    .lifecycle
                    .sync_page(SyncRequest {
                        tenant_id: envelope.tenant,
                        adapter_id: &adapter_id,
                        component_ref: &record.component_ref,
                        expected_sha256: Some(record.component_sha256.as_str()),
                        grant,
                        config_json: &config_json,
                        cursor: &run.start_cursor,
                        limit,
                    })
                    .await
                {
                    Ok(page) => page,
                    Err(error) => {
                        self.runtime
                            .store
                            .bridge_sync
                            .fail(
                                envelope.tenant,
                                run.id,
                                &error.to_string(),
                                None,
                                &provenance,
                            )
                            .await?;
                        return Err(handler_error(error));
                    }
                };
                let changes = page
                    .changes
                    .changes
                    .into_iter()
                    .map(|change| {
                        let operation = match change.op {
                            bridge_host::bindings::hydra::bridge::types::ChangeOp::Upserted => {
                                store::BridgeSyncOperation::Upsert
                            }
                            bridge_host::bindings::hydra::bridge::types::ChangeOp::Deleted => {
                                store::BridgeSyncOperation::Delete
                            }
                        };
                        let body = serde_json::from_str(&change.rec.data).map_err(|error| {
                            store::BridgeSyncConflict {
                                operation,
                                kind: change.rec.kind.clone(),
                                external_ref: change.rec.id.clone(),
                                conflict_kind: "invalid_json".to_owned(),
                                reason: format!("adapter record JSON is invalid: {error}"),
                            }
                        })?;
                        Ok(store::BridgeSyncChange {
                            operation,
                            kind: change.rec.kind,
                            external_id: change.rec.id,
                            body,
                        })
                    })
                    .collect::<Result<Vec<_>, store::BridgeSyncConflict>>();
                let changes = match changes {
                    Ok(changes) => changes,
                    Err(conflict) => {
                        let reason = conflict.reason.clone();
                        self.runtime
                            .store
                            .bridge_sync
                            .fail(
                                envelope.tenant,
                                run.id,
                                &reason,
                                Some(conflict),
                                &provenance,
                            )
                            .await?;
                        return Err(handler_error("adapter returned invalid JSON record"));
                    }
                };
                let applied = match self
                    .runtime
                    .store
                    .bridge_sync
                    .apply_page(
                        envelope.tenant,
                        run.id,
                        &adapter_id,
                        &kind,
                        &changes,
                        &page.changes.next_cursor,
                        &provenance,
                    )
                    .await
                {
                    Ok(applied) => applied,
                    Err(store::BridgeSyncApplyError::Conflict(conflict)) => {
                        let reason = conflict.reason.clone();
                        self.runtime
                            .store
                            .bridge_sync
                            .fail(
                                envelope.tenant,
                                run.id,
                                &reason,
                                Some(conflict),
                                &provenance,
                            )
                            .await?;
                        return Err(handler_error("bridge sync page parked a conflict"));
                    }
                    Err(store::BridgeSyncApplyError::Store(error)) => {
                        self.runtime
                            .store
                            .bridge_sync
                            .fail(
                                envelope.tenant,
                                run.id,
                                &error.to_string(),
                                None,
                                &provenance,
                            )
                            .await?;
                        return Err(error.into());
                    }
                };
                (applied, "incremental", page.fuel_remaining, json!({}))
            } else {
                let relist = match self
                    .runtime
                    .lifecycle
                    .full_relist(FullRelistRequest {
                        tenant_id: envelope.tenant,
                        adapter_id: &adapter_id,
                        component_ref: &record.component_ref,
                        expected_sha256: Some(record.component_sha256.as_str()),
                        grant,
                        config_json: &config_json,
                        kind: &kind,
                        limit,
                    })
                    .await
                {
                    Ok(relist) => relist,
                    Err(error) => {
                        self.runtime
                            .store
                            .bridge_sync
                            .fail(
                                envelope.tenant,
                                run.id,
                                &error.to_string(),
                                None,
                                &provenance,
                            )
                            .await?;
                        return Err(handler_error(error));
                    }
                };
                let records_seen = relist.records.len();
                let changes = relist
                    .records
                    .into_iter()
                    .map(|record| {
                        let body = serde_json::from_str(&record.data).map_err(|error| {
                            store::BridgeSyncConflict {
                                operation: store::BridgeSyncOperation::Upsert,
                                kind: record.kind.clone(),
                                external_ref: record.id.clone(),
                                conflict_kind: "invalid_json".to_owned(),
                                reason: format!("adapter record JSON is invalid: {error}"),
                            }
                        })?;
                        Ok(store::BridgeSyncChange {
                            operation: store::BridgeSyncOperation::Upsert,
                            kind: record.kind,
                            external_id: record.id,
                            body,
                        })
                    })
                    .collect::<Result<Vec<_>, store::BridgeSyncConflict>>();
                let changes = match changes {
                    Ok(changes) => changes,
                    Err(conflict) => {
                        let reason = conflict.reason.clone();
                        self.runtime
                            .store
                            .bridge_sync
                            .fail(
                                envelope.tenant,
                                run.id,
                                &reason,
                                Some(conflict),
                                &provenance,
                            )
                            .await?;
                        return Err(handler_error("adapter returned invalid JSON record"));
                    }
                };
                let page_count = relist.page_count;
                let total_bytes = relist.total_bytes;
                let fuel_remaining = relist.fuel_remaining;
                let applied = match self
                    .runtime
                    .store
                    .bridge_sync
                    .apply_full_relist(
                        envelope.tenant,
                        run.id,
                        &adapter_id,
                        &kind,
                        &changes,
                        &provenance,
                    )
                    .await
                {
                    Ok(applied) => applied,
                    Err(store::BridgeSyncApplyError::Conflict(conflict)) => {
                        let reason = conflict.reason.clone();
                        self.runtime
                            .store
                            .bridge_sync
                            .fail(
                                envelope.tenant,
                                run.id,
                                &reason,
                                Some(conflict),
                                &provenance,
                            )
                            .await?;
                        return Err(handler_error("full relist parked a conflict"));
                    }
                    Err(store::BridgeSyncApplyError::Store(error)) => {
                        self.runtime
                            .store
                            .bridge_sync
                            .fail(
                                envelope.tenant,
                                run.id,
                                &error.to_string(),
                                None,
                                &provenance,
                            )
                            .await?;
                        return Err(error.into());
                    }
                };
                (
                    applied,
                    "full_relist",
                    fuel_remaining,
                    json!({
                        "page_count": page_count,
                        "records_seen": records_seen,
                        "total_bytes": total_bytes,
                    }),
                )
            };
        Ok(HandlerReceipt {
            envelope_id: envelope.id,
            affected_targets: vec![record.id],
            details: json!({
                "adapter_id": adapter_id,
                "registry_id": record.id,
                "component_sha256": record.component_sha256,
                "run_id": run.id,
                "kind": kind,
                "strategy": strategy,
                "mode_details": mode_details,
                "next_cursor": applied.next_cursor,
                "applied_upserts": applied.applied_upserts,
                "applied_deletes": applied.applied_deletes,
                "fuel_remaining": fuel_remaining,
            }),
        })
    }

    async fn verify(
        &self,
        _context: &ExecutionContext,
        envelope: &ActionEnvelope,
        receipt: &HandlerReceipt,
    ) -> Result<bool, ExecutionRegistryError> {
        let Some(run_id) = receipt
            .details
            .get("run_id")
            .and_then(Value::as_str)
            .and_then(|value| value.parse().ok())
        else {
            return Ok(false);
        };
        let Some(run) = self
            .runtime
            .store
            .bridge_sync
            .get_run(envelope.tenant, run_id)
            .await?
        else {
            return Ok(false);
        };
        Ok(run.status == store::BridgeSyncRunStatus::Succeeded
            && receipt.affected_targets.len() == 1
            && receipt.affected_targets[0]
                == self
                    .runtime
                    .store
                    .bridge_adapters
                    .get(
                        envelope.tenant,
                        receipt
                            .details
                            .get("adapter_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
                    .await?
                    .map(|record| record.id)
                    .unwrap_or_default())
    }
}

#[async_trait]
impl ExecutionHandler for ResumeAdapterHandler {
    fn descriptor(&self) -> ExecutionHandlerDescriptor {
        descriptor(
            RESUME_CAPABILITY,
            "resume_adapter",
            lifecycle_schema(),
            fabric::RiskClass::Low,
            Reversal::Snapshot,
        )
    }

    async fn execute(
        &self,
        _context: &ExecutionContext,
        envelope: &ActionEnvelope,
    ) -> Result<HandlerReceipt, ExecutionRegistryError> {
        let adapter_id = adapter_id(&envelope.payload)?.to_owned();
        let Some(record) = self
            .runtime
            .store
            .bridge_adapters
            .get(envelope.tenant, &adapter_id)
            .await?
        else {
            return Err(store::StoreError::NotFound.into());
        };
        if record.state == store::BridgeAdapterState::Active {
            return Ok(receipt(
                envelope,
                &record,
                store::BridgeAdapterState::Active.as_str(),
                json!({ "idempotent": true }),
            ));
        }
        if record.state != store::BridgeAdapterState::Paused {
            return Err(handler_error("only a paused adapter can be resumed"));
        }
        let grant = grant_from_config(&adapter_id, &record.grant_config)?;
        let config_json = serde_json::to_string(&record.config)
            .map_err(|error| handler_error(format!("serialize adapter config: {error}")))?;
        self.runtime
            .lifecycle
            .probe(ProbeRequest {
                tenant_id: envelope.tenant,
                adapter_id: &adapter_id,
                component_ref: &record.component_ref,
                expected_sha256: Some(record.component_sha256.as_str()),
                grant,
                config_json: &config_json,
            })
            .await
            .map_err(handler_error)?;
        let active = self
            .runtime
            .store
            .bridge_adapters
            .transition(store::BridgeAdapterTransition {
                tenant_id: envelope.tenant,
                adapter_id,
                expected_revision: record.revision,
                expected_state: store::BridgeAdapterState::Paused,
                new_state: store::BridgeAdapterState::Active,
                descriptor: None,
                last_error: None,
                event: json!({ "event": "resumed", "envelope_id": envelope.id }),
            })
            .await?;
        Ok(receipt(
            envelope,
            &active,
            store::BridgeAdapterState::Active.as_str(),
            json!({}),
        ))
    }

    async fn verify(
        &self,
        _context: &ExecutionContext,
        envelope: &ActionEnvelope,
        receipt: &HandlerReceipt,
    ) -> Result<bool, ExecutionRegistryError> {
        verify_state(
            &self.runtime,
            envelope,
            receipt,
            store::BridgeAdapterState::Active,
        )
        .await
    }
}

async fn verify_state(
    runtime: &BridgeLifecycleRuntime,
    envelope: &ActionEnvelope,
    receipt: &HandlerReceipt,
    expected: store::BridgeAdapterState,
) -> Result<bool, ExecutionRegistryError> {
    let adapter_id = adapter_id(&envelope.payload)?;
    let Some(record) = runtime
        .store
        .bridge_adapters
        .get(envelope.tenant, adapter_id)
        .await?
    else {
        return Ok(false);
    };
    Ok(record.state == expected && receipt.affected_targets == vec![record.id])
}
