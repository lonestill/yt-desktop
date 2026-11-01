use crate::dpi::tls::SniInfo;
use std::env;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesyncMode {
    /// Auto-detect: direct pass-through if VPN is active, TLS record fragmentation if direct ISP
    Auto,
    /// Cut at byte 1 of TLS record header (0x16)
    FirstByte,
    /// Cut at byte 1 + mid-SNI + end-SNI
    MultiSplit,
    /// Cut in the middle of SNI
    SniSplit,
    /// Direct pass-through
    None,
}

static LAST_VPN_CHECK: Mutex<Option<(Instant, Option<String>)>> = Mutex::new(None);

/// Queries macOS route table to see if default gateway routes through a TUN/VPN interface.
pub fn detect_vpn_interface() -> Option<String> {
    let mut guard = LAST_VPN_CHECK.lock().unwrap();
    if let Some((instant, ref iface)) = *guard {
        if instant.elapsed() < Duration::from_secs(3) {
            return iface.clone();
        }
    }

    let detected = (|| {
        let output = std::process::Command::new("/sbin/route")
            .args(["-n", "get", "default"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("interface:") {
                let iface = trimmed.strip_prefix("interface:").unwrap_or("").trim().to_string();
                if iface.starts_with("utun") || iface.starts_with("ppp") || iface.starts_with("ipsec") {
                    return Some(iface);
                }
                return None;
            }
        }
        None
    })();

    *guard = Some((Instant::now(), detected.clone()));
    detected
}

#[derive(Debug, Clone)]
pub struct DesyncConfig {
    pub mode: DesyncMode,
    #[allow(dead_code)]
    pub split_offset: usize,      // Offset inside SNI (default: 2)
    pub delay_ms: u64,            // Delay in ms between sending chunks (default: 2)
    #[allow(dead_code)]
    pub fake_sni: Option<String>, // Fake SNI domain if enabled (disabled by default)
    #[allow(dead_code)]
    pub fake_ttl: u32,            // Low TTL for fake packet (default: 5 hops)
    pub target_domains: Vec<String>, // Empty = desync ALL domains
}

impl Default for DesyncConfig {
    fn default() -> Self {
        let mode_str = env::var("DESYNC_MODE").unwrap_or_else(|_| "auto".to_string());
        let mode = match mode_str.to_lowercase().as_str() {
            "auto" => DesyncMode::Auto,
            "firstbyte" | "first" | "1" => DesyncMode::FirstByte,
            "multi" | "multisplit" => DesyncMode::MultiSplit,
            "sni" | "snisplit" => DesyncMode::SniSplit,
            "none" | "direct" => DesyncMode::None,
            _ => DesyncMode::Auto,
        };

        let fake_enabled = env::var("DESYNC_FAKE").map(|v| v == "1" || v == "true").unwrap_or(false);
        let fake_sni_val = env::var("DESYNC_FAKE_SNI").unwrap_or_else(|_| "yandex.ru".to_string());
        let fake_sni = if fake_enabled { Some(fake_sni_val) } else { None };

        let fake_ttl = env::var("DESYNC_FAKE_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let delay_ms = env::var("DESYNC_DELAY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);

        Self {
            mode,
            split_offset: 2,
            delay_ms,
            fake_sni,
            fake_ttl,
            target_domains: Vec::new(),
        }
    }
}

impl DesyncConfig {
    /// Checks if a given domain or SNI matches the target list.
    pub fn is_target(&self, hostname: &str) -> bool {
        if self.target_domains.is_empty() {
            return true;
        }
        let lower = hostname.to_lowercase();
        self.target_domains.iter().any(|d| lower.ends_with(d) || lower == *d)
    }

    /// Computes split indices for a given buffer.
    #[allow(dead_code)]
    pub fn compute_split_points(&self, data: &[u8], sni: Option<&SniInfo>) -> Vec<usize> {
        let mut cuts = Vec::new();

        match self.mode {
            DesyncMode::FirstByte => {
                // Cut at byte 1: sends [0x16] alone, then micro-delay, then rest
                if data.len() > 1 {
                    cuts.push(1);
                }
            }
            DesyncMode::MultiSplit => {
                if data.len() > 1 {
                    cuts.push(1);
                }
                if let Some(sni_info) = sni {
                    let offset = std::cmp::min(self.split_offset, sni_info.sni_len.saturating_sub(1));
                    let cut_sni = sni_info.sni_offset + offset;
                    if cut_sni > 1 && cut_sni < data.len() {
                        cuts.push(cut_sni);
                    }
                    let cut_end_sni = sni_info.sni_offset + sni_info.sni_len;
                    if cut_end_sni > cut_sni && cut_end_sni < data.len() {
                        cuts.push(cut_end_sni);
                    }
                }
            }
            DesyncMode::SniSplit => {
                if let Some(sni_info) = sni {
                    let offset = std::cmp::min(self.split_offset, sni_info.sni_len.saturating_sub(1));
                    let cut = sni_info.sni_offset + offset;
                    if cut > 0 && cut < data.len() {
                        cuts.push(cut);
                    }
                }
            }
            DesyncMode::None | DesyncMode::Auto => {}
        }

        cuts.sort_unstable();
        cuts.dedup();
        cuts
    }
}
