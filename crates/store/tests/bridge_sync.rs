use cdm::{HydraEventPayload, HydraEventType};
use serde_json::json;
use store::{
    BridgeAdapterState, BridgeSyncApplyError, BridgeSyncChange, BridgeSyncOperation,
    BridgeSyncRunStatus, EventProvenance, NewBridgeAdapter, NewBridgeSyncRun, Store, StoreError,
    TestDb,
};
use uuid::Uuid;

fn adapter(tenant_id: Uuid) -> NewBridgeAdapter {
    NewBridgeAdapter {
        tenant_id,
        adapter_id: "memcrm".to_owned(),
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

fn party(id: &str, display_name: &str) -> BridgeSyncChange {
    BridgeSyncChange {
        operation: BridgeSyncOperation::Upsert,
        kind: "party".to_owned(),
        external_id: id.to_owned(),
        body: json!({ "display_name": display_name }),
    }
}

fn deleted_party(id: &str) -> BridgeSyncChange {
    BridgeSyncChange {
        operation: BridgeSyncOperation::Delete,
        kind: "party".to_owned(),
        external_id: id.to_owned(),
        body: json!({}),
    }
}

#[tokio::test]
async fn sync_page_is_atomic_and_replayable_with_soft_delete(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        let provenance = EventProvenance::bridge("bridge:memcrm");
        let record = store.bridge_adapters.create(adapter(tenant)).await?;
        assert_eq!(record.state, BridgeAdapterState::Inactive);

        let first = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: Some("corr-1".to_owned()),
                causation_id: Some("cause-1".to_owned()),
                envelope_id: Some(Uuid::new_v4()),
            })
            .await?;
        let applied = store
            .bridge_sync
            .apply_page(
                tenant,
                first.id,
                "memcrm",
                "party",
                &[party("party-1", "Ada"), party("party-2", "Grace")],
                "cursor-1",
                &provenance,
            )
            .await?;
        assert_eq!(applied.applied_upserts, 2);
        assert_eq!(applied.next_cursor, "cursor-1");
        let state = store
            .bridge_sync
            .current(tenant, "memcrm", "party")
            .await?
            .expect("sync state");
        assert_eq!(state.cursor, "cursor-1");
        assert_eq!(state.revision, 1);
        assert_eq!(
            store
                .bridge_sync
                .get_run(tenant, first.id)
                .await?
                .expect("successful sync run should remain queryable")
                .status,
            BridgeSyncRunStatus::Succeeded
        );

        let second = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: None,
                causation_id: None,
                envelope_id: None,
            })
            .await?;
        store
            .bridge_sync
            .apply_page(
                tenant,
                second.id,
                "memcrm",
                "party",
                &[party("party-1", "Ada Lovelace")],
                "cursor-2",
                &provenance,
            )
            .await?;
        let entities = store.entities.list(tenant, "party", None, 10).await?;
        assert_eq!(entities.len(), 2);
        let updated = entities
            .iter()
            .find(|entity| entity.origin_ref.as_deref() == Some("party:party-1"))
            .expect("updated bridge entity");
        assert_eq!(updated.version, 2);
        assert_eq!(updated.body["display_name"], "Ada Lovelace");

        let third = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: None,
                causation_id: None,
                envelope_id: None,
            })
            .await?;
        store
            .bridge_sync
            .apply_page(
                tenant,
                third.id,
                "memcrm",
                "party",
                &[deleted_party("party-1")],
                "cursor-3",
                &provenance,
            )
            .await?;
        assert!(matches!(
            store.entities.get(tenant, updated.id).await,
            Err(StoreError::NotFound)
        ));
        let deleted_at: Option<String> = sqlx::query_scalar(
            "SELECT deleted_at::text FROM entity WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant)
        .bind(updated.id)
        .fetch_one(&db.pool)
        .await?;
        assert!(deleted_at.is_some());

        let event_types: Vec<String> =
            sqlx::query_scalar("SELECT kind FROM event_log WHERE tenant_id = $1 ORDER BY seq")
                .bind(tenant)
                .fetch_all(&db.pool)
                .await?;
        assert_eq!(
            event_types,
            vec![
                HydraEventType::EntityCreated.as_str().to_owned(),
                HydraEventType::EntityCreated.as_str().to_owned(),
                HydraEventType::EntityUpdated.as_str().to_owned(),
                HydraEventType::EntityDeleted.as_str().to_owned()
            ]
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn sync_runs_are_tenant_scoped_and_single_writer() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        store.bridge_adapters.create(adapter(tenant)).await?;

        let run = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: None,
                causation_id: None,
                envelope_id: None,
            })
            .await?;
        let duplicate = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: None,
                causation_id: None,
                envelope_id: None,
            })
            .await;
        assert!(matches!(duplicate, Err(StoreError::Invariant(_))));
        assert!(store
            .bridge_sync
            .get_run(other_tenant, run.id)
            .await?
            .is_none());
        assert!(store
            .bridge_sync
            .current(other_tenant, "memcrm", "party")
            .await?
            .is_none());
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn full_relist_diffs_without_version_churn_and_soft_deletes_only_missing_rows(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        store.bridge_adapters.create(adapter(tenant)).await?;
        let provenance = EventProvenance::bridge("bridge:memcrm");

        let first = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: Some("full-relist-1".to_owned()),
                causation_id: None,
                envelope_id: None,
            })
            .await?;
        let applied = store
            .bridge_sync
            .apply_full_relist(
                tenant,
                first.id,
                "memcrm",
                "party",
                &[party("party-1", "Ada"), party("party-2", "Grace")],
                &provenance,
            )
            .await?;
        assert_eq!(applied.applied_upserts, 2);
        assert_eq!(applied.applied_deletes, 0);

        let second = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: None,
                causation_id: None,
                envelope_id: None,
            })
            .await?;
        let unchanged = store
            .bridge_sync
            .apply_full_relist(
                tenant,
                second.id,
                "memcrm",
                "party",
                &[party("party-1", "Ada")],
                &provenance,
            )
            .await?;
        assert_eq!(unchanged.applied_upserts, 0);
        assert_eq!(unchanged.applied_deletes, 1);
        let party_one = store
            .entities
            .list(tenant, "party", None, 10)
            .await?
            .into_iter()
            .next()
            .expect("party one remains active");
        assert_eq!(party_one.version, 1);
        let deleted_at: Option<String> = sqlx::query_scalar(
            "SELECT deleted_at::text FROM entity WHERE tenant_id = $1 AND origin_ref = $2",
        )
        .bind(tenant)
        .bind("party:party-2")
        .fetch_one(&db.pool)
        .await?;
        assert!(deleted_at.is_some());

        let third = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: None,
                causation_id: None,
                envelope_id: None,
            })
            .await?;
        let revived = store
            .bridge_sync
            .apply_full_relist(
                tenant,
                third.id,
                "memcrm",
                "party",
                &[party("party-1", "Ada"), party("party-2", "Grace Hopper")],
                &provenance,
            )
            .await?;
        assert_eq!(revived.applied_upserts, 1);
        assert_eq!(revived.applied_deletes, 0);
        let active = store.entities.list(tenant, "party", None, 10).await?;
        assert_eq!(active.len(), 2);
        let revived_party = active
            .iter()
            .find(|entity| entity.origin_ref.as_deref() == Some("party:party-2"))
            .expect("relisted party should revive the tombstone");
        assert_eq!(revived_party.version, 3);
        assert_eq!(revived_party.body["display_name"], "Grace Hopper");
        assert_eq!(
            store
                .bridge_sync
                .current(tenant, "memcrm", "party")
                .await?
                .expect("full relist state")
                .revision,
            3
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn invalid_full_relist_snapshot_is_rejected_before_persistence(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        store.bridge_adapters.create(adapter(tenant)).await?;
        let run = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: None,
                causation_id: None,
                envelope_id: None,
            })
            .await?;
        let invalid = BridgeSyncChange {
            operation: BridgeSyncOperation::Upsert,
            kind: "party".to_owned(),
            external_id: "invalid".to_owned(),
            body: json!("not-an-object"),
        };
        let error = store
            .bridge_sync
            .apply_full_relist(
                tenant,
                run.id,
                "memcrm",
                "party",
                &[party("would-roll-back", "Ada"), invalid],
                &EventProvenance::bridge("bridge:memcrm"),
            )
            .await
            .expect_err("invalid full relist must fail");
        assert!(matches!(error, BridgeSyncApplyError::Conflict(_)));
        assert!(store
            .entities
            .list(tenant, "party", None, 10)
            .await?
            .is_empty());
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn invalid_page_rolls_back_and_parks_a_conflict() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        store.bridge_adapters.create(adapter(tenant)).await?;
        let run = store
            .bridge_sync
            .start(NewBridgeSyncRun {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                correlation_id: Some("corr-conflict".to_owned()),
                causation_id: None,
                envelope_id: None,
            })
            .await?;

        let invalid = BridgeSyncChange {
            operation: BridgeSyncOperation::Upsert,
            kind: "party".to_owned(),
            external_id: "bad-party".to_owned(),
            body: json!("not-an-object"),
        };
        let conflict = store
            .bridge_sync
            .apply_page(
                tenant,
                run.id,
                "memcrm",
                "party",
                &[party("rolled-back", "Not persisted"), invalid],
                "cursor-never-advanced",
                &EventProvenance::bridge("bridge:memcrm"),
            )
            .await
            .expect_err("invalid page must fail");
        let BridgeSyncApplyError::Conflict(conflict) = conflict else {
            panic!("expected validation conflict");
        };
        assert_eq!(conflict.conflict_kind, "invalid_record_shape");
        store
            .bridge_sync
            .fail(
                tenant,
                run.id,
                "record validation conflict",
                Some(conflict),
                &EventProvenance::bridge("bridge:memcrm"),
            )
            .await?;

        assert_eq!(
            store
                .bridge_sync
                .current(tenant, "memcrm", "party")
                .await?
                .expect("failed sync should preserve cursor state")
                .cursor,
            ""
        );
        assert!(store
            .entities
            .list(tenant, "party", None, 10)
            .await?
            .is_empty());
        assert_eq!(
            store
                .bridge_sync
                .get_run(tenant, run.id)
                .await?
                .expect("failed sync run should remain queryable")
                .status,
            BridgeSyncRunStatus::Failed
        );
        let conflict_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM bridge_sync_conflict WHERE tenant_id = $1")
                .bind(tenant)
                .fetch_one(&db.pool)
                .await?;
        assert_eq!(conflict_count, 1);
        let event_payload: serde_json::Value =
            sqlx::query_scalar("SELECT payload FROM event_log WHERE tenant_id = $1")
                .bind(tenant)
                .fetch_one(&db.pool)
                .await?;
        let event: cdm::HydraEventEnvelope = serde_json::from_value(event_payload)?;
        assert_eq!(event.event_type, HydraEventType::SyncConflict);
        assert!(matches!(
            event.payload,
            HydraEventPayload::SyncConflict { .. }
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}
