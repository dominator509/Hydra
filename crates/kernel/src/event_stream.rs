use std::time::Duration;

use async_nats::jetstream;
use async_trait::async_trait;
use uuid::Uuid;

pub const HYDRA_EVENT_STREAM_NAME: &str = "HYDRA_CRM_EVENTS_V1";
pub const HYDRA_EVENT_SUBJECTS: &str = "hydra.crm.>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventStreamConfig {
    pub name: String,
    pub subjects: Vec<String>,
    pub max_age: Duration,
    pub max_messages: i64,
    pub max_bytes: i64,
    pub max_message_size: i32,
    pub duplicate_window: Duration,
    pub storage: jetstream::stream::StorageType,
}

impl EventStreamConfig {
    pub fn nexus_v1() -> Self {
        Self {
            name: HYDRA_EVENT_STREAM_NAME.to_owned(),
            subjects: vec![HYDRA_EVENT_SUBJECTS.to_owned()],
            max_age: Duration::from_secs(30 * 24 * 60 * 60),
            max_messages: 1_000_000,
            max_bytes: 10 * 1024 * 1024 * 1024,
            max_message_size: 1024 * 1024,
            duplicate_window: Duration::from_secs(24 * 60 * 60),
            storage: jetstream::stream::StorageType::File,
        }
    }

    fn as_nats_config(&self) -> jetstream::stream::Config {
        jetstream::stream::Config {
            name: self.name.clone(),
            description: Some("Hydra canonical CRM interoperability events v1".to_owned()),
            subjects: self.subjects.clone(),
            retention: jetstream::stream::RetentionPolicy::Limits,
            discard: jetstream::stream::DiscardPolicy::Old,
            max_age: self.max_age,
            max_messages: self.max_messages,
            max_bytes: self.max_bytes,
            max_message_size: self.max_message_size,
            duplicate_window: self.duplicate_window,
            storage: self.storage,
            num_replicas: 1,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventPublishRequest {
    pub event_id: Uuid,
    pub subject: String,
    pub payload: Vec<u8>,
    pub trace_context: Option<store::TraceContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventPublishAck {
    pub stream: String,
    pub sequence: u64,
    pub duplicate: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum EventStreamError {
    #[error("JetStream stream bootstrap failed: {0}")]
    Bootstrap(String),
    #[error("JetStream stream configuration mismatch: {0}")]
    Configuration(&'static str),
    #[error("JetStream publish request failed")]
    Publish,
    #[error("JetStream publish acknowledgement failed")]
    Acknowledgement,
    #[error("JetStream acknowledgement did not match the configured stream")]
    WrongStream,
    #[error("JetStream acknowledgement sequence was zero")]
    InvalidSequence,
    #[error("event trace context is invalid")]
    InvalidTraceContext,
}

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(
        &self,
        request: EventPublishRequest,
    ) -> Result<EventPublishAck, EventStreamError>;
}

#[derive(Clone)]
pub struct JetStreamEventPublisher {
    context: jetstream::Context,
    stream_name: String,
    requested_config: jetstream::stream::Config,
}

impl JetStreamEventPublisher {
    pub async fn bootstrap(
        client: async_nats::Client,
        config: EventStreamConfig,
    ) -> Result<Self, EventStreamError> {
        let context = jetstream::new(client);
        let requested = config.as_nats_config();
        let stream = context
            .get_or_create_stream(requested.clone())
            .await
            .map_err(|error| EventStreamError::Bootstrap(error.to_string()))?;
        verify_stream_config(&requested, &stream.cached_info().config)?;
        Ok(Self {
            context,
            stream_name: requested.name.clone(),
            requested_config: requested,
        })
    }

    pub fn stream_name(&self) -> &str {
        &self.stream_name
    }

    pub async fn check_health(&self) -> Result<(), EventStreamError> {
        let stream = self
            .context
            .get_stream(&self.stream_name)
            .await
            .map_err(|error| EventStreamError::Bootstrap(error.to_string()))?;
        verify_stream_config(&self.requested_config, &stream.cached_info().config)
    }
}

#[async_trait]
impl EventPublisher for JetStreamEventPublisher {
    async fn publish(
        &self,
        request: EventPublishRequest,
    ) -> Result<EventPublishAck, EventStreamError> {
        let mut publish = jetstream::message::PublishMessage::build()
            .payload(request.payload.into())
            .message_id(request.event_id.to_string());
        if let Some(trace_context) = request.trace_context {
            trace_context
                .validate()
                .map_err(|_| EventStreamError::InvalidTraceContext)?;
            publish = publish.header("traceparent", trace_context.traceparent);
            if let Some(tracestate) = trace_context.tracestate {
                publish = publish.header("tracestate", tracestate);
            }
        }
        let acknowledgement = self
            .context
            .send_publish(request.subject, publish)
            .await
            .map_err(|_| EventStreamError::Publish)?
            .await
            .map_err(|_| EventStreamError::Acknowledgement)?;
        if acknowledgement.stream != self.stream_name {
            return Err(EventStreamError::WrongStream);
        }
        if acknowledgement.sequence == 0 {
            return Err(EventStreamError::InvalidSequence);
        }
        Ok(EventPublishAck {
            stream: acknowledgement.stream,
            sequence: acknowledgement.sequence,
            duplicate: acknowledgement.duplicate,
        })
    }
}

fn verify_stream_config(
    requested: &jetstream::stream::Config,
    actual: &jetstream::stream::Config,
) -> Result<(), EventStreamError> {
    let checks = [
        (requested.name == actual.name, "name"),
        (requested.subjects == actual.subjects, "subjects"),
        (requested.retention == actual.retention, "retention"),
        (requested.discard == actual.discard, "discard"),
        (requested.max_age == actual.max_age, "max_age"),
        (
            requested.max_messages == actual.max_messages,
            "max_messages",
        ),
        (requested.max_bytes == actual.max_bytes, "max_bytes"),
        (
            requested.max_message_size == actual.max_message_size,
            "max_message_size",
        ),
        (
            requested.duplicate_window == actual.duplicate_window,
            "duplicate_window",
        ),
        (requested.storage == actual.storage, "storage"),
        (
            requested.num_replicas == actual.num_replicas,
            "num_replicas",
        ),
        (!actual.no_ack, "publish_acknowledgements"),
    ];
    for (matches, field) in checks {
        if !matches {
            return Err(EventStreamError::Configuration(field));
        }
    }
    Ok(())
}
