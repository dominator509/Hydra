use serde_json::json;
use store::{
    BridgeAdapterState, BridgeAdapterTransition, BridgeScheduleState, NewBridgeAdapter,
    NewBridgeSyncSchedule, Store, StoreError, TestDb, MAX_INTERVAL_SECONDS, MAX_PAGE_LIMIT,
    MIN_INTERVAL_SECONDS,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn adapter(tenant_id: Uuid, adapter_id: &str) -> NewBridgeAdapter {
    NewBridgeAdapter {
        tenant_id,
        adapter_id: adapter_id.to_owned(),
        component_ref: "memcrm.wasm".to_owned(),
        component_sha256: "a".repeat(64),
        grant_config: json!({
            "origins": [],
            "secret_names": [],
            "dsn_name": null,
            "fuel": 100000
        }),
        config: json!({}),
    }
}

async fn active_adapter(
    store: &Store,
    tenant_id: Uuid,
    adapter_id: &str,
) -> Result<(), StoreError> {
    store
        .bridge_adapters
        .create(adapter(tenant_id, adapter_id))
        .await?;
    store
        .bridge_adapters
        .transition(BridgeAdapterTransition {
            tenant_id,
            adapter_id: adapter_id.to_owned(),
            expected_revision: 0,
            expected_state: BridgeAdapterState::Inactive,
            new_state: BridgeAdapterState::Activating,
            descriptor: None,
            last_error: None,
            event: json!({"event": "activation_started"}),
        })
        .await?;
    store
        .bridge_adapters
        .transition(BridgeAdapterTransition {
            tenant_id,
            adapter_id: adapter_id.to_owned(),
            expected_revision: 1,
            expected_state: BridgeAdapterState::Activating,
            new_state: BridgeAdapterState::Active,
            descriptor: Some(json!({
                "name": adapter_id,
                "version": "1.0.0",
                "capabilities": {"read": true, "incremental_sync": true},
                "kinds": ["party"]
            })),
            last_error: None,
            event: json!({"event": "activation_succeeded"}),
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn schedules_validate_bounds_and_remain_tenant_scoped(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        active_adapter(&store, tenant, "memcrm").await?;

        let schedule = store
            .bridge_schedules
            .create(NewBridgeSyncSchedule {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                interval_seconds: MIN_INTERVAL_SECONDS,
                page_limit: MAX_PAGE_LIMIT,
            })
            .await?;
        assert!(!schedule.enabled);
        assert_eq!(schedule.interval_seconds, MIN_INTERVAL_SECONDS);
        assert_eq!(schedule.page_limit, MAX_PAGE_LIMIT);
        assert!(store
            .bridge_schedules
            .list_for_tenant(Uuid::new_v4())
            .await?
            .is_empty());

        let duplicate = store
            .bridge_schedules
            .create(NewBridgeSyncSchedule {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                interval_seconds: MAX_INTERVAL_SECONDS,
                page_limit: 1,
            })
            .await;
        assert!(matches!(duplicate, Err(StoreError::Database(_))));

        let invalid_interval = store
            .bridge_schedules
            .create(NewBridgeSyncSchedule {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "lead".to_owned(),
                interval_seconds: MIN_INTERVAL_SECONDS - 1,
                page_limit: 1,
            })
            .await;
        assert!(matches!(invalid_interval, Err(StoreError::Invariant(_))));

        let disabled = store
            .bridge_schedules
            .set_state(tenant, schedule.id, BridgeScheduleState::Disabled)
            .await?;
        assert!(!disabled.enabled);
        let enabled = store
            .bridge_schedules
            .set_state(tenant, schedule.id, BridgeScheduleState::Enabled)
            .await?;
        assert!(enabled.enabled);
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn due_claims_are_exclusive_and_stale_leases_are_reclaimable(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        active_adapter(&store, tenant, "memcrm").await?;
        let schedule = store
            .bridge_schedules
            .create(NewBridgeSyncSchedule {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                interval_seconds: MIN_INTERVAL_SECONDS,
                page_limit: 10,
            })
            .await?;
        store
            .bridge_schedules
            .set_state(tenant, schedule.id, BridgeScheduleState::Enabled)
            .await?;

        let now = OffsetDateTime::now_utc();
        let (first, second) = tokio::join!(
            store.bridge_schedules.claim_due(now, 120, 1),
            store.bridge_schedules.claim_due(now, 120, 1),
        );
        let mut claims = Vec::new();
        claims.extend(first?);
        claims.extend(second?);
        assert_eq!(claims.len(), 1);
        let claim = claims.remove(0);
        assert_eq!(claim.schedule.id, schedule.id);
        assert_eq!(claim.schedule.lease_token, Some(claim.lease_token));

        let stale = store
            .bridge_schedules
            .complete_claim(
                tenant,
                schedule.id,
                Uuid::new_v4(),
                now,
                store::BridgeSyncScheduleCompletion {
                    envelope_id: None,
                    error: Some("must not complete".to_owned()),
                },
            )
            .await;
        assert!(matches!(stale, Err(StoreError::Conflict(0))));

        let reclaimed = store
            .bridge_schedules
            .claim_due(now + Duration::seconds(121), 120, 1)
            .await?;
        assert_eq!(reclaimed.len(), 1);
        assert_ne!(reclaimed[0].lease_token, claim.lease_token);

        let completed = store
            .bridge_schedules
            .complete_claim(
                tenant,
                schedule.id,
                reclaimed[0].lease_token,
                now + Duration::seconds(121),
                store::BridgeSyncScheduleCompletion {
                    envelope_id: Some(Uuid::new_v4()),
                    error: None,
                },
            )
            .await?;
        assert!(completed.lease_token.is_none());
        assert!(completed.last_envelope_id.is_some());
        assert!(completed.next_due_at > now + Duration::seconds(121));
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}
