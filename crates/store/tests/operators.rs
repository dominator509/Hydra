use sqlx::Row;
use store::{BindingStatus, NewExternalTenantBinding, NewOperatorUser, Store, StoreError, TestDb};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn operator_lifecycle_is_atomic_and_disables_sessions(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let store = Store::new(db.pool.clone());
    let tenant_id = Uuid::new_v4();
    let user = store
        .operators
        .create(NewOperatorUser {
            tenant_id,
            username: "owner@example.test".to_owned(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$synthetic$synthetic".to_owned(),
            display_name: Some("Owner".to_owned()),
            role: "admin".to_owned(),
        })
        .await?;
    assert_eq!(store.operators.list_for_tenant(tenant_id).await?.len(), 1);
    sqlx::query(
        "INSERT INTO hydra_session (user_id, tenant_id, token, expires_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(user.id)
    .bind(tenant_id)
    .bind("operator-session-token")
    .bind(OffsetDateTime::now_utc() + Duration::hours(1))
    .execute(&db.pool)
    .await?;
    let disabled = store
        .operators
        .set_disabled(tenant_id, user.id, true)
        .await?;
    assert!(disabled.disabled_at.is_some());
    let session_count =
        sqlx::query("SELECT COUNT(*) AS count FROM hydra_session WHERE user_id = $1")
            .bind(user.id)
            .fetch_one(&db.pool)
            .await?
            .get::<i64, _>("count");
    assert_eq!(session_count, 0);
    let enabled = store
        .operators
        .set_disabled(tenant_id, user.id, false)
        .await?;
    assert!(enabled.disabled_at.is_none());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn operator_validation_and_seed_protection_fail_closed(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let store = Store::new(db.pool.clone());
    let invalid = store
        .operators
        .create(NewOperatorUser {
            tenant_id: Uuid::new_v4(),
            username: "bad user".to_owned(),
            password_hash: "not-a-hash".to_owned(),
            display_name: None,
            role: "root".to_owned(),
        })
        .await;
    assert!(matches!(invalid, Err(StoreError::Invariant(_))));
    let seed = store
        .operators
        .set_disabled(
            Uuid::parse_str("00000000-0000-0000-0000-000000000001")?,
            Uuid::parse_str("00000000-0000-0000-0000-000000000001")?,
            false,
        )
        .await;
    assert!(matches!(seed, Err(StoreError::NotFound)));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn binding_status_is_in_place_and_soft() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let store = Store::new(db.pool.clone());
    let binding = store
        .external_bindings
        .create(NewExternalTenantBinding {
            provider: "nexus".to_owned(),
            external_tenant_id: "tenant-1".to_owned(),
            external_business_id: "business-1".to_owned(),
            hydra_tenant_id: Uuid::new_v4(),
        })
        .await?;
    let revoked = store
        .external_bindings
        .set_status(binding.id, BindingStatus::Revoked)
        .await?;
    assert_eq!(revoked.status, BindingStatus::Revoked);
    assert!(store
        .external_bindings
        .resolve("nexus", "tenant-1", "business-1")
        .await?
        .is_some());
    db.cleanup().await?;
    Ok(())
}
