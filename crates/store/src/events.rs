use cdm::{EventActorRef, EventActorType, HydraEventEnvelope, HydraEventType, HYDRA_EVENT_SOURCE};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{outbox::OutboxRepo, StoreError, TraceContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventProvenance {
    pub actor: EventActorRef,
    pub external_binding_id: Option<Uuid>,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub envelope_id: Option<Uuid>,
    pub trace_context: Option<TraceContext>,
}

impl EventProvenance {
    pub fn hydra_system(actor_id: impl Into<String>) -> Self {
        Self {
            actor: EventActorRef {
                actor_id: actor_id.into(),
                actor_type: EventActorType::HydraSystem,
            },
            external_binding_id: None,
            correlation_id: None,
            causation_id: None,
            envelope_id: None,
            trace_context: None,
        }
    }

    pub fn bridge(actor_id: impl Into<String>) -> Self {
        Self {
            actor: EventActorRef {
                actor_id: actor_id.into(),
                actor_type: EventActorType::Bridge,
            },
            external_binding_id: None,
            correlation_id: None,
            causation_id: None,
            envelope_id: None,
            trace_context: None,
        }
    }

    pub fn for_envelope_proposal(envelope: &governor::ActionEnvelope) -> Self {
        let actor_id = envelope
            .invocation
            .external_actor_id
            .clone()
            .unwrap_or_else(|| "store.envelopes".to_owned());
        let actor_type = envelope
            .invocation
            .external_actor_type
            .as_deref()
            .and_then(event_actor_type)
            .unwrap_or(EventActorType::HydraSystem);
        Self::from_envelope(
            envelope,
            EventActorRef {
                actor_id,
                actor_type,
            },
        )
    }

    pub fn for_envelope_transition(envelope: &governor::ActionEnvelope, actor_id: &str) -> Self {
        let actor_type = if matches!(actor_id, "governor" | "executor") {
            EventActorType::HydraSystem
        } else if actor_id.starts_with("bridge:") {
            EventActorType::Bridge
        } else if envelope.invocation.external_actor_id.as_deref() == Some(actor_id) {
            envelope
                .invocation
                .external_actor_type
                .as_deref()
                .and_then(event_actor_type)
                .unwrap_or(EventActorType::LocalHydraUser)
        } else if envelope.invocation.approval_id.is_some() {
            EventActorType::Human
        } else {
            EventActorType::LocalHydraUser
        };
        Self::from_envelope(
            envelope,
            EventActorRef {
                actor_id: actor_id.to_owned(),
                actor_type,
            },
        )
    }

    fn from_envelope(envelope: &governor::ActionEnvelope, actor: EventActorRef) -> Self {
        Self {
            actor,
            external_binding_id: envelope.invocation.external_binding_id,
            correlation_id: envelope.invocation.correlation_id.clone(),
            causation_id: envelope.invocation.causation_id.clone(),
            envelope_id: Some(envelope.id),
            trace_context: None,
        }
    }

    pub fn with_trace_context(mut self, trace_context: Option<TraceContext>) -> Self {
        self.trace_context = trace_context;
        self
    }

    pub(crate) fn apply(&self, event: &mut HydraEventEnvelope) {
        event.external_binding_id = self.external_binding_id;
        event.correlation_id.clone_from(&self.correlation_id);
        event.causation_id.clone_from(&self.causation_id);
        event.envelope_id = self.envelope_id;
    }
}

#[derive(Clone)]
pub struct EventsRepo {
    pool: PgPool,
}

impl EventsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn append(
        tx: &mut Transaction<'_, Postgres>,
        tenant: Uuid,
        actor: &str,
        kind: &str,
        payload: &Value,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT INTO event_log (tenant_id, actor, kind, payload)
            VALUES ($1, $2, $3, $4)
            "#,
            tenant,
            actor,
            kind,
            payload.clone()
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn append_canonical(
        tx: &mut Transaction<'_, Postgres>,
        event: &HydraEventEnvelope,
    ) -> Result<(), StoreError> {
        Self::append_canonical_with_trace(tx, event, None).await
    }

    pub async fn append_canonical_with_trace(
        tx: &mut Transaction<'_, Postgres>,
        event: &HydraEventEnvelope,
        trace_context: Option<&TraceContext>,
    ) -> Result<(), StoreError> {
        event.validate()?;
        if event.source != HYDRA_EVENT_SOURCE || event.subject != event.event_type.as_str() {
            return Err(StoreError::Invariant(
                "canonical event source or subject is inconsistent".to_owned(),
            ));
        }
        let document = serde_json::to_value(event).map_err(|error| {
            StoreError::Invariant(format!("serialize canonical event: {error}"))
        })?;
        sqlx::query!(
            r#"
            INSERT INTO event_log (event_id, tenant_id, actor, kind, payload)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            event.event_id,
            event.hydra_tenant_id,
            event.actor.actor_id,
            event.event_type.as_str(),
            document.clone(),
        )
        .execute(&mut **tx)
        .await?;
        OutboxRepo::append(tx, event, &document, trace_context).await
    }
}

pub(crate) fn canonical_now() -> Result<String, StoreError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| StoreError::Invariant(format!("format event timestamp: {error}")))
}

pub(crate) fn envelope_event_type(state: governor::EnvelopeState) -> Option<HydraEventType> {
    match state {
        governor::EnvelopeState::PendingApproval => Some(HydraEventType::EnvelopeQueued),
        governor::EnvelopeState::Approved => Some(HydraEventType::EnvelopeApproved),
        governor::EnvelopeState::Executed => Some(HydraEventType::EnvelopeExecuted),
        governor::EnvelopeState::Failed => Some(HydraEventType::EnvelopeFailed),
        _ => None,
    }
}

fn event_actor_type(value: &str) -> Option<EventActorType> {
    match value {
        "human" => Some(EventActorType::Human),
        "nexus_service" => Some(EventActorType::NexusService),
        "nexus_agent" => Some(EventActorType::NexusAgent),
        "hydra_internal_agent" => Some(EventActorType::HydraInternalAgent),
        "local_hydra_user" => Some(EventActorType::LocalHydraUser),
        _ => None,
    }
}
