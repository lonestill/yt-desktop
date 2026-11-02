pub mod forwarder;
pub mod server;
pub mod socks5;
pub mod strategy;
pub mod tls;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use strategy::DesyncConfig;
use tokio::net::TcpListener;

pub struct DpiProxyHandle {
    pub port: u16,
    child: Option<Child>,
}

impl Drop for DpiProxyHandle {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}

fn find_tpws_binary() -> Option<PathBuf> {
    // 1. Next to current executable (inside YouTube.app/Contents/MacOS/tpws)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("tpws");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // 2. Local workspace candidates
    let candidate1 = Path::new("tpws/tpws");
    if candidate1.is_file() {
        return Some(candidate1.to_path_buf());
    }
    let candidate2 = Path::new("target/release/tpws");
    if candidate2.is_file() {
        return Some(candidate2.to_path_buf());
    }
    None
}

/// Resolves a hostname strictly prioritizing IPv4.
/// Critical for avoiding broken/dead IPv6 routes on macOS systems without IPv6 gateways.
pub async fn resolve_ipv4(host: &str, port: u16) -> Result<SocketAddr, std::io::Error> {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    let addrs = tokio::net::lookup_host(format!("{}:{}", host, port)).await?;
    for addr in addrs {
        if addr.is_ipv4() {
            return Ok(addr);
        }
    }
    // Fallback if no IPv4
    tokio::net::lookup_host(format!("{}:{}", host, port))
        .await?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No IP address found for host"))
}

/// Spawns the embedded anti-DPI proxy.
/// Uses the battle-tested native Zapret TPWS engine (`--split-pos=midsld --disorder=tls`)
/// if available, with automatic fallback to the internal Tokio SOCKS5 engine.
pub fn start_dpi_proxy() -> Result<DpiProxyHandle, Box<dyn std::error::Error + Send + Sync>> {
    // Check for native zapret engine first (proven to bypass Rostelecom & Russian TSPU)
    if let Some(tpws_path) = find_tpws_binary() {
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0")?;
            l.local_addr()?.port()
        };

        match Command::new(&tpws_path)
            .args([
                "--socks",
                "--bind-addr=127.0.0.1",
                &format!("--port={port}"),
                "--split-pos=midsld",
                "--disorder=tls",
                "--debug=2",
            ])
            .spawn()
        {
            Ok(child) => {
                println!(
                    "[DPI-BYPASS] 🚀 Powered by Zapret TPWS at 127.0.0.1:{port} (mode: midsld + disorder=tls)"
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
                return Ok(DpiProxyHandle {
                    port,
                    child: Some(child),
                });
            }
            Err(e) => {
                eprintln!("[DPI-BYPASS] ⚠️ Failed to spawn tpws ({e}), falling back to internal engine");
            }
        }
    }

    // Fallback: in-process Tokio proxy
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;

    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        rt.block_on(async move {
            let listener = match TcpListener::bind("127.0.0.1:0").await {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            };

            let port = match listener.local_addr() {
                Ok(addr) => addr.port(),
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            };

            let config = Arc::new(DesyncConfig::default());
            let vpn_status = match crate::dpi::strategy::detect_vpn_interface() {
                Some(ref iface) => format!("VPN Active ({iface}) -> Auto Pass-through"),
                None => "Direct ISP -> Anti-DPI Desync".to_string(),
            };
            println!(
                "[DPI-BYPASS] ⚡ Proxy online at 127.0.0.1:{port} | Mode: {:?} ({vpn_status}) | Delay: {}ms",
                config.mode, config.delay_ms
            );

            let _ = tx.send(Ok(port));
            let _ = server::run_embedded_proxy(listener, config).await;
        });
    });

    let port = rx
        .recv()?
        .map_err(|e| format!("Failed to start DPI proxy: {e}"))?;

    Ok(DpiProxyHandle { port, child: None })
}
