use serde_json::Value;
use sqlx::types::Json;
use sqlx::PgPool;
use sqlx::Transaction;
use uuid::Uuid;

use crate::StoreError;

struct EnvelopeRow {
    id: Uuid,
    tenant_id: Uuid,
    state: String,
    doc: Json<Value>,
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
        if envelope.tenant != tenant {
            return Err(StoreError::TenantMismatch);
        }

        let row = sqlx::query_as!(
            EnvelopeRow,
            r#"
            INSERT INTO envelope (id, tenant_id, state, doc)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE
            SET tenant_id = EXCLUDED.tenant_id,
                state = EXCLUDED.state,
                doc = EXCLUDED.doc,
                updated_at = now()
            RETURNING
                id,
                tenant_id,
                state,
                doc as "doc!: Json<Value>"
            "#,
            envelope.id,
            tenant,
            state_name(envelope.state),
            serde_json::to_value(envelope)
                .map_err(|error| StoreError::Invariant(format!("serialize envelope: {error}")))?,
        )
        .fetch_one(&self.pool)
        .await?;

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
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query_as!(
            EnvelopeRow,
            r#"
            SELECT
                id,
                tenant_id,
                state,
                doc as "doc!: Json<Value>"
            FROM envelope
            WHERE tenant_id = $1 AND id = $2
            FOR UPDATE
            "#,
            tenant,
            envelope_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            return Err(StoreError::NotFound);
        };

        let mut envelope = row_to_envelope(row)?;
        envelope.transition(to, actor, clock)?;

        persist_transition(&mut tx, tenant, &envelope).await?;
        tx.commit().await?;
        Ok(envelope)
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
                doc as "doc!: Json<Value>"
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
}

async fn persist_transition(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    tenant: Uuid,
    envelope: &governor::ActionEnvelope,
) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        UPDATE envelope
        SET state = $3,
            doc = $4,
            updated_at = now()
        WHERE tenant_id = $1 AND id = $2
        "#,
        tenant,
        envelope.id,
        state_name(envelope.state),
        serde_json::to_value(envelope)
            .map_err(|error| StoreError::Invariant(format!("serialize envelope: {error}")))?,
    )
    .execute(&mut **tx)
    .await?;

    let transition = envelope.history.last().ok_or_else(|| {
        StoreError::Invariant("missing envelope history entry after transition".to_owned())
    })?;
    sqlx::query!(
        r#"
        INSERT INTO envelope_transition (envelope_id, ts, from_state, to_state, actor)
        VALUES ($1, ($2::text)::timestamptz, $3, $4, $5)
        "#,
        envelope.id,
        transition.at_rfc3339,
        state_name(transition.from),
        state_name(transition.to),
        transition.actor,
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
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

    Ok(envelope)
}

fn state_name(state: governor::EnvelopeState) -> &'static str {
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
