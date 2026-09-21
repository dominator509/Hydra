use sqlx::Row;
use store::{SessionRepo, TestDb};
use time::OffsetDateTime;
use uuid::Uuid;

#[tokio::test]
async fn session_tokens_are_hashed_and_legacy_rows_upgrade_on_lookup(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let username = format!("session-user-{}", Uuid::new_v4());
        sqlx::query(
            r#"INSERT INTO hydra_user
               (id, tenant_id, username, password_hash, auth_source)
               VALUES ($1, $2, $3, $4, 'operator')"#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(&username)
        .bind("unused-test-hash")
        .execute(&db.pool)
        .await?;

        let repo = SessionRepo::new(db.pool.clone());
        let token = "new-session-token";
        repo.create_session(
            user_id,
            tenant_id,
            token,
            OffsetDateTime::now_utc() + time::Duration::hours(1),
        )
        .await?;

        let row = sqlx::query(
            "SELECT token, token_hash FROM hydra_session WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&db.pool)
        .await?;
        let plaintext: Option<String> = row.get("token");
        let stored_hash: Option<String> = row.get("token_hash");
        assert!(plaintext.is_none(), "new bearer tokens must not be stored");
        let stored_hash = stored_hash.expect("new session hash");
        assert_eq!(stored_hash.len(), 64);
        assert!(stored_hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(stored_hash, token);

        let session = repo
            .find_active_session(token)
            .await?
            .expect("new hashed session should resolve");
        assert_eq!(session.token, token);

        let legacy = "legacy-session-token";
        sqlx::query(
            r#"INSERT INTO hydra_session (user_id, tenant_id, token, expires_at)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(legacy)
        .bind(OffsetDateTime::now_utc() + time::Duration::hours(1))
        .execute(&db.pool)
        .await?;

        repo.find_active_session(legacy)
            .await?
            .expect("legacy session should resolve once");
        let legacy_row = sqlx::query(
            "SELECT token, token_hash FROM hydra_session WHERE user_id = $1 AND token_hash IS NOT NULL ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_one(&db.pool)
        .await?;
        let legacy_plaintext: Option<String> = legacy_row.get("token");
        let legacy_hash: Option<String> = legacy_row.get("token_hash");
        assert!(legacy_plaintext.is_none());
        assert!(legacy_hash.is_some());

        repo.revoke(token).await?;
        repo.revoke(legacy).await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM hydra_session WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(count, 0);
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}
