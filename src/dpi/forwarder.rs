use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::dpi::strategy::DesyncConfig;
use crate::dpi::tls::parse_client_hello_sni;

/// Handles bidirectional streaming between client and remote server,
/// applying DPI desynchronization splitting to the initial handshake payload.
pub async fn handle_desync_tunnel(
    mut client_stream: TcpStream,
    mut remote_stream: TcpStream,
    target_host: String,
    target_port: u16,
    config: Arc<DesyncConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Disable Nagle's algorithm so each split chunk emits a separate TCP segment immediately
    let _ = client_stream.set_nodelay(true);
    let _ = remote_stream.set_nodelay(true);

    let mut initial_buf = vec![0u8; 8192];
    let n = client_stream.read(&mut initial_buf).await?;
    if n == 0 {
        return Ok(());
    }

    let payload = &initial_buf[..n];
    let sni = parse_client_hello_sni(payload);

    let effective_host = sni
        .as_ref()
        .map(|s| s.hostname.clone())
        .unwrap_or_else(|| target_host.clone());

    let is_targeted = config.is_target(&effective_host);

    if is_targeted {
        let cuts = config.compute_split_points(payload, sni.as_ref());

        if !cuts.is_empty() {
            println!(
                "[DPI-BYPASS] ⚡ Desyncing TLS SNI for {} -> {}:{} | cuts: {:?}",
                effective_host, target_host, target_port, cuts
            );

            // Send segmented chunks with micro-delays
            let mut prev = 0;
            for cut in cuts {
                if cut > prev && cut <= payload.len() {
                    remote_stream.write_all(&payload[prev..cut]).await?;
                    remote_stream.flush().await?;
                    if config.delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(config.delay_ms)).await;
                    }
                    prev = cut;
                }
            }

            if prev < payload.len() {
                remote_stream.write_all(&payload[prev..]).await?;
                remote_stream.flush().await?;
            }
        } else {
            remote_stream.write_all(payload).await?;
            remote_stream.flush().await?;
        }
    } else {
        // Direct pass-through for untargeted traffic
        remote_stream.write_all(payload).await?;
        remote_stream.flush().await?;
    }

    // Full-duplex bidirectional bridge
    let _ = tokio::io::copy_bidirectional(&mut client_stream, &mut remote_stream).await;

    Ok(())
}
