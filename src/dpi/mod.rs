pub mod forwarder;
pub mod server;
pub mod strategy;
pub mod tls;

use std::sync::Arc;
use strategy::DesyncConfig;
use tokio::net::TcpListener;

pub struct DpiProxyHandle {
    pub port: u16,
}

/// Spawns the embedded anti-DPI proxy on a background Tokio runtime using an ephemeral port.
/// Returns the bound port for WKWebView proxy configuration.
pub fn start_dpi_proxy() -> Result<DpiProxyHandle, Box<dyn std::error::Error + Send + Sync>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
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

            let _ = tx.send(Ok(port));

            let config = Arc::new(DesyncConfig::default());
            let _ = server::run_embedded_proxy(listener, config).await;
        });
    });

    let port = rx
        .recv()?
        .map_err(|e| format!("Failed to start DPI proxy: {e}"))?;

    println!("[DPI-BYPASS] ⚡ Embedded anti-DPI proxy online at 127.0.0.1:{port}");
    Ok(DpiProxyHandle { port })
}
