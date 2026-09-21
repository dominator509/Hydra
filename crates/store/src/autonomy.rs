use serde_json::{json, Value};
use sqlx::types::Json;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{events::EventsRepo, StoreError};

struct AutonomyCellRow {
    domain: String,
    action: String,
    kind: Option<String>,
    level: String,
    cfg: Json<Value>,
}

struct PolicyRevisionRow {
    revision: i64,
}

struct AutonomyFreezeRow {
    status: String,
    reason: Option<String>,
    actor: String,
    updated_at: OffsetDateTime,
}

struct FreezeFlagRow {
    frozen: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredAutonomyCell {
    pub domain: String,
    pub action: String,
    pub kind: Option<String>,
    pub level: governor::Level,
    pub cfg: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutonomyFreeze {
    pub tenant_id: Uuid,
    pub frozen: bool,
    pub reason: Option<String>,
    pub actor: String,
    pub updated_at: OffsetDateTime,
}

#[derive(Clone)]
pub struct AutonomyRepo {
    pool: PgPool,
}

impl AutonomyRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert_cell(
        &self,
        tenant: Uuid,
        domain: &str,
        action: &str,
        kind: Option<&str>,
        level: governor::Level,
        cfg: &Value,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT INTO autonomy_cell (tenant_id, domain, action, kind, level, cfg)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, domain, action, kind_key) DO UPDATE
            SET level = EXCLUDED.level,
                cfg = EXCLUDED.cfg
            "#,
            tenant,
            domain,
            action,
            kind,
            level_name(level),
            cfg.clone(),
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list(&self, tenant: Uuid) -> Result<Vec<StoredAutonomyCell>, StoreError> {
        let rows = sqlx::query_as!(
            AutonomyCellRow,
            r#"
            SELECT
                domain,
                action,
                kind,
                level,
                cfg as "cfg!: Json<Value>"
            FROM autonomy_cell
            WHERE tenant_id = $1
            ORDER BY domain, action, kind_key
            "#,
            tenant
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_cell).collect()
    }

    pub async fn replace_cells(
        &self,
        tenant: Uuid,
        actor: &str,
        cells: &[StoredAutonomyCell],
    ) -> Result<Vec<StoredAutonomyCell>, StoreError> {
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r#"
            DELETE FROM autonomy_cell
            WHERE tenant_id = $1
            "#,
            tenant
        )
        .execute(&mut *tx)
        .await?;

        for cell in cells {
            sqlx::query!(
                r#"
                INSERT INTO autonomy_cell (tenant_id, domain, action, kind, level, cfg)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
                tenant,
                cell.domain,
                cell.action,
                cell.kind,
                level_name(cell.level),
                cell.cfg.clone(),
            )
            .execute(&mut *tx)
            .await?;
        }

        let payload = json!({
            "cells": cells
                .iter()
                .map(|cell| {
                    json!({
                        "domain": cell.domain,
                        "action": cell.action,
                        "kind": cell.kind,
                        "level": level_name(cell.level),
                        "cfg": cell.cfg,
                    })
                })
                .collect::<Vec<_>>()
        });
        EventsRepo::append(&mut tx, tenant, actor, "autonomy.cells.updated", &payload).await?;
        tx.commit().await?;
        Ok(cells.to_vec())
    }

    pub async fn matrix(&self, tenant: Uuid) -> Result<governor::PolicyMatrix, StoreError> {
        let rows = self.list(tenant).await?;
        let frozen = self.is_frozen(tenant).await?;

        let mut matrix = governor::PolicyMatrix::default();
        for row in rows {
            let batch_max = row
                .cfg
                .get("batch_max")
                .and_then(Value::as_u64)
                .map(|value| {
                    u32::try_from(value).map_err(|_| {
                        StoreError::Invariant(format!(
                            "batch_max overflow for cell {}/{:?}",
                            row.domain, row.kind
                        ))
                    })
                })
                .transpose()?;
            matrix.insert(
                &row.domain,
                Some(&row.action),
                row.kind.as_deref(),
                governor::Cell {
                    level: if frozen {
                        governor::Level::L1
                    } else {
                        row.level
                    },
                    batch_max,
                },
            )?;
        }

        Ok(matrix)
    }

    pub async fn freeze_status(&self, tenant: Uuid) -> Result<AutonomyFreeze, StoreError> {
        validate_tenant(tenant)?;
        let row = sqlx::query_as!(
            AutonomyFreezeRow,
            r#"
            SELECT status, reason, actor, updated_at
            FROM autonomy_freeze
            WHERE tenant_id = $1
            "#,
            tenant,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(match row {
            Some(row) => AutonomyFreeze {
                tenant_id: tenant,
                frozen: row.status == "frozen",
                reason: row.reason,
                actor: row.actor,
                updated_at: row.updated_at,
            },
            None => AutonomyFreeze {
                tenant_id: tenant,
                frozen: false,
                reason: None,
                actor: "system".to_owned(),
                updated_at: OffsetDateTime::UNIX_EPOCH,
            },
        })
    }

    pub async fn set_frozen(
        &self,
        tenant: Uuid,
        frozen: bool,
        reason: Option<&str>,
        actor: &str,
    ) -> Result<AutonomyFreeze, StoreError> {
        validate_tenant(tenant)?;
        validate_actor(actor)?;
        let reason = match (frozen, reason.map(str::trim)) {
            (true, Some(value)) if !value.is_empty() && value.len() <= 500 => Some(value),
            (true, _) => {
                return Err(StoreError::Invariant(
                    "autonomy freeze reason must be non-empty and at most 500 bytes".to_owned(),
                ));
            }
            (false, _) => None,
        };
        let status = if frozen { "frozen" } else { "active" };
        let mut tx = self.pool.begin().await?;
        let current = sqlx::query_as!(
            AutonomyFreezeRow,
            r#"
            SELECT status, reason, actor, updated_at
            FROM autonomy_freeze
            WHERE tenant_id = $1
            FOR UPDATE
            "#,
            tenant,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if current
            .as_ref()
            .is_some_and(|row| row.status == status && row.reason.as_deref() == reason)
        {
            tx.commit().await?;
            return self.freeze_status(tenant).await;
        }

        let row = sqlx::query_as!(
            AutonomyFreezeRow,
            r#"
            INSERT INTO autonomy_freeze (tenant_id, status, reason, actor)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (tenant_id) DO UPDATE
            SET status = EXCLUDED.status,
                reason = EXCLUDED.reason,
                actor = EXCLUDED.actor,
                updated_at = now()
            RETURNING status, reason, actor, updated_at
            "#,
            tenant,
            status,
            reason,
            actor,
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO autonomy_policy_revision (tenant_id, revision, updated_at)
            VALUES ($1, 1, now())
            ON CONFLICT (tenant_id) DO UPDATE
            SET revision = autonomy_policy_revision.revision + 1,
                updated_at = now()
            "#,
            tenant,
        )
        .execute(&mut *tx)
        .await?;

        let event = cdm::HydraEventEnvelope::new(
            Uuid::new_v4(),
            cdm::HydraEventType::AutonomyFreezeChanged,
            crate::events::canonical_now()?,
            tenant,
            cdm::EventActorRef {
                actor_id: actor.to_owned(),
                actor_type: cdm::EventActorType::LocalHydraUser,
            },
            cdm::EventDataClass::Restricted,
            cdm::HydraEventPayload::AutonomyFreeze {
                status: status.to_owned(),
                reason: reason.map(str::to_owned),
            },
        );
        EventsRepo::append_canonical(&mut tx, &event).await?;
        tx.commit().await?;

        Ok(AutonomyFreeze {
            tenant_id: tenant,
            frozen: row.status == "frozen",
            reason: row.reason,
            actor: row.actor,
            updated_at: row.updated_at,
        })
    }

    pub async fn revision(&self, tenant: Uuid) -> Result<u64, StoreError> {
        let row = sqlx::query_as!(
            PolicyRevisionRow,
            r#"
            SELECT revision
            FROM autonomy_policy_revision
            WHERE tenant_id = $1
            "#,
            tenant,
        )
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => u64::try_from(row.revision).map_err(|_| {
                StoreError::Invariant(format!(
                    "negative autonomy policy revision for tenant {tenant}"
                ))
            }),
            None => Ok(0),
        }
    }

    async fn is_frozen(&self, tenant: Uuid) -> Result<bool, StoreError> {
        validate_tenant(tenant)?;
        let frozen = sqlx::query_as!(
            FreezeFlagRow,
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM autonomy_freeze
                WHERE tenant_id = $1 AND status = 'frozen'
            ) AS "frozen!"
            "#,
            tenant,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(frozen.frozen)
    }
}

fn validate_tenant(tenant: Uuid) -> Result<(), StoreError> {
    if tenant.is_nil() {
        return Err(StoreError::Invariant(
            "autonomy tenant_id cannot be nil".to_owned(),
        ));
    }
    Ok(())
}

fn validate_actor(actor: &str) -> Result<(), StoreError> {
    if actor.trim().is_empty()
        || actor.len() > 200
        || actor.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(StoreError::Invariant(
            "autonomy actor must be bounded and contain no control characters".to_owned(),
        ));
    }
    Ok(())
}

fn row_to_cell(row: AutonomyCellRow) -> Result<StoredAutonomyCell, StoreError> {
    Ok(StoredAutonomyCell {
        domain: row.domain,
        action: row.action,
        kind: row.kind,
        level: parse_level(&row.level)?,
        cfg: row.cfg.0,
    })
}

fn level_name(level: governor::Level) -> &'static str {
    match level {
        governor::Level::L0 => "L0",
        governor::Level::L1 => "L1",
        governor::Level::L2 => "L2",
        governor::Level::L3 => "L3",
        governor::Level::L4 => "L4",
        governor::Level::L5 => "L5",
    }
}

fn parse_level(level: &str) -> Result<governor::Level, StoreError> {
    match level {
        "L0" => Ok(governor::Level::L0),
        "L1" => Ok(governor::Level::L1),
        "L2" => Ok(governor::Level::L2),
        "L3" => Ok(governor::Level::L3),
        "L4" => Ok(governor::Level::L4),
        "L5" => Ok(governor::Level::L5),
        other => Err(StoreError::Invariant(format!(
            "unknown autonomy level '{other}'"
        ))),
    }
}
