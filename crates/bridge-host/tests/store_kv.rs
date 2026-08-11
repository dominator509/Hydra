use anyhow::Result;
use bridge_host::{KvStore, StoreKvStore};
use store::{AdapterKvRepo, TestDb};

#[tokio::test]
async fn store_kv_round_trip_is_adapter_scoped() -> Result<()> {
    let db = TestDb::new().await?;
    let result = async {
        let repo = AdapterKvRepo::new(db.pool.clone());
        let store_a = StoreKvStore::new(repo.clone(), "adapter-a");
        let store_b = StoreKvStore::new(repo, "adapter-b");

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
