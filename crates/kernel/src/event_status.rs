use async_trait::async_trait;

use crate::event_stream::JetStreamEventPublisher;
use crate::relay::RelayHealth;

#[derive(Clone)]
pub struct EventRuntimeStatusService {
    publisher: JetStreamEventPublisher,
    relay: RelayHealth,
}

impl EventRuntimeStatusService {
    pub fn new(publisher: JetStreamEventPublisher, relay: RelayHealth) -> Self {
        Self { publisher, relay }
    }
}

#[async_trait]
impl fabric::EventStatusService for EventRuntimeStatusService {
    async fn status(&self) -> Result<fabric::EventInfrastructureStatus, fabric::FabricError> {
        let stream_available = self.publisher.check_health().await.is_ok();
        let relay_operational = self.relay.running() && self.relay.operational();
        let available = stream_available && relay_operational;
        let reason = if !stream_available {
            Some("canonical_event_stream_unavailable".to_owned())
        } else if !relay_operational {
            Some("canonical_event_relay_unavailable".to_owned())
        } else {
            None
        };

        Ok(fabric::EventInfrastructureStatus {
            available,
            contract_version: Some(cdm::HYDRA_EVENT_SPEC_VERSION.to_owned()),
            stream: Some(self.publisher.stream_name().to_owned()),
            jetstream_acknowledged_publish: stream_available,
            relay_operational,
            reason,
        })
    }
}

pub async fn required_event_infrastructure_ready(
    required: bool,
    status: &dyn fabric::EventStatusService,
) -> bool {
    if !required {
        return true;
    }
    status.status().await.is_ok_and(|status| status.available)
}
