use crate::dpi::tls::SniInfo;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesyncMode {
    /// Multi-fragment: cut inside TLS header (byte 2) + mid-SNI + end-SNI
    MultiSplit,
    /// Head-split: cut at byte 2 of TLS record header
    HeadSplit,
    /// Sni-split: cut in the middle of SNI
    SniSplit,
}

#[derive(Debug, Clone)]
pub struct DesyncConfig {
    pub mode: DesyncMode,
    pub split_offset: usize, // Offset inside SNI (default: 2)
    pub delay_ms: u64,       // Delay in ms between sending chunks (default: 3)
    pub target_domains: Vec<String>, // Empty = desync ALL domains
}

impl Default for DesyncConfig {
    fn default() -> Self {
        Self {
            mode: DesyncMode::MultiSplit,
            split_offset: 2,
            delay_ms: 3,
            // Empty list means desync ALL connections passing through this dedicated proxy
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
    pub fn compute_split_points(&self, data: &[u8], sni: Option<&SniInfo>) -> Vec<usize> {
        let mut cuts = Vec::new();

        if let Some(sni_info) = sni {
            match self.mode {
                DesyncMode::MultiSplit => {
                    // Cut 1: Right inside TLS record header (byte 2).
                    // Prevents TSPU from recognizing the record as TLS 0x16 0x03.
                    if data.len() > 2 {
                        cuts.push(2);
                    }

                    // Cut 2: In the middle of SNI hostname
                    let offset = std::cmp::min(self.split_offset, sni_info.sni_len.saturating_sub(1));
                    let cut_sni = sni_info.sni_offset + offset;
                    if cut_sni > 2 && cut_sni < data.len() {
                        cuts.push(cut_sni);
                    }

                    // Cut 3: End of SNI hostname
                    let cut_end_sni = sni_info.sni_offset + sni_info.sni_len;
                    if cut_end_sni > cut_sni && cut_end_sni < data.len() {
                        cuts.push(cut_end_sni);
                    }
                }
                DesyncMode::HeadSplit => {
                    if data.len() > 2 {
                        cuts.push(2);
                    }
                }
                DesyncMode::SniSplit => {
                    let offset = std::cmp::min(self.split_offset, sni_info.sni_len.saturating_sub(1));
                    let cut = sni_info.sni_offset + offset;
                    if cut > 0 && cut < data.len() {
                        cuts.push(cut);
                    }
                }
            }
        } else if data.len() > 4 {
            // Non-SNI or raw TLS: split at byte 2
            cuts.push(2);
        }

        cuts.sort_unstable();
        cuts.dedup();
        cuts
    }
}
