use std::collections::HashSet;

use cdm::{Entity, EventActorType, HydraEventEnvelope, HydraEventPayload, HydraEventType};
use governor::{ActionEnvelope, BlastRadius, Clock, EnvelopeState, InvocationContext, Reversal};
use serde_json::{json, Value};
use store::{IdempotencyResolution, NewIdempotencyRecord, Store, StoreError, TestDb};
use time::OffsetDateTime;
use uuid::Uuid;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }
}

#[tokio::test]
async fn interoperability_events_preserve_bridge_origin_and_stable_outbox_identity(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let mut entity = bridge_party(tenant, 1, "Ada Lovelace");

        store.entities.upsert(tenant, entity.clone()).await?;
        entity.version = 2;
        entity.body = json!({ "display_name": "Ada L." });
        store.entities.upsert(tenant, entity.clone()).await?;
        store.entities.soft_delete(tenant, entity.id).await?;

        let documents: Vec<Value> = sqlx::query_scalar(
            r#"
            SELECT payload
            FROM event_log
            WHERE tenant_id = $1
              AND kind LIKE 'hydra.crm.entity.%'
            ORDER BY seq
            "#,
        )
        .bind(tenant)
        .fetch_all(&db.pool)
        .await?;
        let events = documents
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<HydraEventEnvelope>, _>>()?;

        assert_eq!(events.len(), 3);
        assert_eq!(
            events
                .iter()
                .map(|event| event.event_type)
                .collect::<Vec<_>>(),
            vec![
                HydraEventType::EntityCreated,
                HydraEventType::EntityUpdated,
                HydraEventType::EntityDeleted,
            ]
        );
        assert_eq!(events[0].actor.actor_type, EventActorType::Bridge);
        for event in &events {
            event.validate()?;
            let entity_ref = event.entity.as_ref().expect("entity event reference");
            assert_eq!(entity_ref.entity_id, entity.id);
            assert_eq!(entity_ref.origin, "bridge:fixture-crm");
            assert_eq!(entity_ref.origin_ref.as_deref(), Some("contact-42"));
            assert_eq!(event.subject, event.event_type.as_str());
            assert!(!event.subject.contains(&tenant.to_string()));

            let first = store.outbox.get_by_event_id(event.event_id).await?;
            let repeated = store.outbox.get_by_event_id(event.event_id).await?;
            assert_eq!(first.event_id, repeated.event_id);
            assert_eq!(first.event, *event);
            assert_eq!(first.subject, event.subject);
        }

        let event_ids = events
            .iter()
            .map(|event| event.event_id)
            .collect::<HashSet<_>>();
        assert_eq!(event_ids.len(), events.len());

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn interoperability_events_preserve_envelope_binding_and_correlation(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let deal = Entity {
            id: Uuid::new_v4(),
            kind: "deal".to_owned(),
            tenant,
            body: json!({ "title": "Nexus renewal", "stage_id": "discovery" }),
            origin: "native".to_owned(),
            origin_ref: None,
            version: 1,
        };
        store.entities.upsert(tenant, deal.clone()).await?;

        let mut envelope = nexus_envelope(tenant, binding_id, deal.id);
        envelope.transition(EnvelopeState::PendingApproval, "governor", &FixedClock)?;
        let resolution = store
            .idempotency
            .resolve_or_create_envelope(
                NewIdempotencyRecord {
                    tenant_id: tenant,
                    origin_system: "nexus".to_owned(),
                    idempotency_key: "stage-change-event-001".to_owned(),
                    capability: "hydra.crm.propose_action".to_owned(),
                    request_hash: "a".repeat(64),
                    envelope_id: envelope.id,
                },
                &envelope,
            )
            .await?;
        assert!(matches!(resolution, IdempotencyResolution::Recorded(_)));

        let documents: Vec<Value> = sqlx::query_scalar(
            r#"
            SELECT payload
            FROM event_log
            WHERE tenant_id = $1
              AND kind LIKE 'hydra.crm.envelope.%'
              AND payload->>'envelope_id' = $2
            ORDER BY seq
            "#,
        )
        .bind(tenant)
        .bind(envelope.id.to_string())
        .fetch_all(&db.pool)
        .await?;
        let events = documents
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<HydraEventEnvelope>, _>>()?;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, HydraEventType::EnvelopeProposed);
        assert_eq!(events[1].event_type, HydraEventType::EnvelopeQueued);
        for event in &events {
            assert_eq!(event.external_binding_id, Some(binding_id));
            assert_eq!(event.correlation_id.as_deref(), Some("corr-event-001"));
            assert_eq!(event.causation_id.as_deref(), Some("cause-event-001"));
            assert_eq!(event.envelope_id, Some(envelope.id));
            assert_eq!(
                event.entity.as_ref().map(|reference| reference.entity_id),
                Some(deal.id)
            );
            assert!(matches!(
                event.payload,
                HydraEventPayload::EnvelopeTransition { .. }
            ));
            assert_eq!(
                store.outbox.get_by_event_id(event.event_id).await?.event,
                *event
            );
        }
        assert_eq!(events[0].actor.actor_type, EventActorType::NexusAgent);
        assert_eq!(events[1].actor.actor_type, EventActorType::HydraSystem);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn interoperability_events_roll_back_mutation_when_outbox_append_fails(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        sqlx::query(
            r#"
            CREATE FUNCTION reject_test_outbox_insert()
            RETURNS trigger
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RAISE EXCEPTION 'forced canonical outbox failure';
            END;
            $$
            "#,
        )
        .execute(&db.pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TRIGGER reject_test_outbox_insert
            BEFORE INSERT ON outbox
            FOR EACH ROW EXECUTE FUNCTION reject_test_outbox_insert()
            "#,
        )
        .execute(&db.pool)
        .await?;

        let tenant = Uuid::new_v4();
        let entity = bridge_party(tenant, 1, "Grace Hopper");
        let store = Store::new(db.pool.clone());
        let error = store
            .entities
            .upsert(tenant, entity.clone())
            .await
            .expect_err("outbox failure must abort the producer transaction");
        assert!(matches!(error, StoreError::Database(_)));

        let entity_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entity WHERE tenant_id = $1 AND id = $2")
                .bind(tenant)
                .bind(entity.id)
                .fetch_one(&db.pool)
                .await?;
        let event_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM event_log WHERE tenant_id = $1")
                .bind(tenant)
                .fetch_one(&db.pool)
                .await?;
        assert_eq!(entity_count, 0);
        assert_eq!(event_count, 0);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

fn bridge_party(tenant: Uuid, version: u64, display_name: &str) -> Entity {
    Entity {
        id: Uuid::new_v4(),
        kind: "party".to_owned(),
        tenant,
        body: json!({ "display_name": display_name }),
        origin: "bridge:fixture-crm".to_owned(),
        origin_ref: Some("contact-42".to_owned()),
        version,
    }
}

fn nexus_envelope(tenant: Uuid, binding_id: Uuid, target: Uuid) -> ActionEnvelope {
    ActionEnvelope {
        id: Uuid::new_v4(),
        tenant,
        domain: "pipeline".to_owned(),
        action: "move_stage".to_owned(),
        kind: Some("deal".to_owned()),
        targets: vec![target],
        payload: json!({ "stage": "qualified" }),
        rationale: "Nexus requested a governed stage change".to_owned(),
        reversal: Reversal::Compensating,
        blast: BlastRadius::default(),
        invocation: InvocationContext {
            request_id: Some("req-event-001".to_owned()),
            correlation_id: Some("corr-event-001".to_owned()),
            causation_id: Some("cause-event-001".to_owned()),
            origin_system: Some("nexus".to_owned()),
            external_actor_id: Some("nexus-agent-event".to_owned()),
            external_actor_type: Some("nexus_agent".to_owned()),
            external_binding_id: Some(binding_id),
            objective_id: Some("objective-event-001".to_owned()),
            task_id: Some("task-event-001".to_owned()),
            approval_id: None,
            idempotency_key: Some("stage-change-event-001".to_owned()),
        },
        state: EnvelopeState::Proposed,
        history: Vec::new(),
    }
}
