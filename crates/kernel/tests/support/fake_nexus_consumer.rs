use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use async_nats::jetstream;
use futures_util::StreamExt;
use jsonschema::{Draft, JSONSchema};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumedEvent {
    pub event_id: Uuid,
    pub event_type: cdm::HydraEventType,
    pub duplicate: bool,
    pub hydra_tenant_id: Uuid,
    pub external_binding_id: Option<Uuid>,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub envelope_id: Option<Uuid>,
}

#[derive(Default)]
struct ProjectionState {
    seen: BTreeSet<Uuid>,
    events: Vec<cdm::HydraEventEnvelope>,
}

#[derive(Clone, Default)]
pub struct FakeNexusProjection {
    state: Arc<Mutex<ProjectionState>>,
}

impl FakeNexusProjection {
    async fn record(&self, event: cdm::HydraEventEnvelope) -> bool {
        let mut state = self.state.lock().await;
        if !state.seen.insert(event.event_id) {
            return false;
        }
        state.events.push(event);
        true
    }

    pub async fn event_ids(&self) -> Vec<Uuid> {
        self.state
            .lock()
            .await
            .events
            .iter()
            .map(|event| event.event_id)
            .collect()
    }

    pub async fn correlations(&self) -> Vec<Option<String>> {
        self.state
            .lock()
            .await
            .events
            .iter()
            .map(|event| event.correlation_id.clone())
            .collect()
    }
}

pub struct FakeNexusConsumer {
    consumer: jetstream::consumer::PullConsumer,
    projection: FakeNexusProjection,
}

impl FakeNexusConsumer {
    pub async fn connect(
        context: &jetstream::Context,
        stream_name: &str,
        durable_name: &str,
        filter_subject: &str,
        projection: FakeNexusProjection,
    ) -> Result<Self, FakeNexusConsumerError> {
        let stream = context
            .get_stream(stream_name)
            .await
            .map_err(broker_error)?;
        let consumer = stream
            .get_or_create_consumer(
                durable_name,
                jetstream::consumer::pull::Config {
                    durable_name: Some(durable_name.to_owned()),
                    description: Some("deterministic fake Nexus v1 consumer".to_owned()),
                    deliver_policy: jetstream::consumer::DeliverPolicy::New,
                    ack_policy: jetstream::consumer::AckPolicy::Explicit,
                    ack_wait: Duration::from_secs(5),
                    max_deliver: 5,
                    filter_subject: filter_subject.to_owned(),
                    ..Default::default()
                },
            )
            .await
            .map_err(broker_error)?;
        Ok(Self {
            consumer,
            projection,
        })
    }

    pub async fn consume_one(
        &self,
        timeout: Duration,
    ) -> Result<ConsumedEvent, FakeNexusConsumerError> {
        let mut messages = self
            .consumer
            .fetch()
            .max_messages(1)
            .expires(timeout)
            .messages()
            .await
            .map_err(broker_error)?;
        let message = messages
            .next()
            .await
            .ok_or(FakeNexusConsumerError::Timeout)?
            .map_err(broker_error)?;
        let document: serde_json::Value =
            serde_json::from_slice(&message.payload).map_err(contract_error)?;
        let schema_document = cdm::hydra_event_v1_schema();
        let schema = JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&schema_document)
            .map_err(contract_error)?;
        if !schema.is_valid(&document) {
            return Err(FakeNexusConsumerError::Contract(
                "canonical event failed v1 JSON Schema validation".to_owned(),
            ));
        }
        let event: cdm::HydraEventEnvelope =
            serde_json::from_value(document).map_err(contract_error)?;
        event.validate().map_err(contract_error)?;
        if message.subject.as_str() != event.subject {
            return Err(FakeNexusConsumerError::Contract(
                "message subject does not match canonical event".to_owned(),
            ));
        }

        let accepted = self.projection.record(event.clone()).await;
        message.ack().await.map_err(broker_error)?;
        Ok(ConsumedEvent {
            event_id: event.event_id,
            event_type: event.event_type,
            duplicate: !accepted,
            hydra_tenant_id: event.hydra_tenant_id,
            external_binding_id: event.external_binding_id,
            correlation_id: event.correlation_id,
            causation_id: event.causation_id,
            envelope_id: event.envelope_id,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FakeNexusConsumerError {
    #[error("fake Nexus consumer broker operation failed: {0}")]
    Broker(String),
    #[error("fake Nexus consumer contract validation failed: {0}")]
    Contract(String),
    #[error("fake Nexus consumer timed out waiting for an event")]
    Timeout,
}

fn broker_error(error: impl std::fmt::Display) -> FakeNexusConsumerError {
    FakeNexusConsumerError::Broker(error.to_string())
}

fn contract_error(error: impl std::fmt::Display) -> FakeNexusConsumerError {
    FakeNexusConsumerError::Contract(error.to_string())
}
