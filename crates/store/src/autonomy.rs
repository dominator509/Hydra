use serde_json::Value;
use sqlx::types::Json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::StoreError;

struct AutonomyCellRow {
    domain: String,
    action: String,
    kind: Option<String>,
    level: String,
    cfg: Json<Value>,
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

    pub async fn matrix(&self, tenant: Uuid) -> Result<governor::PolicyMatrix, StoreError> {
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

        let mut matrix = governor::PolicyMatrix::default();
        for row in rows {
            let level = parse_level(&row.level)?;
            let batch_max = row
                .cfg
                .0
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
                governor::Cell { level, batch_max },
            )?;
        }

        Ok(matrix)
    }
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
