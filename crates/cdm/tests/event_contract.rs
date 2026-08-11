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
