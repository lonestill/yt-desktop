use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::dpi::strategy::DesyncConfig;
use crate::dpi::tls::{parse_client_hello_sni, replace_client_hello_sni, FAKE_YANDEX_CHELLO};

static CONN_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// Handles bidirectional streaming between client and remote server,
/// applying DPI desynchronization splitting with comprehensive diagnostic telemetry.
pub async fn handle_desync_tunnel(
    mut client_stream: TcpStream,
    mut remote_stream: TcpStream,
    target_host: String,
    target_port: u16,
    config: Arc<DesyncConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn_id = CONN_COUNTER.fetch_add(1, Ordering::Relaxed);
    let _ = client_stream.set_nodelay(true);
    let _ = remote_stream.set_nodelay(true);

    let mut initial_buf = vec![0u8; 8192];
    let n = client_stream.read(&mut initial_buf).await?;
    if n == 0 {
        println!("[CONN #{conn_id}] ⚠️ Client closed before sending data to {target_host}:{target_port}");
        return Ok(());
    }

    let payload = &initial_buf[..n];
    let sni = parse_client_hello_sni(payload);

    let effective_host = sni
        .as_ref()
        .map(|s| s.hostname.clone())
        .unwrap_or_else(|| target_host.clone());

    println!(
        "[CONN #{conn_id}] 📡 Client -> {target_host}:{target_port} (read {n} bytes, SNI: {})",
        sni.as_ref().map(|s| s.hostname.as_str()).unwrap_or("none")
    );

    let is_targeted = config.is_target(&effective_host);

    if is_targeted {
        // 1. Optional Fake Packet Injection (only if explicitly enabled via DESYNC_FAKE=1)
        if let Some(ref fake_sni) = config.fake_sni {
            let fake_bytes = if fake_sni == "yandex.ru" {
                FAKE_YANDEX_CHELLO.to_vec()
            } else if let Some(rewritten) = replace_client_hello_sni(payload, fake_sni) {
                rewritten
            } else {
                FAKE_YANDEX_CHELLO.to_vec()
            };

            println!(
                "[CONN #{conn_id}] 🎭 FAKE SNI: Sending fake packet for '{}' with TTL {}",
                fake_sni, config.fake_ttl
            );

            let _ = remote_stream.set_ttl(config.fake_ttl);
            let _ = remote_stream.write_all(&fake_bytes).await;
            let _ = remote_stream.flush().await;

            if config.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(config.delay_ms)).await;
            }

            let _ = remote_stream.set_ttl(64);
        }

        // 2. Real ClientHello Segmentation
        let cuts = config.compute_split_points(payload, sni.as_ref());
        if !cuts.is_empty() {
            println!(
                "[CONN #{conn_id}] ⚡ Splitting ClientHello at cuts: {:?} (delay: {}ms)",
                cuts, config.delay_ms
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

    // 3. Monitor initial response from remote server
    let mut first_resp = [0u8; 4096];
    match tokio::time::timeout(Duration::from_secs(5), remote_stream.read(&mut first_resp)).await {
        Ok(Ok(resp_n)) if resp_n > 0 => {
            let is_tls = first_resp[0] == 0x16;
            let hs_type = if resp_n > 5 { first_resp[5] } else { 0 };
            let status_str = if is_tls && hs_type == 0x02 {
                "ServerHello (TLS OK)"
            } else if is_tls && hs_type == 0x15 {
                "TLS Alert (Server Rejected)"
            } else if first_resp.starts_with(b"HTTP/") {
                "HTTP Response"
            } else {
                "Data Chunk"
            };

            println!(
                "[CONN #{conn_id}] 🟢 Remote server responded! ({resp_n} bytes, {status_str})"
            );

            client_stream.write_all(&first_resp[..resp_n]).await?;
            client_stream.flush().await?;
        }
        Ok(Ok(_)) => {
            eprintln!("[CONN #{conn_id}] 🔴 Remote server closed connection immediately (RST/EOF)");
            return Ok(());
        }
        Ok(Err(e)) => {
            eprintln!("[CONN #{conn_id}] 🔴 Remote socket read error: {e}");
            return Ok(());
        }
        Err(_) => {
            eprintln!("[CONN #{conn_id}] 🟡 Timeout (5s) waiting for response from {target_host}");
            return Ok(());
        }
    }

    // 4. Bidirectional Full-Duplex Proxying
    let (mut cr, mut cw) = client_stream.into_split();
    let (mut rr, mut rw) = remote_stream.into_split();

    let client_to_remote = tokio::spawn(async move {
        tokio::io::copy(&mut cr, &mut rw).await
    });
    let remote_to_client = tokio::spawn(async move {
        tokio::io::copy(&mut rr, &mut cw).await
    });

    let (c2r, r2c) = tokio::join!(client_to_remote, remote_to_client);
    let sent = c2r.unwrap_or(Ok(0)).unwrap_or(0);
    let recv = r2c.unwrap_or(Ok(0)).unwrap_or(0);

    println!("[CONN #{conn_id}] 🏁 Tunnel finished (up: {sent} B, down: {recv} B)");

    Ok(())
}
