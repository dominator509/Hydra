use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

pub const HYDRA_EVENT_SPEC_VERSION: &str = "1.0";
pub const HYDRA_EVENT_SCHEMA_VERSION: &str = "1.0";
pub const HYDRA_EVENT_SOURCE: &str = "urn:hydra:crm";
const EVENT_TEXT_PATTERN: &str = r"^[^\u0000-\u001F\u007F]+$";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HydraEventType {
    #[serde(rename = "hydra.crm.entity.created.v1")]
    EntityCreated,
    #[serde(rename = "hydra.crm.entity.updated.v1")]
    EntityUpdated,
    #[serde(rename = "hydra.crm.entity.deleted.v1")]
    EntityDeleted,
    #[serde(rename = "hydra.crm.envelope.proposed.v1")]
    EnvelopeProposed,
    #[serde(rename = "hydra.crm.envelope.queued.v1")]
    EnvelopeQueued,
    #[serde(rename = "hydra.crm.envelope.approved.v1")]
    EnvelopeApproved,
    #[serde(rename = "hydra.crm.envelope.executed.v1")]
    EnvelopeExecuted,
    #[serde(rename = "hydra.crm.envelope.failed.v1")]
    EnvelopeFailed,
    #[serde(rename = "hydra.crm.bridge.health_changed.v1")]
    BridgeHealthChanged,
    #[serde(rename = "hydra.crm.sync.conflict.v1")]
    SyncConflict,
    #[serde(rename = "hydra.crm.autonomy.freeze_changed.v1")]
    AutonomyFreezeChanged,
}

pub const HYDRA_EVENT_TYPES_V1: [HydraEventType; 11] = [
    HydraEventType::EntityCreated,
    HydraEventType::EntityUpdated,
    HydraEventType::EntityDeleted,
    HydraEventType::EnvelopeProposed,
    HydraEventType::EnvelopeQueued,
    HydraEventType::EnvelopeApproved,
    HydraEventType::EnvelopeExecuted,
    HydraEventType::EnvelopeFailed,
    HydraEventType::BridgeHealthChanged,
    HydraEventType::SyncConflict,
    HydraEventType::AutonomyFreezeChanged,
];

impl HydraEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EntityCreated => "hydra.crm.entity.created.v1",
            Self::EntityUpdated => "hydra.crm.entity.updated.v1",
            Self::EntityDeleted => "hydra.crm.entity.deleted.v1",
            Self::EnvelopeProposed => "hydra.crm.envelope.proposed.v1",
            Self::EnvelopeQueued => "hydra.crm.envelope.queued.v1",
            Self::EnvelopeApproved => "hydra.crm.envelope.approved.v1",
            Self::EnvelopeExecuted => "hydra.crm.envelope.executed.v1",
            Self::EnvelopeFailed => "hydra.crm.envelope.failed.v1",
            Self::BridgeHealthChanged => "hydra.crm.bridge.health_changed.v1",
            Self::SyncConflict => "hydra.crm.sync.conflict.v1",
            Self::AutonomyFreezeChanged => "hydra.crm.autonomy.freeze_changed.v1",
        }
    }

    pub const fn is_entity(self) -> bool {
        matches!(
            self,
            Self::EntityCreated | Self::EntityUpdated | Self::EntityDeleted
        )
    }

    pub const fn is_envelope(self) -> bool {
        matches!(
            self,
            Self::EnvelopeProposed
                | Self::EnvelopeQueued
                | Self::EnvelopeApproved
                | Self::EnvelopeExecuted
                | Self::EnvelopeFailed
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventActorType {
    Human,
    NexusService,
    NexusAgent,
    HydraInternalAgent,
    LocalHydraUser,
    HydraSystem,
    Bridge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventActorRef {
    pub actor_id: String,
    pub actor_type: EventActorType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventDataClass {
    Public,
    Internal,
    Private,
    Restricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEntityRef {
    pub entity_id: Uuid,
    pub kind: String,
    pub origin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "payload_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HydraEventPayload {
    EntityChange {
        operation: String,
        version: u64,
    },
    EnvelopeTransition {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_state: Option<String>,
        to_state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capability: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outcome: Option<String>,
    },
    BridgeHealth {
        bridge_id: String,
        status: String,
    },
    SyncConflict {
        conflict_kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bridge_id: Option<String>,
    },
    AutonomyFreeze {
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HydraEventEnvelope {
    pub event_id: Uuid,
    pub spec_version: String,
    pub event_type: HydraEventType,
    pub schema_version: String,
    pub source: String,
    pub subject: String,
    pub occurred_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
    pub hydra_tenant_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_binding_id: Option<Uuid>,
    pub actor: EventActorRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<EventEntityRef>,
    pub data_class: EventDataClass,
    pub payload: HydraEventPayload,
}

impl HydraEventEnvelope {
    pub fn new(
        event_id: Uuid,
        event_type: HydraEventType,
        occurred_at: impl Into<String>,
        hydra_tenant_id: Uuid,
        actor: EventActorRef,
        data_class: EventDataClass,
        payload: HydraEventPayload,
    ) -> Self {
        Self {
            event_id,
            spec_version: HYDRA_EVENT_SPEC_VERSION.to_owned(),
            event_type,
            schema_version: HYDRA_EVENT_SCHEMA_VERSION.to_owned(),
            source: HYDRA_EVENT_SOURCE.to_owned(),
            subject: event_type.as_str().to_owned(),
            occurred_at: occurred_at.into(),
            observed_at: None,
            hydra_tenant_id,
            external_binding_id: None,
            actor,
            correlation_id: None,
            causation_id: None,
            envelope_id: None,
            entity: None,
            data_class,
            payload,
        }
    }

    pub fn validate(&self) -> Result<(), EventContractError> {
        if self.event_id.is_nil() {
            return Err(EventContractError::InvalidField("event_id"));
        }
        if self.hydra_tenant_id.is_nil() {
            return Err(EventContractError::InvalidField("hydra_tenant_id"));
        }
        if self.spec_version != HYDRA_EVENT_SPEC_VERSION {
            return Err(EventContractError::InvalidField("spec_version"));
        }
        if self.schema_version != HYDRA_EVENT_SCHEMA_VERSION {
            return Err(EventContractError::InvalidField("schema_version"));
        }
        if self.source != HYDRA_EVENT_SOURCE {
            return Err(EventContractError::InvalidField("source"));
        }
        if self.subject != self.event_type.as_str() {
            return Err(EventContractError::InvalidField("subject"));
        }
        validate_timestamp("occurred_at", &self.occurred_at)?;
        validate_text("actor.actor_id", &self.actor.actor_id)?;
        validate_optional_timestamp("observed_at", self.observed_at.as_deref())?;
        validate_optional_text("correlation_id", self.correlation_id.as_deref())?;
        validate_optional_text("causation_id", self.causation_id.as_deref())?;
        if self.external_binding_id.is_some_and(|id| id.is_nil()) {
            return Err(EventContractError::InvalidField("external_binding_id"));
        }

        if self.event_type.is_entity() && self.entity.is_none() {
            return Err(EventContractError::MissingReference("entity"));
        }
        if let Some(entity) = &self.entity {
            validate_entity(entity)?;
        }
        if self.event_type.is_envelope()
            && self
                .envelope_id
                .is_none_or(|envelope_id| envelope_id.is_nil())
        {
            return Err(EventContractError::MissingReference("envelope_id"));
        }
        if !payload_matches_type(self.event_type, &self.payload) {
            return Err(EventContractError::TypePayloadMismatch);
        }
        validate_payload(&self.payload)?;

        let serialized = serde_json::to_value(self)
            .map_err(|error| EventContractError::Serialization(error.to_string()))?;
        if let Some(key) = forbidden_key(&serialized) {
            return Err(EventContractError::ForbiddenMetadata(key));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EventContractError {
    #[error("invalid canonical event field '{0}'")]
    InvalidField(&'static str),
    #[error("canonical event is missing required reference '{0}'")]
    MissingReference(&'static str),
    #[error("canonical event type does not match its typed payload")]
    TypePayloadMismatch,
    #[error("canonical event contains forbidden metadata key '{0}'")]
    ForbiddenMetadata(String),
    #[error("canonical event serialization failed: {0}")]
    Serialization(String),
}

fn payload_matches_type(event_type: HydraEventType, payload: &HydraEventPayload) -> bool {
    match (event_type, payload) {
        (HydraEventType::EntityCreated, HydraEventPayload::EntityChange { operation, .. }) => {
            operation == "created"
        }
        (HydraEventType::EntityUpdated, HydraEventPayload::EntityChange { operation, .. }) => {
            operation == "updated"
        }
        (HydraEventType::EntityDeleted, HydraEventPayload::EntityChange { operation, .. }) => {
            operation == "deleted"
        }
        (event_type, HydraEventPayload::EnvelopeTransition { .. }) if event_type.is_envelope() => {
            true
        }
        (HydraEventType::BridgeHealthChanged, HydraEventPayload::BridgeHealth { .. }) => true,
        (HydraEventType::SyncConflict, HydraEventPayload::SyncConflict { .. }) => true,
        (
            HydraEventType::AutonomyFreezeChanged,
            HydraEventPayload::AutonomyFreeze { status, .. },
        ) => matches!(status.as_str(), "active" | "frozen"),
        _ => false,
    }
}

fn validate_entity(entity: &EventEntityRef) -> Result<(), EventContractError> {
    if entity.entity_id.is_nil() {
        return Err(EventContractError::InvalidField("entity.entity_id"));
    }
    validate_text("entity.kind", &entity.kind)?;
    validate_text("entity.origin", &entity.origin)?;
    validate_optional_text("entity.origin_ref", entity.origin_ref.as_deref())?;
    Ok(())
}

fn validate_payload(payload: &HydraEventPayload) -> Result<(), EventContractError> {
    match payload {
        HydraEventPayload::EntityChange { operation, version } => {
            validate_text("payload.operation", operation)?;
            if *version == 0 {
                return Err(EventContractError::InvalidField("payload.version"));
            }
        }
        HydraEventPayload::EnvelopeTransition {
            from_state,
            to_state,
            capability,
            outcome,
        } => {
            validate_optional_text("payload.from_state", from_state.as_deref())?;
            validate_text("payload.to_state", to_state)?;
            validate_optional_text("payload.capability", capability.as_deref())?;
            validate_optional_text("payload.outcome", outcome.as_deref())?;
        }
        HydraEventPayload::BridgeHealth { bridge_id, status } => {
            validate_text("payload.bridge_id", bridge_id)?;
            validate_text("payload.status", status)?;
        }
        HydraEventPayload::SyncConflict {
            conflict_kind,
            bridge_id,
        } => {
            validate_text("payload.conflict_kind", conflict_kind)?;
            validate_optional_text("payload.bridge_id", bridge_id.as_deref())?;
        }
        HydraEventPayload::AutonomyFreeze { status, reason } => {
            validate_text("payload.status", status)?;
            validate_optional_text_with_max("payload.reason", reason.as_deref(), 500)?;
        }
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), EventContractError> {
    validate_text_with_max(field, value, 512)
}

fn validate_timestamp(field: &'static str, value: &str) -> Result<(), EventContractError> {
    validate_text(field, value)?;
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|_| ())
        .map_err(|_| EventContractError::InvalidField(field))
}

fn validate_text_with_max(
    field: &'static str,
    value: &str,
    max_length: usize,
) -> Result<(), EventContractError> {
    if value.trim().is_empty()
        || value.chars().count() > max_length
        || value.chars().any(char::is_control)
    {
        return Err(EventContractError::InvalidField(field));
    }
    Ok(())
}

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), EventContractError> {
    if let Some(value) = value {
        validate_text(field, value)?;
    }
    Ok(())
}

fn validate_optional_text_with_max(
    field: &'static str,
    value: Option<&str>,
    max_length: usize,
) -> Result<(), EventContractError> {
    if let Some(value) = value {
        validate_text_with_max(field, value, max_length)?;
    }
    Ok(())
}

fn validate_optional_timestamp(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), EventContractError> {
    if let Some(value) = value {
        validate_timestamp(field, value)?;
    }
    Ok(())
}

fn forbidden_key(value: &Value) -> Option<String> {
    const FORBIDDEN: [&str; 8] = [
        "accesstoken",
        "refreshtoken",
        "authorization",
        "clientsecret",
        "password",
        "prompt",
        "rawemailbody",
        "bearertoken",
    ];

    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                let normalized = key
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect::<String>();
                if FORBIDDEN.contains(&normalized.as_str()) {
                    return Some(key.clone());
                }
                if let Some(found) = forbidden_key(nested) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(values) => values.iter().find_map(forbidden_key),
        _ => None,
    }
}

fn text_schema(max_length: usize) -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": max_length,
        "pattern": EVENT_TEXT_PATTERN
    })
}

fn date_time_schema() -> Value {
    let mut schema = text_schema(512);
    schema["format"] = Value::String("date-time".to_owned());
    schema
}

pub fn hydra_event_v1_schema() -> Value {
    let event_types = HYDRA_EVENT_TYPES_V1
        .iter()
        .map(|event_type| event_type.as_str())
        .collect::<Vec<_>>();
    let uuid = json!({ "type": "string", "format": "uuid" });
    let date_time = date_time_schema();
    let text = text_schema(512);
    let reason = text_schema(500);
    let actor = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["actor_id", "actor_type"],
        "properties": {
            "actor_id": text.clone(),
            "actor_type": {
                "enum": [
                    "human", "nexus_service", "nexus_agent", "hydra_internal_agent",
                    "local_hydra_user", "hydra_system", "bridge"
                ]
            }
        }
    });
    let entity = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["entity_id", "kind", "origin"],
        "properties": {
            "entity_id": uuid.clone(),
            "kind": text.clone(),
            "origin": text.clone(),
            "origin_ref": text.clone()
        }
    });
    let entity_change = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["payload_type", "operation", "version"],
        "properties": {
            "payload_type": { "const": "entity_change" },
            "operation": { "enum": ["created", "updated", "deleted"] },
            "version": { "type": "integer", "minimum": 1 }
        }
    });
    let envelope_transition = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["payload_type", "to_state"],
        "properties": {
            "payload_type": { "const": "envelope_transition" },
            "from_state": text.clone(),
            "to_state": text.clone(),
            "capability": text.clone(),
            "outcome": text.clone()
        }
    });
    let bridge_health = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["payload_type", "bridge_id", "status"],
        "properties": {
            "payload_type": { "const": "bridge_health" },
            "bridge_id": text.clone(),
            "status": text.clone()
        }
    });
    let sync_conflict = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["payload_type", "conflict_kind"],
        "properties": {
            "payload_type": { "const": "sync_conflict" },
            "conflict_kind": text.clone(),
            "bridge_id": text.clone()
        }
    });
    let autonomy_freeze = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["payload_type", "status"],
        "properties": {
            "payload_type": { "const": "autonomy_freeze" },
            "status": { "enum": ["active", "frozen"] },
            "reason": reason
        }
    });

    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Hydra canonical CRM event v1",
        "type": "object",
        "additionalProperties": false,
        "required": [
            "event_id", "spec_version", "event_type", "schema_version", "source",
            "subject", "occurred_at", "hydra_tenant_id", "actor", "data_class", "payload"
        ],
        "properties": {
            "event_id": uuid.clone(),
            "spec_version": { "const": HYDRA_EVENT_SPEC_VERSION },
            "event_type": { "enum": event_types.clone() },
            "schema_version": { "const": HYDRA_EVENT_SCHEMA_VERSION },
            "source": { "const": HYDRA_EVENT_SOURCE },
            "subject": { "enum": event_types },
            "occurred_at": date_time.clone(),
            "observed_at": date_time,
            "hydra_tenant_id": uuid.clone(),
            "external_binding_id": uuid,
            "actor": actor,
            "correlation_id": text.clone(),
            "causation_id": text,
            "envelope_id": json!({ "type": "string", "format": "uuid" }),
            "entity": entity,
            "data_class": { "enum": ["public", "internal", "private", "restricted"] },
            "payload": {
                "oneOf": [
                    entity_change,
                    envelope_transition,
                    bridge_health,
                    sync_conflict,
                    autonomy_freeze
                ]
            }
        }
    })
}
