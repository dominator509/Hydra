use store::{BindingStatus, NewExternalTenantBinding, Store, StoreError, TestDb};
use uuid::Uuid;

#[tokio::test]
async fn external_binding_is_unique_resolvable_and_soft_revocable(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let store = Store::new(db.pool.clone());
        let hydra_tenant = Uuid::new_v4();
        let other_hydra_tenant = Uuid::new_v4();
        let created = store
            .external_bindings
            .create(NewExternalTenantBinding {
                provider: "nexus".to_owned(),
                external_tenant_id: "nexus-tenant-a".to_owned(),
                external_business_id: "business-a".to_owned(),
                hydra_tenant_id: hydra_tenant,
            })
            .await?;

        assert_eq!(created.status, BindingStatus::Active);
        assert_eq!(created.hydra_tenant_id, hydra_tenant);

        let resolved = store
            .external_bindings
            .resolve("nexus", "nexus-tenant-a", "business-a")
            .await?
            .ok_or("active binding should resolve")?;
        assert_eq!(resolved, created);

        let duplicate = store
            .external_bindings
            .create(NewExternalTenantBinding {
                provider: "nexus".to_owned(),
                external_tenant_id: "nexus-tenant-a".to_owned(),
                external_business_id: "business-a".to_owned(),
                hydra_tenant_id: other_hydra_tenant,
            })
            .await;
        assert!(matches!(duplicate, Err(StoreError::Database(_))));

        let disabled = store
            .external_bindings
            .set_status(created.id, BindingStatus::Disabled)
            .await?;
        assert_eq!(disabled.status, BindingStatus::Disabled);
        assert_eq!(disabled.hydra_tenant_id, hydra_tenant);

        let revoked = store
            .external_bindings
            .set_status(created.id, BindingStatus::Revoked)
            .await?;
        assert_eq!(revoked.status, BindingStatus::Revoked);

        let tenant_bindings = store
            .external_bindings
            .list_for_hydra_tenant(hydra_tenant)
            .await?;
        assert_eq!(tenant_bindings, vec![revoked]);
        assert!(store
            .external_bindings
            .list_for_hydra_tenant(other_hydra_tenant)
            .await?
            .is_empty());

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn external_binding_rejects_blank_or_nil_authority_fields(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let store = Store::new(db.pool.clone());
        let blank = store
            .external_bindings
            .create(NewExternalTenantBinding {
                provider: " ".to_owned(),
                external_tenant_id: "tenant".to_owned(),
                external_business_id: "business".to_owned(),
                hydra_tenant_id: Uuid::new_v4(),
            })
            .await;
        assert!(matches!(blank, Err(StoreError::Invariant(_))));

        let nil_tenant = store
            .external_bindings
            .create(NewExternalTenantBinding {
                provider: "nexus".to_owned(),
                external_tenant_id: "tenant".to_owned(),
                external_business_id: "business".to_owned(),
                hydra_tenant_id: Uuid::nil(),
            })
            .await;
        assert!(matches!(nil_tenant, Err(StoreError::Invariant(_))));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn external_binding_database_constraint_rejects_unsafe_identifiers(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let invalid_provider = sqlx::query(
            "INSERT INTO external_tenant_binding \
             (provider, external_tenant_id, external_business_id, hydra_tenant_id) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind("nexus\nprovider")
        .bind("tenant")
        .bind("business")
        .bind(Uuid::new_v4())
        .execute(&db.pool)
        .await;
        assert!(invalid_provider.is_err());

        let invalid_business = sqlx::query(
            "INSERT INTO external_tenant_binding \
             (provider, external_tenant_id, external_business_id, hydra_tenant_id) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind("nexus")
        .bind("tenant")
        .bind("x".repeat(513))
        .bind(Uuid::new_v4())
        .execute(&db.pool)
        .await;
        assert!(invalid_business.is_err());

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}
