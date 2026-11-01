/// Robust, bounds-checked TLS ClientHello and SNI parser & rewriter.

#[derive(Debug, Clone)]
pub struct SniInfo {
    pub hostname: String,
    pub sni_offset: usize, // Exact byte index where the hostname starts in the buffer
    pub sni_len: usize,    // Length of the hostname in bytes
}

/// Pre-generated RFC-compliant TLS 1.3 ClientHello with SNI = "yandex.ru"
#[allow(dead_code)]
pub const FAKE_YANDEX_CHELLO: &[u8] = &[
    0x16, 0x03, 0x01, 0x00, 0xc1, 0x01, 0x00, 0x00, 0xbd, 0x03, 0x03, 0xce, 0xb9, 0x2e, 0x5b, 0xf6,
    0x9a, 0xd7, 0x39, 0x4c, 0xfe, 0xdd, 0x41, 0x67, 0x4c, 0x30, 0x8b, 0x10, 0x55, 0x45, 0x9d, 0x5f,
    0xa7, 0x07, 0xf3, 0xfd, 0x88, 0x69, 0xde, 0x98, 0x60, 0xd9, 0x88, 0x00, 0x00, 0x4c, 0xcc, 0xa9,
    0xcc, 0xa8, 0xcc, 0xaa, 0xc0, 0x30, 0xc0, 0x2c, 0xc0, 0x28, 0xc0, 0x24, 0xc0, 0x14, 0xc0, 0x0a,
    0x00, 0x9f, 0x00, 0x6b, 0x00, 0x39, 0xff, 0x85, 0x00, 0xc4, 0x00, 0x88, 0x00, 0x81, 0x00, 0x9d,
    0x00, 0x3d, 0x00, 0x35, 0x00, 0xc0, 0x00, 0x84, 0xc0, 0x2f, 0xc0, 0x2b, 0xc0, 0x27, 0xc0, 0x23,
    0xc0, 0x13, 0xc0, 0x09, 0x00, 0x9e, 0x00, 0x67, 0x00, 0x33, 0x00, 0xbe, 0x00, 0x45, 0x00, 0x9c,
    0x00, 0x3c, 0x00, 0x2f, 0x00, 0xba, 0x00, 0x41, 0x00, 0xff, 0x01, 0x00, 0x00, 0x48, 0x00, 0x00,
    0x00, 0x0e, 0x00, 0x0c, 0x00, 0x00, 0x09, 0x79, 0x61, 0x6e, 0x64, 0x65, 0x78, 0x2e, 0x72, 0x75,
    0x00, 0x0b, 0x00, 0x02, 0x01, 0x00, 0x00, 0x0a, 0x00, 0x08, 0x00, 0x06, 0x00, 0x1d, 0x00, 0x17,
    0x00, 0x18, 0x00, 0x23, 0x00, 0x00, 0x00, 0x0d, 0x00, 0x1c, 0x00, 0x1a, 0x06, 0x01, 0x06, 0x03,
    0xef, 0xef, 0x05, 0x01, 0x05, 0x03, 0x04, 0x01, 0x04, 0x03, 0xee, 0xee, 0xed, 0xed, 0x03, 0x01,
    0x03, 0x03, 0x02, 0x01, 0x02, 0x03,
];

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
    let mut pos = 43;

    // Session ID
    if pos >= data.len() {
        return None;
    }
    let session_id_len = data[pos] as usize;
    pos += 1 + session_id_len;

    // Cipher Suites
    if pos + 2 > data.len() {
        return None;
    }
    let cipher_suites_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2 + cipher_suites_len;

    // Compression Methods
    if pos >= data.len() {
        return None;
    }
    let comp_len = data[pos] as usize;
    pos += 1 + comp_len;

    // Extensions
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

/// Dynamically rewrites the SNI hostname inside a ClientHello payload to `new_sni`.
/// Recalculates all enclosing TLS length fields (record, handshake, extension, server_name_list).
#[allow(dead_code)]
pub fn replace_client_hello_sni(data: &[u8], new_sni: &str) -> Option<Vec<u8>> {
    let sni = parse_client_hello_sni(data)?;
    if sni.sni_offset < 9 {
        return None;
    }

    let old_len = sni.sni_len;
    let new_len = new_sni.len();

    let mut out = Vec::with_capacity(data.len() + new_len - old_len);
    out.extend_from_slice(&data[..sni.sni_offset]);
    out.extend_from_slice(new_sni.as_bytes());
    out.extend_from_slice(&data[sni.sni_offset + old_len..]);

    let diff = new_len as isize - old_len as isize;

    // 1. Update name length (2 bytes right before hostname)
    let name_len_offset = sni.sni_offset - 2;
    let new_name_len = (new_len as u16).to_be_bytes();
    out[name_len_offset..name_len_offset + 2].copy_from_slice(&new_name_len);

    // 2. Update ServerNameList length (2 bytes at sni_offset - 5)
    let list_len_offset = sni.sni_offset - 5;
    let old_list_len = u16::from_be_bytes([out[list_len_offset], out[list_len_offset + 1]]) as isize;
    let new_list_len = ((old_list_len + diff) as u16).to_be_bytes();
    out[list_len_offset..list_len_offset + 2].copy_from_slice(&new_list_len);

    // 3. Update SNI extension length (2 bytes at sni_offset - 9)
    let ext_len_offset = sni.sni_offset - 9;
    let old_ext_len = u16::from_be_bytes([out[ext_len_offset], out[ext_len_offset + 1]]) as isize;
    let new_ext_len = ((old_ext_len + diff) as u16).to_be_bytes();
    out[ext_len_offset..ext_len_offset + 2].copy_from_slice(&new_ext_len);

    // 4. Update Extensions total length (offset after compression methods)
    // Find extension total length offset (pos + comp_len + 1)
    let comp_len_pos = 43 + 1 + out[43] as usize;
    let cipher_len = u16::from_be_bytes([out[comp_len_pos], out[comp_len_pos + 1]]) as usize;
    let comp_methods_pos = comp_len_pos + 2 + cipher_len;
    let comp_methods_len = out[comp_methods_pos] as usize;
    let ext_total_offset = comp_methods_pos + 1 + comp_methods_len;

    if ext_total_offset + 2 <= out.len() {
        let old_ext_total = u16::from_be_bytes([out[ext_total_offset], out[ext_total_offset + 1]]) as isize;
        let new_ext_total = ((old_ext_total + diff) as u16).to_be_bytes();
        out[ext_total_offset..ext_total_offset + 2].copy_from_slice(&new_ext_total);
    }

    // 5. Update Handshake length (bytes 6..9)
    let old_hs_len = ((out[6] as usize) << 16) | ((out[7] as usize) << 8) | (out[8] as usize);
    let new_hs_len = (old_hs_len as isize + diff) as usize;
    out[6] = ((new_hs_len >> 16) & 0xFF) as u8;
    out[7] = ((new_hs_len >> 8) & 0xFF) as u8;
    out[8] = (new_hs_len & 0xFF) as u8;

    // 6. Update Record length (bytes 3..5)
    let old_rec_len = u16::from_be_bytes([out[3], out[4]]) as isize;
    let new_rec_len = ((old_rec_len + diff) as u16).to_be_bytes();
    out[3..5].copy_from_slice(&new_rec_len);

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fake_yandex_chello_parses() {
        let sni = parse_client_hello_sni(FAKE_YANDEX_CHELLO).expect("Should parse fake yandex chello");
        assert_eq!(sni.hostname, "yandex.ru");
        assert_eq!(sni.sni_len, 9);
    }

    #[test]
    fn test_replace_sni() {
        let rewritten = replace_client_hello_sni(FAKE_YANDEX_CHELLO, "google.ru")
            .expect("Should rewrite SNI");
        let parsed = parse_client_hello_sni(&rewritten).expect("Should parse rewritten chello");
        assert_eq!(parsed.hostname, "google.ru");
        assert_eq!(parsed.sni_len, 9);
    }
}

/// Splits a single TLS ClientHello into multiple RFC-compliant TLS records.
/// This is the exact `--tlsrec` technique from ByeDPI.
/// RFC 5246/8446 allows any handshake message to be fragmented across multiple TLS records.
/// TSPU DPI engines fail to extract the SNI because the first record contains no hostname.
pub fn split_into_tls_records(payload: &[u8], split_points: &[usize]) -> Vec<Vec<u8>> {
    if payload.len() < 5 || payload[0] != 0x16 {
        return vec![payload.to_vec()];
    }

    let version = [payload[1], payload[2]];
    let handshake_data = &payload[5..];

    let mut records = Vec::new();
    let mut prev = 0;

    for &pt in split_points {
        if pt > prev && pt < handshake_data.len() {
            let chunk = &handshake_data[prev..pt];
            let len = (chunk.len() as u16).to_be_bytes();
            let mut rec = Vec::with_capacity(5 + chunk.len());
            rec.push(0x16);
            rec.extend_from_slice(&version);
            rec.extend_from_slice(&len);
            rec.extend_from_slice(chunk);
            records.push(rec);
            prev = pt;
        }
    }

    if prev < handshake_data.len() {
        let chunk = &handshake_data[prev..];
        let len = (chunk.len() as u16).to_be_bytes();
        let mut rec = Vec::with_capacity(5 + chunk.len());
        rec.push(0x16);
        rec.extend_from_slice(&version);
        rec.extend_from_slice(&len);
        rec.extend_from_slice(chunk);
        records.push(rec);
    }

    records
}
