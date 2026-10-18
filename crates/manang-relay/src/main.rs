//! `manang-relay`: the self-hosted ciphertext relay.
//!
//! ```text
//! manang-relay [--listen 127.0.0.1:8787] [--max-frame-bytes N] [--max-rooms N]
//!              [--max-peers-per-room N] [--idle-timeout-secs N]
//! ```
//!
//! Logs are JSON lines on stderr (`RUST_LOG` selects the level) and never
//! contain payloads. Terminate with SIGINT or SIGTERM; there is no state to flush.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;

use manang_relay::RelayConfig;
use tracing_subscriber::EnvFilter;

fn parse_args() -> Result<(SocketAddr, RelayConfig), String> {
    let mut addr: SocketAddr = "127.0.0.1:8787"
        .parse()
        .map_err(|e| format!("bad default address: {e}"))?;
    let mut config = RelayConfig::default();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        let num = |s: String, name: &str| {
            s.parse::<u64>()
                .map_err(|_| format!("{name} must be a number"))
        };
        match flag.as_str() {
            "--listen" => {
                addr = value("--listen")?
                    .parse()
                    .map_err(|e| format!("--listen: {e}"))?;
            }
            "--max-frame-bytes" => {
                config.max_frame_bytes =
                    num(value("--max-frame-bytes")?, "--max-frame-bytes")? as usize;
            }
            "--max-rooms" => {
                config.max_rooms = num(value("--max-rooms")?, "--max-rooms")? as usize;
            }
            "--max-peers-per-room" => {
                config.max_peers_per_room =
                    num(value("--max-peers-per-room")?, "--max-peers-per-room")? as usize;
            }
            "--idle-timeout-secs" => {
                config.idle_timeout =
                    Duration::from_secs(num(value("--idle-timeout-secs")?, "--idle-timeout-secs")?);
            }
            "--help" | "-h" => {
                return Err("usage: manang-relay [--listen ADDR] [--max-frame-bytes N] [--max-rooms N] [--max-peers-per-room N] [--idle-timeout-secs N]".to_owned());
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok((addr, config))
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let (addr, config) = match parse_args() {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    let relay = match manang_relay::spawn(addr, config).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot listen on {addr}: {e}");
            return ExitCode::FAILURE;
        }
    };
    wait_for_shutdown().await;
    relay.stop().await;
    ExitCode::SUCCESS
}

/// Resolve on SIGINT, or SIGTERM where available (systemd, containers).
async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    if tokio::signal::ctrl_c().await.is_err() {
        // No signal support: run until killed.
        std::future::pending::<()>().await;
    }
}
