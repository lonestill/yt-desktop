use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::dpi::strategy::DesyncConfig;
use crate::dpi::tls::{parse_client_hello_sni, replace_client_hello_sni, FAKE_YANDEX_CHELLO};

/// Handles bidirectional streaming between client and remote server,
/// applying fake SNI packet spoofing (yandex.ru) and DPI desynchronization splitting.
pub async fn handle_desync_tunnel(
    mut client_stream: TcpStream,
    mut remote_stream: TcpStream,
    target_host: String,
    target_port: u16,
    config: Arc<DesyncConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        // 1. Inject FAKE SNI packet with low TTL (yandex.ru at TTL 5)
        if let Some(ref fake_sni) = config.fake_sni {
            let fake_bytes = if fake_sni == "yandex.ru" {
                FAKE_YANDEX_CHELLO.to_vec()
            } else if let Some(rewritten) = replace_client_hello_sni(payload, fake_sni) {
                rewritten
            } else {
                FAKE_YANDEX_CHELLO.to_vec()
            };

            println!(
                "[DPI-BYPASS] 🎭 Spoofing SNI: injected fake packet for '{}' with TTL {} before connecting to {}",
                fake_sni, config.fake_ttl, effective_host
            );

            // Set low TTL so fake packet dies after crossing TSPU, before reaching destination
            let _ = remote_stream.set_ttl(config.fake_ttl);
            let _ = remote_stream.write_all(&fake_bytes).await;
            let _ = remote_stream.flush().await;

            if config.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(config.delay_ms)).await;
            }

            // Restore normal TTL for real handshake
            let _ = remote_stream.set_ttl(64);
        }

        // 2. Send real handshake with MultiSplit segmentation
        let cuts = config.compute_split_points(payload, sni.as_ref());
        if !cuts.is_empty() {
            println!(
                "[DPI-BYPASS] ⚡ Desyncing real handshake for {} -> {}:{} | cuts: {:?}",
                effective_host, target_host, target_port, cuts
            );

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
        remote_stream.write_all(payload).await?;
        remote_stream.flush().await?;
    }

    let _ = tokio::io::copy_bidirectional(&mut client_stream, &mut remote_stream).await;
    Ok(())
}
