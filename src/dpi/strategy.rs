use crate::dpi::tls::SniInfo;
use std::env;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesyncMode {
    /// Cut at byte 1 of TLS record header (0x16)
    FirstByte,
    /// Cut at byte 1 + mid-SNI + end-SNI
    MultiSplit,
    /// Cut in the middle of SNI
    SniSplit,
    /// Direct pass-through
    None,
}

#[derive(Debug, Clone)]
pub struct DesyncConfig {
    pub mode: DesyncMode,
    #[allow(dead_code)]
    pub split_offset: usize,      // Offset inside SNI (default: 2)
    pub delay_ms: u64,            // Delay in ms between sending chunks (default: 2)
    pub fake_sni: Option<String>, // Fake SNI domain if enabled (disabled by default)
    #[allow(dead_code)]
    pub fake_ttl: u32,            // Low TTL for fake packet (default: 5 hops)
    pub target_domains: Vec<String>, // Empty = desync ALL domains
}

impl Default for DesyncConfig {
    fn default() -> Self {
        // Read configuration from environment if present for dynamic testing
        let mode_str = env::var("DESYNC_MODE").unwrap_or_else(|_| "firstbyte".to_string());
        let mode = match mode_str.to_lowercase().as_str() {
            "firstbyte" | "first" | "1" => DesyncMode::FirstByte,
            "multi" | "multisplit" => DesyncMode::MultiSplit,
            "sni" | "snisplit" => DesyncMode::SniSplit,
            "none" | "direct" => DesyncMode::None,
            _ => DesyncMode::FirstByte,
        };

        // Fake packet injection disabled by default to avoid breaking TCP streams
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
            DesyncMode::None => {}
        }

        cuts.sort_unstable();
        cuts.dedup();
        cuts
    }
}
