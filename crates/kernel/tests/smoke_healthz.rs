use std::io;
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
    let addr = reserve_addr()?;
    let bin = std::env::var("CARGO_BIN_EXE_hydra-kernel")?;
    let mut child = Command::new(bin)
        .env("HYDRA_BIND", addr.to_string())
        .env(
            "DATABASE_URL",
            std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://hydra:hydra@localhost:5432/hydra".to_owned()),
        )
        .env(
            "NATS_URL",
            std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_owned()),
        )
        .env("HYDRA_VAULT_KEY", "SET_LOCAL_DEV_VAULT_KEY")
        .env("HYDRA_BASE_URL", "http://127.0.0.1:8080")
        .env("HYDRA_ENV", "dev")
        .env("TK_HIT_RATIO_TARGET", "0.97")
        .env("TK_OUTPUT_BUDGET_BYTES", "16384")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let healthz = wait_for_endpoint(addr, "/healthz", &mut child).await;
    let readyz = wait_for_endpoint(addr, "/readyz", &mut child).await;
    let _ = shutdown_child(&mut child);
    let healthz = healthz?;
    let readyz = readyz?;

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
            return Err(io::Error::other(format!(
                "hydra-kernel exited before {path} was reachable: {status}"
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
