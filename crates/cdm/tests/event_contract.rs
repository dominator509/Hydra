use cdm::{
    hydra_event_v1_schema, EventContractError, HydraEventEnvelope, HydraEventPayload,
    HydraEventType, HYDRA_EVENT_SCHEMA_VERSION, HYDRA_EVENT_SOURCE, HYDRA_EVENT_SPEC_VERSION,
    HYDRA_EVENT_TYPES_V1,
};
use jsonschema::{Draft, JSONSchema};
use serde_json::Value;

const ENTITY_CREATED: &str =
    include_str!("../../kernel/tests/fixtures/events/hydra.crm.entity.created.v1.json");
const ENVELOPE_EXECUTED: &str =
    include_str!("../../kernel/tests/fixtures/events/hydra.crm.envelope.executed.v1.json");

#[test]
fn event_contract_fixtures_round_trip_and_validate_schema() {
    let schema = hydra_event_v1_schema();
    let validator = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&schema)
        .expect("canonical event schema should compile");

    for fixture in [ENTITY_CREATED, ENVELOPE_EXECUTED] {
        let value: Value = serde_json::from_str(fixture).expect("fixture should be JSON");
        assert!(validator.is_valid(&value), "fixture must validate: {value}");
        let event: HydraEventEnvelope =
            serde_json::from_value(value.clone()).expect("fixture should deserialize");
        event.validate().expect("fixture should satisfy invariants");
        assert_eq!(
            serde_json::to_value(event).expect("event should serialize"),
            value
        );
    }
}

#[test]
fn event_contract_names_versions_and_subjects_are_stable() {
    let names = HYDRA_EVENT_TYPES_V1
        .iter()
        .map(|event_type| event_type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "hydra.crm.entity.created.v1",
            "hydra.crm.entity.updated.v1",
            "hydra.crm.entity.deleted.v1",
            "hydra.crm.envelope.proposed.v1",
            "hydra.crm.envelope.queued.v1",
            "hydra.crm.envelope.approved.v1",
            "hydra.crm.envelope.executed.v1",
            "hydra.crm.envelope.failed.v1",
            "hydra.crm.bridge.health_changed.v1",
            "hydra.crm.sync.conflict.v1",
            "hydra.crm.autonomy.freeze_changed.v1",
        ]
    );
    assert_eq!(HYDRA_EVENT_SPEC_VERSION, "1.0");
    assert_eq!(HYDRA_EVENT_SCHEMA_VERSION, "1.0");
    assert_eq!(HYDRA_EVENT_SOURCE, "urn:hydra:crm");
    assert!(names.iter().all(|name| !name.contains('@')));
}

#[test]
fn event_contract_rejects_type_payload_mismatch() {
    let mut event: HydraEventEnvelope =
        serde_json::from_str(ENTITY_CREATED).expect("fixture should deserialize");
    event.event_type = HydraEventType::EntityDeleted;
    event.subject = HydraEventType::EntityDeleted.as_str().to_owned();
    assert_eq!(
        event.validate(),
        Err(EventContractError::TypePayloadMismatch)
    );

    event.payload = HydraEventPayload::EntityChange {
        operation: "deleted".to_owned(),
        version: 1,
    };
    event.validate().expect("matching payload should validate");
}

#[test]
fn event_contract_bounds_nested_text_and_rejects_control_characters() {
    let mut entity: HydraEventEnvelope =
        serde_json::from_str(ENTITY_CREATED).expect("fixture should deserialize");
    entity
        .entity
        .as_mut()
        .expect("fixture has entity")
        .origin_ref = Some("x".repeat(513));
    assert_eq!(
        entity.validate(),
        Err(EventContractError::InvalidField("entity.origin_ref"))
    );

    let mut envelope: HydraEventEnvelope =
        serde_json::from_str(ENVELOPE_EXECUTED).expect("fixture should deserialize");
    if let HydraEventPayload::EnvelopeTransition { outcome, .. } = &mut envelope.payload {
        *outcome = Some("verified\nwith-control".to_owned());
    } else {
        panic!("fixture should contain an envelope transition");
    }
    assert_eq!(
        envelope.validate(),
        Err(EventContractError::InvalidField("payload.outcome"))
    );
}

#[test]
fn event_contract_rejects_zero_entity_version() {
    let mut event: HydraEventEnvelope =
        serde_json::from_str(ENTITY_CREATED).expect("fixture should deserialize");
    event.payload = HydraEventPayload::EntityChange {
        operation: "created".to_owned(),
        version: 0,
    };
    assert_eq!(
        event.validate(),
        Err(EventContractError::InvalidField("payload.version"))
    );
}

#[test]
fn event_contract_requires_rfc3339_timestamps() {
    let mut event: HydraEventEnvelope =
        serde_json::from_str(ENTITY_CREATED).expect("fixture should deserialize");
    event.occurred_at = "not-a-timestamp".to_owned();
    assert_eq!(
        event.validate(),
        Err(EventContractError::InvalidField("occurred_at"))
    );

    event.occurred_at = "2026-08-11T12:00:00Z".to_owned();
    event.observed_at = Some("also-not-a-timestamp".to_owned());
    assert_eq!(
        event.validate(),
        Err(EventContractError::InvalidField("observed_at"))
    );
}

#[test]
fn event_schema_bounds_nested_text_and_rejects_control_characters() {
    let schema = hydra_event_v1_schema();
    let validator = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&schema)
        .expect("canonical event schema should compile");

    let mut oversized: Value =
        serde_json::from_str(ENTITY_CREATED).expect("fixture should deserialize");
    oversized["entity"]["origin_ref"] = Value::String("x".repeat(513));
    assert!(!validator.is_valid(&oversized));

    let mut control_character: Value =
        serde_json::from_str(ENVELOPE_EXECUTED).expect("fixture should deserialize");
    control_character["payload"]["outcome"] = Value::String("verified\nwith-control".to_owned());
    assert!(!validator.is_valid(&control_character));
}

#[test]
fn event_contract_fixtures_contain_no_secret_shaped_fields() {
    for fixture in [ENTITY_CREATED, ENVELOPE_EXECUTED] {
        let compact = fixture.to_ascii_lowercase().replace(['_', '-'], "");
        for forbidden in [
            "accesstoken",
            "refreshtoken",
            "authorization",
            "clientsecret",
            "password",
            "bearertoken",
            "rawemailbody",
            "prompt",
        ] {
            assert!(
                !compact.contains(forbidden),
                "fixture contains forbidden secret-shaped field {forbidden}"
            );
        }
    }
}
