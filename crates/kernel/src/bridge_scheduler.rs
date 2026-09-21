use std::sync::Arc;
use std::time::Duration;

use fabric::{ScheduledBridgeSyncProposal, StoreEnvelopeService};
use time::OffsetDateTime;
use tokio::sync::watch;
use tracing::{error, warn};
use uuid::Uuid;

use crate::supervisor::TaskHealth;

const DEFAULT_BATCH_SIZE: i64 = 16;
const DEFAULT_LEASE_SECONDS: i64 = 120;
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Durable, opt-in scheduler for governed bridge synchronization proposals.
pub struct BridgeSyncScheduler {
    store: store::Store,
    envelopes: Arc<StoreEnvelopeService>,
    enabled: bool,
    health: TaskHealth,
    poll_interval: Duration,
    lease_seconds: i64,
    batch_size: i64,
}

impl BridgeSyncScheduler {
    pub fn new(
        store: store::Store,
        envelopes: Arc<StoreEnvelopeService>,
        enabled: bool,
        health: TaskHealth,
    ) -> Self {
        Self {
            store,
            envelopes,
            enabled,
            health,
            poll_interval: DEFAULT_POLL_INTERVAL,
            lease_seconds: DEFAULT_LEASE_SECONDS,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let _health_guard = self.health.start();
        let mut interval = tokio::time::interval(self.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return;
                    }
                }
                _ = interval.tick() => {
                    if !self.enabled {
                        continue;
                    }
                    if let Err(error) = self.run_once(OffsetDateTime::now_utc()).await {
                        record_metric("poll_failed");
                        warn!(error = %error, "bridge sync scheduler poll failed");
                    }
                }
            }
        }
    }

    async fn run_once(&self, now: OffsetDateTime) -> Result<(), SchedulerError> {
        let claims = self
            .store
            .bridge_schedules
            .claim_due(now, self.lease_seconds, self.batch_size)
            .await?;

        for claim in claims {
            record_metric("claimed");
            let schedule = &claim.schedule;
            let slot_key = slot_key(schedule.id, claim.slot_due_at);
            let correlation_id = format!("hydra.scheduler/{slot_key}");
            let proposal = ScheduledBridgeSyncProposal {
                tenant_id: schedule.tenant_id,
                schedule_id: schedule.id,
                adapter_id: schedule.adapter_id.clone(),
                kind: schedule.kind.clone(),
                page_limit: schedule.page_limit as u32,
                slot_key,
                correlation_id,
                trace: store::TraceContext::fresh(),
            };

            let completion = match self.envelopes.propose_scheduled_bridge_sync(proposal).await {
                Ok(envelope) => store::BridgeSyncScheduleCompletion {
                    // The idempotency boundary makes a replay return the same envelope.
                    envelope_id: Some(envelope.id),
                    error: None,
                },
                Err(error) => {
                    error!(
                        tenant_id = %schedule.tenant_id,
                        schedule_id = %schedule.id,
                        error = %error,
                        "scheduled bridge sync proposal failed"
                    );
                    record_metric("proposal_failed");
                    store::BridgeSyncScheduleCompletion {
                        envelope_id: None,
                        error: Some(error.to_string()),
                    }
                }
            };

            if completion.error.is_none() {
                record_metric("proposal_succeeded");
            }

            if let Err(error) = self
                .store
                .bridge_schedules
                .complete_claim(
                    schedule.tenant_id,
                    schedule.id,
                    claim.lease_token,
                    OffsetDateTime::now_utc(),
                    completion,
                )
                .await
            {
                record_metric("lease_finalize_failed");
                error!(
                    tenant_id = %schedule.tenant_id,
                    schedule_id = %schedule.id,
                    error = %error,
                    "bridge sync scheduler could not finalize lease"
                );
            } else {
                record_metric("lease_completed");
            }
        }

        Ok(())
    }
}

fn record_metric(outcome: &'static str) {
    crate::metrics::registry().inc_counter(
        "hydra_bridge_sync_scheduler_operations_total",
        vec![("outcome".to_owned(), outcome.to_owned())],
    );
}

pub fn slot_key(schedule_id: Uuid, due_at: OffsetDateTime) -> String {
    format!("hydra.scheduler/{schedule_id}/{}", due_at.unix_timestamp())
}

#[derive(Debug, thiserror::Error)]
enum SchedulerError {
    #[error(transparent)]
    Store(#[from] store::StoreError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use governor::{Cell, Constitution, Governor, Level, PolicyMatrix};
    use serde_json::json;
    use time::Duration;

    #[test]
    fn slot_key_is_stable_for_a_schedule_slot() {
        let schedule_id = Uuid::from_u128(7);
        let due_at = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid timestamp");
        assert_eq!(
            slot_key(schedule_id, due_at),
            "hydra.scheduler/00000000-0000-0000-0000-000000000007/1700000000"
        );
    }

    #[tokio::test]
    async fn poll_claims_active_schedule_and_records_governed_envelope(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = store::TestDb::new().await?;
        let result = async {
            let tenant_id = Uuid::new_v4();
            let store = store::Store::new(db.pool.clone());
            let adapter = store
                .bridge_adapters
                .create(store::NewBridgeAdapter {
                    tenant_id,
                    adapter_id: "memcrm".to_owned(),
                    component_ref: "fixture://memcrm".to_owned(),
                    component_sha256: "0".repeat(64),
                    grant_config: json!({}),
                    config: json!({}),
                })
                .await?;
            let activating = store
                .bridge_adapters
                .transition(store::BridgeAdapterTransition {
                    tenant_id,
                    adapter_id: "memcrm".to_owned(),
                    expected_revision: adapter.revision,
                    expected_state: store::BridgeAdapterState::Inactive,
                    new_state: store::BridgeAdapterState::Activating,
                    descriptor: Some(json!({"sync": ["party"]})),
                    last_error: None,
                    event: json!({"event": "test_activate"}),
                })
                .await?;
            store
                .bridge_adapters
                .transition(store::BridgeAdapterTransition {
                    tenant_id,
                    adapter_id: "memcrm".to_owned(),
                    expected_revision: activating.revision,
                    expected_state: store::BridgeAdapterState::Activating,
                    new_state: store::BridgeAdapterState::Active,
                    descriptor: None,
                    last_error: None,
                    event: json!({"event": "test_active"}),
                })
                .await?;
            let schedule = store
                .bridge_schedules
                .create(store::NewBridgeSyncSchedule {
                    tenant_id,
                    adapter_id: "memcrm".to_owned(),
                    kind: "party".to_owned(),
                    interval_seconds: 60,
                    page_limit: 10,
                })
                .await?;
            store
                .bridge_schedules
                .set_state(tenant_id, schedule.id, store::BridgeScheduleState::Enabled)
                .await?;

            let envelopes = Arc::new(StoreEnvelopeService::new(store.clone(), bridge_governor()));
            let scheduler =
                BridgeSyncScheduler::new(store.clone(), envelopes, true, TaskHealth::default());
            scheduler.run_once(OffsetDateTime::now_utc()).await?;

            let stored_schedule = store
                .bridge_schedules
                .list_for_tenant(tenant_id)
                .await?
                .into_iter()
                .next()
                .expect("schedule exists");
            let envelope_id = stored_schedule.last_envelope_id.expect("envelope recorded");
            assert!(stored_schedule.last_error.is_none());
            let envelope = store.envelopes.get(tenant_id, envelope_id).await?;
            assert_eq!(envelope.state, governor::EnvelopeState::PendingApproval);
            assert_eq!(
                envelope.invocation.origin_system.as_deref(),
                Some("hydra.scheduler")
            );
            let metrics = crate::metrics::registry().render();
            assert!(metrics.contains(
                "hydra_bridge_sync_scheduler_operations_total{outcome=\"lease_completed\"}"
            ));
            assert!(!metrics.contains(&tenant_id.to_string()));
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        .await;
        db.cleanup().await?;
        result
    }

    #[tokio::test]
    async fn concurrent_workers_and_interrupted_proposal_are_idempotent(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = store::TestDb::new().await?;
        let result = async {
            let tenant_id = Uuid::new_v4();
            let store = store::Store::new(db.pool.clone());
            let adapter = store
                .bridge_adapters
                .create(store::NewBridgeAdapter {
                    tenant_id,
                    adapter_id: "memcrm".to_owned(),
                    component_ref: "fixture://memcrm".to_owned(),
                    component_sha256: "0".repeat(64),
                    grant_config: json!({}),
                    config: json!({}),
                })
                .await?;
            let activating = store
                .bridge_adapters
                .transition(store::BridgeAdapterTransition {
                    tenant_id,
                    adapter_id: "memcrm".to_owned(),
                    expected_revision: adapter.revision,
                    expected_state: store::BridgeAdapterState::Inactive,
                    new_state: store::BridgeAdapterState::Activating,
                    descriptor: Some(json!({"sync": ["party"]})),
                    last_error: None,
                    event: json!({"event": "test_activate"}),
                })
                .await?;
            store
                .bridge_adapters
                .transition(store::BridgeAdapterTransition {
                    tenant_id,
                    adapter_id: "memcrm".to_owned(),
                    expected_revision: activating.revision,
                    expected_state: store::BridgeAdapterState::Activating,
                    new_state: store::BridgeAdapterState::Active,
                    descriptor: None,
                    last_error: None,
                    event: json!({"event": "test_active"}),
                })
                .await?;
            let schedule = store
                .bridge_schedules
                .create(store::NewBridgeSyncSchedule {
                    tenant_id,
                    adapter_id: "memcrm".to_owned(),
                    kind: "party".to_owned(),
                    interval_seconds: 60,
                    page_limit: 10,
                })
                .await?;
            store
                .bridge_schedules
                .set_state(tenant_id, schedule.id, store::BridgeScheduleState::Enabled)
                .await?;

            let scheduler_a = BridgeSyncScheduler::new(
                store.clone(),
                Arc::new(StoreEnvelopeService::new(store.clone(), bridge_governor())),
                true,
                TaskHealth::default(),
            );
            let scheduler_b = BridgeSyncScheduler::new(
                store.clone(),
                Arc::new(StoreEnvelopeService::new(store.clone(), bridge_governor())),
                true,
                TaskHealth::default(),
            );
            let now = OffsetDateTime::now_utc();
            let (a, b) = tokio::join!(scheduler_a.run_once(now), scheduler_b.run_once(now));
            a?;
            b?;

            let rows = store.bridge_schedules.list_for_tenant(tenant_id).await?;
            let row = rows.into_iter().next().expect("schedule exists");
            let envelope_id = row
                .last_envelope_id
                .expect("one worker records an envelope");
            let envelope_count = store
                .envelopes
                .list(tenant_id, governor::EnvelopeState::PendingApproval)
                .await?
                .len();
            assert_eq!(envelope_count, 1, "two workers must not duplicate a slot");

            let claim = store
                .bridge_schedules
                .claim_due(now + Duration::seconds(61), 120, 1)
                .await?
                .into_iter()
                .next()
                .expect("next interval is reclaimable");
            let replay_slot = slot_key(schedule.id, claim.slot_due_at);
            let replay = StoreEnvelopeService::new(store.clone(), bridge_governor())
                .propose_scheduled_bridge_sync(ScheduledBridgeSyncProposal {
                    tenant_id,
                    schedule_id: schedule.id,
                    adapter_id: "memcrm".to_owned(),
                    kind: "party".to_owned(),
                    page_limit: 10,
                    slot_key: replay_slot.clone(),
                    correlation_id: format!("hydra.scheduler/{replay_slot}"),
                    trace: store::TraceContext::fresh(),
                })
                .await?;
            assert_ne!(replay.id, envelope_id, "a later interval has a new slot");
            store
                .bridge_schedules
                .complete_claim(
                    tenant_id,
                    schedule.id,
                    claim.lease_token,
                    now + Duration::seconds(61),
                    store::BridgeSyncScheduleCompletion {
                        envelope_id: Some(replay.id),
                        error: None,
                    },
                )
                .await?;

            let interrupted = store
                .bridge_schedules
                .claim_due(now + Duration::seconds(122), 120, 1)
                .await?
                .into_iter()
                .next()
                .expect("third interval is reclaimable");
            let interrupted_slot = slot_key(schedule.id, interrupted.slot_due_at);
            let proposal = ScheduledBridgeSyncProposal {
                tenant_id,
                schedule_id: schedule.id,
                adapter_id: "memcrm".to_owned(),
                kind: "party".to_owned(),
                page_limit: 10,
                slot_key: interrupted_slot.clone(),
                correlation_id: format!("hydra.scheduler/{interrupted_slot}"),
                trace: store::TraceContext::fresh(),
            };
            let first = StoreEnvelopeService::new(store.clone(), bridge_governor())
                .propose_scheduled_bridge_sync(proposal.clone())
                .await?;
            // Simulate a worker exiting before lease completion.
            let reclaimed = store
                .bridge_schedules
                .claim_due(now + Duration::seconds(243), 120, 1)
                .await?
                .into_iter()
                .next()
                .expect("expired interrupted lease is reclaimable");
            let second = StoreEnvelopeService::new(store.clone(), bridge_governor())
                .propose_scheduled_bridge_sync(proposal)
                .await?;
            assert_eq!(first.id, second.id, "replayed slot must reuse its envelope");
            let stale = store
                .bridge_schedules
                .complete_claim(
                    tenant_id,
                    schedule.id,
                    interrupted.lease_token,
                    now + Duration::seconds(243),
                    store::BridgeSyncScheduleCompletion {
                        envelope_id: Some(first.id),
                        error: None,
                    },
                )
                .await;
            assert!(matches!(stale, Err(store::StoreError::Conflict(0))));
            store
                .bridge_schedules
                .complete_claim(
                    tenant_id,
                    schedule.id,
                    reclaimed.lease_token,
                    now + Duration::seconds(243),
                    store::BridgeSyncScheduleCompletion {
                        envelope_id: Some(second.id),
                        error: None,
                    },
                )
                .await?;
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        .await;
        db.cleanup().await?;
        result
    }

    fn bridge_governor() -> Governor {
        let mut matrix = PolicyMatrix::default();
        matrix
            .insert(
                "bridges",
                Some("sync_adapter"),
                None,
                Cell {
                    level: Level::L2,
                    batch_max: Some(1),
                },
            )
            .expect("test policy is valid");
        Governor {
            matrix,
            constitution: Constitution {
                monthly_spend_cap_cents: 50_000,
                pii_egress_allowlist: vec!["private".to_owned()],
                blast_entities_ceiling: 100,
                blast_sends_ceiling: 0,
                blast_money_ceiling_cents: 0,
            },
        }
    }
}
