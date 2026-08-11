//! layer L6 operations entrypoint and health surface placeholder for EP-003 persistence work.

mod config;
mod metrics;
mod telemetry;

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use async_nats::Client as NatsClient;
use axum::{
    extract::Extension,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::config::{Config, ConfigError, HydraEnv};
use fabric::{
    middleware::security_headers,
    rate::{rate_limit_middleware, RateLimiter},
};
use hydra_kernel::event_status::{required_event_infrastructure_ready, EventRuntimeStatusService};
use hydra_kernel::event_stream::{EventStreamConfig, JetStreamEventPublisher};
use hydra_kernel::policy_provider::PersistedGovernorProvider;
use hydra_kernel::relay::RelayHealth;
use hydra_kernel::runtime_services::{LlmRuntimeConfig, RuntimeServices};

#[derive(Debug, thiserror::Error)]
enum KernelError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("failed to connect postgres: {0}")]
    Postgres(#[from] sqlx::Error),
    #[error("failed to connect nats: {0}")]
    Nats(String),
    #[error("failed to initialize the canonical event stream: {0}")]
    EventStream(#[from] hydra_kernel::event_stream::EventStreamError),
    #[error("failed to bind {bind}: {source}")]
    Bind {
        bind: std::net::SocketAddr,
        source: std::io::Error,
    },
    #[error("failed to read local address: {0}")]
    LocalAddr(std::io::Error),
    #[error("server exited with error: {0}")]
    Serve(std::io::Error),
    #[error("relay task join error: {0}")]
    RelayJoin(tokio::task::JoinError),
    #[error("failed to configure Nexus interoperability: {0}")]
    Nexus(String),
    #[error("failed to construct kernel runtime services: {0}")]
    Runtime(String),
    #[error("executor task join error: {0}")]
    ExecutorJoin(tokio::task::JoinError),
}

#[derive(Clone)]
struct EventReadiness {
    required: bool,
    status: Arc<dyn fabric::EventStatusService>,
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    // Support --migrate flag for one-shot migration (used by docker migrate service)
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--migrate") {
        return run_migrations_cli().await;
    }

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(KernelError::Config(error)) => {
            error!("{error}");
            ExitCode::from(78)
        }
        Err(error) => {
            error!("{error}");
            ExitCode::from(1)
        }
    }
}

/// Run sqlx migrations and exit. Used by the migrate one-shot container.
async fn run_migrations_cli() -> ExitCode {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            error!("DATABASE_URL is required for --migrate");
            return ExitCode::from(78);
        }
    };

    info!("running database migrations...");

    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            error!("failed to connect to database: {error}");
            return ExitCode::from(1);
        }
    };

    match store::run_migrations(&pool).await {
        Ok(()) => {
            info!("migrations applied successfully");
            ExitCode::SUCCESS
        }
        Err(error) => {
            error!("migration failed: {error}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<(), KernelError> {
    let config = Config::validate()?;
    let _config_touch = (
        &config.hydra_vault_key,
        &config.hydra_base_url,
        config.hydra_env,
        &config.deepseek_api_key,
        &config.anthropic_api_key,
        &config.openai_compat_base_url,
        &config.openai_compat_model,
        config.tk_hit_ratio_target,
        config.tk_output_budget_bytes,
    );
    let pool = connect_pool(&config).await?;
    let nats = connect_nats(&config).await?;
    let event_publisher =
        JetStreamEventPublisher::bootstrap(nats.clone(), EventStreamConfig::nexus_v1()).await?;
    let relay_health = RelayHealth::default();
    let event_status: Arc<dyn fabric::EventStatusService> = Arc::new(
        EventRuntimeStatusService::new(event_publisher.clone(), relay_health.clone()),
    );

    // Build fabric service layer.
    let store = store::Store::new(pool.clone());
    let session_store = Arc::new(fabric::auth::SessionStore::new(pool.clone()));
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
    let (runtime_services, executor_worker) = RuntimeServices::build_with_config(
        store.clone(),
        LlmRuntimeConfig {
            deepseek_api_key: config.deepseek_api_key.clone(),
            anthropic_api_key: config.anthropic_api_key.clone(),
            openai_compat_base_url: config.openai_compat_base_url.clone(),
            openai_compat_model: config.openai_compat_model.clone(),
            output_budget_bytes: config.tk_output_budget_bytes as usize,
        },
    )
    .map_err(|error| KernelError::Runtime(error.to_string()))?;
    info!(components = ?runtime_services.components, "kernel runtime components constructed");
    let mut runtime_capabilities = runtime_services.execution_registry.runtime_capabilities();
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
    let envelope_service: Arc<dyn fabric::EnvelopeService> = Arc::new(
        fabric::StoreEnvelopeService::with_governor_provider(
            store.clone(),
            governor_provider.clone(),
        )
        .with_execution_dispatcher(runtime_services.dispatcher.clone())
        .with_authorization(authorization.clone()),
    );
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

    let rate_limiter = Arc::new(RateLimiter::new(60, 60));
    let mut fabric_state = fabric::AppState::new(
        session_store,
        entity_service,
        autonomy_service,
        bridge_service,
        envelope_service,
        tk_stats_service,
        concierge_service,
    )
    .with_authorization(authorization)
    .with_capabilities(Arc::new(capability_registry))
    .with_rate_limiter(rate_limiter.clone())
    .with_nexus_control_plane(nexus_control_plane)
    .with_event_status(event_status.clone())
    .with_development_identity(matches!(config.hydra_env, HydraEnv::Dev));
    if let Some(external_auth) = external_auth {
        fabric_state = fabric_state.with_external_auth(external_auth);
    }

    // Kernel health-check and metrics routes (use Extension for pool/nats).
    let kernel_router = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics::metrics_handler))
        .layer(Extension(pool.clone()))
        .layer(Extension(nats.clone()))
        .layer(Extension(EventReadiness {
            required: config.nexus_integration_enabled,
            status: event_status,
        }));

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
        ));

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .map_err(|source| KernelError::Bind {
            bind: config.bind,
            source,
        })?;
    let local_addr = listener.local_addr().map_err(KernelError::LocalAddr)?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let relay_handle = tokio::spawn(hydra_kernel::relay::run_with_health(
        shutdown_rx,
        store.outbox.clone(),
        Arc::new(event_publisher),
        relay_health,
    ));
    let executor_handle = tokio::spawn(
        executor_worker.run(runtime_services.executor.clone(), shutdown_tx.subscribe()),
    );

    info!("hydra: listening on {local_addr}");

    let shutdown_signal = shutdown_tx.clone();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        if let Err(error) = tokio::signal::ctrl_c().await {
            warn!(error = %error, "ctrl_c listener failed; shutting down kernel");
        }
        let _ = shutdown_signal.send(true);
    });

    let serve_result = server.await.map_err(KernelError::Serve);
    let _ = shutdown_tx.send(true);
    let relay_result = relay_handle.await.map_err(KernelError::RelayJoin);
    let executor_result = executor_handle.await.map_err(KernelError::ExecutorJoin);
    let _ = nats.flush().await;

    serve_result?;
    relay_result?;
    executor_result?;
    Ok(())
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
        },
        Arc::new(store.external_bindings.clone()),
    )
    .map_err(|error| KernelError::Nexus(error.to_string()))?;
    Ok((control_plane, Some(Arc::new(authenticator))))
}

fn init_tracing() {
    telemetry::init_telemetry();
}

async fn connect_pool(config: &Config) -> Result<PgPool, KernelError> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    sqlx::query!("SELECT 1 as \"one!\"")
        .fetch_one(&pool)
        .await?;
    Ok(pool)
}

async fn connect_nats(config: &Config) -> Result<NatsClient, KernelError> {
    let client = async_nats::connect(&config.nats_url)
        .await
        .map_err(|error| KernelError::Nats(error.to_string()))?;
    client
        .flush()
        .await
        .map_err(|error| KernelError::Nats(error.to_string()))?;
    Ok(client)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(
    Extension(pool): Extension<PgPool>,
    Extension(nats): Extension<NatsClient>,
    Extension(event_readiness): Extension<EventReadiness>,
) -> impl IntoResponse {
    if let Err(error) = sqlx::query!("SELECT 1 as \"one!\"").fetch_one(&pool).await {
        warn!(error = %error, "readyz postgres check failed");
        return (StatusCode::SERVICE_UNAVAILABLE, "postgres");
    }

    if let Err(error) = nats.flush().await {
        warn!(error = %error, "readyz nats check failed");
        return (StatusCode::SERVICE_UNAVAILABLE, "nats");
    }

    if !required_event_infrastructure_ready(
        event_readiness.required,
        event_readiness.status.as_ref(),
    )
    .await
    {
        warn!("readyz canonical event infrastructure check failed");
        return (StatusCode::SERVICE_UNAVAILABLE, "events");
    }

    (StatusCode::OK, "ok")
}
