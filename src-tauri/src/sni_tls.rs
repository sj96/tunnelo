//! Minimal TLS ClientHello SNI extraction (no rustls dependency).

use std::time::Duration;
use tokio::net::TcpStream;

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
    String::from_utf8(list[3..3 + name_len].to_vec())
        .ok()
        .map(|s| s.to_ascii_lowercase())
}

/// Extract hostname from cleartext HTTP Host header or TLS ClientHello SNI.
pub fn peek_routing_hostname(data: &[u8]) -> Option<String> {
    if !data.is_empty() && data[0] == 0x16 {
        return parse_sni_hostname(data);
    }
    parse_host_header(data)
}

/// Extract hostname from the first HTTP request's Host header.
pub fn parse_host_header(data: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(data).ok()?;
    for line in text.split("\r\n") {
        if let Some(rest) = line.strip_prefix("Host:").or_else(|| line.strip_prefix("host:")) {
            let host = rest.trim().split(':').next()?.trim();
            if !host.is_empty() {
                return Some(host.to_ascii_lowercase());
            }
        }
        if line.is_empty() {
            break;
        }
    }
    None
}

/// Peek repeatedly until SNI / Host can be parsed or the stream is clearly complete.
pub async fn await_routing_hostname(stream: &mut TcpStream) -> Option<String> {
    const MAX: usize = 16_384;
    let mut buf = [0u8; MAX];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    loop {
        let n = match tokio::time::timeout_at(deadline, stream.peek(&mut buf)).await {
            Ok(Ok(0)) => return None,
            Ok(Ok(n)) => n,
            _ => return None,
        };

        if let Some(host) = peek_routing_hostname(&buf[..n]) {
            return Some(host);
        }

        if routing_peek_exhausted(&buf[..n]) {
            return None;
        }

        if n >= MAX {
            return None;
        }

        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// True when enough bytes arrived that routing metadata will not appear later.
fn routing_peek_exhausted(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    if data[0] == 0x16 {
        if data.len() < 5 {
            return false;
        }
        let record_len = u16::from_be_bytes([data[3], data[4]]) as usize;
        return data.len() >= 5 + record_len;
    }
    data.windows(4).any(|w| w == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_buffer() {
        assert!(parse_sni_hostname(&[0x16, 0x03, 0x01]).is_none());
    }

    #[test]
    fn parses_sni_from_minimal_client_hello() {
        let hello = build_client_hello_with_sni("app2.example.com");
        assert_eq!(
            parse_sni_hostname(&hello),
            Some("app2.example.com".into())
        );
    }

    #[test]
    fn partial_tls_record_waits_until_complete() {
        let hello = build_client_hello_with_sni("app2.example.com");
        assert!(parse_sni_hostname(&hello[..8]).is_none());
        assert!(!routing_peek_exhausted(&hello[..8]));
        assert!(routing_peek_exhausted(&hello));
    }

    fn build_client_hello_with_sni(hostname: &str) -> Vec<u8> {
        let host_bytes = hostname.as_bytes();
        let sni_ext = {
            let mut ext = Vec::new();
            let list_len = 1 + 2 + host_bytes.len();
            ext.push((list_len >> 8) as u8);
            ext.push(list_len as u8);
            ext.push(0); // host_name type
            ext.push((host_bytes.len() >> 8) as u8);
            ext.push(host_bytes.len() as u8);
            ext.extend_from_slice(host_bytes);
            ext
        };

        let mut extensions = Vec::new();
        extensions.push(0);
        extensions.push(0); // server_name
        extensions.push((sni_ext.len() >> 8) as u8);
        extensions.push(sni_ext.len() as u8);
        extensions.extend_from_slice(&sni_ext);

        let mut body = Vec::new();
        body.extend_from_slice(&[0x03, 0x03]); // TLS 1.2
        body.extend_from_slice(&[0u8; 32]); // random
        body.push(0); // session id len
        body.extend_from_slice(&[0, 2, 0, 0x00]); // cipher suites
        body.push(1);
        body.push(0); // compression
        body.push((extensions.len() >> 8) as u8);
        body.push(extensions.len() as u8);
        body.extend_from_slice(&extensions);

        let mut hs = Vec::new();
        hs.push(0x01); // ClientHello
        let hs_len = body.len();
        hs.push((hs_len >> 16) as u8);
        hs.push((hs_len >> 8) as u8);
        hs.push(hs_len as u8);
        hs.extend_from_slice(&body);

        let mut record = Vec::new();
        record.push(0x16);
        record.extend_from_slice(&[0x03, 0x01]);
        let rec_len = hs.len();
        record.push((rec_len >> 8) as u8);
        record.push(rec_len as u8);
        record.extend_from_slice(&hs);
        record
    }
}
