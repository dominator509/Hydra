use std::time::Duration;

use async_nats::jetstream;

pub use super::fake_nexus_consumer::{ConsumedEvent, FakeNexusProjection};
use super::fake_nexus_consumer::{FakeNexusConsumer, FakeNexusConsumerError};

pub struct FakeEventConsumer {
    context: jetstream::Context,
    stream_name: String,
    durable_name: String,
    projection: FakeNexusProjection,
    inner: FakeNexusConsumer,
}

impl FakeEventConsumer {
    pub async fn connect(
        context: jetstream::Context,
        stream_name: impl Into<String>,
        filter_subject: &str,
    ) -> Result<Self, FakeNexusConsumerError> {
        let stream_name = stream_name.into();
        let durable_name = format!("fake_nexus_e2e_{}", uuid::Uuid::new_v4().simple());
        let projection = FakeNexusProjection::default();
        let inner = FakeNexusConsumer::connect(
            &context,
            &stream_name,
            &durable_name,
            filter_subject,
            projection.clone(),
        )
        .await?;
        Ok(Self {
            context,
            stream_name,
            durable_name,
            projection,
            inner,
        })
    }

    pub async fn consume_one(
        &self,
        timeout: Duration,
    ) -> Result<ConsumedEvent, FakeNexusConsumerError> {
        self.inner.consume_one(timeout).await
    }

    pub fn projection(&self) -> FakeNexusProjection {
        self.projection.clone()
    }

    pub async fn shutdown(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        drop(self.inner);
        self.context
            .get_stream(&self.stream_name)
            .await?
            .delete_consumer(&self.durable_name)
            .await?;
        Ok(())
    }
}
