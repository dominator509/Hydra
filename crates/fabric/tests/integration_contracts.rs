use std::net::SocketAddr;
use std::sync::Arc;

use fabric::app;
use fabric::mcp;
use fabric::services::{
    demo_governor, AppState, BlastRadiusDto, EnvelopeCreateRequest, StoreEnvelopeService,
    StoreTkStatsService,
};
use serde_json::json;
use store::{Store, TestDb};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn contract_openapi_envelope_flow_and_mcp_schema() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
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
        assert!(openapi["paths"]["/v1/envelopes"].is_object());
        assert!(openapi["paths"]["/v1/tk/ledger"].is_object());

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
