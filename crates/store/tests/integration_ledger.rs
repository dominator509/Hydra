use store::{LedgerRow, Store, TestDb};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn regress_ledger_route_ratio_filters_window_and_route(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        let base = OffsetDateTime::UNIX_EPOCH;

        for row in [
            ledger_row(base - Duration::hours(2), tenant, "concierge", 10, 90, 1),
            ledger_row(base + Duration::minutes(1), tenant, "concierge", 970, 30, 2),
            ledger_row(base + Duration::minutes(2), tenant, "concierge", 980, 20, 3),
            ledger_row(base + Duration::minutes(3), tenant, "other", 50, 50, 4),
        ] {
            store.ledger.record(&row).await?;
        }

        let ratio = store
            .ledger
            .route_ratio("concierge", base)
            .await?
            .expect("recent concierge rows should yield a ratio");
        assert!(ratio > 0.97 && ratio < 0.98, "unexpected ratio: {ratio}");

        let other_ratio = store
            .ledger
            .route_ratio("other", base)
            .await?
            .expect("other route rows should yield a ratio");
        assert_eq!(other_ratio, 0.5);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn regress_ledger_month_to_date_spend_sums_per_tenant(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let base = OffsetDateTime::UNIX_EPOCH;

        store
            .ledger
            .record(&ledger_row(base, tenant_a, "concierge", 10, 10, 11))
            .await?;
        store
            .ledger
            .record(&ledger_row(
                base + Duration::minutes(1),
                tenant_a,
                "concierge",
                20,
                10,
                17,
            ))
            .await?;
        store
            .ledger
            .record(&ledger_row(
                base + Duration::minutes(2),
                tenant_b,
                "concierge",
                30,
                10,
                23,
            ))
            .await?;

        let spend = store.ledger.month_to_date_cents(tenant_a, base).await?;
        assert_eq!(spend, 28);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

fn ledger_row(
    ts: OffsetDateTime,
    tenant_id: Uuid,
    route: &str,
    hit_tokens: i32,
    miss_tokens: i32,
    cost_cents: i32,
) -> LedgerRow {
    LedgerRow {
        ts,
        tenant_id,
        route: route.to_owned(),
        provider: "deepseek".to_owned(),
        prefix_sha: "prefix-1".to_owned(),
        hit_tokens,
        miss_tokens,
        out_tokens: 128,
        out_bytes: 512,
        aborted: false,
        cost_cents,
    }
}
