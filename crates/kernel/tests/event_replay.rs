use std::process::{Command, Stdio};
use std::time::Duration;

use cdm::Entity;
use hydra_kernel::event_stream::{EventStreamConfig, JetStreamEventPublisher};
use serde_json::json;
use store::{Store, TestDb};
use uuid::Uuid;

#[path = "support/fake_nexus_consumer.rs"]
mod fake_nexus_consumer;

use fake_nexus_consumer::{FakeNexusConsumer, FakeNexusConsumerError, FakeNexusProjection};

#[test]
fn replay_cli_rejects_missing_confirmation_before_connecting() {
    let binary = std::env::var("CARGO_BIN_EXE_hydra-kernel").expect("kernel binary path");
    let result = Command::new(binary)
        .arg("--replay-events")
        .env_remove("DATABASE_URL")
        .env_remove("NATS_URL")
        .env_remove("HYDRA_EVENT_REPLAY_CONFIRM")
        .env("RUST_LOG", "info")
        .output()
        .expect("run replay CLI");
    assert!(!result.status.success());
    assert!(output(&result).contains("replay_confirmation_required"));
}

#[test]
fn replay_cli_rejects_missing_endpoints_and_invalid_limits_before_connecting() {
    let binary = std::env::var("CARGO_BIN_EXE_hydra-kernel").expect("kernel binary path");
    let missing_endpoint = Command::new(&binary)
        .arg("--replay-events")
        .env_remove("DATABASE_URL")
        .env_remove("NATS_URL")
        .env("HYDRA_EVENT_REPLAY_CONFIRM", "I_UNDERSTAND")
        .env("RUST_LOG", "info")
        .output()
        .expect("run replay CLI");
    assert!(!missing_endpoint.status.success());
    assert!(output(&missing_endpoint).contains("database_url_required"));

    let invalid_limit = Command::new(binary)
        .arg("--replay-events")
        .env("DATABASE_URL", "not-a-database-url")
        .env("NATS_URL", "not-a-nats-url")
        .env("HYDRA_EVENT_REPLAY_CONFIRM", "I_UNDERSTAND")
        .env("HYDRA_EVENT_REPLAY_LIMIT", "1001")
        .env("RUST_LOG", "info")
        .output()
        .expect("run replay CLI");
    assert!(!invalid_limit.status.success());
    assert!(output(&invalid_limit).contains("invalid_replay_limit"));
}

#[tokio::test]
async fn replay_cli_reemits_canonical_event_without_duplicate_message(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        let entity_id = Uuid::new_v4();
        store
            .entities
            .upsert(
                tenant,
                Entity {
                    id: entity_id,
                    kind: "party".to_owned(),
                    tenant,
                    body: json!({"display_name": "EP-050 replay fixture"}),
                    origin: "native".to_owned(),
                    origin_ref: None,
                    version: 1,
                },
            )
            .await?;
        let record = store.outbox.list_for_replay(0, 1).await?.remove(0);

        let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://[::1]:4222".to_owned());
        let client = async_nats::connect(&nats_url).await?;
        let publisher =
            JetStreamEventPublisher::bootstrap(client.clone(), EventStreamConfig::nexus_v1())
                .await?;
        let context = async_nats::jetstream::new(client);
        let durable_name = format!("ep050_replay_{}", Uuid::new_v4().simple());
        let projection = FakeNexusProjection::default();
        let consumer = FakeNexusConsumer::connect(
            &context,
            publisher.stream_name(),
            &durable_name,
            &record.subject,
            projection.clone(),
        )
        .await?;

        let database_url = db.scoped_database_url()?;
        let binary = std::env::var("CARGO_BIN_EXE_hydra-kernel")?;
        let first = run_replay(&binary, &database_url, &nats_url).await?;
        assert!(first.status.success(), "replay failed: {}", output(&first));
        assert!(
            output(&first).contains("event replay: ok"),
            "unexpected replay output: {}",
            output(&first)
        );

        let consumed = consumer.consume_one(Duration::from_secs(5)).await?;
        assert_eq!(consumed.event_id, record.event_id);
        assert_eq!(consumed.hydra_tenant_id, tenant);
        assert!(!consumed.duplicate);

        let second = run_replay(&binary, &database_url, &nats_url).await?;
        assert!(
            second.status.success(),
            "repeat replay failed: {}",
            output(&second)
        );
        assert!(
            output(&second).contains("event replay: ok"),
            "unexpected repeat output: {}",
            output(&second)
        );
        assert!(matches!(
            consumer.consume_one(Duration::from_millis(500)).await,
            Err(FakeNexusConsumerError::Timeout)
        ));
        assert_eq!(projection.event_ids().await, vec![record.event_id]);
        assert_eq!(projection.correlations().await.len(), 1);

        let unchanged = store.outbox.get_by_event_id(record.event_id).await?;
        assert_eq!(unchanged.published_at, record.published_at);
        assert_eq!(unchanged.jetstream_sequence, record.jetstream_sequence);

        context
            .get_stream(publisher.stream_name())
            .await?
            .delete_consumer(&durable_name)
            .await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

async fn run_replay(
    binary: &str,
    database_url: &str,
    nats_url: &str,
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let binary = binary.to_owned();
    let database_url = database_url.to_owned();
    let nats_url = nats_url.to_owned();
    Ok(tokio::task::spawn_blocking(move || {
        Command::new(binary)
            .arg("--replay-events")
            .env("DATABASE_URL", database_url)
            .env("NATS_URL", nats_url)
            .env("HYDRA_EVENT_REPLAY_CONFIRM", "I_UNDERSTAND")
            .env("HYDRA_EVENT_REPLAY_AFTER_ID", "0")
            .env("HYDRA_EVENT_REPLAY_LIMIT", "100")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    })
    .await??)
}

fn output(output: &std::process::Output) -> String {
    format!(
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
