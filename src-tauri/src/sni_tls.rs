//! Minimal TLS ClientHello SNI extraction (no rustls dependency).

/// Peek at the first bytes of a TLS stream and return the SNI hostname if present.
pub fn parse_sni_hostname(data: &[u8]) -> Option<String> {
    // TLS record: type(1) + version(2) + length(2) + handshake...
    if data.len() < 5 || data[0] != 0x16 {
        return None;
    }
    let record_len = u16::from_be_bytes([data[3], data[4]]) as usize;
    if data.len() < 5 + record_len {
        return None;
    }
    let hs = &data[5..5 + record_len];
    // handshake: type(1) + length(3) + ...
    if hs.len() < 4 || hs[0] != 0x01 {
        return None;
    }
    let hs_len = ((hs[1] as usize) << 16) | ((hs[2] as usize) << 8) | (hs[3] as usize);
    if hs.len() < 4 + hs_len {
        return None;
    }
    let body = &hs[4..4 + hs_len];
    // ClientHello: version(2) + random(32) + session_id_len(1) + ...
    if body.len() < 35 {
        return None;
    }
    let mut i = 34; // after version + random
    let sid_len = body[i] as usize;
    i += 1 + sid_len;
    if i + 2 > body.len() {
        return None;
    }
    let cipher_len = u16::from_be_bytes([body[i], body[i + 1]]) as usize;
    i += 2 + cipher_len;
    if i >= body.len() {
        return None;
    }
    let comp_len = body[i] as usize;
    i += 1 + comp_len;
    if i + 2 > body.len() {
        return None;
    }
    let ext_total = u16::from_be_bytes([body[i], body[i + 1]]) as usize;
    i += 2;
    let ext_end = i + ext_total;
    if ext_end > body.len() {
        return None;
    }
    while i + 4 <= ext_end {
        let ext_type = u16::from_be_bytes([body[i], body[i + 1]]);
        let ext_len = u16::from_be_bytes([body[i + 2], body[i + 3]]) as usize;
        i += 4;
        if i + ext_len > ext_end {
            break;
        }
        if ext_type == 0 {
            // server_name extension
            return parse_server_name(&body[i..i + ext_len]);
        }
        i += ext_len;
    }
    None
}

fn parse_server_name(ext: &[u8]) -> Option<String> {
    if ext.len() < 2 {
        return None;
    }
    let list_len = u16::from_be_bytes([ext[0], ext[1]]) as usize;
    if ext.len() < 2 + list_len {
        return None;
    }
    let list = &ext[2..2 + list_len];
    if list.len() < 3 {
        return None;
    }
    let name_type = list[0];
    if name_type != 0 {
        return None;
    }
    let name_len = u16::from_be_bytes([list[1], list[2]]) as usize;
    if list.len() < 3 + name_len {
        return None;
    }
    String::from_utf8(list[3..3 + name_len].to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_buffer() {
        assert!(parse_sni_hostname(&[0x16, 0x03, 0x01]).is_none());
    }
}
