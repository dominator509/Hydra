use serde_json::json;
use store::{
    BridgeAdapterState, BridgeAdapterTransition, NewBridgeAdapter, Store, StoreError, TestDb,
};
use uuid::Uuid;

fn new_adapter(tenant_id: Uuid, adapter_id: &str) -> NewBridgeAdapter {
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

#[tokio::test]
async fn bridge_adapter_registry_is_tenant_scoped_and_append_audited(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant_id = Uuid::new_v4();
        let other_tenant_id = Uuid::new_v4();
        let created = store
            .bridge_adapters
            .create(new_adapter(tenant_id, "memcrm"))
            .await?;
        assert_eq!(created.state, BridgeAdapterState::Inactive);
        assert_eq!(created.revision, 0);
        assert_eq!(created.grant_config["fuel"], 100000);

        let activating = store
            .bridge_adapters
            .transition(BridgeAdapterTransition {
                tenant_id,
                adapter_id: "memcrm".to_owned(),
                expected_revision: 0,
                expected_state: BridgeAdapterState::Inactive,
                new_state: BridgeAdapterState::Activating,
                descriptor: None,
                last_error: None,
                event: json!({"event": "activation_started"}),
            })
            .await?;
        assert_eq!(activating.state, BridgeAdapterState::Activating);
        assert_eq!(activating.revision, 1);

        let descriptor = json!({"name": "memcrm", "version": "1.0.0"});
        let active = store
            .bridge_adapters
            .transition(BridgeAdapterTransition {
                tenant_id,
                adapter_id: "memcrm".to_owned(),
                expected_revision: 1,
                expected_state: BridgeAdapterState::Activating,
                new_state: BridgeAdapterState::Active,
                descriptor: Some(descriptor.clone()),
                last_error: None,
                event: json!({"event": "activation_succeeded"}),
            })
            .await?;
        assert_eq!(active.state, BridgeAdapterState::Active);
        assert_eq!(active.descriptor, Some(descriptor));

        let history = store.bridge_adapters.history(tenant_id, "memcrm").await?;
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].from_state, None);
        assert_eq!(history[0].to_state, BridgeAdapterState::Inactive);
        assert_eq!(history[2].to_state, BridgeAdapterState::Active);

        assert!(store
            .bridge_adapters
            .get(other_tenant_id, "memcrm")
            .await?
            .is_none());
        assert_eq!(
            store
                .bridge_adapters
                .list_for_tenant(other_tenant_id)
                .await?,
            Vec::new()
        );

        let duplicate = store
            .bridge_adapters
            .create(new_adapter(tenant_id, "memcrm"))
            .await;
        assert!(matches!(duplicate, Err(StoreError::Database(_))));

        let stale = store
            .bridge_adapters
            .transition(BridgeAdapterTransition {
                tenant_id,
                adapter_id: "memcrm".to_owned(),
                expected_revision: 0,
                expected_state: BridgeAdapterState::Inactive,
                new_state: BridgeAdapterState::Paused,
                descriptor: None,
                last_error: None,
                event: json!({"event": "stale"}),
            })
            .await;
        assert!(matches!(stale, Err(StoreError::Conflict(0))));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn bridge_adapter_registry_rejects_invalid_authority_and_digest(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant_id = Uuid::new_v4();

        let invalid_digest = store
            .bridge_adapters
            .create(NewBridgeAdapter {
                component_sha256: "A".repeat(64),
                ..new_adapter(tenant_id, "invalid")
            })
            .await;
        assert!(matches!(invalid_digest, Err(StoreError::Invariant(_))));

        let nil_tenant = store
            .bridge_adapters
            .create(new_adapter(Uuid::nil(), "invalid"))
            .await;
        assert!(matches!(nil_tenant, Err(StoreError::Invariant(_))));

        let invalid_event = store
            .bridge_adapters
            .transition(BridgeAdapterTransition {
                tenant_id,
                adapter_id: "missing".to_owned(),
                expected_revision: 0,
                expected_state: BridgeAdapterState::Inactive,
                new_state: BridgeAdapterState::Active,
                descriptor: None,
                last_error: None,
                event: json!("not-an-object"),
            })
            .await;
        assert!(matches!(invalid_event, Err(StoreError::Invariant(_))));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}
