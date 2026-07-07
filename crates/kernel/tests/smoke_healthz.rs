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
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let response = wait_for_healthz(addr, &mut child).await;
    let _ = shutdown_child(&mut child);
    let response = response?;

    if !response.contains("200 OK") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected HTTP 200 from /healthz, got response: {response}"),
        )
        .into());
    }

    if !response.contains("\r\n\r\nok") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected body 'ok' from /healthz, got response: {response}"),
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

async fn wait_for_healthz(
    addr: SocketAddr,
    child: &mut Child,
) -> Result<String, Box<dyn std::error::Error>> {
    // Windows child-process startup can occasionally lag enough to miss the
    // original 6s budget even when the kernel is healthy; keep the smoke gate
    // deterministic by allowing a modest 12s startup window.
    for _ in 0..HEALTHZ_WAIT_ATTEMPTS {
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!(
                "hydra-kernel exited before /healthz was reachable: {status}"
            ))
            .into());
        }

        match fetch_healthz(addr).await {
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
        format!("timed out waiting for /healthz on {addr}"),
    )
    .into())
}

async fn fetch_healthz(addr: SocketAddr) -> io::Result<String> {
    let mut stream = TcpStream::connect(addr).await?;
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    stream.shutdown().await?;

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
