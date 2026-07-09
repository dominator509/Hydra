//! layer L6 operations entrypoint and health surface placeholder for EP-003 persistence work.

mod config;
mod metrics;
mod relay;
mod telemetry;

use std::process::ExitCode;
use std::sync::Arc;

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

use fabric::{middleware::security_headers, rate::rate_limit_middleware};
use crate::config::{Config, ConfigError};

#[derive(Debug, thiserror::Error)]
enum KernelError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("failed to connect postgres: {0}")]
    Postgres(#[from] sqlx::Error),
    #[error("failed to connect nats: {0}")]
    Nats(String),
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
        config.tk_hit_ratio_target,
        config.tk_output_budget_bytes,
    );
    let pool = connect_pool(&config).await?;
    let nats = connect_nats(&config).await?;

    // Build fabric service layer.
    let store = store::Store::new(pool.clone());
    let session_store = Arc::new(fabric::auth::SessionStore::new(pool.clone()));

    // Governor does not implement Clone; create two separate instances.
    let entity_service: Arc<dyn fabric::EntityService> =
        Arc::new(fabric::StoreEntityService::new(store.clone()));
    let envelope_service: Arc<dyn fabric::EnvelopeService> = Arc::new(
        fabric::StoreEnvelopeService::new(store.clone(), fabric::services::demo_governor()),
    );
    let autonomy_service: Arc<dyn fabric::AutonomyService> =
        Arc::new(fabric::StoreAutonomyService::new(store.clone()));
    let bridge_service: Arc<dyn fabric::BridgeService> = Arc::new(fabric::StoreBridgeService::new(
        store.clone(),
        fabric::services::demo_governor(),
    ));
    let tk_stats_service: Arc<dyn fabric::TkStatsService> = Arc::new(
        fabric::StoreTkStatsService::new(store.ledger.clone(), vec!["concierge".into()]),
    );
    let concierge_service: Arc<dyn fabric::ConciergeService> =
        Arc::new(fabric::ConciergeServiceImpl);

    let fabric_state = fabric::AppState::new(
        session_store,
        entity_service,
        autonomy_service,
        bridge_service,
        envelope_service,
        tk_stats_service,
        concierge_service,
    );

    // Kernel health-check and metrics routes (use Extension for pool/nats).
    let kernel_router = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics::metrics_handler))
        .layer(Extension(pool.clone()))
        .layer(Extension(nats.clone()));

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
        .layer(axum::middleware::from_fn(rate_limit_middleware));

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .map_err(|source| KernelError::Bind {
            bind: config.bind,
            source,
        })?;
    let local_addr = listener.local_addr().map_err(KernelError::LocalAddr)?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let relay_handle = tokio::spawn(relay::run(shutdown_rx, pool.clone(), nats.clone()));

    info!("hydra: listening on {local_addr}");

    let shutdown_signal = shutdown_tx.clone();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        if let Err(error) = tokio::signal::ctrl_c().await {
            warn!(error = %error, "ctrl_c listener failed; shutting down kernel");
        }
        let _ = shutdown_signal.send(true);
    });

    let serve_result = server.await.map_err(KernelError::Serve);
    let _ = shutdown_tx.send(true);
    let relay_result = relay_handle.await.map_err(KernelError::RelayJoin);
    let _ = nats.flush().await;

    serve_result?;
    relay_result?;
    Ok(())
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
) -> impl IntoResponse {
    if let Err(error) = sqlx::query!("SELECT 1 as \"one!\"").fetch_one(&pool).await {
        warn!(error = %error, "readyz postgres check failed");
        return (StatusCode::SERVICE_UNAVAILABLE, "postgres");
    }

    if let Err(error) = nats.flush().await {
        warn!(error = %error, "readyz nats check failed");
        return (StatusCode::SERVICE_UNAVAILABLE, "nats");
    }

    (StatusCode::OK, "ok")
}
