use sqlx::Row;
use store::{RateLimitsRepo, StoreError, TestDb};

#[tokio::test]
async fn fixed_window_is_atomic_and_isolates_digests() -> Result<(), StoreError> {
    let db = TestDb::new().await?;
    let repo = RateLimitsRepo::new(db.pool.clone());
    let raw_first = "principal:tenant-a:user-a";
    let raw_second = "network:192.0.2.10";
    let first = "a".repeat(64);
    let second = "b".repeat(64);

    let first_decision = repo.check(&first, 2, 60).await?;
    let second_decision = repo.check(&first, 2, 60).await?;
    let third_decision = repo.check(&first, 2, 60).await?;
    let isolated_decision = repo.check(&second, 2, 60).await?;

    assert!(first_decision.allowed);
    assert!(second_decision.allowed);
    assert!(!third_decision.allowed);
    assert!(third_decision.retry_after_secs >= 1);
    assert!(isolated_decision.allowed);

    let stored = sqlx::query("SELECT key_digest FROM rate_limit_window")
        .fetch_all(&db.pool)
        .await?;
    let stored_keys = stored
        .iter()
        .map(|row| row.get::<String, _>("key_digest"))
        .collect::<Vec<_>>();
    assert!(stored_keys.iter().all(|key| key.len() == 64));
    assert!(!stored_keys.iter().any(|key| key == raw_first));
    assert!(!stored_keys.iter().any(|key| key == raw_second));

    db.cleanup().await
}

#[tokio::test]
async fn concurrent_requests_do_not_exceed_the_window_limit() -> Result<(), StoreError> {
    let db = TestDb::new().await?;
    let repo = RateLimitsRepo::new(db.pool.clone());
    let digest = "c".repeat(64);

    let (one, two, three, four) = tokio::join!(
        repo.check(&digest, 2, 60),
        repo.check(&digest, 2, 60),
        repo.check(&digest, 2, 60),
        repo.check(&digest, 2, 60),
    );
    let decisions = [one?, two?, three?, four?];
    assert_eq!(
        decisions.iter().filter(|decision| decision.allowed).count(),
        2
    );

    db.cleanup().await
}

#[tokio::test]
async fn expired_windows_reset_and_pruning_is_bounded() -> Result<(), StoreError> {
    let db = TestDb::new().await?;
    let repo = RateLimitsRepo::new(db.pool.clone());
    let digest = "d".repeat(64);

    assert!(repo.check(&digest, 1, 60).await?.allowed);
    sqlx::query("UPDATE rate_limit_window SET window_started_at = now() - interval '181 seconds'")
        .execute(&db.pool)
        .await?;
    assert!(repo.check(&digest, 1, 60).await?.allowed);
    sqlx::query("UPDATE rate_limit_window SET window_started_at = now() - interval '181 seconds'")
        .execute(&db.pool)
        .await?;
    assert_eq!(repo.prune_expired(120).await?, 1);

    assert!(matches!(
        repo.check("not-a-digest", 1, 60).await,
        Err(StoreError::Invariant(_))
    ));
    assert!(matches!(
        repo.check(&digest, 0, 60).await,
        Err(StoreError::Invariant(_))
    ));
    assert!(matches!(
        repo.prune_expired(0).await,
        Err(StoreError::Invariant(_))
    ));

    db.cleanup().await
}
