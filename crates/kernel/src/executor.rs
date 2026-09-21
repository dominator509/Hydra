use std::sync::Arc;

use async_trait::async_trait;
use cdm::Entity;
use governor::{Clock, EnvelopeState, ExecuteToken, Reversal};
use serde_json::{json, Value};

use crate::execution_registry::{
    ExecutionContext, ExecutionHandler, ExecutionHandlerDescriptor, ExecutionRegistry,
    ExecutionRegistryError, HandlerReceipt,
};

#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    #[error(transparent)]
    Store(#[from] store::StoreError),
    #[error(transparent)]
    Registry(#[from] ExecutionRegistryError),
    #[error("executor only runs approved envelopes")]
    EnvelopeNotApproved,
    #[error("required immutable approval assertion is missing")]
    MissingApproval,
    #[error("immutable approval assertion does not match the envelope")]
    InvalidApproval,
}

#[derive(Clone)]
pub struct Executor {
    store: store::Store,
    registry: ExecutionRegistry,
    context: ExecutionContext,
}

impl Executor {
    pub fn new(store: store::Store) -> Self {
        let handlers: Vec<Arc<dyn ExecutionHandler>> = vec![Arc::new(PipelineMoveStageHandler)];
        let registry = ExecutionRegistry::new(handlers)
            .unwrap_or_else(|error| panic!("built-in execution registry is invalid: {error}"));
        Self::with_registry(store, registry)
    }

    pub fn with_registry(store: store::Store, registry: ExecutionRegistry) -> Self {
        let context = ExecutionContext {
            entities: store.entities.clone(),
            trace_context: None,
        };
        Self {
            store,
            registry,
            context,
        }
    }

    pub fn registry(&self) -> &ExecutionRegistry {
        &self.registry
    }

    pub async fn execute(
        &self,
        token: ExecuteToken,
        clock: &dyn Clock,
    ) -> Result<governor::ActionEnvelope, ExecuteError> {
        self.execute_by_identity(token.tenant(), token.envelope_id(), clock)
            .await
    }

    pub(crate) async fn execute_recovered(
        &self,
        tenant: uuid::Uuid,
        envelope_id: uuid::Uuid,
        clock: &dyn Clock,
    ) -> Result<governor::ActionEnvelope, ExecuteError> {
        self.execute_by_identity(tenant, envelope_id, clock).await
    }

    async fn execute_by_identity(
        &self,
        tenant: uuid::Uuid,
        envelope_id: uuid::Uuid,
        clock: &dyn Clock,
    ) -> Result<governor::ActionEnvelope, ExecuteError> {
        let mut envelope = self.store.envelopes.get(tenant, envelope_id).await?;
        let trace_context = self
            .store
            .envelopes
            .trace_context(tenant, envelope_id)
            .await?
            .map(|context| context.child());
        let execution_context = ExecutionContext {
            entities: self.context.entities.clone(),
            trace_context: trace_context.clone(),
        };
        if envelope.state != EnvelopeState::Approved {
            return Err(ExecuteError::EnvelopeNotApproved);
        }
        if envelope
            .history
            .iter()
            .any(|transition| transition.to == EnvelopeState::PendingApproval)
        {
            let approval_id = envelope
                .invocation
                .approval_id
                .as_deref()
                .ok_or(ExecuteError::MissingApproval)
                .and_then(|value| {
                    uuid::Uuid::parse_str(value).map_err(|_| ExecuteError::InvalidApproval)
                })?;
            let approval = self.store.approvals.get(tenant, approval_id).await?;
            if approval.envelope_id != envelope_id
                || approval.decision != store::ApprovalDecision::Approved
            {
                return Err(ExecuteError::InvalidApproval);
            }
        }
        self.registry
            .validate(&execution_context, &envelope)
            .await?;
        let descriptor = self.registry.descriptor_for(&envelope)?.clone();

        envelope = self
            .store
            .envelopes
            .transition_with_trace(
                tenant,
                envelope_id,
                EnvelopeState::Executing,
                "executor",
                clock,
                trace_context.as_ref(),
            )
            .await?;

        let handler_receipt = match self
            .registry
            .execute_and_verify(&execution_context, &envelope)
            .await
        {
            Ok(receipt) => receipt,
            Err(error) => {
                self.store
                    .envelopes
                    .finish_execution_with_trace(
                        tenant,
                        envelope_id,
                        EnvelopeState::Failed,
                        "executor",
                        clock,
                        store::NewExecutionReceipt {
                            id: uuid::Uuid::new_v4(),
                            tenant_id: tenant,
                            envelope_id,
                            capability: descriptor.capability_name.clone(),
                            handler: descriptor.runtime_capability(),
                            outcome: store::ExecutionOutcome::Failed,
                            affected_targets: envelope.targets.clone(),
                            details: json!({ "error": error.to_string() }),
                            invocation: envelope.invocation.clone(),
                        },
                        trace_context
                            .as_ref()
                            .map(store::TraceContext::child)
                            .as_ref(),
                    )
                    .await?;
                return Err(error.into());
            }
        };

        let handler_name = descriptor.runtime_capability();
        let (executed, _) = self
            .store
            .envelopes
            .finish_execution_with_trace(
                tenant,
                envelope_id,
                EnvelopeState::Executed,
                "executor",
                clock,
                store::NewExecutionReceipt {
                    id: uuid::Uuid::new_v4(),
                    tenant_id: tenant,
                    envelope_id,
                    capability: descriptor.capability_name,
                    handler: handler_name,
                    outcome: store::ExecutionOutcome::Verified,
                    affected_targets: handler_receipt.affected_targets,
                    details: handler_receipt.details,
                    invocation: envelope.invocation.clone(),
                },
                trace_context
                    .as_ref()
                    .map(store::TraceContext::child)
                    .as_ref(),
            )
            .await?;
        Ok(executed)
    }
}

pub struct PipelineMoveStageHandler;

#[async_trait]
impl ExecutionHandler for PipelineMoveStageHandler {
    fn descriptor(&self) -> ExecutionHandlerDescriptor {
        ExecutionHandlerDescriptor {
            capability_name: "hydra.crm.propose_action".to_owned(),
            domain: "pipeline".to_owned(),
            action: "move_stage".to_owned(),
            kind: Some("deal".to_owned()),
            payload_schema: json!({
                "type": "object",
                "required": ["stage"],
                "properties": {
                    "stage": { "type": "string", "minLength": 1, "maxLength": 200 }
                },
                "additionalProperties": false
            }),
            required_target_types: vec!["deal".to_owned()],
            risk_class: fabric::RiskClass::Moderate,
            reversal: Reversal::Compensating,
            supports_compensation: true,
        }
    }

    async fn execute(
        &self,
        context: &ExecutionContext,
        envelope: &governor::ActionEnvelope,
    ) -> Result<HandlerReceipt, ExecutionRegistryError> {
        let stage = envelope
            .payload
            .get("stage")
            .and_then(Value::as_str)
            .ok_or_else(|| ExecutionRegistryError::InvalidPayload("missing stage".to_owned()))?;
        let mut previous = Vec::with_capacity(envelope.targets.len());
        let provenance = store::EventProvenance::for_envelope_transition(envelope, "executor")
            .with_trace_context(
                context
                    .trace_context
                    .as_ref()
                    .map(store::TraceContext::child),
            );
        for target in &envelope.targets {
            let entity = context.entities.get(envelope.tenant, *target).await?;
            previous.push(json!({
                "entity_id": entity.id,
                "stage_id": entity.body.get("stage_id").cloned().unwrap_or(Value::Null)
            }));
            let mut body = entity.body.clone();
            body["stage_id"] = json!(stage);
            context
                .entities
                .upsert_with_provenance(
                    envelope.tenant,
                    Entity {
                        id: entity.id,
                        kind: entity.kind,
                        tenant: entity.tenant,
                        body,
                        origin: entity.origin,
                        origin_ref: entity.origin_ref,
                        version: entity.version + 1,
                    },
                    provenance.clone(),
                )
                .await?;
        }
        Ok(HandlerReceipt {
            envelope_id: envelope.id,
            affected_targets: envelope.targets.clone(),
            details: json!({ "previous": previous, "stage": stage }),
        })
    }

    async fn verify(
        &self,
        context: &ExecutionContext,
        envelope: &governor::ActionEnvelope,
        receipt: &HandlerReceipt,
    ) -> Result<bool, ExecutionRegistryError> {
        let Some(expected_stage) = receipt.details.get("stage").and_then(Value::as_str) else {
            return Ok(false);
        };
        for target in &envelope.targets {
            let entity = context.entities.get(envelope.tenant, *target).await?;
            if entity.body.get("stage_id").and_then(Value::as_str) != Some(expected_stage) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn compensate(
        &self,
        context: &ExecutionContext,
        envelope: &governor::ActionEnvelope,
        receipt: &HandlerReceipt,
    ) -> Result<(), ExecutionRegistryError> {
        let previous = receipt
            .details
            .get("previous")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                ExecutionRegistryError::Handler("missing compensation snapshot".to_owned())
            })?;
        for snapshot in previous {
            let entity_id = snapshot
                .get("entity_id")
                .and_then(Value::as_str)
                .and_then(|value| uuid::Uuid::parse_str(value).ok())
                .ok_or_else(|| {
                    ExecutionRegistryError::Handler(
                        "invalid compensation entity identifier".to_owned(),
                    )
                })?;
            let entity = context.entities.get(envelope.tenant, entity_id).await?;
            let mut body = entity.body.clone();
            body["stage_id"] = snapshot.get("stage_id").cloned().unwrap_or(Value::Null);
            context
                .entities
                .upsert_with_provenance(
                    envelope.tenant,
                    Entity {
                        id: entity.id,
                        kind: entity.kind,
                        tenant: entity.tenant,
                        body,
                        origin: entity.origin,
                        origin_ref: entity.origin_ref,
                        version: entity.version + 1,
                    },
                    store::EventProvenance::for_envelope_transition(envelope, "executor")
                        .with_trace_context(
                            context
                                .trace_context
                                .as_ref()
                                .map(store::TraceContext::child),
                        ),
                )
                .await?;
        }
        Ok(())
    }
}
