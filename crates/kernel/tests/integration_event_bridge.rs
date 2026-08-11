use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use cdm::Entity;
use hydra_kernel::event_stream::{
    EventPublishAck, EventPublishRequest, EventPublisher, EventStreamConfig, EventStreamError,
    JetStreamEventPublisher,
};
use hydra_kernel::relay::publish_once;
use serde_json::json;
use store::{Store, TestDb};
use tokio::sync::Mutex;
use uuid::Uuid;

#[path = "support/fake_nexus_consumer.rs"]
mod fake_nexus_consumer;

use fake_nexus_consumer::{FakeNexusConsumer, FakeNexusProjection};

struct FakePublisher {
    failures_remaining: AtomicUsize,
    requests: Mutex<Vec<EventPublishRequest>>,
}

impl FakePublisher {
    fn new(failures: usize) -> Self {
        Self {
            failures_remaining: AtomicUsize::new(failures),
            requests: Mutex::new(Vec::new()),
        }
    }

    async fn event_ids(&self) -> Vec<Uuid> {
        self.requests
            .lock()
            .await
            .iter()
            .map(|request| request.event_id)
            .collect()
    }
}

#[async_trait]
impl EventPublisher for FakePublisher {
    async fn publish(
        &self,
        request: EventPublishRequest,
    ) -> Result<EventPublishAck, EventStreamError> {
        self.requests.lock().await.push(request);
        if self.failures_remaining.load(Ordering::SeqCst) > 0 {
            self.failures_remaining.fetch_sub(1, Ordering::SeqCst);
            return Err(EventStreamError::Publish);
        }
        Ok(EventPublishAck {
            stream: "FAKE_HYDRA_EVENTS".to_owned(),
            sequence: 41,
            duplicate: false,
        })
    }
}

#[tokio::test]
async fn event_relay_publish_failure_leaves_outbox_pending(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let event_id = create_event(&store, Uuid::new_v4()).await?;
        let publisher = FakePublisher::new(1);

        let iteration =
            publish_once(&store.outbox, &publisher, 100, Duration::from_secs(30)).await?;
        assert_eq!(iteration.claimed, 1);
        assert_eq!(iteration.published, 0);
        assert_eq!(iteration.retried, 1);

        let record = store.outbox.get_by_event_id(event_id).await?;
        assert!(record.published_at.is_none());
        assert!(record.jetstream_sequence.is_none());
        assert_eq!(record.attempt_count, 1);
        assert_eq!(
            record.last_error.as_deref(),
            Some("jetstream_publish_failed")
        );
        assert!(record.claim_token.is_none());
        assert!(record.claimed_at.is_none());

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn event_relay_acknowledgement_marks_outbox_published(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let event_id = create_event(&store, Uuid::new_v4()).await?;
        let publisher = FakePublisher::new(0);

        let iteration =
            publish_once(&store.outbox, &publisher, 100, Duration::from_secs(30)).await?;
        assert_eq!(iteration.published, 1);
        assert_eq!(iteration.retried, 0);

        let record = store.outbox.get_by_event_id(event_id).await?;
        assert!(record.published_at.is_some());
        assert_eq!(record.jetstream_sequence, Some(41));
        assert_eq!(record.attempt_count, 1);
        assert!(record.last_error.is_none());

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn event_relay_retry_reuses_one_logical_event_id() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let event_id = create_event(&store, Uuid::new_v4()).await?;
        let publisher = FakePublisher::new(1);

        let first = publish_once(&store.outbox, &publisher, 100, Duration::from_secs(30)).await?;
        let second = publish_once(&store.outbox, &publisher, 100, Duration::from_secs(30)).await?;
        assert_eq!(first.retried, 1);
        assert_eq!(second.published, 1);
        assert_eq!(publisher.event_ids().await, vec![event_id, event_id]);

        let record = store.outbox.get_by_event_id(event_id).await?;
        assert!(record.published_at.is_some());
        assert_eq!(record.attempt_count, 2);
        let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox WHERE event_id = $1")
            .bind(event_id)
            .fetch_one(&db.pool)
            .await?;
        assert_eq!(row_count, 1);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn event_relay_parks_invalid_canonical_rows_without_publishing(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let event_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO outbox (event_id, subject, event)
            VALUES ($1, 'hydra.crm.entity.created.v1', '{"not":"canonical"}'::jsonb)
            "#,
        )
        .bind(event_id)
        .execute(&db.pool)
        .await?;
        let publisher = FakePublisher::new(0);

        let iteration =
            publish_once(&store.outbox, &publisher, 100, Duration::from_secs(30)).await?;
        assert_eq!(iteration.claimed, 0);
        assert_eq!(iteration.parked, 1);
        assert!(publisher.event_ids().await.is_empty());

        let row: (Option<time::OffsetDateTime>, Option<String>, i32) = sqlx::query_as(
            "SELECT parked_at, last_error, attempt_count FROM outbox WHERE event_id = $1",
        )
        .bind(event_id)
        .fetch_one(&db.pool)
        .await?;
        assert!(row.0.is_some());
        assert_eq!(row.1.as_deref(), Some("canonical_event_invalid"));
        assert_eq!(row.2, 1);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn event_relay_real_jetstream_ack_deduplicates_by_event_id(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".to_owned());
    let client = async_nats::connect(&nats_url).await?;

    let result = async {
        let publisher =
            JetStreamEventPublisher::bootstrap(client, EventStreamConfig::nexus_v1()).await?;
        let store = Store::new(db.pool.clone());
        let event_id = create_event(&store, Uuid::new_v4()).await?;
        let record = store.outbox.get_by_event_id(event_id).await?;
        let request = EventPublishRequest {
            event_id,
            subject: record.subject.clone(),
            payload: serde_json::to_vec(&record.event)?,
            trace_context: record.trace_context.clone(),
        };

        let first = publisher.publish(request.clone()).await?;
        let repeated = publisher.publish(request).await?;
        assert!(!first.duplicate);
        assert!(repeated.duplicate);
        assert_eq!(first.sequence, repeated.sequence);

        let iteration =
            publish_once(&store.outbox, &publisher, 100, Duration::from_secs(30)).await?;
        assert_eq!(iteration.published, 1);
        let stored = store.outbox.get_by_event_id(event_id).await?;
        assert_eq!(
            stored.jetstream_sequence,
            i64::try_from(first.sequence).ok()
        );

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn integration_event_bridge_fake_nexus_consumer_deduplicates_and_resumes(
) -> Result<(), Box<dyn std::error::Error>> {
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".to_owned());
    let client = async_nats::connect(&nats_url).await?;
    let publisher =
        JetStreamEventPublisher::bootstrap(client.clone(), EventStreamConfig::nexus_v1()).await?;
    let context = async_nats::jetstream::new(client);
    let durable_name = format!("fake_nexus_{}", Uuid::new_v4().simple());
    let projection = FakeNexusProjection::default();
    let event_one = fake_nexus_event(
        Uuid::new_v4(),
        "correlation-consumer-1",
        "causation-consumer-1",
    );
    let event_two = fake_nexus_event(
        Uuid::new_v4(),
        "correlation-consumer-2",
        "causation-consumer-2",
    );
    let result = async {
        let consumer = FakeNexusConsumer::connect(
            &context,
            publisher.stream_name(),
            &durable_name,
            &event_one.subject,
            projection.clone(),
        )
        .await?;

        publish_raw_event(&context, &event_one).await?;
        publish_raw_event(&context, &event_one).await?;
        let first = consumer.consume_one(Duration::from_secs(5)).await?;
        let duplicate = consumer.consume_one(Duration::from_secs(5)).await?;
        assert!(!first.duplicate);
        assert!(duplicate.duplicate);
        assert_eq!(first.event_id, duplicate.event_id);
        assert_eq!(
            first.correlation_id.as_deref(),
            Some("correlation-consumer-1")
        );
        assert_eq!(first.causation_id.as_deref(), Some("causation-consumer-1"));
        drop(consumer);

        publish_raw_event(&context, &event_two).await?;
        let restarted = FakeNexusConsumer::connect(
            &context,
            publisher.stream_name(),
            &durable_name,
            &event_two.subject,
            projection.clone(),
        )
        .await?;
        let resumed = restarted.consume_one(Duration::from_secs(5)).await?;
        assert_eq!(resumed.event_id, event_two.event_id);
        assert!(!resumed.duplicate);
        assert_eq!(
            projection.event_ids().await,
            vec![event_one.event_id, event_two.event_id]
        );
        assert_eq!(
            projection.correlations().await,
            vec![
                Some("correlation-consumer-1".to_owned()),
                Some("correlation-consumer-2".to_owned())
            ]
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    let stream = context.get_stream(publisher.stream_name()).await?;
    let cleanup = stream.delete_consumer(&durable_name).await;
    result?;
    cleanup?;
    Ok(())
}

async fn publish_raw_event(
    context: &async_nats::jetstream::Context,
    event: &cdm::HydraEventEnvelope,
) -> Result<(), Box<dyn std::error::Error>> {
    context
        .publish(event.subject.clone(), serde_json::to_vec(event)?.into())
        .await?
        .await?;
    Ok(())
}

fn fake_nexus_event(
    event_id: Uuid,
    correlation_id: &str,
    causation_id: &str,
) -> cdm::HydraEventEnvelope {
    let mut event = cdm::HydraEventEnvelope::new(
        event_id,
        cdm::HydraEventType::EntityCreated,
        "2026-08-11T00:00:00Z".to_owned(),
        Uuid::new_v4(),
        cdm::EventActorRef {
            actor_id: "nexus-consumer-fixture".to_owned(),
            actor_type: cdm::EventActorType::HydraSystem,
        },
        cdm::EventDataClass::Private,
        cdm::HydraEventPayload::EntityChange {
            operation: "created".to_owned(),
            version: 1,
        },
    );
    event.correlation_id = Some(correlation_id.to_owned());
    event.causation_id = Some(causation_id.to_owned());
    event.entity = Some(cdm::EventEntityRef {
        entity_id: Uuid::new_v4(),
        kind: "deal".to_owned(),
        origin: "native".to_owned(),
        origin_ref: None,
    });
    event
}

async fn create_event(store: &Store, tenant: Uuid) -> Result<Uuid, store::StoreError> {
    let entity = Entity {
        id: Uuid::new_v4(),
        kind: "party".to_owned(),
        tenant,
        body: json!({ "display_name": "Relay fixture" }),
        origin: "native".to_owned(),
        origin_ref: None,
        version: 1,
    };
    store.entities.upsert(tenant, entity).await?;
    sqlx::query_scalar("SELECT event_id FROM outbox WHERE published_at IS NULL ORDER BY id LIMIT 1")
        .fetch_one(&store.pool)
        .await
        .map_err(store::StoreError::from)
}
