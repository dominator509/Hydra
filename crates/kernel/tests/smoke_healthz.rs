use std::io::{self, Read};
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::sleep;

const HEALTHZ_WAIT_ATTEMPTS: usize = 120;
const HEALTHZ_WAIT_INTERVAL: Duration = Duration::from_millis(100);

#[tokio::test]
async fn smoke_healthz() -> Result<(), Box<dyn std::error::Error>> {
    let db = store::TestDb::new().await?;
    let result: Result<(), Box<dyn std::error::Error>> = async {
        let database_url = db.scoped_database_url()?;
        let addr = reserve_addr()?;
        let bin = std::env::var("CARGO_BIN_EXE_hydra-kernel")?;
        let mut child = Command::new(bin)
            .env("HYDRA_BIND", addr.to_string())
            .env("DATABASE_URL", database_url)
            .env(
                "NATS_URL",
                std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_owned()),
            )
            .env("HYDRA_VAULT_KEY", "SET_LOCAL_DEV_VAULT_KEY")
            .env("HYDRA_BASE_URL", "http://127.0.0.1:8080")
            .env("HYDRA_ENV", "dev")
            .env("TK_HIT_RATIO_TARGET", "0.97")
            .env("TK_OUTPUT_BUDGET_BYTES", "16384")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let result: Result<(), Box<dyn std::error::Error>> = async {
            let healthz = wait_for_endpoint(addr, "/healthz", &mut child).await?;
            let readyz = wait_for_endpoint(addr, "/readyz", &mut child).await?;
            let readyz_details = wait_for_endpoint(addr, "/readyz/details", &mut child).await?;

            if !healthz.contains("200 OK") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected HTTP 200 from /healthz, got response: {healthz}"),
                )
                .into());
            }

            if !healthz.contains("\r\n\r\nok") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected body 'ok' from /healthz, got response: {healthz}"),
                )
                .into());
            }

            if !readyz.contains("200 OK") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected HTTP 200 from /readyz, got response: {readyz}"),
                )
                .into());
            }

            if !readyz.contains("\r\n\r\nok") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected body 'ok' from /readyz, got response: {readyz}"),
                )
                .into());
            }

            if !readyz_details.contains("200 OK")
                || !readyz_details.contains("\"status\":\"ready\"")
                || !readyz_details.contains("\"postgres\"")
                || !readyz_details.contains("\"nats\"")
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "expected structured readiness details, got response: {readyz_details}"
                    ),
                )
                .into());
            }

            Ok(())
        }
        .await;
        let shutdown = shutdown_child(&mut child);

        result?;
        shutdown?;
        Ok(())
    }
    .await;
    let cleanup = db.cleanup().await;

    result?;
    cleanup?;
    Ok(())
}

fn reserve_addr() -> io::Result<SocketAddr> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    drop(listener);
    Ok(addr)
}

async fn wait_for_endpoint(
    addr: SocketAddr,
    path: &str,
    child: &mut Child,
) -> Result<String, Box<dyn std::error::Error>> {
    // Windows child-process startup can occasionally lag enough to miss the
    // original 6s budget even when the kernel is healthy; keep the smoke gate
    // deterministic by allowing a modest 12s startup window.
    for _ in 0..HEALTHZ_WAIT_ATTEMPTS {
        if let Some(status) = child.try_wait()? {
            let mut stdout = String::new();
            if let Some(mut pipe) = child.stdout.take() {
                pipe.read_to_string(&mut stdout)?;
            }
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                pipe.read_to_string(&mut stderr)?;
            }
            return Err(io::Error::other(format!(
                "hydra-kernel exited before {path} was reachable: {status}; stdout: {}; stderr: {}",
                stdout.trim(),
                stderr.trim()
            ))
            .into());
        }

        match fetch_path(addr, path).await {
            Ok(response)
                if response.starts_with("HTTP/1.1") || response.starts_with("HTTP/1.0") =>
            {
                return Ok(response);
            }
            Ok(_) | Err(_) => sleep(HEALTHZ_WAIT_INTERVAL).await,
        }
    }

    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("timed out waiting for {path} on {addr}"),
    )
    .into())
}

async fn fetch_path(addr: SocketAddr, path: &str) -> io::Result<String> {
    let mut stream = TcpStream::connect(addr).await?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    stream.flush().await?;

    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await?;

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn shutdown_child(child: &mut Child) -> io::Result<()> {
    if child.try_wait()?.is_none() {
        child.kill()?;
    }

    let _ = child.wait()?;
    Ok(())
}
