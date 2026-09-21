use fabric::{ScheduledBridgeSyncProposal, StoreEnvelopeService};
use governor::{Cell, Constitution, Governor, Level, PolicyMatrix};
use store::{Store, TestDb};
use uuid::Uuid;

#[tokio::test]
async fn scheduled_bridge_proposals_are_governed_and_idempotent(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let schedule_id = Uuid::new_v4();
        let service =
            StoreEnvelopeService::new(Store::new(db.pool.clone()), bridge_governor(Level::L2));
        let first = service
            .propose_scheduled_bridge_sync(proposal(tenant, schedule_id, "slot-1", "party"))
            .await?;
        assert_eq!(first.state, governor::EnvelopeState::PendingApproval);
        assert_eq!(
            first.invocation.origin_system.as_deref(),
            Some("hydra.scheduler")
        );
        assert_eq!(
            first.invocation.external_actor_id.as_deref(),
            Some("hydra-scheduler")
        );
        assert_eq!(
            first.invocation.external_actor_type.as_deref(),
            Some("hydra_internal_agent")
        );
        assert_eq!(first.invocation.idempotency_key.as_deref(), Some("slot-1"));

        let retry = service
            .propose_scheduled_bridge_sync(proposal(tenant, schedule_id, "slot-1", "party"))
            .await?;
        assert_eq!(retry.id, first.id);

        let conflict = service
            .propose_scheduled_bridge_sync(proposal(tenant, schedule_id, "slot-1", "deal"))
            .await
            .expect_err("a slot key cannot be reused for a different request");
        assert!(matches!(conflict, fabric::FabricError::IdempotencyConflict));

        let stored = Store::new(db.pool.clone())
            .envelopes
            .get(tenant, first.id)
            .await?;
        assert_eq!(stored.payload["adapter_id"], "memcrm");
        assert_eq!(stored.payload["kind"], "party");
        assert_eq!(stored.payload["limit"], 25);
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

fn proposal(
    tenant_id: Uuid,
    schedule_id: Uuid,
    slot_key: &str,
    kind: &str,
) -> ScheduledBridgeSyncProposal {
    ScheduledBridgeSyncProposal {
        tenant_id,
        schedule_id,
        adapter_id: "memcrm".to_owned(),
        kind: kind.to_owned(),
        page_limit: 25,
        slot_key: slot_key.to_owned(),
        correlation_id: "hydra.scheduler/slot-1".to_owned(),
        trace: store::TraceContext::fresh(),
    }
}

fn bridge_governor(level: Level) -> Governor {
    let mut matrix = PolicyMatrix::default();
    matrix
        .insert(
            "bridges",
            Some("sync_adapter"),
            None,
            Cell {
                level,
                batch_max: Some(1),
            },
        )
        .expect("test policy is valid");
    Governor {
        matrix,
        constitution: Constitution {
            monthly_spend_cap_cents: 50_000,
            pii_egress_allowlist: vec!["private".to_owned()],
            blast_entities_ceiling: 100,
            blast_sends_ceiling: 0,
            blast_money_ceiling_cents: 0,
        },
    }
}
