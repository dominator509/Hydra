use anyhow::Result;
use bridge_host::{KvStore, TenantStoreKvStore};
use store::{AdapterKvRepo, TestDb};
use uuid::Uuid;

#[tokio::test]
async fn store_kv_round_trip_is_tenant_and_adapter_scoped() -> Result<()> {
    let db = TestDb::new().await?;
    let result = async {
        let repo = AdapterKvRepo::new(db.pool.clone());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let store_a = TenantStoreKvStore::new(repo.clone(), tenant_a, "shared-adapter");
        let store_b = TenantStoreKvStore::new(repo, tenant_b, "shared-adapter");

        store_a.set("cursor", "one").await?;
        store_b.set("cursor", "two").await?;

        assert_eq!(store_a.get("cursor").await?, Some("one".into()));
        assert_eq!(store_b.get("cursor").await?, Some("two".into()));
        Ok::<(), anyhow::Error>(())
    }
    .await;

    db.cleanup().await?;
    result
}
