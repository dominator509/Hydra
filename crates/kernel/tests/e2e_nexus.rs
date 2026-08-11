mod support;

use std::time::Duration;

use cdm::HydraEventType;
use fabric::mcp::MCP_PROTOCOL_VERSION;
use governor::EnvelopeState;
use serde_json::{json, Value};
use support::fake_mcp_client::FakeMcpClient;
use support::fake_nexus::{FakeNexusHarness, HarnessError};
use uuid::Uuid;

#[tokio::test]
async fn fake_nexus_harness_authenticates_distinct_principals_and_caches_jwks(
) -> Result<(), HarnessError> {
    let harness = FakeNexusHarness::start().await?;
    let result = async {
        let capabilities = harness
            .service_client()
            .get_json("/v1/nexus/capabilities", "component-service")
            .await?;
        assert!(capabilities["capabilities"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));

        let initialized = harness.agent_mcp_client().initialize().await?;
        assert_eq!(initialized["protocolVersion"], MCP_PROTOCOL_VERSION);

        let binding = harness
            .human_client()
            .get_json("/v1/nexus/bindings", "component-human")
            .await?;
        assert_eq!(
            binding["bindings"][0]["id"],
            Value::String(harness.binding_id.to_string())
        );

        let other_context = harness
            .other_business_client()
            .get_json("/v1/nexus/context", "component-other-business")
            .await?;
        assert_eq!(
            other_context["tenant"]["hydra_tenant_id"],
            Value::String(harness.other_hydra_tenant_id.to_string())
        );
        assert_eq!(harness.jwks_requests(), 1, "JWKS must be cached");
        Ok::<(), HarnessError>(())
    }
    .await;

    let cleanup = harness.shutdown().await;
    result?;
    cleanup
}

#[tokio::test]
async fn e2e_nexus_round_trip() -> Result<(), HarnessError> {
    let harness = FakeNexusHarness::start().await?;
    let consumer = harness
        .event_consumer(HydraEventType::EnvelopeExecuted.as_str())
        .await?;
    let result = async {
        let capabilities = harness
            .service_client()
            .get_json("/v1/nexus/capabilities", &harness.correlation_id)
            .await?;
        let proposal_capability = capabilities["capabilities"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["name"] == "hydra.crm.propose_action")
            })
            .ok_or("proposal capability missing")?;
        assert_eq!(proposal_capability["available"], true);

        let context = harness
            .service_client()
            .get_json("/v1/nexus/context", &harness.correlation_id)
            .await?;
        assert_eq!(
            context["tenant"]["hydra_tenant_id"],
            Value::String(harness.hydra_tenant_id.to_string())
        );
        assert_eq!(
            context["tenant"]["external_business_id"],
            harness.external_business_id
        );
        assert_eq!(
            context["tenant"]["external_tenant_id"],
            harness.external_tenant_id
        );

        let mcp = harness.agent_mcp_client();
        let initialized = mcp.initialize().await?;
        assert_eq!(initialized["protocolVersion"], MCP_PROTOCOL_VERSION);
        let search = mcp
            .call_tool(
                "hydra.crm.search",
                json!({"query": "Hydra Nexus E2E", "kind": "deal", "limit": 10}),
                &harness.correlation_id,
                Some(&harness.causation_id),
            )
            .await?;
        let search_content =
            FakeMcpClient::structured_content(&search).ok_or("search content missing")?;
        let found = search_content["items"]
            .as_array()
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item["id"].as_str() == Some(harness.deal.id.to_string().as_str())
                })
            });
        assert!(found, "MCP search must honor the supplied query");

        let idempotency_key = format!("stage-change-{}", Uuid::new_v4());
        let proposal_arguments = json!({
            "deal_id": harness.deal.id,
            "stage": "won",
            "rationale": "Fake Nexus completed the qualification objective",
            "idempotency_key": idempotency_key,
            "objective_id": harness.objective_id,
            "task_id": harness.task_id
        });
        let proposal = mcp
            .call_tool(
                "hydra.crm.propose_action",
                proposal_arguments.clone(),
                &harness.correlation_id,
                Some(&harness.causation_id),
            )
            .await?;
        let proposal_content =
            FakeMcpClient::structured_content(&proposal).ok_or("proposal content missing")?;
        assert_eq!(proposal_content["state"], "pending_approval");
        let envelope_id = proposal_content["envelope_id"]
            .as_str()
            .ok_or("proposal envelope ID missing")?
            .parse::<Uuid>()?;
        let queued = harness
            .store
            .envelopes
            .get(harness.hydra_tenant_id, envelope_id)
            .await?;
        assert_eq!(queued.state, EnvelopeState::PendingApproval);
        assert_eq!(
            queued.invocation.objective_id.as_deref(),
            Some(harness.objective_id.as_str())
        );
        assert_eq!(
            queued.invocation.task_id.as_deref(),
            Some(harness.task_id.as_str())
        );

        let approval = harness
            .human_client()
            .post_json(
                &format!("/v1/nexus/envelopes/{envelope_id}/approval"),
                &json!({
                    "decision": "approve",
                    "comment": "Distinct human reviewed the objective and CRM impact"
                }),
                &harness.correlation_id,
                Some(&harness.causation_id),
            )
            .await?;
        assert_eq!(approval["state"], "Approved");
        let executed = wait_for_state(&harness, envelope_id, EnvelopeState::Executed).await?;
        assert_eq!(
            executed.invocation.correlation_id.as_deref(),
            Some(harness.correlation_id.as_str())
        );
        assert_eq!(
            executed.invocation.causation_id.as_deref(),
            Some(harness.causation_id.as_str())
        );

        let updated = harness
            .store
            .entities
            .get(harness.hydra_tenant_id, harness.deal.id)
            .await?;
        assert_eq!(updated.body["stage_id"], "won");
        let receipt = harness
            .store
            .execution_receipts
            .get_for_envelope(harness.hydra_tenant_id, envelope_id)
            .await?;
        assert_eq!(receipt.outcome, store::ExecutionOutcome::Verified);

        let event = consumer.consume_one(Duration::from_secs(10)).await?;
        assert_eq!(event.event_type, HydraEventType::EnvelopeExecuted);
        assert_eq!(event.envelope_id, Some(envelope_id));
        assert_eq!(event.hydra_tenant_id, harness.hydra_tenant_id);
        assert_eq!(event.external_binding_id, Some(harness.binding_id));
        assert_eq!(
            event.correlation_id.as_deref(),
            Some(harness.correlation_id.as_str())
        );
        assert_eq!(
            event.causation_id.as_deref(),
            Some(harness.causation_id.as_str())
        );
        let outbox = wait_for_outbox_publish(&harness, event.event_id).await?;
        assert!(outbox.published_at.is_some());
        assert!(outbox.jetstream_sequence.is_some());
        assert_eq!(consumer.projection().event_ids().await, vec![event.event_id]);
        assert_eq!(
            consumer.projection().correlations().await,
            vec![Some(harness.correlation_id.clone())]
        );

        let repeated = mcp
            .call_tool(
                "hydra.crm.propose_action",
                proposal_arguments,
                &harness.correlation_id,
                Some(&harness.causation_id),
            )
            .await?;
        let repeated_content =
            FakeMcpClient::structured_content(&repeated).ok_or("retry content missing")?;
        assert_eq!(
            repeated_content["envelope_id"],
            Value::String(envelope_id.to_string())
        );
        assert_eq!(repeated_content["state"], "executed");

        let envelope_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM envelope WHERE tenant_id = $1 AND id = $2",
        )
        .bind(harness.hydra_tenant_id)
        .bind(envelope_id)
        .fetch_one(harness.store.events.pool())
        .await?;
        let receipt_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_receipt WHERE tenant_id = $1 AND envelope_id = $2",
        )
        .bind(harness.hydra_tenant_id)
        .bind(envelope_id)
        .fetch_one(harness.store.events.pool())
        .await?;
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_log WHERE tenant_id = $1 AND kind = $2 AND payload->>'envelope_id' = $3",
        )
        .bind(harness.hydra_tenant_id)
        .bind(HydraEventType::EnvelopeExecuted.as_str())
        .bind(envelope_id.to_string())
        .fetch_one(harness.store.events.pool())
        .await?;
        assert_eq!(envelope_count, 1);
        assert_eq!(receipt_count, 1);
        assert_eq!(audit_count, 1);
        Ok::<(), HarnessError>(())
    }
    .await;

    let consumer_cleanup = consumer.shutdown().await;
    let harness_cleanup = harness.shutdown().await;
    result?;
    consumer_cleanup?;
    harness_cleanup
}

#[tokio::test]
async fn e2e_cross_business_blocked() -> Result<(), HarnessError> {
    let harness = FakeNexusHarness::start().await?;
    let result = async {
        let context = harness
            .other_business_client()
            .get_json("/v1/nexus/context", "cross-business-context")
            .await?;
        assert_eq!(
            context["tenant"]["hydra_tenant_id"],
            Value::String(harness.other_hydra_tenant_id.to_string())
        );
        assert_eq!(
            context["tenant"]["external_business_id"],
            harness.other_external_business_id
        );

        let mcp = harness.other_business_mcp_client();
        mcp.initialize().await?;
        let denied = mcp
            .call_tool(
                "hydra.crm.get",
                json!({"kind": "deal", "entity_id": harness.deal.id}),
                "cross-business-read",
                None,
            )
            .await?;
        assert_eq!(denied["isError"], true);
        let serialized = denied.to_string();
        assert!(!serialized.contains("Hydra Nexus E2E Renewal"));
        assert!(!serialized.contains(&harness.hydra_tenant_id.to_string()));
        assert!(!serialized.contains(&harness.external_business_id));
        Ok::<(), HarnessError>(())
    }
    .await;

    let cleanup = harness.shutdown().await;
    result?;
    cleanup
}

async fn wait_for_state(
    harness: &FakeNexusHarness,
    envelope_id: Uuid,
    expected: EnvelopeState,
) -> Result<governor::ActionEnvelope, HarnessError> {
    for _ in 0..500 {
        let envelope = harness
            .store
            .envelopes
            .get(harness.hydra_tenant_id, envelope_id)
            .await?;
        if envelope.state == expected {
            return Ok(envelope);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("envelope {envelope_id} did not reach {expected:?}").into())
}

async fn wait_for_outbox_publish(
    harness: &FakeNexusHarness,
    event_id: Uuid,
) -> Result<store::OutboxRecord, HarnessError> {
    for _ in 0..500 {
        let record = harness.store.outbox.get_by_event_id(event_id).await?;
        if record.published_at.is_some() && record.jetstream_sequence.is_some() {
            return Ok(record);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("outbox event {event_id} was not marked published").into())
}
