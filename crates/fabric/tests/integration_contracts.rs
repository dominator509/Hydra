use std::net::SocketAddr;
use std::sync::Arc;

use fabric::app;
use fabric::mcp;
use fabric::services::{
    demo_governor, AppState, AutonomyCellDto, BlastRadiusDto, EnvelopeCreateRequest,
    StoreAutonomyService, StoreEntityService, StoreEnvelopeService, StoreTkStatsService,
};
use serde_json::json;
use sqlx::types::Json;
use store::{Store, TestDb};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn contract_openapi_envelope_flow_and_mcp_schema() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        std::env::set_var("HYDRA_ENV", "dev");
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        store
            .ledger
            .record(&store::LedgerRow {
                ts: OffsetDateTime::now_utc() - Duration::minutes(5),
                tenant_id: tenant,
                route: "concierge".into(),
                provider: "deepseek".into(),
                prefix_sha: "abc".into(),
                hit_tokens: 97,
                miss_tokens: 3,
                out_tokens: 12,
                out_bytes: 64,
                aborted: false,
                cost_cents: 1,
            })
            .await?;

        let state = AppState::new(
            Arc::new(StoreEntityService::new(store.clone())),
            Arc::new(StoreAutonomyService::new(store.clone())),
            Arc::new(StoreEnvelopeService::new(store.clone(), demo_governor())),
            Arc::new(StoreTkStatsService::new(
                store.ledger.clone(),
                vec!["concierge".into()],
            )),
        );
        let addr = spawn_app(app(state)).await?;
        let client = reqwest::Client::new();
        let tenant_header = tenant.to_string();

        let openapi = client
            .get(format!("http://{addr}/v1/openapi.json"))
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        assert_eq!(openapi["info"]["version"], "1.0.0");
        assert!(openapi["paths"]["/v1/autonomy/cells"].is_object());
        assert!(openapi["paths"]["/v1/entities/{kind}"].is_object());
        assert!(openapi["paths"]["/v1/entities/{kind}/{id}"].is_object());
        assert!(openapi["paths"]["/v1/envelopes"].is_object());
        assert!(openapi["paths"]["/v1/tk/ledger"].is_object());

        let empty_cells = client
            .get(format!("http://{addr}/v1/autonomy/cells"))
            .header("x-hydra-tenant", &tenant_header)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<AutonomyCellDto>>()
            .await?;
        assert!(empty_cells.is_empty());

        let forbidden = client
            .put(format!("http://{addr}/v1/autonomy/cells"))
            .header("x-hydra-tenant", &tenant_header)
            .json(&vec![AutonomyCellDto {
                domain: "pipeline".into(),
                action: "move_stage".into(),
                kind: Some("deal".into()),
                level: "L4".into(),
                cfg: json!({ "batch_max": 10 }),
            }])
            .send()
            .await?;
        assert_eq!(forbidden.status(), reqwest::StatusCode::FORBIDDEN);
        let forbidden = forbidden.json::<fabric::ProblemJson>().await?;
        assert_eq!(forbidden.code, "authz_denied");

        let mut autonomy = client
            .put(format!("http://{addr}/v1/autonomy/cells"))
            .header("x-hydra-tenant", &tenant_header)
            .header("Authorization", "Bearer hydra-dev-admin")
            .json(&vec![
                AutonomyCellDto {
                    domain: "pipeline".into(),
                    action: "move_stage".into(),
                    kind: Some("deal".into()),
                    level: "L4".into(),
                    cfg: json!({ "batch_max": 10 }),
                },
                AutonomyCellDto {
                    domain: "bridges".into(),
                    action: "deploy_adapter".into(),
                    kind: None,
                    level: "L2".into(),
                    cfg: json!({}),
                },
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<AutonomyCellDto>>()
            .await?;
        let mut expected_autonomy = vec![
            AutonomyCellDto {
                domain: "pipeline".into(),
                action: "move_stage".into(),
                kind: Some("deal".into()),
                level: "L4".into(),
                cfg: json!({ "batch_max": 10 }),
            },
            AutonomyCellDto {
                domain: "bridges".into(),
                action: "deploy_adapter".into(),
                kind: None,
                level: "L2".into(),
                cfg: json!({}),
            },
        ];
        sort_autonomy_cells(&mut autonomy);
        sort_autonomy_cells(&mut expected_autonomy);
        assert_eq!(autonomy, expected_autonomy);

        let mut listed_cells = client
            .get(format!("http://{addr}/v1/autonomy/cells"))
            .header("x-hydra-tenant", &tenant_header)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<AutonomyCellDto>>()
            .await?;
        sort_autonomy_cells(&mut listed_cells);
        assert_eq!(listed_cells, autonomy);

        let autonomy_event = sqlx::query!(
            r#"
            SELECT
                kind,
                actor,
                payload as "payload!: Json<serde_json::Value>"
            FROM event_log
            WHERE tenant_id = $1
            ORDER BY seq DESC
            LIMIT 1
            "#,
            tenant
        )
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(autonomy_event.kind, "autonomy.cells.updated");
        assert_eq!(autonomy_event.actor, "dev-admin");
        assert_eq!(
            autonomy_event
                .payload
                .0
                .get("cells")
                .and_then(serde_json::Value::as_array)
                .map(|cells| cells.len()),
            Some(2)
        );

        let created = client
            .post(format!("http://{addr}/v1/entities/party"))
            .header("x-hydra-tenant", &tenant_header)
            .json(&json!({ "display_name": "Ada Lovelace" }))
            .send()
            .await?
            .error_for_status()?
            .json::<cdm::Entity>()
            .await?;
        assert_eq!(created.kind, "party");
        assert_eq!(created.version, 1);
        assert_eq!(created.body["display_name"], "Ada Lovelace");

        let listed = client
            .get(format!("http://{addr}/v1/entities/party?limit=5"))
            .header("x-hydra-tenant", &tenant_header)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<cdm::Entity>>()
            .await?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);

        let fetched = client
            .get(format!("http://{addr}/v1/entities/party/{}", created.id))
            .header("x-hydra-tenant", &tenant_header)
            .send()
            .await?
            .error_for_status()?
            .json::<cdm::Entity>()
            .await?;
        assert_eq!(fetched.id, created.id);

        let conflict = client
            .patch(format!("http://{addr}/v1/entities/party/{}", created.id))
            .header("x-hydra-tenant", &tenant_header)
            .header("If-Match", "0")
            .json(&json!({ "email": "wrong@example.com" }))
            .send()
            .await?;
        assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);
        let conflict = conflict.json::<fabric::ProblemJson>().await?;
        assert_eq!(conflict.code, "version_conflict");

        let patched = client
            .patch(format!("http://{addr}/v1/entities/party/{}", created.id))
            .header("x-hydra-tenant", &tenant_header)
            .header("If-Match", "1")
            .json(&json!({
                "email": "ada@example.com",
                "profile": { "timezone": "UTC" }
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<cdm::Entity>()
            .await?;
        assert_eq!(patched.version, 2);
        assert_eq!(patched.body["display_name"], "Ada Lovelace");
        assert_eq!(patched.body["email"], "ada@example.com");
        assert_eq!(patched.body["profile"]["timezone"], "UTC");

        let deleted = client
            .delete(format!("http://{addr}/v1/entities/party/{}", created.id))
            .header("x-hydra-tenant", &tenant_header)
            .send()
            .await?
            .error_for_status()?
            .json::<fabric::EntityDeleteResponse>()
            .await?;
        assert!(deleted.deleted);
        assert_eq!(deleted.version, 3);

        let missing = client
            .get(format!("http://{addr}/v1/entities/party/{}", created.id))
            .header("x-hydra-tenant", &tenant_header)
            .send()
            .await?;
        assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
        let missing = missing.json::<fabric::ProblemJson>().await?;
        assert_eq!(missing.code, "not_found");

        let proposed = client
            .post(format!("http://{addr}/v1/envelopes"))
            .header("x-hydra-tenant", &tenant_header)
            .json(&EnvelopeCreateRequest {
                domain: "bridges".into(),
                action: "deploy_adapter".into(),
                kind: None,
                targets: vec![Uuid::new_v4()],
                payload: json!({ "adapter": "memcrm", "grant": "dev" }),
                rationale: "contract test".into(),
                reversal: governor::Reversal::Compensating,
                blast: BlastRadiusDto {
                    entities: 1,
                    external_sends: 0,
                    money_cents: 0,
                    pii_egress: false,
                },
            })
            .send()
            .await?
            .error_for_status()?
            .json::<governor::ActionEnvelope>()
            .await?;
        assert_eq!(proposed.state, governor::EnvelopeState::PendingApproval);

        let pending = client
            .get(format!("http://{addr}/v1/envelopes?state=PendingApproval"))
            .header("x-hydra-tenant", &tenant_header)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<governor::ActionEnvelope>>()
            .await?;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, proposed.id);

        let approved = client
            .post(format!(
                "http://{addr}/v1/envelopes/{}/approve",
                proposed.id
            ))
            .header("x-hydra-tenant", &tenant_header)
            .send()
            .await?
            .error_for_status()?
            .json::<governor::ActionEnvelope>()
            .await?;
        assert_eq!(approved.state, governor::EnvelopeState::Approved);

        let tk = client
            .get(format!("http://{addr}/v1/tk/ledger?window=1h"))
            .send()
            .await?
            .error_for_status()?
            .json::<fabric::TkWindowStats>()
            .await?;
        assert_eq!(tk.window, "1h");
        assert_eq!(tk.routes.len(), 1);
        assert_eq!(tk.routes[0].route, "concierge");
        assert!(
            tk.routes[0]
                .hit_ratio
                .expect("seeded route ratio should be present")
                > 0.95
        );

        let mcp = mcp::tool_schema();
        let tools = mcp["tools"]
            .as_array()
            .expect("mcp tool schema should expose a tool list");
        assert_eq!(tools.len(), 7);
        assert_eq!(tools[0]["name"], "hydra.search_entities");
        assert_eq!(tools[6]["name"], "hydra.tk_stats");

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

async fn spawn_app(router: axum::Router) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("fabric integration server should stay alive for the test");
    });
    Ok(addr)
}

fn sort_autonomy_cells(cells: &mut [AutonomyCellDto]) {
    cells.sort_by(|left, right| {
        (
            left.domain.as_str(),
            left.action.as_str(),
            left.kind.as_deref().unwrap_or(""),
        )
            .cmp(&(
                right.domain.as_str(),
                right.action.as_str(),
                right.kind.as_deref().unwrap_or(""),
            ))
    });
}
