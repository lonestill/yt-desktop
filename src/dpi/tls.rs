/// Robust, bounds-checked TLS ClientHello and SNI parser.
/// Directly adopted from twitter-swarm desync engine.

#[derive(Debug, Clone)]
pub struct SniInfo {
    pub hostname: String,
    pub sni_offset: usize, // Exact byte index where the hostname starts in the buffer
    pub sni_len: usize,    // Length of the hostname in bytes
}

/// Attempts to parse a TLS ClientHello from the beginning of a buffer.
/// Returns SniInfo if a valid Server Name Indication extension is found.
pub fn parse_client_hello_sni(data: &[u8]) -> Option<SniInfo> {
    if data.len() < 9 {
        return None;
    }

    // Byte 0: ContentType (0x16 = Handshake)
    if data[0] != 0x16 {
        return None;
    }

    // Bytes 1..3: Record Version (must be TLS 1.0..1.3: 0x03, 0x01..0x04)
    if data[1] != 0x03 {
        return None;
    }

    // Byte 5: Handshake Type (0x01 = ClientHello)
    if data[5] != 0x01 {
        return None;
    }

    // Handshake starts at offset 5.
    // Bytes 6..9: Handshake Length (3 bytes)
    // Bytes 9..11: Client Version (2 bytes)
    // Bytes 11..43: Random (32 bytes)
    let mut pos = 43;

    // Session ID (1 byte length + length bytes)
    if pos >= data.len() {
        return None;
    }
    let session_id_len = data[pos] as usize;
    pos += 1 + session_id_len;

    // Cipher Suites (2 bytes length + length bytes)
    if pos + 2 > data.len() {
        return None;
    }
    let cipher_suites_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2 + cipher_suites_len;

    // Compression Methods (1 byte length + length bytes)
    if pos >= data.len() {
        return None;
    }
    let comp_len = data[pos] as usize;
    pos += 1 + comp_len;

    // Extensions (2 bytes length + extensions)
    if pos + 2 > data.len() {
        return None;
    }
    let ext_total_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2;

    let ext_end = std::cmp::min(pos + ext_total_len, data.len());

    // Iterate through extensions
    while pos + 4 <= ext_end {
        let ext_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let ext_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        if ext_type == 0x0000 {
            // Extension: server_name (SNI)
            if pos + ext_len > data.len() || ext_len < 5 {
                return None;
            }

            // ServerNameList:
            // Bytes 0..2: list length
            // Byte 2: name_type (0 = host_name)
            // Bytes 3..5: name length
            let name_type = data[pos + 2];
            if name_type == 0x00 {
                let name_len = u16::from_be_bytes([data[pos + 3], data[pos + 4]]) as usize;
                let name_start = pos + 5;

                if name_start + name_len <= data.len() {
                    let hostname_bytes = &data[name_start..name_start + name_len];
                    if let Ok(hostname) = std::str::from_utf8(hostname_bytes) {
                        return Some(SniInfo {
                            hostname: hostname.to_string(),
                            sni_offset: name_start,
                            sni_len: name_len,
                        });
                    }
                }
            }
        }

        pos += ext_len;
    }

    None
}
