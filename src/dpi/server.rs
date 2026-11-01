use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::dpi::forwarder::handle_desync_tunnel;
use crate::dpi::resolve_ipv4;
use crate::dpi::socks5::handle_socks5_client;
use crate::dpi::strategy::DesyncConfig;

pub async fn run_embedded_proxy(
    listener: TcpListener,
    config: Arc<DesyncConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(_) => continue,
        };

        let cfg = Arc::clone(&config);
        tokio::spawn(async move {
            let mut first_byte = [0u8; 1];
            if let Ok(n) = stream.peek(&mut first_byte).await {
                if n > 0 && first_byte[0] == 0x05 {
                    let _ = handle_socks5_client(stream, cfg).await;
                    return;
                }
            }
            let _ = handle_http_client(stream, cfg).await;
        });
    }
}

async fn handle_http_client(
    mut stream: TcpStream,
    config: Arc<DesyncConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut header_buf = [0u8; 4096];
    let n = stream.read(&mut header_buf).await?;
    if n == 0 {
        return Ok(());
    }

    let header_str = String::from_utf8_lossy(&header_buf[..n]);
    let first_line = header_str.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();

    if parts.is_empty() {
        return Ok(());
    }

    if parts[0].eq_ignore_ascii_case("CONNECT") {
        if parts.len() < 2 {
            stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await?;
            return Ok(());
        }

        let target = parts[1];
        let (target_host, target_port) = if let Some(idx) = target.rfind(':') {
            let host = &target[..idx];
            let port: u16 = target[idx + 1..].parse().unwrap_or(443);
            (host.to_string(), port)
        } else {
            (target.to_string(), 443)
        };

        let remote_addr = match resolve_ipv4(&target_host, target_port).await {
            Ok(addr) => addr,
            Err(e) => {
                eprintln!("[HTTP] ❌ DNS resolution failed for {target_host}:{target_port}: {e}");
                let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                return Err(format!("DNS failed for {target_host}: {e}").into());
            }
        };

        println!("[HTTP] 🎯 Connecting to {target_host}:{target_port} -> IPv4 [{remote_addr}]");
        let remote_stream = match TcpStream::connect(remote_addr).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[HTTP] ❌ Remote connect failed for {remote_addr}: {e}");
                let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                return Ok(());
            }
        };

        stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
        stream.flush().await?;

        handle_desync_tunnel(stream, remote_stream, target_host, target_port, config).await
    } else {
        // Plain HTTP forward
        let host_line = header_str
            .lines()
            .find(|line| line.to_lowercase().starts_with("host:"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, val)| val.trim());

        let target_host = if let Some(h) = host_line {
            h.to_string()
        } else if parts.len() > 1 && parts[1].starts_with("http://") {
            parts[1]
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or("")
                .to_string()
        } else {
            return Ok(());
        };

        let (host, port) = if let Some(idx) = target_host.rfind(':') {
            let h = &target_host[..idx];
            let p: u16 = target_host[idx + 1..].parse().unwrap_or(80);
            (h.to_string(), p)
        } else {
            (target_host, 80)
        };

        let remote_addr = match resolve_ipv4(&host, port).await {
            Ok(addr) => addr,
            Err(e) => {
                eprintln!("[HTTP] ❌ DNS resolution failed for {host}:{port}: {e}");
                let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                return Ok(());
            }
        };

        let mut remote_stream = match TcpStream::connect(remote_addr).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[HTTP] ❌ Remote connect failed for {remote_addr}: {e}");
                let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                return Ok(());
            }
        };

        remote_stream.write_all(&header_buf[..n]).await?;
        remote_stream.flush().await?;

        let _ = tokio::io::copy_bidirectional(&mut stream, &mut remote_stream).await;
        Ok(())
    }
}
