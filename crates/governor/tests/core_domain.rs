use governor::{
    ActionEnvelope, BlastRadius, Cell, Clock, Constitution, Decision, DomainError, EnvelopeState,
    Governor, Level, PolicyMatrix, Reversal, SpendSnapshot,
};
use proptest::prelude::*;
use time::OffsetDateTime;
use uuid::Uuid;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }
}

#[test]
fn envelope_illegal_transition_rejected() {
    let mut envelope = envelope_with(Level::L4);

    let error = envelope
        .transition(EnvelopeState::Executing, "system", &FixedClock)
        .expect_err("proposed -> executing must be rejected");

    assert_eq!(
        error,
        DomainError::IllegalTransition {
            from: EnvelopeState::Proposed,
            to: EnvelopeState::Executing,
        }
    );
}

#[test]
fn envelope_transition_records_history() {
    let mut envelope = envelope_with(Level::L4);

    envelope
        .transition(EnvelopeState::Approved, "governor", &FixedClock)
        .expect("proposed -> approved must be legal");
    envelope
        .transition(EnvelopeState::Executing, "executor", &FixedClock)
        .expect("approved -> executing must be legal");

    assert_eq!(envelope.state, EnvelopeState::Executing);
    assert_eq!(envelope.history.len(), 2);
    assert_eq!(envelope.history[0].actor, "governor");
    assert_eq!(envelope.history[0].at_rfc3339, "1970-01-01T00:00:00Z");
}

#[test]
fn regress_policy_resolution_duplicate_is_rejected() {
    let mut matrix = PolicyMatrix::default();
    matrix
        .insert(
            "pipeline",
            Some("move_stage"),
            Some("deal"),
            Cell {
                level: Level::L4,
                batch_max: None,
            },
        )
        .expect("first matrix insert must succeed");

    let error = matrix
        .insert(
            "pipeline",
            Some("move_stage"),
            Some("deal"),
            Cell {
                level: Level::L3,
                batch_max: Some(5),
            },
        )
        .expect_err("duplicate specificity should be rejected");

    assert!(matches!(error, DomainError::PolicyResolutionAmbiguous(_)));
}

#[test]
fn regress_constitution_cap_boundary_blocks_at_cap() {
    let constitution = constitution_with_private();
    let envelope = envelope_with(Level::L4);

    assert!(constitution
        .check(
            &envelope,
            &SpendSnapshot {
                month_to_date_cents: 4_999,
            }
        )
        .is_ok());
    assert!(constitution
        .check(
            &envelope,
            &SpendSnapshot {
                month_to_date_cents: 5_000,
            }
        )
        .is_err());
    assert!(constitution
        .check(
            &envelope,
            &SpendSnapshot {
                month_to_date_cents: 5_001,
            }
        )
        .is_err());
}

#[test]
fn regress_constitution_rejects_pii_egress_without_private_allowlist() {
    let constitution = Constitution {
        pii_egress_allowlist: vec!["public".to_owned()],
        ..constitution_with_private()
    };
    let mut envelope = envelope_with(Level::L4);
    envelope.blast.pii_egress = true;

    let error = constitution
        .check(&envelope, &zero_spend())
        .expect_err("pii egress without the private allowlist entry must be blocked");

    assert_eq!(error, governor::Rule::PiiEgressNotAllowListed);
}

#[test]
fn regress_constitution_rejects_hard_delete_actions() {
    let constitution = constitution_with_private();
    let mut envelope = envelope_with(Level::L4);
    envelope.action = "hard_delete".to_owned();

    let error = constitution
        .check(&envelope, &zero_spend())
        .expect_err("hard_delete actions must be blocked");

    assert_eq!(error, governor::Rule::HardDeleteForbidden);
}

#[test]
fn regress_big_blast_clamps_execute_to_queue() {
    let governor = governor_with(Level::L5);
    let mut envelope = envelope_with(Level::L5);
    envelope.blast.entities = 999;

    let decision = governor.evaluate(&envelope, &zero_spend());

    assert!(matches!(decision, Decision::Queue));
}

proptest! {
    #[test]
    fn prop_irreversible_demotion_monotonicity(level in arb_level()) {
        let governor = governor_with(level);
        let spend = zero_spend();
        let reversible = envelope_with(level);
        let mut irreversible = envelope_with(level);
        irreversible.reversal = Reversal::Irreversible;

        let reversible_rank = decision_rank(&governor.evaluate(&reversible, &spend));
        let irreversible_rank = decision_rank(&governor.evaluate(&irreversible, &spend));

        prop_assert!(irreversible_rank <= reversible_rank);
    }

    #[test]
    fn prop_matrix_resolution_specificity(
        domain in "[a-z]{3,8}",
        action in "[a-z_]{3,8}",
        kind in "[a-z]{3,8}",
        exact_level in arb_level(),
        action_level in arb_level(),
        default_level in arb_level(),
    ) {
        let mut matrix = PolicyMatrix::default();
        matrix
            .insert(&domain, None, None, Cell { level: default_level, batch_max: None })
            .expect("domain default insert should succeed");
        matrix
            .insert(&domain, Some(&action), None, Cell { level: action_level, batch_max: None })
            .expect("action insert should succeed");
        matrix
            .insert(&domain, Some(&action), Some(&kind), Cell { level: exact_level, batch_max: None })
            .expect("exact insert should succeed");

        prop_assert_eq!(matrix.resolve(&domain, &action, Some(&kind)).level, exact_level);
        prop_assert_eq!(matrix.resolve(&domain, &action, Some("other")).level, action_level);
        prop_assert_eq!(matrix.resolve(&domain, "different", Some(&kind)).level, default_level);
    }

    #[test]
    fn prop_envelope_transition_table_exhaustive(from in arb_state(), to in arb_state()) {
        let expected = legal_transition(from, to);
        let mut envelope = envelope_with(Level::L4);
        envelope.state = from;
        let result = envelope.transition(to, "prop", &FixedClock);

        prop_assert_eq!(result.is_ok(), expected);
    }
}

#[cfg(not(debug_assertions))]
#[test]
#[ignore = "release-only performance gate"]
fn perf_governor_eval_p99_under_5ms() {
    let governor = governor_with(Level::L5);
    let spend = zero_spend();
    let mut samples = Vec::with_capacity(10_000);

    for index in 0..10_000_u64 {
        let mut envelope = envelope_with(if index % 2 == 0 { Level::L5 } else { Level::L4 });
        envelope.action = if index % 3 == 0 {
            "send_email".to_owned()
        } else {
            "move_stage".to_owned()
        };
        envelope.kind = if index % 5 == 0 {
            Some("party".to_owned())
        } else {
            Some("deal".to_owned())
        };
        envelope.reversal = if index % 7 == 0 {
            Reversal::Irreversible
        } else {
            Reversal::Compensating
        };
        envelope.blast.entities = (index % 30) as u32;
        envelope.blast.external_sends = (index % 4) as u32;
        envelope.blast.money_cents = index % 2_000;

        let start = Instant::now();
        let _ = governor.evaluate(&envelope, &spend);
        samples.push(start.elapsed());
    }

    samples.sort_unstable();
    let p99 = samples[(samples.len() * 99) / 100];
    assert!(p99.as_millis() < 5, "expected p99 < 5ms, got {:?}", p99);
}

fn arb_level() -> impl Strategy<Value = Level> {
    prop_oneof![
        Just(Level::L0),
        Just(Level::L1),
        Just(Level::L2),
        Just(Level::L3),
        Just(Level::L4),
        Just(Level::L5),
    ]
}

fn arb_state() -> impl Strategy<Value = EnvelopeState> {
    prop_oneof![
        Just(EnvelopeState::Proposed),
        Just(EnvelopeState::PendingApproval),
        Just(EnvelopeState::Approved),
        Just(EnvelopeState::Executing),
        Just(EnvelopeState::Executed),
        Just(EnvelopeState::Failed),
        Just(EnvelopeState::RolledBack),
        Just(EnvelopeState::Rejected),
    ]
}

fn constitution_with_private() -> Constitution {
    Constitution {
        monthly_spend_cap_cents: 5_000,
        pii_egress_allowlist: vec!["private".to_owned()],
        blast_entities_ceiling: 50,
        blast_sends_ceiling: 10,
        blast_money_ceiling_cents: 1_000,
    }
}

fn governor_with(level: Level) -> Governor {
    let mut matrix = PolicyMatrix::default();
    matrix
        .insert(
            "pipeline",
            None,
            None,
            Cell {
                level,
                batch_max: Some(25),
            },
        )
        .expect("seed policy matrix insert should succeed");

    Governor {
        matrix,
        constitution: constitution_with_private(),
    }
}

fn envelope_with(_level: Level) -> ActionEnvelope {
    ActionEnvelope {
        id: Uuid::new_v4(),
        tenant: Uuid::new_v4(),
        domain: "pipeline".to_owned(),
        action: "move_stage".to_owned(),
        kind: Some("deal".to_owned()),
        targets: vec![Uuid::new_v4()],
        payload: serde_json::json!({}),
        rationale: "test".to_owned(),
        reversal: Reversal::Compensating,
        blast: BlastRadius::default(),
        state: EnvelopeState::Proposed,
        history: Vec::new(),
    }
}

fn decision_rank(decision: &Decision) -> u8 {
    match decision {
        Decision::Block(_) => 0,
        Decision::SuggestOnly => 1,
        Decision::Queue => 2,
        Decision::Execute(_) => 3,
    }
}

fn legal_transition(from: EnvelopeState, to: EnvelopeState) -> bool {
    match from {
        EnvelopeState::Proposed => matches!(
            to,
            EnvelopeState::PendingApproval | EnvelopeState::Approved | EnvelopeState::Rejected
        ),
        EnvelopeState::PendingApproval => {
            matches!(to, EnvelopeState::Approved | EnvelopeState::Rejected)
        }
        EnvelopeState::Approved => matches!(to, EnvelopeState::Executing),
        EnvelopeState::Executing => matches!(to, EnvelopeState::Executed | EnvelopeState::Failed),
        EnvelopeState::Executed => matches!(to, EnvelopeState::RolledBack),
        EnvelopeState::Failed => matches!(to, EnvelopeState::RolledBack),
        EnvelopeState::RolledBack => false,
        EnvelopeState::Rejected => false,
    }
}

fn zero_spend() -> SpendSnapshot {
    SpendSnapshot {
        month_to_date_cents: 0,
    }
}

#[cfg(not(debug_assertions))]
use std::time::Instant;
