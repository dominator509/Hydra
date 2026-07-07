use serde_json::{json, Value};
use sqlx::types::Json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{events::EventsRepo, StoreError};

struct AutonomyCellRow {
    domain: String,
    action: String,
    kind: Option<String>,
    level: String,
    cfg: Json<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredAutonomyCell {
    pub domain: String,
    pub action: String,
    pub kind: Option<String>,
    pub level: governor::Level,
    pub cfg: Value,
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
        insert_outbox(&mut tx, &payload).await?;

        tx.commit().await?;
        Ok(cells.to_vec())
    }

    pub async fn matrix(&self, tenant: Uuid) -> Result<governor::PolicyMatrix, StoreError> {
        let rows = self.list(tenant).await?;

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
                    level: row.level,
                    batch_max,
                },
            )?;
        }

        Ok(matrix)
    }
}

async fn insert_outbox(
    tx: &mut Transaction<'_, Postgres>,
    event: &Value,
) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        INSERT INTO outbox (event)
        VALUES ($1)
        "#,
        event.clone()
    )
    .execute(&mut **tx)
    .await?;
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
