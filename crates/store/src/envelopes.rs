use cdm::{EventDataClass, EventEntityRef, HydraEventEnvelope, HydraEventPayload, HydraEventType};
use serde_json::Value;
use sqlx::types::Json;
use sqlx::{PgPool, Transaction};
use uuid::Uuid;

use crate::{
    events::{canonical_now, envelope_event_type, EventProvenance, EventsRepo},
    execution_receipts, ExecutionReceipt, NewExecutionReceipt, StoreError, TraceContext,
};

struct EnvelopeRow {
    id: Uuid,
    tenant_id: Uuid,
    state: String,
    doc: Json<Value>,
    revision: i64,
}

struct TraceContextRow {
    trace_context: Option<Json<Value>>,
}

#[derive(Clone)]
pub struct EnvelopesRepo {
    pool: PgPool,
}

impl EnvelopesRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn save(
        &self,
        tenant: Uuid,
        envelope: &governor::ActionEnvelope,
    ) -> Result<governor::ActionEnvelope, StoreError> {
        self.save_with_trace(tenant, envelope, None).await
    }

    pub async fn save_with_trace(
        &self,
        tenant: Uuid,
        envelope: &governor::ActionEnvelope,
        trace_context: Option<&TraceContext>,
    ) -> Result<governor::ActionEnvelope, StoreError> {
        if envelope.tenant != tenant || tenant.is_nil() {
            return Err(StoreError::TenantMismatch);
        }

        let document = serde_json::to_value(envelope)
            .map_err(|error| StoreError::Invariant(format!("serialize envelope: {error}")))?;
        let trace_document = serialize_trace_context(trace_context)?;
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query_as!(
            EnvelopeRow,
            r#"
            INSERT INTO envelope (id, tenant_id, state, doc, trace_context)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO NOTHING
            RETURNING
                id,
                tenant_id,
                state,
                doc as "doc!: Json<Value>",
                revision
            "#,
            envelope.id,
            tenant,
            state_name(envelope.state),
            document.clone(),
            trace_document.clone(),
        )
        .fetch_optional(&mut *tx)
        .await?;

        let row = if let Some(row) = inserted {
            append_proposed_event(&mut tx, envelope, None, trace_context).await?;
            row
        } else {
            sqlx::query_as!(
                EnvelopeRow,
                r#"
                UPDATE envelope
                SET state = $3,
                    doc = $4,
                    trace_context = COALESCE($5, trace_context),
                    revision = revision + 1,
                    updated_at = now()
                WHERE tenant_id = $1 AND id = $2
                RETURNING
                    id,
                    tenant_id,
                    state,
                    doc as "doc!: Json<Value>",
                    revision
                "#,
                tenant,
                envelope.id,
                state_name(envelope.state),
                document,
                trace_document,
            )
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::TenantMismatch)?
        };

        tx.commit().await?;

        row_to_envelope(row)
    }

    pub async fn transition(
        &self,
        tenant: Uuid,
        envelope_id: Uuid,
        to: governor::EnvelopeState,
        actor: &str,
        clock: &dyn governor::Clock,
    ) -> Result<governor::ActionEnvelope, StoreError> {
        self.transition_with_trace(tenant, envelope_id, to, actor, clock, None)
            .await
    }

    pub async fn transition_with_trace(
        &self,
        tenant: Uuid,
        envelope_id: Uuid,
        to: governor::EnvelopeState,
        actor: &str,
        clock: &dyn governor::Clock,
        trace_context: Option<&TraceContext>,
    ) -> Result<governor::ActionEnvelope, StoreError> {
        if tenant.is_nil() || actor.trim().is_empty() {
            return Err(StoreError::TenantMismatch);
        }
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query_as!(
            EnvelopeRow,
            r#"
            SELECT
                id,
                tenant_id,
                state,
                doc as "doc!: Json<Value>",
                revision
            FROM envelope
            WHERE tenant_id = $1 AND id = $2
            FOR UPDATE
            "#,
            tenant,
            envelope_id
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;

        let expected_revision = as_u64(row.revision)?;
        let mut envelope = row_to_envelope(row)?;
        envelope.transition(to, actor, clock)?;

        persist_transition(&mut tx, tenant, expected_revision, &envelope, trace_context).await?;
        append_transition_records(&mut tx, &envelope, trace_context).await?;

        tx.commit().await?;
        Ok(envelope)
    }

    pub async fn finish_execution(
        &self,
        tenant: Uuid,
        envelope_id: Uuid,
        to: governor::EnvelopeState,
        actor: &str,
        clock: &dyn governor::Clock,
        receipt: NewExecutionReceipt,
    ) -> Result<(governor::ActionEnvelope, ExecutionReceipt), StoreError> {
        self.finish_execution_with_trace(tenant, envelope_id, to, actor, clock, receipt, None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn finish_execution_with_trace(
        &self,
        tenant: Uuid,
        envelope_id: Uuid,
        to: governor::EnvelopeState,
        actor: &str,
        clock: &dyn governor::Clock,
        receipt: NewExecutionReceipt,
        trace_context: Option<&TraceContext>,
    ) -> Result<(governor::ActionEnvelope, ExecutionReceipt), StoreError> {
        if tenant.is_nil()
            || actor.trim().is_empty()
            || receipt.tenant_id != tenant
            || receipt.envelope_id != envelope_id
            || !matches!(
                (to, receipt.outcome),
                (
                    governor::EnvelopeState::Executed,
                    crate::ExecutionOutcome::Verified
                ) | (
                    governor::EnvelopeState::Failed,
                    crate::ExecutionOutcome::Failed
                )
            )
        {
            return Err(StoreError::TenantMismatch);
        }
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as!(
            EnvelopeRow,
            r#"
            SELECT
                id,
                tenant_id,
                state,
                doc as "doc!: Json<Value>",
                revision
            FROM envelope
            WHERE tenant_id = $1 AND id = $2
            FOR UPDATE
            "#,
            tenant,
            envelope_id,
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let expected_revision = as_u64(row.revision)?;
        let mut envelope = row_to_envelope(row)?;
        envelope.transition(to, actor, clock)?;
        persist_transition(&mut tx, tenant, expected_revision, &envelope, trace_context).await?;
        append_transition_records(&mut tx, &envelope, trace_context).await?;
        let receipt = execution_receipts::insert(&mut tx, &receipt).await?;
        tx.commit().await?;
        Ok((envelope, receipt))
    }

    pub async fn list(
        &self,
        tenant: Uuid,
        state: governor::EnvelopeState,
    ) -> Result<Vec<governor::ActionEnvelope>, StoreError> {
        let rows = sqlx::query_as!(
            EnvelopeRow,
            r#"
            SELECT
                id,
                tenant_id,
                state,
                doc as "doc!: Json<Value>",
                revision
            FROM envelope
            WHERE tenant_id = $1 AND state = $2
            ORDER BY updated_at DESC, id
            "#,
            tenant,
            state_name(state),
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_envelope).collect()
    }

    pub async fn get(
        &self,
        tenant: Uuid,
        envelope_id: Uuid,
    ) -> Result<governor::ActionEnvelope, StoreError> {
        let row = sqlx::query_as!(
            EnvelopeRow,
            r#"
            SELECT
                id,
                tenant_id,
                state,
                doc as "doc!: Json<Value>",
                revision
            FROM envelope
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant,
            envelope_id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        row_to_envelope(row)
    }

    pub async fn trace_context(
        &self,
        tenant: Uuid,
        envelope_id: Uuid,
    ) -> Result<Option<TraceContext>, StoreError> {
        let row = sqlx::query_as!(
            TraceContextRow,
            r#"
            SELECT trace_context as "trace_context: Json<Value>"
            FROM envelope
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant,
            envelope_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        deserialize_trace_context(row.trace_context)
    }
}

pub(crate) async fn persist_transition(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    tenant: Uuid,
    expected_revision: u64,
    envelope: &governor::ActionEnvelope,
    trace_context: Option<&TraceContext>,
) -> Result<(), StoreError> {
    let trace_document = serialize_trace_context(trace_context)?;
    let result = sqlx::query!(
        r#"
        UPDATE envelope
        SET state = $3,
            doc = $4,
            trace_context = COALESCE($6, trace_context),
            revision = revision + 1,
            updated_at = now()
        WHERE tenant_id = $1 AND id = $2 AND revision = $5
        "#,
        tenant,
        envelope.id,
        state_name(envelope.state),
        serde_json::to_value(envelope)
            .map_err(|error| StoreError::Invariant(format!("serialize envelope: {error}")))?,
        as_i64(expected_revision)?,
        trace_document,
    )
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(StoreError::Conflict(expected_revision));
    }

    Ok(())
}

pub(crate) async fn append_transition_records(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    envelope: &governor::ActionEnvelope,
    trace_context: Option<&TraceContext>,
) -> Result<(), StoreError> {
    let transition = envelope.history.last().ok_or_else(|| {
        StoreError::Invariant("missing envelope history entry after transition".to_owned())
    })?;
    sqlx::query!(
        r#"
        INSERT INTO envelope_transition (
            envelope_id,
            tenant_id,
            ts,
            from_state,
            to_state,
            actor,
            invocation,
            trace_context
        )
        VALUES ($1, $2, ($3::text)::timestamptz, $4, $5, $6, $7, $8)
        "#,
        envelope.id,
        envelope.tenant,
        transition.at_rfc3339,
        state_name(transition.from),
        state_name(transition.to),
        transition.actor,
        serde_json::to_value(&envelope.invocation)
            .map_err(|error| StoreError::Invariant(format!("serialize invocation: {error}")))?,
        serialize_trace_context(trace_context)?,
    )
    .execute(&mut **tx)
    .await?;

    if let Some(event_type) = envelope_event_type(transition.to) {
        let provenance = EventProvenance::for_envelope_transition(envelope, &transition.actor)
            .with_trace_context(trace_context.cloned());
        let mut event = HydraEventEnvelope::new(
            Uuid::new_v4(),
            event_type,
            transition.at_rfc3339.clone(),
            envelope.tenant,
            provenance.actor.clone(),
            EventDataClass::Private,
            HydraEventPayload::EnvelopeTransition {
                from_state: Some(event_state_name(transition.from).to_owned()),
                to_state: event_state_name(transition.to).to_owned(),
                capability: capability_name(envelope).map(str::to_owned),
                outcome: match transition.to {
                    governor::EnvelopeState::Executed => Some("verified".to_owned()),
                    governor::EnvelopeState::Failed => Some("failed".to_owned()),
                    _ => None,
                },
            },
        );
        provenance.apply(&mut event);
        event.entity = first_target_ref(tx, envelope).await?;
        EventsRepo::append_canonical_with_trace(tx, &event, provenance.trace_context.as_ref())
            .await?;
    }

    Ok(())
}

pub(crate) async fn append_proposed_event(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    envelope: &governor::ActionEnvelope,
    capability: Option<&str>,
    trace_context: Option<&TraceContext>,
) -> Result<(), StoreError> {
    let provenance =
        EventProvenance::for_envelope_proposal(envelope).with_trace_context(trace_context.cloned());
    let occurred_at = envelope
        .history
        .first()
        .map(|transition| transition.at_rfc3339.clone())
        .map(Ok)
        .unwrap_or_else(canonical_now)?;
    let mut event = HydraEventEnvelope::new(
        Uuid::new_v4(),
        HydraEventType::EnvelopeProposed,
        occurred_at,
        envelope.tenant,
        provenance.actor.clone(),
        EventDataClass::Private,
        HydraEventPayload::EnvelopeTransition {
            from_state: None,
            to_state: "proposed".to_owned(),
            capability: capability
                .map(str::to_owned)
                .or_else(|| capability_name(envelope).map(str::to_owned)),
            outcome: None,
        },
    );
    provenance.apply(&mut event);
    event.entity = first_target_ref(tx, envelope).await?;
    EventsRepo::append_canonical_with_trace(tx, &event, provenance.trace_context.as_ref()).await
}

fn serialize_trace_context(
    trace_context: Option<&TraceContext>,
) -> Result<Option<Value>, StoreError> {
    if let Some(trace_context) = trace_context {
        trace_context.validate()?;
        serde_json::to_value(trace_context)
            .map(Some)
            .map_err(|error| StoreError::Invariant(format!("serialize trace context: {error}")))
    } else {
        Ok(None)
    }
}

fn deserialize_trace_context(
    trace_context: Option<Json<Value>>,
) -> Result<Option<TraceContext>, StoreError> {
    let trace_context = trace_context
        .map(|value| serde_json::from_value::<TraceContext>(value.0))
        .transpose()
        .map_err(|error| StoreError::Invariant(format!("deserialize trace context: {error}")))?;
    if let Some(trace_context) = &trace_context {
        trace_context.validate()?;
    }
    Ok(trace_context)
}

async fn first_target_ref(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    envelope: &governor::ActionEnvelope,
) -> Result<Option<EventEntityRef>, StoreError> {
    let Some(entity_id) = envelope.targets.first() else {
        return Ok(None);
    };
    let row = sqlx::query!(
        r#"
        SELECT id, kind, origin, origin_ref
        FROM entity
        WHERE tenant_id = $1 AND id = $2
        "#,
        envelope.tenant,
        entity_id,
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|row| EventEntityRef {
        entity_id: row.id,
        kind: row.kind,
        origin: row.origin,
        origin_ref: row.origin_ref,
    }))
}

fn capability_name(envelope: &governor::ActionEnvelope) -> Option<&'static str> {
    if envelope.domain == "pipeline"
        && envelope.action == "move_stage"
        && envelope.kind.as_deref() == Some("deal")
    {
        Some("hydra.crm.propose_action")
    } else {
        None
    }
}

fn event_state_name(state: governor::EnvelopeState) -> &'static str {
    match state {
        governor::EnvelopeState::Proposed => "proposed",
        governor::EnvelopeState::PendingApproval => "pending_approval",
        governor::EnvelopeState::Approved => "approved",
        governor::EnvelopeState::Executing => "executing",
        governor::EnvelopeState::Executed => "executed",
        governor::EnvelopeState::Failed => "failed",
        governor::EnvelopeState::RolledBack => "rolled_back",
        governor::EnvelopeState::Rejected => "rejected",
    }
}

fn row_to_envelope(row: EnvelopeRow) -> Result<governor::ActionEnvelope, StoreError> {
    let envelope = serde_json::from_value::<governor::ActionEnvelope>(row.doc.0)
        .map_err(|error| StoreError::Invariant(format!("deserialize envelope: {error}")))?;

    if envelope.tenant != row.tenant_id {
        return Err(StoreError::Invariant(format!(
            "envelope {} tenant mismatch between row and doc",
            row.id
        )));
    }
    if state_name(envelope.state) != row.state {
        return Err(StoreError::Invariant(format!(
            "envelope {} state mismatch between row and doc",
            row.id
        )));
    }
    let _ = as_u64(row.revision)?;

    Ok(envelope)
}

pub(crate) fn state_name(state: governor::EnvelopeState) -> &'static str {
    match state {
        governor::EnvelopeState::Proposed => "Proposed",
        governor::EnvelopeState::PendingApproval => "PendingApproval",
        governor::EnvelopeState::Approved => "Approved",
        governor::EnvelopeState::Executing => "Executing",
        governor::EnvelopeState::Executed => "Executed",
        governor::EnvelopeState::Failed => "Failed",
        governor::EnvelopeState::RolledBack => "RolledBack",
        governor::EnvelopeState::Rejected => "Rejected",
    }
}

fn as_u64(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value)
        .map_err(|_| StoreError::Invariant(format!("negative envelope revision: {value}")))
}

fn as_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value)
        .map_err(|_| StoreError::Invariant(format!("envelope revision overflow: {value}")))
}
