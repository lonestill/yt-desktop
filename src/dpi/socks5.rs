use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::dpi::forwarder::handle_desync_tunnel;
use crate::dpi::strategy::DesyncConfig;

pub async fn handle_socks5_client(
    mut stream: TcpStream,
    config: Arc<DesyncConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Authentication negotiation
    let mut ver_buf = [0u8; 2];
    stream.read_exact(&mut ver_buf).await?;
    if ver_buf[0] != 0x05 {
        return Err("Unsupported SOCKS version".into());
    }
    let nmethods = ver_buf[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).await?;

    // Respond: SOCKS5, No Authentication Required (0x00)
    stream.write_all(&[0x05, 0x00]).await?;
    stream.flush().await?;

    // 2. Request negotiation
    let mut req_header = [0u8; 4];
    stream.read_exact(&mut req_header).await?;
    let (ver, cmd, _rsv, atyp) = (req_header[0], req_header[1], req_header[2], req_header[3]);

    if ver != 0x05 || cmd != 0x01 {
        stream.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
        return Err("Only CONNECT command supported".into());
    }

    let (target_host, target_port) = match atyp {
        0x01 => {
            // IPv4
            let mut ip_buf = [0u8; 4];
            stream.read_exact(&mut ip_buf).await?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await?;
            let ip = Ipv4Addr::from(ip_buf);
            let port = u16::from_be_bytes(port_buf);
            (ip.to_string(), port)
        }
        0x03 => {
            // Domain name
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await?;
            let domain_len = len_buf[0] as usize;
            let mut domain_buf = vec![0u8; domain_len];
            stream.read_exact(&mut domain_buf).await?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await?;
            let domain = String::from_utf8(domain_buf)?;
            let port = u16::from_be_bytes(port_buf);
            (domain, port)
        }
        0x04 => {
            // IPv6
            let mut ip_buf = [0u8; 16];
            stream.read_exact(&mut ip_buf).await?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await?;
            let ip = std::net::Ipv6Addr::from(ip_buf);
            let port = u16::from_be_bytes(port_buf);
            (ip.to_string(), port)
        }
        _ => {
            stream.write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
            return Err("Address type not supported".into());
        }
    };

    // 3. Connect to remote host
    let remote_target = format!("{}:{}", target_host, target_port);
    let remote_stream = match TcpStream::connect(&remote_target).await {
        Ok(s) => s,
        Err(e) => {
            stream.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
            return Err(format!("Could not connect to {}: {}", remote_target, e).into());
        }
    };

    // Reply SOCKS5 success
    stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
    stream.flush().await?;

    // 4. Tunnel with Desync
    handle_desync_tunnel(stream, remote_stream, target_host, target_port, config).await
}
