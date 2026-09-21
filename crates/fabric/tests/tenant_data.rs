use std::sync::Arc;

use cdm::Entity;
use fabric::services::{
    demo_governor, AppState, ConciergeServiceImpl, StoreAutonomyService, StoreBridgeService,
    StoreEntityService, StoreEnvelopeService, StoreTenantDataService, StoreTkStatsService,
};
use fabric::{app, SessionStore};
use reqwest::StatusCode;
use serde_json::json;
use store::{Store, TenantDataExport, TestDb};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn tenant_data_routes_are_admin_only_tenant_scoped_and_bounded(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        store
            .entities
            .upsert(
                tenant,
                Entity {
                    id: Uuid::new_v4(),
                    kind: "deal".to_owned(),
                    tenant,
                    body: json!({"title": "Tenant A"}),
                    origin: "native".to_owned(),
                    origin_ref: None,
                    version: 1,
                },
            )
            .await?;
        store
            .entities
            .upsert(
                other_tenant,
                Entity {
                    id: Uuid::new_v4(),
                    kind: "deal".to_owned(),
                    tenant: other_tenant,
                    body: json!({"title": "Tenant B"}),
                    origin: "native".to_owned(),
                    origin_ref: None,
                    version: 1,
                },
            )
            .await?;

        let admin_id = Uuid::new_v4();
        let admin_token = "tenant-data-admin-token";
        sqlx::query(
            "INSERT INTO hydra_user (id, tenant_id, username, password_hash) VALUES ($1, $2, $3, $4)",
        )
        .bind(admin_id)
        .bind(tenant)
        .bind("tenant-data-admin")
        .bind("$argon2id$v=19$m=19456,t=2,p=1$dGVzdHNhbHRmb3JkZXZzZWVk$nw4dFA2YRXmBPZqFRqNZT8YOcDxVHIKGKEfnKFg5m9M")
        .execute(&db.pool)
        .await?;
        sqlx::query("INSERT INTO hydra_role (user_id, tenant_id, role) VALUES ($1, $2, 'admin')")
            .bind(admin_id)
            .bind(tenant)
            .execute(&db.pool)
            .await?;
        sqlx::query(
            "INSERT INTO hydra_session (user_id, tenant_id, token, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(admin_id)
        .bind(tenant)
        .bind(admin_token)
        .bind(OffsetDateTime::now_utc() + Duration::hours(1))
        .execute(&db.pool)
        .await?;

        let state = AppState::new(
            Arc::new(SessionStore::new(db.pool.clone())),
            Arc::new(StoreEntityService::new(store.clone())),
            Arc::new(StoreAutonomyService::new(store.clone())),
            Arc::new(StoreBridgeService::new(store.clone(), demo_governor())),
            Arc::new(StoreEnvelopeService::new(store.clone(), demo_governor())),
            Arc::new(StoreTkStatsService::new(store.ledger.clone(), Vec::new())),
            Arc::new(ConciergeServiceImpl),
        )
        .with_tenant_data(Arc::new(StoreTenantDataService::new(store)));
        let addr = spawn_app(app(state)).await?;
        let tenant_header = tenant.to_string();
        let admin = reqwest::Client::builder()
            .default_headers(session_headers(&tenant_header, admin_token)?)
            .build()?;

        let export = admin
            .get(format!("http://{addr}/v1/tenant/export?max_records=10"))
            .send()
            .await?;
        assert_eq!(export.status(), StatusCode::OK);
        let export = export.json::<TenantDataExport>().await?;
        assert_eq!(export.tenant_id, tenant);
        assert!(!export.truncated);
        assert_eq!(export.entities.len(), 1);
        assert_eq!(export.entities[0].body["title"], "Tenant A");

        let preview = admin
            .get(format!("http://{addr}/v1/tenant/retention-preview?age_days=30"))
            .send()
            .await?;
        assert_eq!(preview.status(), StatusCode::OK);
        let preview = preview.json::<store::RetentionPreview>().await?;
        assert_eq!(preview.tenant_id, tenant);

        let mismatch_client = reqwest::Client::builder()
            .default_headers(bearer_headers(admin_token)?)
            .build()?;
        let mismatch = mismatch_client
            .get("http://".to_owned() + &addr.to_string() + "/v1/tenant/export")
            .header("x-hydra-tenant", other_tenant.to_string())
            .send()
            .await?;
        assert_eq!(mismatch.status(), StatusCode::FORBIDDEN);

        let invalid = admin
            .get(format!("http://{addr}/v1/tenant/export?max_records=0"))
            .send()
            .await?;
        assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let anonymous = reqwest::Client::new()
            .get(format!("http://{addr}/v1/tenant/export"))
            .header("x-hydra-tenant", &tenant_header)
            .send()
            .await?;
        assert_eq!(anonymous.status(), StatusCode::FORBIDDEN);

        let viewer_id = Uuid::new_v4();
        let viewer_token = "tenant-data-viewer-token";
        sqlx::query(
            "INSERT INTO hydra_user (id, tenant_id, username, password_hash) VALUES ($1, $2, $3, $4)",
        )
        .bind(viewer_id)
        .bind(tenant)
        .bind("tenant-data-viewer")
        .bind("$argon2id$v=19$m=19456,t=2,p=1$dGVzdHNhbHRmb3JkZXZzZWVk$nw4dFA2YRXmBPZqFRqNZT8YOcDxVHIKGKEfnKFg5m9M")
        .execute(&db.pool)
        .await?;
        sqlx::query("INSERT INTO hydra_role (user_id, tenant_id, role) VALUES ($1, $2, 'viewer')")
            .bind(viewer_id)
            .bind(tenant)
            .execute(&db.pool)
            .await?;
        sqlx::query(
            "INSERT INTO hydra_session (user_id, tenant_id, token, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(viewer_id)
        .bind(tenant)
        .bind(viewer_token)
        .bind(OffsetDateTime::now_utc() + Duration::hours(1))
        .execute(&db.pool)
        .await?;
        let viewer = reqwest::Client::builder()
            .default_headers(session_headers(&tenant_header, viewer_token)?)
            .build()?;
        let viewer_response = viewer
            .get(format!("http://{addr}/v1/tenant/export"))
            .send()
            .await?;
        assert_eq!(viewer_response.status(), StatusCode::FORBIDDEN);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

fn bearer_headers(
    token: &str,
) -> Result<reqwest::header::HeaderMap, reqwest::header::InvalidHeaderValue> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::try_from(format!("Bearer {token}"))?,
    );
    Ok(headers)
}

fn session_headers(
    tenant: &str,
    token: &str,
) -> Result<reqwest::header::HeaderMap, reqwest::header::InvalidHeaderValue> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::try_from(format!("Bearer {token}"))?,
    );
    headers.insert(
        "x-hydra-tenant",
        reqwest::header::HeaderValue::try_from(tenant)?,
    );
    Ok(headers)
}

async fn spawn_app(
    router: axum::Router,
) -> Result<std::net::SocketAddr, Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("tenant data test server should stay alive");
    });
    Ok(addr)
}
