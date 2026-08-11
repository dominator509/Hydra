//! Integration-level tests for the BridgeEngineer loop and DataSteward.
//!
//! These tests exercise the full orchestration path, proving the state machine
//! drives through Discovery → Introspect before the expected synthesis placeholder.

use agents::bridge_engineer::{AgentError, BridgeEngineer, LoopStep};
use agents::data_steward::DataSteward;
use cdm::{Entity, MergeProposal};
use serde_json::json;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// BridgeEngineer loop integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_loop_reaches_synthesis_and_errors() {
    let result = BridgeEngineer::run("suitecrm");
    match result {
        Err(AgentError::SynthesisNotImplemented(msg)) => {
            assert!(
                msg.contains("LLM synthesis"),
                "SynthesisNotImplemented message should mention LLM synthesis: {msg}"
            );
        }
        other => panic!("expected SynthesisNotImplemented, got: {other:?}"),
    }
}

#[test]
fn test_loop_steps_are_ordered() {
    let steps = BridgeEngineer::steps();
    assert_eq!(
        steps,
        vec![
            LoopStep::Discover,
            LoopStep::Introspect,
            LoopStep::Synthesize,
            LoopStep::Conform,
            LoopStep::Wire,
            LoopStep::Canary,
            LoopStep::Draft,
        ]
    );
}

#[test]
fn test_empty_target_fails_discovery() {
    let result = BridgeEngineer::run("");
    match result {
        Err(AgentError::DiscoveryFailed(msg)) => {
            assert!(!msg.is_empty(), "discovery error should have a message");
        }
        other => panic!("expected DiscoveryFailed, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// DataSteward dedup integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_dedup_finds_email_collision() {
    let tenant = Uuid::nil();
    let entities = vec![
        Entity {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001")
                .expect("fixture UUID should parse"),
            kind: "party".into(),
            tenant,
            body: json!({"name": "Alice", "email": "alice@example.com"}),
            origin: "crm-a".into(),
            origin_ref: None,
            version: 1,
        },
        Entity {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002")
                .expect("fixture UUID should parse"),
            kind: "party".into(),
            tenant,
            body: json!({"name": "Alice Smith", "email": "alice@example.com", "phone": "+14255550101"}),
            origin: "crm-b".into(),
            origin_ref: None,
            version: 1,
        },
    ];

    let proposals = DataSteward::deduplicate(tenant, &entities);
    assert!(
        !proposals.is_empty(),
        "should find at least one merge proposal"
    );

    let p = &proposals[0];
    assert_eq!(p.ids.len(), 2);
    assert!(
        p.evidence.iter().any(|e| e.starts_with("email:")),
        "evidence should include email-based match"
    );
    assert!(
        (p.confidence - 1.0).abs() < f32::EPSILON,
        "email collision should produce confidence 1.0"
    );
}

#[test]
fn test_dedup_no_collision() {
    let tenant = Uuid::nil();
    let entities = vec![
        Entity {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001")
                .expect("fixture UUID should parse"),
            kind: "party".into(),
            tenant,
            body: json!({"name": "Alice", "email": "alice@crm-a.com"}),
            origin: "crm-a".into(),
            origin_ref: None,
            version: 1,
        },
        Entity {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002")
                .expect("fixture UUID should parse"),
            kind: "party".into(),
            tenant,
            body: json!({"name": "Bob", "email": "bob@crm-b.com"}),
            origin: "crm-b".into(),
            origin_ref: None,
            version: 1,
        },
    ];

    let proposals = DataSteward::deduplicate(tenant, &entities);
    assert!(
        proposals.is_empty(),
        "should not propose merge for unrelated parties"
    );
}

#[test]
fn test_merge_is_a_governed_proposal() {
    let tenant = Uuid::new_v4();
    let id1 =
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("fixture UUID should parse");
    let id2 =
        Uuid::parse_str("00000000-0000-0000-0000-000000000002").expect("fixture UUID should parse");

    let entities = vec![
        Entity {
            id: id1,
            kind: "party".into(),
            tenant,
            body: json!({"name": "Alice", "email": "alice@example.com"}),
            origin: "crm-a".into(),
            origin_ref: None,
            version: 1,
        },
        Entity {
            id: id2,
            kind: "party".into(),
            tenant,
            body: json!({"phone": "+14255550101"}),
            origin: "crm-b".into(),
            origin_ref: None,
            version: 1,
        },
    ];

    let proposal = MergeProposal {
        ids: vec![id1, id2],
        confidence: 0.9,
        evidence: vec!["name:fuzzy".into()],
    };

    let envelope = DataSteward::merge(&proposal, &entities).expect("merge should propose");
    assert_eq!(envelope.domain, "data");
    assert_eq!(envelope.action, "merge_parties");
    assert_eq!(envelope.targets, vec![id1, id2]);
    assert_eq!(envelope.payload["survivor_id"], id1.to_string());
    assert!(envelope.payload.get("email").is_none());
}

#[test]
fn test_agent_capability_truth_is_machine_readable() {
    assert_eq!(
        BridgeEngineer::capability().availability,
        agents::AgentCapabilityAvailability::Unavailable
    );
    assert_eq!(
        DataSteward::capability().availability,
        agents::AgentCapabilityAvailability::Experimental
    );
    assert_eq!(
        agents::comms::Comms::draft_capability().availability,
        agents::AgentCapabilityAvailability::Available
    );
    assert_eq!(
        agents::comms::Comms::transport_capability().availability,
        agents::AgentCapabilityAvailability::Unavailable
    );
}
