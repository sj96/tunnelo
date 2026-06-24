//! IPv4 wildcard bastion discovery — expand patterns and scan for open SSH ports.

use anyhow::{bail, Context, Result};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch, Semaphore};

pub const MAX_CANDIDATES: usize = 512;
const SCAN_TIMEOUT: Duration = Duration::from_millis(500);
const SCAN_CONCURRENCY: usize = 32;

/// `true` if `host` is an IPv4 pattern with at least one `*` octet.
pub fn is_wildcard_host(host: &str) -> bool {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().any(|p| *p == "*")
}

enum Octet {
    Fixed(u8),
    Wildcard,
}

fn parse_octet(s: &str) -> Result<Octet> {
    if s == "*" {
        return Ok(Octet::Wildcard);
    }
    let n: u8 = s
        .parse()
        .with_context(|| format!("invalid octet `{s}` (expected 0-255 or *)"))?;
    Ok(Octet::Fixed(n))
}

/// Expand an IPv4 literal or wildcard pattern into candidate addresses.
pub fn expand_wildcard(host: &str) -> Result<Vec<Ipv4Addr>> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        bail!("bastion host must be IPv4 form a.b.c.d (got `{host}`)");
    }

    let octets: Vec<Octet> = parts
        .iter()
        .map(|p| parse_octet(p))
        .collect::<Result<_>>()?;

    let mut count = 1usize;
    for o in &octets {
        count = count
            .checked_mul(match o {
                Octet::Fixed(_) => 1,
                Octet::Wildcard => 254,
            })
            .context("pattern too large")?;
        if count > MAX_CANDIDATES {
            bail!(
                "wildcard pattern `{host}` expands to more than {MAX_CANDIDATES} addresses"
            );
        }
    }

    let mut out = Vec::with_capacity(count);
    expand_recurse(&octets, 0, [0; 4], &mut out);
    Ok(out)
}

fn expand_recurse(octets: &[Octet], idx: usize, buf: [u8; 4], out: &mut Vec<Ipv4Addr>) {
    if idx == 4 {
        out.push(Ipv4Addr::from(buf));
        return;
    }
    match &octets[idx] {
        Octet::Fixed(n) => {
            let mut buf = buf;
            buf[idx] = *n;
            expand_recurse(octets, idx + 1, buf, out);
        }
        Octet::Wildcard => {
            for n in 1..=254u8 {
                let mut buf = buf;
                buf[idx] = n;
                expand_recurse(octets, idx + 1, buf, out);
            }
        }
    }
}

/// Parallel TCP scan; returns the first IP with an open `port`, or `None`.
pub async fn scan_ssh_port(
    candidates: Vec<Ipv4Addr>,
    port: u16,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<Option<Ipv4Addr>> {
    if candidates.is_empty() {
        return Ok(None);
    }

    let (tx, mut rx) = mpsc::channel::<Ipv4Addr>(1);
    let sem = Arc::new(Semaphore::new(SCAN_CONCURRENCY));

    for ip in candidates {
        if *shutdown.borrow() {
            break;
        }
        let tx = tx.clone();
        let sem = sem.clone();
        let permit = sem.acquire_owned().await.expect("semaphore");
        tokio::spawn(async move {
            let _permit = permit;
            let addr = SocketAddr::from((ip, port));
            let ok = tokio::time::timeout(SCAN_TIMEOUT, TcpStream::connect(addr))
                .await
                .ok()
                .and_then(|r| r.ok())
                .is_some();
            if ok {
                let _ = tx.try_send(ip);
            }
        });
    }
    drop(tx);

    loop {
        tokio::select! {
            biased;
            res = shutdown.changed() => {
                if res.is_err() || *shutdown.borrow() {
                    return Ok(None);
                }
            }
            ip = rx.recv() => {
                match ip {
                    Some(ip) => return Ok(Some(ip)),
                    None => return Ok(None),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_wildcard_detects_star() {
        assert!(is_wildcard_host("192.168.1.*"));
        assert!(!is_wildcard_host("192.168.1.11"));
        assert!(!is_wildcard_host("gitlab.example.com"));
    }

    #[test]
    fn expand_single_literal() {
        let addrs = expand_wildcard("10.0.0.5").unwrap();
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0], Ipv4Addr::new(10, 0, 0, 5));
    }

    #[test]
    fn expand_one_wildcard_octet() {
        let addrs = expand_wildcard("192.168.1.*").unwrap();
        assert_eq!(addrs.len(), 254);
        assert!(addrs.contains(&Ipv4Addr::new(192, 168, 1, 1)));
        assert!(addrs.contains(&Ipv4Addr::new(192, 168, 1, 254)));
        assert!(!addrs.contains(&Ipv4Addr::new(192, 168, 1, 0)));
        assert!(!addrs.contains(&Ipv4Addr::new(192, 168, 1, 255)));
    }

    #[test]
    fn expand_rejects_too_many() {
        assert!(expand_wildcard("*.*.*.*").is_err());
        assert!(expand_wildcard("192.168.*.*").is_err());
    }
}
