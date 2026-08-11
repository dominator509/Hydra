use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use governor::{ActionEnvelope, EnvelopeState, Reversal};
use jsonschema::{Draft, JSONSchema};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionHandlerDescriptor {
    pub capability_name: String,
    pub domain: String,
    pub action: String,
    pub kind: Option<String>,
    pub payload_schema: Value,
    pub required_target_types: Vec<String>,
    pub risk_class: fabric::RiskClass,
    pub reversal: Reversal,
    pub supports_compensation: bool,
}

impl ExecutionHandlerDescriptor {
    pub fn runtime_capability(&self) -> String {
        format!(
            "execution-handler:{}/{}/{}",
            self.domain,
            self.action,
            self.kind.as_deref().unwrap_or("*")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerReceipt {
    pub envelope_id: Uuid,
    pub affected_targets: Vec<Uuid>,
    pub details: Value,
}

#[derive(Clone)]
pub struct ExecutionContext {
    pub entities: store::EntitiesRepo,
    pub trace_context: Option<store::TraceContext>,
}

#[async_trait]
pub trait ExecutionHandler: Send + Sync {
    fn descriptor(&self) -> ExecutionHandlerDescriptor;

    async fn execute(
        &self,
        context: &ExecutionContext,
        envelope: &ActionEnvelope,
    ) -> Result<HandlerReceipt, ExecutionRegistryError>;

    async fn verify(
        &self,
        context: &ExecutionContext,
        envelope: &ActionEnvelope,
        receipt: &HandlerReceipt,
    ) -> Result<bool, ExecutionRegistryError>;

    async fn compensate(
        &self,
        _context: &ExecutionContext,
        _envelope: &ActionEnvelope,
        _receipt: &HandlerReceipt,
    ) -> Result<(), ExecutionRegistryError> {
        Err(ExecutionRegistryError::CompensationUnavailable)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionRegistryError {
    #[error("duplicate execution handler for {0}")]
    DuplicateHandler(String),
    #[error("invalid execution handler descriptor: {0}")]
    InvalidDescriptor(String),
    #[error("unsupported approved envelope {0}")]
    UnsupportedEnvelope(String),
    #[error("executor only accepts approved envelopes")]
    EnvelopeNotApproved,
    #[error("invalid handler payload: {0}")]
    InvalidPayload(String),
    #[error("invalid target type for entity {entity_id}: {actual}")]
    InvalidTargetType { entity_id: Uuid, actual: String },
    #[error("envelope reversal does not match handler contract")]
    ReversalMismatch,
    #[error("handler verification failed")]
    VerificationFailed,
    #[error("handler execution failed: {0}")]
    Handler(String),
    #[error("handler compensation is unavailable")]
    CompensationUnavailable,
    #[error(transparent)]
    Store(#[from] store::StoreError),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct HandlerKey {
    domain: String,
    action: String,
    kind: Option<String>,
}

impl HandlerKey {
    fn from_descriptor(descriptor: &ExecutionHandlerDescriptor) -> Self {
        Self {
            domain: descriptor.domain.clone(),
            action: descriptor.action.clone(),
            kind: descriptor.kind.clone(),
        }
    }

    fn from_envelope(envelope: &ActionEnvelope) -> Self {
        Self {
            domain: envelope.domain.clone(),
            action: envelope.action.clone(),
            kind: envelope.kind.clone(),
        }
    }

    fn display(&self) -> String {
        format!(
            "{}/{}/{}",
            self.domain,
            self.action,
            self.kind.as_deref().unwrap_or("*")
        )
    }
}

struct RegisteredHandler {
    descriptor: ExecutionHandlerDescriptor,
    schema: JSONSchema,
    handler: Arc<dyn ExecutionHandler>,
}

#[derive(Clone)]
pub struct ExecutionRegistry {
    handlers: Arc<BTreeMap<HandlerKey, RegisteredHandler>>,
}

impl ExecutionRegistry {
    pub fn new(
        handlers: impl IntoIterator<Item = Arc<dyn ExecutionHandler>>,
    ) -> Result<Self, ExecutionRegistryError> {
        let mut registered = BTreeMap::new();
        let mut capability_names = BTreeSet::new();
        for handler in handlers {
            let descriptor = handler.descriptor();
            validate_descriptor(&descriptor)?;
            let key = HandlerKey::from_descriptor(&descriptor);
            if registered.contains_key(&key)
                || !capability_names.insert(descriptor.capability_name.clone())
            {
                return Err(ExecutionRegistryError::DuplicateHandler(key.display()));
            }
            let schema = JSONSchema::options()
                .with_draft(Draft::Draft7)
                .compile(&descriptor.payload_schema)
                .map_err(|error| {
                    ExecutionRegistryError::InvalidDescriptor(format!(
                        "{} payload schema: {error}",
                        key.display()
                    ))
                })?;
            registered.insert(
                key,
                RegisteredHandler {
                    descriptor,
                    schema,
                    handler,
                },
            );
        }
        Ok(Self {
            handlers: Arc::new(registered),
        })
    }

    pub fn runtime_capabilities(&self) -> BTreeSet<String> {
        self.handlers
            .values()
            .map(|registered| registered.descriptor.runtime_capability())
            .collect()
    }

    pub fn supports(&self, envelope: &ActionEnvelope) -> bool {
        self.handlers
            .contains_key(&HandlerKey::from_envelope(envelope))
    }

    pub fn descriptor_for(
        &self,
        envelope: &ActionEnvelope,
    ) -> Result<&ExecutionHandlerDescriptor, ExecutionRegistryError> {
        let key = HandlerKey::from_envelope(envelope);
        self.handlers
            .get(&key)
            .map(|registered| &registered.descriptor)
            .ok_or_else(|| ExecutionRegistryError::UnsupportedEnvelope(key.display()))
    }

    pub async fn execute_and_verify(
        &self,
        context: &ExecutionContext,
        envelope: &ActionEnvelope,
    ) -> Result<HandlerReceipt, ExecutionRegistryError> {
        if !matches!(
            envelope.state,
            EnvelopeState::Approved | EnvelopeState::Executing
        ) {
            return Err(ExecutionRegistryError::EnvelopeNotApproved);
        }
        let registered = self.validate_contract(context, envelope).await?;

        let receipt = registered.handler.execute(context, envelope).await?;
        if receipt.envelope_id != envelope.id
            || !registered
                .handler
                .verify(context, envelope, &receipt)
                .await?
        {
            return Err(ExecutionRegistryError::VerificationFailed);
        }
        Ok(receipt)
    }

    pub async fn validate(
        &self,
        context: &ExecutionContext,
        envelope: &ActionEnvelope,
    ) -> Result<(), ExecutionRegistryError> {
        if envelope.state != EnvelopeState::Approved {
            return Err(ExecutionRegistryError::EnvelopeNotApproved);
        }
        let _ = self.validate_contract(context, envelope).await?;
        Ok(())
    }

    async fn validate_contract<'a>(
        &'a self,
        context: &ExecutionContext,
        envelope: &ActionEnvelope,
    ) -> Result<&'a RegisteredHandler, ExecutionRegistryError> {
        let key = HandlerKey::from_envelope(envelope);
        let registered = self
            .handlers
            .get(&key)
            .ok_or_else(|| ExecutionRegistryError::UnsupportedEnvelope(key.display()))?;
        if registered.descriptor.reversal != envelope.reversal {
            return Err(ExecutionRegistryError::ReversalMismatch);
        }
        if let Err(errors) = registered.schema.validate(&envelope.payload) {
            let detail = errors
                .take(3)
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(ExecutionRegistryError::InvalidPayload(detail));
        }
        if envelope.targets.is_empty() {
            return Err(ExecutionRegistryError::InvalidPayload(
                "at least one target is required".to_owned(),
            ));
        }
        for target in &envelope.targets {
            let entity = context.entities.get(envelope.tenant, *target).await?;
            if !registered.descriptor.required_target_types.is_empty()
                && !registered
                    .descriptor
                    .required_target_types
                    .contains(&entity.kind)
            {
                return Err(ExecutionRegistryError::InvalidTargetType {
                    entity_id: entity.id,
                    actual: entity.kind,
                });
            }
        }

        Ok(registered)
    }
}

fn validate_descriptor(
    descriptor: &ExecutionHandlerDescriptor,
) -> Result<(), ExecutionRegistryError> {
    if descriptor.capability_name.trim().is_empty()
        || descriptor.domain.trim().is_empty()
        || descriptor.action.trim().is_empty()
        || descriptor.payload_schema.as_object().is_none()
        || descriptor
            .kind
            .as_ref()
            .is_some_and(|kind| kind.trim().is_empty())
        || descriptor
            .required_target_types
            .iter()
            .any(|kind| kind.trim().is_empty())
    {
        return Err(ExecutionRegistryError::InvalidDescriptor(
            descriptor.capability_name.clone(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FakeHandler;

    #[async_trait]
    impl ExecutionHandler for FakeHandler {
        fn descriptor(&self) -> ExecutionHandlerDescriptor {
            ExecutionHandlerDescriptor {
                capability_name: "hydra.crm.propose_action".to_owned(),
                domain: "pipeline".to_owned(),
                action: "move_stage".to_owned(),
                kind: Some("deal".to_owned()),
                payload_schema: json!({
                    "type": "object",
                    "required": ["stage"],
                    "properties": { "stage": { "type": "string" } },
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
            _context: &ExecutionContext,
            envelope: &ActionEnvelope,
        ) -> Result<HandlerReceipt, ExecutionRegistryError> {
            Ok(HandlerReceipt {
                envelope_id: envelope.id,
                affected_targets: envelope.targets.clone(),
                details: Value::Null,
            })
        }

        async fn verify(
            &self,
            _context: &ExecutionContext,
            _envelope: &ActionEnvelope,
            _receipt: &HandlerReceipt,
        ) -> Result<bool, ExecutionRegistryError> {
            Ok(true)
        }
    }

    #[test]
    fn execution_registry_rejects_duplicate_handlers_at_boot() {
        let handlers: Vec<Arc<dyn ExecutionHandler>> =
            vec![Arc::new(FakeHandler), Arc::new(FakeHandler)];
        let result = ExecutionRegistry::new(handlers);
        assert!(matches!(
            result,
            Err(ExecutionRegistryError::DuplicateHandler(_))
        ));
    }

    #[test]
    fn execution_registry_exports_capability_availability_key() {
        let handlers: Vec<Arc<dyn ExecutionHandler>> = vec![Arc::new(FakeHandler)];
        let registry = ExecutionRegistry::new(handlers).expect("handler is valid");
        assert_eq!(
            registry.runtime_capabilities(),
            BTreeSet::from(["execution-handler:pipeline/move_stage/deal".to_owned()])
        );
    }
}
