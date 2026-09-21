use std::sync::Arc;

use async_trait::async_trait;
use bridge_host::{
    BridgeHost, BridgeLifecycle, ComponentRoot, ConformanceRequest, EgressClient,
    FullRelistRequest, Grant, ProbeRequest, SyncRequest,
};
use store::TestDb;
use uuid::Uuid;

const ADAPTER_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../adapters");

#[derive(Default)]
struct FixtureEgress;

#[async_trait]
impl EgressClient for FixtureEgress {
    async fn send(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<Vec<u8>>,
    ) -> anyhow::Result<(u16, Vec<(String, String)>, Vec<u8>)> {
        Ok((200, Vec::new(), br#"{}"#.to_vec()))
    }
}

fn grant(adapter_id: &str) -> Grant {
    Grant {
        adapter_id: adapter_id.to_owned(),
        origins: vec!["https://fixture.example".to_owned()],
        secret_names: Vec::new(),
        dsn_name: None,
        fuel: 100_000,
    }
}

#[tokio::test]
async fn lifecycle_component_root_rejects_traversal_and_digest_changes() -> anyhow::Result<()> {
    let root = ComponentRoot::new(ADAPTER_ROOT)?;
    assert!(root.load("../Cargo.toml", None).is_err());
    let artifact = root.load("memcrm.wasm", None)?;
    assert_eq!(artifact.component_ref, "memcrm.wasm");
    assert_eq!(
        artifact.path.file_name().and_then(|name| name.to_str()),
        Some("memcrm.wasm")
    );
    assert!(root.load("memcrm.wasm", Some(&"0".repeat(64))).is_err());
    Ok(())
}

#[tokio::test]
async fn lifecycle_probe_checks_grant_and_returns_fixture_descriptor() -> anyhow::Result<()> {
    let db = TestDb::new().await?;
    let result = async {
        let root = ComponentRoot::new(ADAPTER_ROOT)?;
        let artifact = root.load("memcrm.wasm", None)?;
        let lifecycle = BridgeLifecycle::new(
            Arc::new(BridgeHost::new()?),
            root,
            store::AdapterKvRepo::new(db.pool.clone()),
            Arc::new(bridge_host::StaticSecretSource::default()),
            Arc::new(FixtureEgress),
        );
        let result = lifecycle
            .probe(ProbeRequest {
                tenant_id: Uuid::new_v4(),
                adapter_id: "memcrm",
                component_ref: "memcrm.wasm",
                expected_sha256: Some(&artifact.sha256),
                grant: grant("memcrm"),
                config_json: "{}",
            })
            .await?;
        assert_eq!(result.descriptor.name, "memcrm");
        assert!(!result.descriptor.version.is_empty());
        assert!(result.fuel_remaining > 0);

        let invalid = lifecycle
            .probe(ProbeRequest {
                tenant_id: Uuid::new_v4(),
                adapter_id: "memcrm",
                component_ref: "memcrm.wasm",
                expected_sha256: None,
                grant: grant("other-adapter"),
                config_json: "{}",
            })
            .await;
        assert!(matches!(
            invalid,
            Err(bridge_host::LifecycleError::InvalidGrant(_))
        ));
        Ok::<(), anyhow::Error>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn lifecycle_sync_page_uses_tenant_scoped_host_state() -> anyhow::Result<()> {
    let db = TestDb::new().await?;
    let result = async {
        let root = ComponentRoot::new(ADAPTER_ROOT)?;
        let artifact = root.load("memcrm.wasm", None)?;
        let lifecycle = BridgeLifecycle::new(
            Arc::new(BridgeHost::new()?),
            root,
            store::AdapterKvRepo::new(db.pool.clone()),
            Arc::new(bridge_host::StaticSecretSource::default()),
            Arc::new(FixtureEgress),
        );
        let tenant_id = Uuid::new_v4();
        let page = lifecycle
            .sync_page(SyncRequest {
                tenant_id,
                adapter_id: "memcrm",
                component_ref: "memcrm.wasm",
                expected_sha256: Some(&artifact.sha256),
                grant: grant("memcrm"),
                config_json: "{}",
                cursor: "",
                limit: 50,
            })
            .await?;
        assert_eq!(page.changes.changes.len(), 0);
        assert_eq!(page.changes.next_cursor, "");
        assert!(page.fuel_remaining > 0);
        let invalid = lifecycle
            .sync_page(SyncRequest {
                tenant_id,
                adapter_id: "memcrm",
                component_ref: "memcrm.wasm",
                expected_sha256: Some(&artifact.sha256),
                grant: grant("memcrm"),
                config_json: "{}",
                cursor: "",
                limit: 101,
            })
            .await;
        assert!(matches!(invalid, Err(bridge_host::LifecycleError::Sync(_))));
        Ok::<(), anyhow::Error>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn lifecycle_full_relist_returns_bounded_metadata_for_fixture() -> anyhow::Result<()> {
    let db = TestDb::new().await?;
    let result = async {
        let root = ComponentRoot::new(ADAPTER_ROOT)?;
        let artifact = root.load("memcrm.wasm", None)?;
        let lifecycle = BridgeLifecycle::new(
            Arc::new(BridgeHost::new()?),
            root,
            store::AdapterKvRepo::new(db.pool.clone()),
            Arc::new(bridge_host::StaticSecretSource::default()),
            Arc::new(FixtureEgress),
        );
        let result = lifecycle
            .full_relist(FullRelistRequest {
                tenant_id: Uuid::new_v4(),
                adapter_id: "memcrm",
                component_ref: "memcrm.wasm",
                expected_sha256: Some(&artifact.sha256),
                grant: grant("memcrm"),
                config_json: "{}",
                kind: "party",
                limit: 25,
            })
            .await?;
        assert_eq!(result.page_count, 1);
        assert!(result.records.is_empty());
        assert_eq!(result.total_bytes, 0);
        assert!(result.fuel_remaining > 0);
        let invalid = lifecycle
            .full_relist(FullRelistRequest {
                tenant_id: Uuid::new_v4(),
                adapter_id: "memcrm",
                component_ref: "memcrm.wasm",
                expected_sha256: Some(&artifact.sha256),
                grant: grant("memcrm"),
                config_json: "{}",
                kind: "party",
                limit: 101,
            })
            .await;
        assert!(matches!(invalid, Err(bridge_host::LifecycleError::Sync(_))));
        Ok::<(), anyhow::Error>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn lifecycle_conformance_returns_bounded_metadata_without_records() -> anyhow::Result<()> {
    let db = TestDb::new().await?;
    let result = async {
        let root = ComponentRoot::new(ADAPTER_ROOT)?;
        let artifact = root.load("memcrm.wasm", None)?;
        let lifecycle = BridgeLifecycle::new(
            Arc::new(BridgeHost::new()?),
            root,
            store::AdapterKvRepo::new(db.pool.clone()),
            Arc::new(bridge_host::StaticSecretSource::default()),
            Arc::new(FixtureEgress),
        );
        let result = lifecycle
            .conformance(ConformanceRequest {
                tenant_id: Uuid::new_v4(),
                adapter_id: "memcrm",
                component_ref: "memcrm.wasm",
                expected_sha256: Some(&artifact.sha256),
                grant: grant("memcrm"),
                config_json: "{}",
                kind: Some("party"),
                limit: 25,
            })
            .await?;
        assert_eq!(result.descriptor.name, "memcrm");
        assert_eq!(result.checked_kind, "party");
        assert_eq!(result.schema_field_count, 4);
        assert_eq!(result.listed_record_count, 0);
        assert_eq!(result.changed_record_count, 0);
        assert!(result.incremental_checked);
        assert!(result.report.contains("changes-since:checked"));
        assert!(!result.report.contains("customer"));
        assert!(result.fuel_remaining > 0);

        let invalid_kind = lifecycle
            .conformance(ConformanceRequest {
                tenant_id: Uuid::new_v4(),
                adapter_id: "memcrm",
                component_ref: "memcrm.wasm",
                expected_sha256: Some(&artifact.sha256),
                grant: grant("memcrm"),
                config_json: "{}",
                kind: Some("Unknown"),
                limit: 25,
            })
            .await;
        assert!(matches!(
            invalid_kind,
            Err(bridge_host::LifecycleError::Conformance(_))
        ));

        let invalid_digest = lifecycle
            .conformance(ConformanceRequest {
                tenant_id: Uuid::new_v4(),
                adapter_id: "memcrm",
                component_ref: "memcrm.wasm",
                expected_sha256: Some(&"0".repeat(64)),
                grant: grant("memcrm"),
                config_json: "{}",
                kind: Some("party"),
                limit: 25,
            })
            .await;
        assert!(matches!(
            invalid_digest,
            Err(bridge_host::LifecycleError::DigestMismatch { .. })
        ));
        Ok::<(), anyhow::Error>(())
    }
    .await;
    db.cleanup().await?;
    result
}
