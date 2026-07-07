//! layer L6 operations entrypoint and health surface placeholder for EP-001.

use std::env;
use std::net::{AddrParseError, SocketAddr};

use axum::{routing::get, Router};
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Error)]
enum KernelError {
    #[error("invalid HYDRA_BIND value '{raw}': {source}")]
    InvalidBind { raw: String, source: AddrParseError },
    #[error("failed to bind {bind}: {source}")]
    Bind {
        bind: SocketAddr,
        source: std::io::Error,
    },
    #[error("failed to read local address: {0}")]
    LocalAddr(std::io::Error),
    #[error("server exited with error: {0}")]
    Serve(std::io::Error),
}

#[derive(Clone, Debug)]
struct Config {
    bind: SocketAddr,
    database_url: Option<String>,
    nats_url: Option<String>,
}

impl Config {
    fn validate() -> Result<Self, KernelError> {
        let bind_raw = env::var("HYDRA_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
        let bind = bind_raw
            .parse()
            .map_err(|source| KernelError::InvalidBind {
                raw: bind_raw,
                source,
            })?;

        Ok(Self {
            bind,
            database_url: env::var("DATABASE_URL").ok(),
            nats_url: env::var("NATS_URL").ok(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), KernelError> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();

    let config = Config::validate()?;
    if config.database_url.is_none() {
        warn!("DATABASE_URL missing; continuing because EP-001 only requires optional DB wiring.");
    }
    if config.nats_url.is_none() {
        warn!("NATS_URL missing; continuing because EP-001 only requires optional NATS wiring.");
    }

    let app = Router::new().route("/healthz", get(healthz));
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .map_err(|source| KernelError::Bind {
            bind: config.bind,
            source,
        })?;
    let local_addr = listener.local_addr().map_err(KernelError::LocalAddr)?;

    tracing::info!("hydra: listening on {local_addr}");

    axum::serve(listener, app).await.map_err(KernelError::Serve)
}

async fn healthz() -> &'static str {
    "ok"
}
