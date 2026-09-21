use fabric::auth::{password, SessionStore};
use sqlx::Row;
use store::TestDb;
use time::OffsetDateTime;
use uuid::Uuid;

const SEED_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

#[tokio::test]
async fn development_seed_is_disabled_and_existing_sessions_fail_closed(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let seed_id = Uuid::parse_str(SEED_USER_ID)?;
        let row = sqlx::query(
            "SELECT auth_source, disabled_at, password_hash FROM hydra_user WHERE id = $1",
        )
        .bind(seed_id)
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(row.get::<String, _>("auth_source"), "development_seed");
        assert!(row
            .get::<Option<OffsetDateTime>, _>("disabled_at")
            .is_some());
        let production_store = SessionStore::new(db.pool.clone());
        assert!(production_store
            .authenticate("admin", "hydra-dev")
            .await
            .is_err());

        let seed_token = "historical-seed-session";
        sqlx::query(
            "INSERT INTO hydra_session (user_id, tenant_id, token, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(seed_id)
        .bind(seed_id)
        .bind(seed_token)
        .bind(OffsetDateTime::now_utc() + time::Duration::hours(1))
        .execute(&db.pool)
        .await?;
        assert!(production_store.lookup(seed_token).await?.is_none());

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn active_operator_credentials_remain_usable() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let password_hash = password::hash_password("active-test-password")?;
        sqlx::query(
            "INSERT INTO hydra_user (id, tenant_id, username, password_hash) VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind("active-operator")
        .bind(password_hash)
        .execute(&db.pool)
        .await?;
        sqlx::query("INSERT INTO hydra_role (user_id, tenant_id, role) VALUES ($1, $2, 'operator')")
            .bind(user_id)
            .bind(tenant_id)
            .execute(&db.pool)
            .await?;

        let session = SessionStore::new(db.pool.clone())
            .authenticate("active-operator", "active-test-password")
            .await?;
        assert_eq!(session.user_id, user_id);
        assert_eq!(session.tenant_id, tenant_id);
        assert!(session
            .roles
            .iter()
            .any(|role| role.as_str() == "operator"));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}
