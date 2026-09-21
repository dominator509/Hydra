use store::{AdapterKvRepo, TestDb};
use uuid::Uuid;

#[tokio::test]
async fn tenant_adapter_kv_isolated_for_equal_adapter_ids() -> Result<(), Box<dyn std::error::Error>>
{
    let db = TestDb::new().await?;
    let result = async {
        let repo = AdapterKvRepo::new(db.pool.clone());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        repo.set_for_tenant(tenant_a, "shared-adapter", "cursor", "tenant-a")
            .await?;
        repo.set_for_tenant(tenant_b, "shared-adapter", "cursor", "tenant-b")
            .await?;

        assert_eq!(
            repo.get_for_tenant(tenant_a, "shared-adapter", "cursor")
                .await?,
            Some("tenant-a".to_owned())
        );
        assert_eq!(
            repo.get_for_tenant(tenant_b, "shared-adapter", "cursor")
                .await?,
            Some("tenant-b".to_owned())
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn legacy_adapter_kv_methods_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let repo = AdapterKvRepo::new(db.pool.clone());
        assert!(repo.get("shared-adapter", "cursor").await.is_err());
        assert!(repo
            .set("shared-adapter", "cursor", "unsafe")
            .await
            .is_err());
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn tenant_adapter_kv_rejects_invalid_scope() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let repo = AdapterKvRepo::new(db.pool.clone());
        assert!(repo
            .get_for_tenant(Uuid::nil(), "adapter", "key")
            .await
            .is_err());
        assert!(repo
            .get_for_tenant(Uuid::new_v4(), "", "key")
            .await
            .is_err());
        assert!(repo
            .set_for_tenant(Uuid::new_v4(), "adapter", "key\0", "value")
            .await
            .is_err());
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}
