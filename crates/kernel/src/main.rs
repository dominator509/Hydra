//! Layer L6 operations entrypoint, lifecycle supervisor, and health surface.

mod config;
mod telemetry;

use std::collections::BTreeMap;
use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use async_nats::Client as NatsClient;
use axum::{
    extract::Extension,
    http::{header, StatusCode},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde::Serialize;
use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::config::{Config, ConfigError, HydraEnv};
use fabric::{
    middleware::security_headers,
    rate::{rate_limit_middleware, RateLimiter},
};
use hydra_kernel::event_status::EventRuntimeStatusService;
use hydra_kernel::event_stream::{
    EventPublishRequest, EventPublisher, EventStreamConfig, JetStreamEventPublisher,
};
use hydra_kernel::metrics;
use hydra_kernel::nats::NatsTransportConfig;
use hydra_kernel::policy_provider::PersistedGovernorProvider;
use hydra_kernel::relay::RelayHealth;
use hydra_kernel::runtime_services::{LlmRuntimeConfig, RuntimeAvailability, RuntimeServices};
use hydra_kernel::supervisor::{
    supervise_background_tasks, wait_for_shutdown, ShutdownState, TaskHealth,
};

#[derive(Debug, thiserror::Error)]
enum KernelError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("failed to connect postgres: {0}")]
    Store(#[from] store::StoreError),
    #[error("postgres dependency operation timed out")]
    PostgresTimeout,
    #[error("failed to connect nats: {0}")]
    Nats(String),
    #[error("nats dependency operation timed out")]
    NatsTimeout,
    #[error("failed to initialize the canonical event stream: {0}")]
    EventStream(#[from] hydra_kernel::event_stream::EventStreamError),
    #[error("canonical event stream initialization timed out")]
    EventStreamTimeout,
    #[error("failed to bind {bind}: {source}")]
    Bind {
        bind: std::net::SocketAddr,
        source: std::io::Error,
    },
    #[error("failed to read local address: {0}")]
    LocalAddr(std::io::Error),
    #[error("server exited with error: {0}")]
    Serve(std::io::Error),
    #[error("failed to configure Nexus interoperability: {0}")]
    Nexus(String),
    #[error("failed to construct kernel runtime services: {0}")]
    Runtime(String),
    #[error("failed to load the configured bridge vault: {0}")]
    Vault(String),
    #[error("background task supervision failed: {0}")]
    Supervisor(#[from] hydra_kernel::supervisor::SupervisorError),
}

#[derive(Clone)]
struct EventReadiness {
    required: bool,
    status: Arc<dyn fabric::EventStatusService>,
}

#[derive(Clone)]
struct RuntimeReadiness {
    bridge_lifecycle_required: bool,
    bridge_lifecycle_available: bool,
    bridge_sync_scheduler_required: bool,
    bridge_sync_scheduler: TaskHealth,
    executor_worker: TaskHealth,
    shutdown: ShutdownState,
    dependency_timeout: Duration,
}

const STALE_EXECUTION_MINUTES: i64 = 15;

#[derive(Debug, Serialize)]
struct ReadinessCheck {
    status: &'static str,
    required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ReadinessDetails {
    status: &'static str,
    checks: BTreeMap<&'static str, ReadinessCheck>,
}

struct ReadinessReport {
    details: ReadinessDetails,
    legacy_failure: Option<&'static str>,
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    // Support --migrate flag for one-shot migration (used by docker migrate service)
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--migrate") {
        return run_migrations_cli().await;
    }
    if args.iter().any(|a| a == "--replay-events") {
        return run_event_replay_cli().await;
    }

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(KernelError::Config(error)) => {
            error!(error = %error, "kernel configuration failed");
            ExitCode::from(78)
        }
        Err(error) => {
            error!(error = %error, "kernel startup failed");
            ExitCode::from(1)
        }
    }
}

/// Run database migrations and exit. Used by the migrate one-shot container.
async fn run_migrations_cli() -> ExitCode {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            error!("DATABASE_URL is required for --migrate");
            return ExitCode::from(78);
        }
    };

    info!("running database migrations...");

    let store = match store::Store::connect(&database_url, 2).await {
        Ok(store) => store,
        Err(error) => {
            error!(error = %error, "failed to connect to database");
            return ExitCode::from(1);
        }
    };

    match store.migrate().await {
        Ok(()) => {
            info!("migrations applied successfully");
            ExitCode::SUCCESS
        }
        Err(error) => {
            error!(error = %error, "migration failed");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReplaySummary {
    scanned: usize,
    replayed: usize,
    last_outbox_id: i64,
}

/// Re-emit canonical events from Postgres without changing outbox state.
///
/// This is intentionally separate from normal startup so an operator cannot
/// accidentally turn a recovery operation into a background replay loop.
async fn run_event_replay_cli() -> ExitCode {
    match replay_events_cli().await {
        Ok(summary) => {
            println!(
                "event replay: ok scanned={} replayed={} last_outbox_id={}",
                summary.scanned, summary.replayed, summary.last_outbox_id
            );
            ExitCode::SUCCESS
        }
        Err(reason) => {
            // Replay validation returns only bounded, source-controlled codes.
            // Keep that operator-facing code visible without reopening the
            // generic `reason` field to arbitrary dependency diagnostics.
            error!(failure_code = reason, "event replay failed");
            ExitCode::from(1)
        }
    }
}

async fn replay_events_cli() -> Result<ReplaySummary, &'static str> {
    if std::env::var("HYDRA_EVENT_REPLAY_CONFIRM").ok().as_deref() != Some("I_UNDERSTAND") {
        return Err("replay_confirmation_required");
    }

    let database_url = required_replay_env("DATABASE_URL", "database_url_required")?;
    let nats_url = required_replay_env("NATS_URL", "nats_url_required")?;
    let after_id = parse_replay_number(
        std::env::var("HYDRA_EVENT_REPLAY_AFTER_ID").ok().as_deref(),
        0,
        0,
        i64::MAX,
        "invalid_replay_cursor",
    )?;
    let limit = parse_replay_number(
        std::env::var("HYDRA_EVENT_REPLAY_LIMIT").ok().as_deref(),
        100,
        1,
        1000,
        "invalid_replay_limit",
    )?;

    let store = store::Store::connect(&database_url, 2)
        .await
        .map_err(|_| "database_connect_failed")?;
    let secure_environment = matches!(
        std::env::var("HYDRA_ENV").ok().as_deref(),
        Some("staging" | "prod")
    );
    let nats_config = NatsTransportConfig::from_environment(nats_url, secure_environment)
        .map_err(|_| "nats_config_invalid")?;
    let nats = nats_config
        .connect()
        .await
        .map_err(|_| "nats_connect_failed")?;
    let publisher = JetStreamEventPublisher::bootstrap(nats, EventStreamConfig::nexus_v1())
        .await
        .map_err(|_| "event_stream_unavailable")?;
    let records = store
        .outbox
        .list_for_replay(after_id, limit)
        .await
        .map_err(|_| "outbox_replay_query_failed")?;

    let scanned = records.len();
    let mut replayed = 0;
    let mut last_outbox_id = after_id;
    for record in records {
        let payload =
            serde_json::to_vec(&record.event).map_err(|_| "event_serialization_failed")?;
        publisher
            .publish(EventPublishRequest {
                event_id: record.event_id,
                subject: record.subject,
                payload,
                trace_context: record.trace_context,
            })
            .await
            .map_err(|_| "event_publish_failed")?;
        replayed += 1;
        last_outbox_id = record.id;
    }

    Ok(ReplaySummary {
        scanned,
        replayed,
        last_outbox_id,
    })
}

fn required_replay_env(name: &str, reason: &'static str) -> Result<String, &'static str> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(reason),
    }
}

fn parse_replay_number(
    raw: Option<&str>,
    default: i64,
    minimum: i64,
    maximum: i64,
    reason: &'static str,
) -> Result<i64, &'static str> {
    let value = match raw {
        Some(value) => value.parse::<i64>().map_err(|_| reason)?,
        None => default,
    };
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(reason)
    }
}

async fn run() -> Result<(), KernelError> {
    let config = Config::validate()?;
    let (bridge_secret_source, bridge_secret_availability) = load_bridge_secret_source(&config)?;
    let nexus_model_gateway_token =
        load_nexus_model_gateway_token(&config, bridge_secret_source.as_ref()).await?;
    let _config_touch = (
        &config.hydra_vault_key,
        &config.hydra_vault_path,
        &config.hydra_base_url,
        config.hydra_env,
        &config.deepseek_api_key,
        &config.anthropic_api_key,
        &config.openai_compat_base_url,
        &config.openai_compat_model,
        config.tk_hit_ratio_target,
        config.tk_output_budget_bytes,
    );
    let store = connect_store(&config).await?;
    let nats = connect_nats(&config).await?;
    let event_publisher = tokio::time::timeout(
        Duration::from_secs(config.dependency_timeout_seconds),
        JetStreamEventPublisher::bootstrap(nats.clone(), EventStreamConfig::nexus_v1()),
    )
    .await
    .map_err(|_| KernelError::EventStreamTimeout)??;
    let relay_health = RelayHealth::default();
    let event_status: Arc<dyn fabric::EventStatusService> = Arc::new(
        EventRuntimeStatusService::new(event_publisher.clone(), relay_health.clone()),
    );

    // Build fabric service layer.
    let session_store = Arc::new(fabric::auth::SessionStore::new(store.sessions.clone()));
    let (nexus_control_plane, external_auth) = build_nexus_control_plane(&config, &store)?;
    let constitution = governor::Constitution {
        monthly_spend_cap_cents: config.governor_monthly_spend_cap_cents,
        pii_egress_allowlist: config.governor_pii_egress_allowlist.clone(),
        blast_entities_ceiling: config.governor_blast_entities_ceiling,
        blast_sends_ceiling: config.governor_blast_sends_ceiling,
        blast_money_ceiling_cents: config.governor_blast_money_ceiling_cents,
    };
    let governor_provider: Arc<dyn fabric::GovernorProvider> = Arc::new(
        PersistedGovernorProvider::new(store.autonomy.clone(), constitution),
    );
    let (runtime_services, executor_worker) = RuntimeServices::build_with_config_and_secrets(
        store.clone(),
        LlmRuntimeConfig {
            egress_proxy_url: config.egress_proxy_url.clone(),
            deepseek_api_key: config.deepseek_api_key.clone(),
            anthropic_api_key: config.anthropic_api_key.clone(),
            openai_compat_base_url: config.openai_compat_base_url.clone(),
            openai_compat_model: config.openai_compat_model.clone(),
            nexus_model_gateway_url: config.nexus_model_gateway_url.clone(),
            nexus_model_gateway_model: config.nexus_model_gateway_model.clone(),
            nexus_model_gateway_token,
            nexus_model_gateway_private: config.nexus_model_gateway_private,
            skills_path: config.hydra_skills_path.clone(),
            skills_trust_file: config.hydra_skills_trust_file.clone(),
            adapters_path: config.hydra_adapters_path.clone(),
            output_budget_bytes: config.tk_output_budget_bytes as usize,
        },
        bridge_secret_source,
        bridge_secret_availability,
    )
    .map_err(|error| KernelError::Runtime(error.to_string()))?;
    let shutdown_state = ShutdownState::default();
    let bridge_sync_scheduler_health = TaskHealth::default();
    let runtime_readiness = RuntimeReadiness {
        bridge_lifecycle_required: config.hydra_adapters_path.is_some(),
        bridge_lifecycle_available: matches!(
            runtime_services.components.bridge_lifecycle,
            RuntimeAvailability::Available
        ),
        bridge_sync_scheduler_required: config.hydra_bridge_sync_scheduler_enabled,
        bridge_sync_scheduler: bridge_sync_scheduler_health.clone(),
        executor_worker: runtime_services.executor_health.clone(),
        shutdown: shutdown_state.clone(),
        dependency_timeout: Duration::from_secs(config.dependency_timeout_seconds),
    };
    info!(components = ?runtime_services.components, "kernel runtime components constructed");
    let mut runtime_capabilities = runtime_services.execution_registry.runtime_capabilities();
    if config.hydra_bridge_sync_scheduler_enabled
        && !runtime_capabilities.contains(fabric::capabilities::RUNTIME_BRIDGE_SYNC_ADAPTER)
    {
        return Err(KernelError::Runtime(
            "bridge sync scheduler requires an available bridge sync execution handler".to_owned(),
        ));
    }
    runtime_capabilities
        .insert(fabric::capabilities::RUNTIME_TENANT_SCOPED_ENVELOPE_GET.to_owned());
    let capability_registry =
        fabric::CapabilityRegistry::nexus_v1_with_runtime_capabilities(runtime_capabilities)
            .map_err(|error| KernelError::Runtime(error.to_string()))?;
    let authorization = Arc::new(fabric::AuthorizationService::new(
        config.nexus_approval_auth_strengths.clone(),
    ));

    let entity_service: Arc<dyn fabric::EntityService> =
        Arc::new(fabric::StoreEntityService::new(store.clone()));
    let envelope_service_impl = Arc::new(
        fabric::StoreEnvelopeService::with_governor_provider(
            store.clone(),
            governor_provider.clone(),
        )
        .with_execution_dispatcher(runtime_services.dispatcher.clone())
        .with_authorization(authorization.clone()),
    );
    let envelope_service: Arc<dyn fabric::EnvelopeService> = envelope_service_impl.clone();
    let autonomy_service: Arc<dyn fabric::AutonomyService> =
        Arc::new(fabric::StoreAutonomyService::new(store.clone()));
    let bridge_service: Arc<dyn fabric::BridgeService> =
        Arc::new(fabric::StoreBridgeService::with_runtime(
            store.clone(),
            governor_provider,
            runtime_services.dispatcher.clone(),
        ));
    let tk_stats_service: Arc<dyn fabric::TkStatsService> = Arc::new(
        fabric::StoreTkStatsService::new(store.ledger.clone(), vec!["concierge".into()]),
    );
    let concierge_service = runtime_services.concierge.clone();
    let a2a_task_service: Arc<dyn fabric::A2aTaskService> =
        Arc::new(fabric::StoreA2aTaskService::new(store.clone()));
    let bridge_synthesis_service = runtime_services
        .bridge_synthesis
        .clone()
        .map(|service| service as Arc<dyn fabric::BridgeSynthesisService>);
    let bridge_conformance_service = runtime_services
        .bridge_conformance
        .clone()
        .map(|service| service as Arc<dyn fabric::BridgeConformanceService>);

    let rate_limiter = Arc::new(RateLimiter::with_store(store.clone(), 60, 60));
    let mut fabric_state = fabric::AppState::new(
        session_store,
        entity_service,
        autonomy_service,
        bridge_service,
        envelope_service,
        tk_stats_service,
        concierge_service,
    )
    .with_a2a_tasks(a2a_task_service)
    .with_authorization(authorization)
    .with_capabilities(Arc::new(capability_registry))
    .with_rate_limiter(rate_limiter.clone())
    .with_nexus_control_plane(nexus_control_plane)
    .with_event_status(event_status.clone())
    .with_development_identity(matches!(config.hydra_env, HydraEnv::Dev))
    .with_secure_cookies(matches!(
        config.hydra_env,
        HydraEnv::Staging | HydraEnv::Prod
    ))
    .with_tenant_data(Arc::new(fabric::StoreTenantDataService::new(store.clone())));
    if let Some(bridge_synthesis_service) = bridge_synthesis_service {
        fabric_state = fabric_state.with_bridge_synthesis(bridge_synthesis_service);
    }
    if let Some(bridge_conformance_service) = bridge_conformance_service {
        fabric_state = fabric_state.with_bridge_conformance(bridge_conformance_service);
    }
    if let Some(external_auth) = external_auth {
        fabric_state = fabric_state.with_external_auth(external_auth);
    }

    // Kernel health-check and metrics routes (use Extension for Store/NATS).
    let kernel_router = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/readyz/details", get(readyz_details))
        .route("/metrics", get(metrics::metrics_handler))
        .layer(Extension(store.clone()))
        .layer(Extension(nats.clone()))
        .layer(Extension(EventReadiness {
            required: config.nexus_integration_enabled,
            status: event_status,
        }));
    let kernel_router = kernel_router.layer(Extension(runtime_readiness));

    // Fabric REST + MCP router (its .with_state is called inside rest::router).
    let fabric_router = fabric::app(fabric_state.clone());

    // Shell server-rendered UI router.
    let shell_router = shell::router(fabric_state);

    // Static assets (vendored htmx).
    let static_router = Router::new().route(
        "/static/htmx.min.js",
        get(|| async {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/javascript")],
                include_str!("../../shell/static/htmx.min.js"),
            )
        }),
    );

    // Merge all routers — each has already resolved its state to ().
    let app = Router::new()
        .merge(kernel_router)
        .merge(fabric_router)
        .merge(shell_router)
        .merge(static_router);

    // Apply security middleware layers (outermost first).
    let app = app
        .layer(axum::middleware::from_fn(security_headers))
        .layer(axum::middleware::from_fn_with_state(
            rate_limiter,
            rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn(
            metrics::request_metrics_middleware,
        ));

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .map_err(|source| KernelError::Bind {
            bind: config.bind,
            source,
        })?;
    let local_addr = listener.local_addr().map_err(KernelError::LocalAddr)?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let bridge_scheduler = hydra_kernel::bridge_scheduler::BridgeSyncScheduler::new(
        store.clone(),
        envelope_service_impl,
        config.hydra_bridge_sync_scheduler_enabled,
        bridge_sync_scheduler_health,
    );
    let relay_handle = tokio::spawn(hydra_kernel::relay::run_with_health(
        shutdown_rx,
        store.outbox.clone(),
        Arc::new(event_publisher),
        relay_health,
    ));
    let executor_handle = tokio::spawn(
        executor_worker.run(runtime_services.executor.clone(), shutdown_tx.subscribe()),
    );
    let scheduler_handle = tokio::spawn(bridge_scheduler.run(shutdown_tx.subscribe()));

    info!("hydra: listening on {local_addr}");

    let shutdown_signal = shutdown_tx.clone();
    let server_shutdown_state = shutdown_state.clone();
    let server_shutdown = shutdown_tx.subscribe();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let reason = wait_for_shutdown(server_shutdown).await;
        info!(?reason, "kernel shutdown requested");
        server_shutdown_state.request();
        let _ = shutdown_signal.send(true);
    })
    .into_future();

    let supervisor = supervise_background_tasks(
        relay_handle,
        executor_handle,
        scheduler_handle,
        shutdown_tx.clone(),
        shutdown_tx.subscribe(),
        shutdown_state.clone(),
        Duration::from_secs(config.shutdown_timeout_seconds),
    );
    tokio::pin!(server);
    tokio::pin!(supervisor);
    let (serve_result, supervisor_result) = tokio::select! {
        result = &mut server => {
            shutdown_state.request();
            let _ = shutdown_tx.send(true);
            (result.map_err(KernelError::Serve), supervisor.await)
        }
        result = &mut supervisor => {
            (server.await.map_err(KernelError::Serve), result)
        }
    };

    serve_result?;
    supervisor_result?;
    match tokio::time::timeout(
        Duration::from_secs(config.dependency_timeout_seconds),
        nats.flush(),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(error = %error, "NATS flush failed during shutdown"),
        Err(_) => warn!("NATS flush timed out during shutdown"),
    }
    Ok(())
}

fn load_bridge_secret_source(
    config: &Config,
) -> Result<(Arc<dyn bridge_host::SecretSource>, RuntimeAvailability), KernelError> {
    load_bridge_secret_source_values(
        config.hydra_env,
        Path::new(&config.hydra_vault_path),
        &config.hydra_vault_key,
    )
}

fn load_bridge_secret_source_values(
    hydra_env: HydraEnv,
    path: &Path,
    passphrase: &str,
) -> Result<(Arc<dyn bridge_host::SecretSource>, RuntimeAvailability), KernelError> {
    if !path.exists() && matches!(hydra_env, HydraEnv::Dev) {
        warn!(
            path = %path.display(),
            "development vault file is absent; bridge secret capability is disabled"
        );
        return Ok((
            Arc::new(bridge_host::StaticSecretSource::default()),
            RuntimeAvailability::Disabled(
                "development vault file is absent; no bridge secrets are available".to_owned(),
            ),
        ));
    }

    let source = bridge_host::VaultSecretSource::load(path, passphrase)
        .map_err(|error| KernelError::Vault(error.to_string()))?;
    Ok((Arc::new(source), RuntimeAvailability::Available))
}

async fn load_nexus_model_gateway_token(
    config: &Config,
    secrets: &dyn bridge_host::SecretSource,
) -> Result<Option<String>, KernelError> {
    let Some(name) = config.nexus_model_gateway_token_secret.as_deref() else {
        return Ok(None);
    };
    let token = secrets
        .get(name)
        .await
        .map_err(|error| KernelError::Vault(format!("read Nexus model gateway secret: {error}")))?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            KernelError::Vault(format!(
                "configured Nexus model gateway secret '{name}' is missing or empty"
            ))
        })?;
    Ok(Some(token))
}

fn build_nexus_control_plane(
    config: &Config,
    store: &store::Store,
) -> Result<
    (
        fabric::NexusControlPlaneConfig,
        Option<Arc<fabric::OidcAuthenticator>>,
    ),
    KernelError,
> {
    let base_url = config.hydra_base_url.trim_end_matches('/');
    let resource = format!("{base_url}/mcp");
    let resource_metadata_url = format!("{base_url}/.well-known/oauth-protected-resource");
    let base_uri = config
        .hydra_base_url
        .parse::<axum::http::Uri>()
        .map_err(|error| KernelError::Nexus(format!("invalid HYDRA_BASE_URL: {error}")))?;
    let mut allowed_hosts = vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "::1".to_owned(),
        config.bind.ip().to_string(),
    ];
    if let Some(authority) = base_uri.authority() {
        allowed_hosts.push(authority.as_str().to_owned());
        allowed_hosts.push(authority.host().to_owned());
    }
    allowed_hosts.sort();
    allowed_hosts.dedup();

    let authorization_servers = config.nexus_oidc_issuer.iter().cloned().collect::<Vec<_>>();
    let control_plane = fabric::NexusControlPlaneConfig {
        enabled: config.nexus_integration_enabled,
        resource,
        resource_metadata_url,
        authorization_servers,
        allowed_hosts,
        allowed_origins: config.nexus_allowed_mcp_origins.clone(),
        max_request_body_bytes: config.nexus_mcp_max_request_bytes,
    };
    if !config.nexus_integration_enabled {
        return Ok((control_plane, None));
    }

    let key_source = match (
        config.nexus_oidc_jwks_url.as_ref(),
        config.nexus_oidc_public_key_file.as_ref(),
    ) {
        (Some(url), None) => fabric::OidcKeySource::JwksUrl(url.clone()),
        (None, Some(path)) => {
            fabric::OidcKeySource::PinnedPublicKey(std::fs::read(path).map_err(|error| {
                KernelError::Nexus(format!("read NEXUS_OIDC_PUBLIC_KEY_FILE '{path}': {error}"))
            })?)
        }
        _ => {
            return Err(KernelError::Nexus(
                "exactly one Nexus OIDC key source is required".to_owned(),
            ));
        }
    };
    let allowed_algorithms = config
        .nexus_oidc_allowed_algorithms
        .iter()
        .map(|algorithm| {
            fabric::parse_algorithm(algorithm)
                .map_err(|error| KernelError::Nexus(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let issuer = config
        .nexus_oidc_issuer
        .clone()
        .ok_or_else(|| KernelError::Nexus("NEXUS_OIDC_ISSUER is required".to_owned()))?;
    let audience = config
        .nexus_oidc_audience
        .clone()
        .ok_or_else(|| KernelError::Nexus("NEXUS_OIDC_AUDIENCE is required".to_owned()))?;
    let authenticator = fabric::OidcAuthenticator::new(
        fabric::NexusOidcConfig {
            provider: "nexus".to_owned(),
            issuer,
            audience,
            key_source,
            allowed_algorithms,
            jwks_cache_ttl: Duration::from_secs(config.nexus_jwks_cache_seconds),
            clock_skew: Duration::from_secs(config.nexus_oidc_clock_skew_seconds),
            egress_proxy_url: config.egress_proxy_url.clone(),
        },
        Arc::new(store.external_bindings.clone()),
    )
    .map_err(|error| KernelError::Nexus(error.to_string()))?;
    Ok((control_plane, Some(Arc::new(authenticator))))
}

fn init_tracing() {
    telemetry::init_telemetry();
}

async fn connect_store(config: &Config) -> Result<store::Store, KernelError> {
    let pool = tokio::time::timeout(
        Duration::from_secs(config.dependency_timeout_seconds),
        store::Store::connect(&config.database_url, 5),
    )
    .await
    .map_err(|_| KernelError::PostgresTimeout)??;
    tokio::time::timeout(
        Duration::from_secs(config.dependency_timeout_seconds),
        pool.health_check(),
    )
    .await
    .map_err(|_| KernelError::PostgresTimeout)??;
    Ok(pool)
}

async fn connect_nats(config: &Config) -> Result<NatsClient, KernelError> {
    let client = tokio::time::timeout(
        Duration::from_secs(config.dependency_timeout_seconds),
        config.nats_transport.connect(),
    )
    .await
    .map_err(|_| KernelError::NatsTimeout)?
    .map_err(|error| KernelError::Nats(error.to_string()))?;
    tokio::time::timeout(
        Duration::from_secs(config.dependency_timeout_seconds),
        client.flush(),
    )
    .await
    .map_err(|_| KernelError::NatsTimeout)?
    .map_err(|error| KernelError::Nats(error.to_string()))?;
    Ok(client)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(
    Extension(store): Extension<store::Store>,
    Extension(nats): Extension<NatsClient>,
    Extension(event_readiness): Extension<EventReadiness>,
    Extension(runtime_readiness): Extension<RuntimeReadiness>,
) -> impl IntoResponse {
    let report = readiness_report(&store, &nats, &event_readiness, &runtime_readiness).await;
    match report.legacy_failure {
        Some(failure) => (StatusCode::SERVICE_UNAVAILABLE, failure),
        None => (StatusCode::OK, "ok"),
    }
}

async fn readyz_details(
    Extension(store): Extension<store::Store>,
    Extension(nats): Extension<NatsClient>,
    Extension(event_readiness): Extension<EventReadiness>,
    Extension(runtime_readiness): Extension<RuntimeReadiness>,
) -> impl IntoResponse {
    let report = readiness_report(&store, &nats, &event_readiness, &runtime_readiness).await;
    let status = if report.legacy_failure.is_some() {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    (status, Json(report.details))
}

async fn readiness_report(
    store: &store::Store,
    nats: &NatsClient,
    event_readiness: &EventReadiness,
    runtime_readiness: &RuntimeReadiness,
) -> ReadinessReport {
    let mut checks = BTreeMap::new();
    let mut legacy_failure = None;

    let shutdown = shutdown_check(&runtime_readiness.shutdown);
    if shutdown.status == "failed" {
        legacy_failure = Some("shutdown");
    }
    checks.insert("shutdown", shutdown);

    let executor_worker = executor_worker_check(&runtime_readiness.executor_worker);
    if executor_worker.status == "failed" {
        legacy_failure.get_or_insert("executor_worker");
    }
    checks.insert("executor_worker", executor_worker);

    let postgres = match tokio::time::timeout(
        runtime_readiness.dependency_timeout,
        store.health_check(),
    )
    .await
    {
        Ok(Ok(())) => readiness_ok(true),
        Ok(Err(error)) => {
            warn!(error = %error, "readyz postgres check failed");
            if legacy_failure.is_none() {
                legacy_failure = Some("postgres");
            }
            readiness_failed(true, "postgres_unavailable")
        }
        Err(_) => {
            warn!("readyz postgres check timed out");
            if legacy_failure.is_none() {
                legacy_failure = Some("postgres");
            }
            readiness_failed(true, "postgres_timeout")
        }
    };
    checks.insert("postgres", postgres);

    let nats_check =
        match tokio::time::timeout(runtime_readiness.dependency_timeout, nats.flush()).await {
            Ok(Ok(())) => readiness_ok(true),
            Ok(Err(error)) => {
                warn!(error = %error, "readyz nats check failed");
                if legacy_failure.is_none() {
                    legacy_failure = Some("nats");
                }
                readiness_failed(true, "nats_unavailable")
            }
            Err(_) => {
                warn!("readyz nats check timed out");
                if legacy_failure.is_none() {
                    legacy_failure = Some("nats");
                }
                readiness_failed(true, "nats_timeout")
            }
        };
    checks.insert("nats", nats_check);

    let events = if event_readiness.required {
        match tokio::time::timeout(
            runtime_readiness.dependency_timeout,
            event_readiness.status.status(),
        )
        .await
        {
            Ok(Ok(status)) if status.available => readiness_ok(true),
            Ok(Ok(status)) => {
                warn!(reason = ?status.reason, "readyz canonical event infrastructure check failed");
                if legacy_failure.is_none() {
                    legacy_failure = Some("events");
                }
                readiness_failed(
                    true,
                    status
                        .reason
                        .as_deref()
                        .unwrap_or("canonical_event_infrastructure_unavailable"),
                )
            }
            Ok(Err(error)) => {
                warn!(error = %error, "readyz canonical event status check failed");
                if legacy_failure.is_none() {
                    legacy_failure = Some("events");
                }
                readiness_failed(true, "canonical_event_status_unavailable")
            }
            Err(_) => {
                warn!("readyz canonical event status check timed out");
                if legacy_failure.is_none() {
                    legacy_failure = Some("events");
                }
                readiness_failed(true, "canonical_event_status_timeout")
            }
        }
    } else {
        readiness_not_required()
    };
    checks.insert("events", events);

    let bridge_lifecycle = bridge_lifecycle_check(
        runtime_readiness.bridge_lifecycle_required,
        runtime_readiness.bridge_lifecycle_available,
    );
    if bridge_lifecycle.status == "failed" {
        warn!("readyz configured bridge lifecycle is unavailable");
        if legacy_failure.is_none() {
            legacy_failure = Some("bridge_lifecycle");
        }
    }
    checks.insert("bridge_lifecycle", bridge_lifecycle);

    let bridge_sync_scheduler = bridge_sync_scheduler_check(
        runtime_readiness.bridge_sync_scheduler_required,
        &runtime_readiness.bridge_sync_scheduler,
    );
    if bridge_sync_scheduler.status == "failed" {
        warn!("readyz configured bridge sync scheduler is unavailable");
        if legacy_failure.is_none() {
            legacy_failure = Some("bridge_sync_scheduler");
        }
    }
    checks.insert("bridge_sync_scheduler", bridge_sync_scheduler);

    let execution_recovery = match tokio::time::timeout(
        runtime_readiness.dependency_timeout,
        store.envelopes.stale_executing_count(),
    )
    .await
    {
        Ok(Ok(stale_count)) => {
            if stale_count > 0 {
                warn!(
                    stale_count,
                    threshold_minutes = STALE_EXECUTION_MINUTES,
                    "readyz found stale in-flight execution"
                );
                if legacy_failure.is_none() {
                    legacy_failure = Some("execution_recovery");
                }
            }
            execution_recovery_check(stale_count)
        }
        Ok(Err(error)) => {
            warn!(error = %error, "readyz execution recovery check failed");
            if legacy_failure.is_none() {
                legacy_failure = Some("execution_recovery");
            }
            readiness_failed(true, "execution_recovery_status_unavailable")
        }
        Err(_) => {
            warn!("readyz execution recovery check timed out");
            if legacy_failure.is_none() {
                legacy_failure = Some("execution_recovery");
            }
            readiness_failed(true, "execution_recovery_timeout")
        }
    };
    checks.insert("execution_recovery", execution_recovery);

    let status = if legacy_failure.is_some() {
        "not_ready"
    } else {
        "ready"
    };
    ReadinessReport {
        details: ReadinessDetails { status, checks },
        legacy_failure,
    }
}

fn readiness_ok(required: bool) -> ReadinessCheck {
    ReadinessCheck {
        status: "ok",
        required,
        reason: None,
    }
}

fn readiness_not_required() -> ReadinessCheck {
    ReadinessCheck {
        status: "not_required",
        required: false,
        reason: None,
    }
}

fn readiness_failed(required: bool, reason: &str) -> ReadinessCheck {
    ReadinessCheck {
        status: "failed",
        required,
        reason: Some(reason.to_owned()),
    }
}

fn shutdown_check(state: &ShutdownState) -> ReadinessCheck {
    if state.is_requested() {
        readiness_failed(true, "shutdown_in_progress")
    } else {
        readiness_ok(true)
    }
}

fn executor_worker_check(health: &TaskHealth) -> ReadinessCheck {
    if health.available() {
        readiness_ok(true)
    } else {
        readiness_failed(true, "executor_worker_unavailable")
    }
}

fn execution_recovery_check(stale_count: i64) -> ReadinessCheck {
    if stale_count == 0 {
        readiness_ok(true)
    } else {
        readiness_failed(true, &format!("execution_recovery_required:{stale_count}"))
    }
}

fn bridge_lifecycle_check(required: bool, available: bool) -> ReadinessCheck {
    if !required {
        readiness_not_required()
    } else if available {
        readiness_ok(true)
    } else {
        readiness_failed(true, "configured_bridge_lifecycle_unavailable")
    }
}

fn bridge_sync_scheduler_check(required: bool, health: &TaskHealth) -> ReadinessCheck {
    if !required {
        readiness_not_required()
    } else if health.available() {
        readiness_ok(true)
    } else {
        readiness_failed(true, "bridge_sync_scheduler_unavailable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_host::EncryptedVault;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_KEY: &str = "test-vault-passphrase-1234";

    #[test]
    fn stale_execution_fails_readiness_without_fabricating_failure() {
        let check = execution_recovery_check(2);
        assert_eq!(check.status, "failed");
        assert!(check.required);
        assert_eq!(
            check.reason.as_deref(),
            Some("execution_recovery_required:2")
        );
    }

    #[test]
    fn clean_execution_recovery_is_ready() {
        assert_eq!(execution_recovery_check(0).status, "ok");
    }

    #[test]
    fn replay_number_defaults_and_enforces_bounds() {
        assert_eq!(parse_replay_number(None, 100, 1, 1000, "invalid"), Ok(100));
        assert_eq!(
            parse_replay_number(Some("1000"), 100, 1, 1000, "invalid"),
            Ok(1000)
        );
        assert_eq!(
            parse_replay_number(Some("0"), 100, 1, 1000, "invalid"),
            Err("invalid")
        );
        assert_eq!(
            parse_replay_number(Some("1001"), 100, 1, 1000, "invalid"),
            Err("invalid")
        );
        assert_eq!(
            parse_replay_number(Some("not-a-number"), 100, 1, 1000, "invalid"),
            Err("invalid")
        );
    }

    #[test]
    fn replay_cursor_accepts_zero_but_rejects_negative_values() {
        assert_eq!(
            parse_replay_number(Some("0"), 0, 0, i64::MAX, "invalid"),
            Ok(0)
        );
        assert_eq!(
            parse_replay_number(Some("-1"), 0, 0, i64::MAX, "invalid"),
            Err("invalid")
        );
    }

    #[test]
    fn executor_worker_readiness_tracks_task_health() {
        let health = TaskHealth::default();
        assert_eq!(
            executor_worker_check(&health).reason.as_deref(),
            Some("executor_worker_unavailable")
        );
        {
            let _guard = health.start();
            assert_eq!(executor_worker_check(&health).status, "ok");
        }
        assert_eq!(executor_worker_check(&health).status, "failed");
    }

    #[test]
    fn shutdown_readiness_fails_only_after_request() {
        let state = ShutdownState::default();
        assert_eq!(shutdown_check(&state).status, "ok");
        state.request();
        assert_eq!(
            shutdown_check(&state).reason.as_deref(),
            Some("shutdown_in_progress")
        );
    }

    fn test_path(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("hydra-kernel-{label}-{stamp}.age"))
    }

    #[test]
    fn development_missing_vault_disables_only_bridge_secrets() {
        let path = test_path("dev-missing");
        let (_, availability) = load_bridge_secret_source_values(HydraEnv::Dev, &path, TEST_KEY)
            .expect("development fallback");
        assert!(matches!(availability, RuntimeAvailability::Disabled(_)));
    }

    #[test]
    fn production_missing_vault_fails_closed() {
        let path = test_path("prod-missing");
        let result = load_bridge_secret_source_values(HydraEnv::Prod, &path, TEST_KEY);
        assert!(matches!(result, Err(KernelError::Vault(_))));
    }

    #[test]
    fn configured_unavailable_bridge_lifecycle_is_not_ready() {
        let check = bridge_lifecycle_check(true, false);
        assert_eq!(check.status, "failed");
        assert!(check.required);
        assert_eq!(
            check.reason.as_deref(),
            Some("configured_bridge_lifecycle_unavailable")
        );
    }

    #[tokio::test]
    async fn valid_vault_is_loaded_into_the_runtime_source() {
        let path = test_path("valid");
        let mut vault = EncryptedVault::new();
        vault
            .set("suitecrm_client_secret", "synthetic-secret")
            .expect("valid synthetic secret");
        vault.save(&path, TEST_KEY).expect("save test vault");

        let (source, availability) =
            load_bridge_secret_source_values(HydraEnv::Staging, &path, TEST_KEY)
                .expect("load staging vault");
        assert_eq!(availability, RuntimeAvailability::Available);
        assert_eq!(
            source
                .get("suitecrm_client_secret")
                .await
                .expect("read runtime source"),
            Some("synthetic-secret".to_owned())
        );
        std::fs::remove_file(path).expect("remove test vault");
    }
}
