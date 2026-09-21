use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use store::{OutboxFailureDisposition, OutboxRepo};
use tokio::sync::watch;
use tracing::warn;

use crate::event_stream::{EventPublishRequest, EventPublisher};

const OUTBOX_BATCH_SIZE: i64 = 100;
const OUTBOX_LEASE_TIMEOUT: Duration = Duration::from_secs(30);
const RELAY_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelayIteration {
    pub claimed: usize,
    pub published: usize,
    pub retried: usize,
    pub parked: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RelayHealth {
    running: Arc<AtomicBool>,
    operational: Arc<AtomicBool>,
    parked: Arc<AtomicBool>,
    parked_state_known: Arc<AtomicBool>,
}

impl RelayHealth {
    pub fn running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn operational(&self) -> bool {
        self.operational.load(Ordering::Acquire)
    }

    pub fn parked(&self) -> bool {
        self.parked.load(Ordering::Acquire)
    }

    pub fn parked_state_known(&self) -> bool {
        self.parked_state_known.load(Ordering::Acquire)
    }

    fn set_parked_state(&self, parked: bool) {
        self.parked.store(parked, Ordering::Release);
        self.parked_state_known.store(true, Ordering::Release);
    }

    fn mark_parked_state_unknown(&self) {
        self.parked_state_known.store(false, Ordering::Release);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("outbox persistence failed: {0}")]
    Store(#[from] store::StoreError),
}

pub async fn run(
    shutdown: watch::Receiver<bool>,
    outbox: OutboxRepo,
    publisher: Arc<dyn EventPublisher>,
) {
    run_with_health(shutdown, outbox, publisher, RelayHealth::default()).await;
}

pub async fn run_with_health(
    mut shutdown: watch::Receiver<bool>,
    outbox: OutboxRepo,
    publisher: Arc<dyn EventPublisher>,
    health: RelayHealth,
) {
    health.running.store(true, Ordering::Release);
    refresh_parked_state(&outbox, &health).await;
    let mut interval = tokio::time::interval(RELAY_POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            _ = interval.tick() => {
                match publish_once(
                    &outbox,
                    publisher.as_ref(),
                    OUTBOX_BATCH_SIZE,
                    OUTBOX_LEASE_TIMEOUT,
                ).await {
                    Ok(iteration) if iteration.retried > 0 || iteration.parked > 0 => {
                        if iteration.parked > 0 {
                            health.set_parked_state(true);
                        } else if !health.parked_state_known() {
                            refresh_parked_state(&outbox, &health).await;
                        }
                        health.operational.store(true, Ordering::Release);
                        warn!(
                            retried = iteration.retried,
                            parked = iteration.parked,
                            "outbox relay completed with deferred events"
                        );
                    }
                    Ok(_) => {
                        if !health.parked_state_known() {
                            refresh_parked_state(&outbox, &health).await;
                        }
                        health.operational.store(true, Ordering::Release);
                    }
                    Err(error) => {
                        health.operational.store(false, Ordering::Release);
                        warn!(error = %error, "outbox relay iteration failed");
                    }
                }
            }
        }
    }
    health.running.store(false, Ordering::Release);
    health.operational.store(false, Ordering::Release);
}

async fn refresh_parked_state(outbox: &OutboxRepo, health: &RelayHealth) {
    match outbox.parked_count().await {
        Ok(count) => health.set_parked_state(count > 0),
        Err(error) => {
            health.mark_parked_state_unknown();
            warn!(error = %error, "outbox parked-state check failed");
        }
    }
}

pub async fn publish_once(
    outbox: &OutboxRepo,
    publisher: &dyn EventPublisher,
    batch_size: i64,
    lease_timeout: Duration,
) -> Result<RelayIteration, RelayError> {
    let batch = outbox.claim_pending(batch_size, lease_timeout).await?;
    let mut iteration = RelayIteration {
        claimed: batch.records.len(),
        parked: batch.parked_count,
        ..RelayIteration::default()
    };

    for record in batch.records {
        let claim_token = record
            .claim_token
            .ok_or(store::StoreError::OutboxClaimLost)?;
        let payload = match serde_json::to_vec(&record.event) {
            Ok(payload) => payload,
            Err(_) => {
                outbox
                    .record_failure(
                        record.id,
                        claim_token,
                        "canonical_event_serialization_failed",
                        OutboxFailureDisposition::Park,
                    )
                    .await?;
                iteration.parked += 1;
                continue;
            }
        };
        let request = EventPublishRequest {
            event_id: record.event_id,
            subject: record.subject,
            payload,
            trace_context: record.trace_context,
        };
        match publisher.publish(request).await {
            Ok(acknowledgement) => {
                outbox
                    .mark_published(record.id, claim_token, acknowledgement.sequence)
                    .await?;
                iteration.published += 1;
            }
            Err(error) => {
                outbox
                    .record_failure(
                        record.id,
                        claim_token,
                        "jetstream_publish_failed",
                        OutboxFailureDisposition::Retry,
                    )
                    .await?;
                iteration.retried += 1;
                warn!(event_id = %record.event_id, error = %error, "JetStream publish deferred");
            }
        }
    }

    Ok(iteration)
}
