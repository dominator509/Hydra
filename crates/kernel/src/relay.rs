use std::time::Duration;

use serde_json::Value;
use sqlx::types::Json;
use sqlx::PgPool;
use tokio::sync::watch;
use tracing::warn;

const OUTBOX_BATCH_SIZE: i64 = 100;
const RELAY_POLL_INTERVAL: Duration = Duration::from_millis(250);

struct OutboxRow {
    id: i64,
    event: Json<Value>,
}

pub async fn run(mut shutdown: watch::Receiver<bool>, pool: PgPool, nats: async_nats::Client) {
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
                if let Err(error) = publish_once(&pool, &nats).await {
                    warn!(error = %error, "outbox relay iteration failed");
                }
            }
        }
    }
}

async fn publish_once(pool: &PgPool, nats: &async_nats::Client) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let rows = sqlx::query_as!(
        OutboxRow,
        r#"
        SELECT
            id,
            event as "event!: Json<Value>"
        FROM outbox
        WHERE published_at IS NULL
        ORDER BY id
        LIMIT $1
        FOR UPDATE SKIP LOCKED
        "#,
        OUTBOX_BATCH_SIZE
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    if rows.is_empty() {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(());
    }

    for row in &rows {
        let tenant_id = row
            .event
            .0
            .get("tenant_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("outbox row {} missing tenant_id", row.id))?;
        let payload = serde_json::to_vec(&row.event.0).map_err(|error| error.to_string())?;
        let subject = format!("hydra.events.{tenant_id}");
        nats.publish(subject, payload.into())
            .await
            .map_err(|error| error.to_string())?;
    }

    nats.flush().await.map_err(|error| error.to_string())?;

    for row in rows {
        sqlx::query!(
            r#"
            UPDATE outbox
            SET published_at = now()
            WHERE id = $1
            "#,
            row.id
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }

    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}
