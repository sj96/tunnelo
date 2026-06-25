//! Global local routing orchestrator — hosts, per-domain loopback IPs, SSH bind.

use crate::hosts;
use crate::model::{format_remote_url, ForwardMapping};
use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct RouteKey {
    hostname: String,
    public_port: u16,
}

#[derive(Debug, Clone)]
struct RouteEntry {
    loopback_ip: String,
    remote_port: u16,
    refs: u32,
}

#[derive(Debug, Clone)]
struct TunnelRegistration {
    key: RouteKey,
}

/// Runtime binding info returned to the tunnel engine for SSH `-L`.
#[derive(Debug, Clone)]
pub struct ActivatedMapping {
    pub mapping_id: String,
    pub bind_host: String,
    pub bind_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub access_url: String,
}

pub struct LocalRouter {
    routes: Arc<Mutex<HashMap<RouteKey, RouteEntry>>>,
    inner: Mutex<RouterState>,
}

struct RouterState {
    tunnel_regs: HashMap<String, Vec<TunnelRegistration>>,
    used_ips: HashSet<String>,
}

impl Default for LocalRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalRouter {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(Mutex::new(HashMap::new())),
            inner: Mutex::new(RouterState {
                tunnel_regs: HashMap::new(),
                used_ips: HashSet::new(),
            }),
        }
    }

    pub fn bootstrap() -> Result<()> {
        hosts::cleanup_orphans()
    }

    pub fn activate_tunnel(
        &self,
        tunnel_id: &str,
        mappings: &[ForwardMapping],
    ) -> Result<Vec<ActivatedMapping>> {
        let mut state = self.inner.lock();
        let mut routes = self.routes.lock();

        if state.tunnel_regs.contains_key(tunnel_id) {
            bail!("tunnel routing already active");
        }

        let mut activated = Vec::new();
        let mut pending: Vec<TunnelRegistration> = Vec::new();

        for mapping in mappings {
            let remote_host = mapping.remote_host.trim();
            if remote_host.is_empty() {
                bail!("each mapping needs a remote target host");
            }
            if mapping.remote_port == 0 {
                bail!("each mapping needs a valid remote port");
            }

            if is_ip_address(remote_host) {
                let access_url = format_remote_url(
                    remote_host,
                    mapping.remote_port,
                    mapping.remote_scheme.as_deref(),
                );
                activated.push(ActivatedMapping {
                    mapping_id: mapping.id.clone(),
                    bind_host: "127.0.0.1".into(),
                    bind_port: mapping.remote_port,
                    remote_host: remote_host.to_string(),
                    remote_port: mapping.remote_port,
                    access_url,
                });
                continue;
            }

            let hostname = normalize_hostname(remote_host);
            let public_port = mapping.remote_port;
            let loopback_ip = reserve_domain_route(
                &mut routes,
                &mut state,
                &hostname,
                public_port,
            )?;

            let key = RouteKey {
                hostname: hostname.clone(),
                public_port,
            };

            let access_url = format_remote_url(
                &hostname,
                public_port,
                mapping.remote_scheme.as_deref(),
            );

            pending.push(TunnelRegistration { key });
            activated.push(ActivatedMapping {
                mapping_id: mapping.id.clone(),
                bind_host: loopback_ip.clone(),
                bind_port: public_port,
                remote_host: hostname.clone(),
                remote_port: mapping.remote_port,
                access_url,
            });
        }

        drop(routes);
        state.tunnel_regs.insert(tunnel_id.to_string(), pending);
        state.sync_hosts_locked(&self.routes)?;

        for m in &activated {
            if !is_ip_address(&m.remote_host) {
                tracing::info!(
                    "routing {} → {}:{}",
                    m.access_url,
                    m.bind_host,
                    m.bind_port
                );
            }
        }

        Ok(activated)
    }

    pub fn deactivate_tunnel(&self, tunnel_id: &str) {
        let mut state = self.inner.lock();
        let mut routes = self.routes.lock();
        let Some(regs) = state.tunnel_regs.remove(tunnel_id) else {
            return;
        };

        for reg in regs {
            if let Some(entry) = routes.get_mut(&reg.key) {
                entry.refs = entry.refs.saturating_sub(1);
                if entry.refs == 0 {
                    let ip = entry.loopback_ip.clone();
                    routes.remove(&reg.key);
                    state.used_ips.remove(&ip);
                }
            }
        }

        drop(routes);
        let _ = state.sync_hosts_locked(&self.routes);
    }

    pub fn shutdown_all(&self) {
        let mut state = self.inner.lock();
        self.routes.lock().clear();
        state.tunnel_regs.clear();
        state.used_ips.clear();
        // Skip hosts sync on quit — `elevation::write_hosts_file` can block for
        // seconds (UAC / PowerShell) and freezes the window. Orphans are removed
        // on the next launch via `bootstrap()`.
    }

    #[cfg(test)]
    fn route_loopback_ip(&self, public_port: u16, hostname: &str) -> Option<String> {
        let key = RouteKey {
            hostname: normalize_hostname(hostname),
            public_port,
        };
        self.routes.lock().get(&key).map(|e| e.loopback_ip.clone())
    }
}

impl RouterState {
    fn sync_hosts_locked(
        &self,
        routes: &Arc<Mutex<HashMap<RouteKey, RouteEntry>>>,
    ) -> Result<()> {
        let mut entries: Vec<(String, String)> = routes
            .lock()
            .iter()
            .map(|(k, e)| (e.loopback_ip.clone(), k.hostname.clone()))
            .collect();
        entries.sort_by(|a, b| a.1.cmp(&b.1));
        hosts::sync_domains(&entries)
    }
}

fn is_ip_address(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok()
}

fn reserve_domain_route(
    routes: &mut HashMap<RouteKey, RouteEntry>,
    state: &mut RouterState,
    remote_host: &str,
    remote_port: u16,
) -> Result<String> {
    let hostname = normalize_hostname(remote_host);
    let public_port = remote_port;
    let key = RouteKey {
        hostname: hostname.clone(),
        public_port,
    };

    if let Some(existing) = routes.get(&key) {
        if existing.remote_port != remote_port {
            bail!(
                "domain conflict: {hostname}:{public_port} is already routed to port {}",
                existing.remote_port
            );
        }
        let loopback_ip = existing.loopback_ip.clone();
        routes.get_mut(&key).unwrap().refs += 1;
        return Ok(loopback_ip);
    }

    let loopback_ip = allocate_loopback_ip(state, public_port, routes)
        .context("loopback IP pool exhausted (127.0.0.2–127.0.0.254)")?;
    routes.insert(
        key,
        RouteEntry {
            loopback_ip: loopback_ip.clone(),
            remote_port,
            refs: 1,
        },
    );
    Ok(loopback_ip)
}

/// Pick a loopback IP for `public_port`. The first domain on a port uses 127.0.0.1;
/// additional domains on the same port get distinct IPs so browsers cannot coalesce
/// TLS connections (which would bypass per-connection SNI routing).
fn allocate_loopback_ip(
    state: &mut RouterState,
    public_port: u16,
    routes: &HashMap<RouteKey, RouteEntry>,
) -> Option<String> {
    let ips_on_port: HashSet<&str> = routes
        .iter()
        .filter(|(k, _)| k.public_port == public_port)
        .map(|(_, e)| e.loopback_ip.as_str())
        .collect();

    if !ips_on_port.contains("127.0.0.1") {
        state.used_ips.insert("127.0.0.1".into());
        return Some("127.0.0.1".into());
    }

    for n in 2..=254 {
        let ip = format!("127.0.0.{n}");
        if !state.used_ips.contains(&ip) && !ips_on_port.contains(ip.as_str()) {
            state.used_ips.insert(ip.clone());
            return Some(ip);
        }
    }
    None
}

/// Normalize a user-supplied hostname so it matches SNI / HTTP Host lookups.
fn normalize_hostname(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(rest) = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
    {
        s = rest;
    }
    s = s.split('/').next().unwrap_or(s).trim();

    if let Some(rest) = s.strip_prefix('[') {
        if let Some((ip, after)) = rest.split_once(']') {
            let host = format!("[{ip}]");
            if let Some(port) = after.strip_prefix(':') {
                if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
                    return host.to_ascii_lowercase();
                }
            }
            return host.to_ascii_lowercase();
        }
    }

    if let Some((host, port)) = s.rsplit_once(':') {
        if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            return host.trim_end_matches('.').to_ascii_lowercase();
        }
    }

    s.trim_end_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_hostname_strips_scheme_port_and_trailing_dot() {
        assert_eq!(
            normalize_hostname("https://GitLab.Example.com:443/"),
            "gitlab.example.com"
        );
        assert_eq!(normalize_hostname("app.example.com."), "app.example.com");
        assert_eq!(normalize_hostname("app.example.com:8443"), "app.example.com");
    }

    #[test]
    fn two_domains_on_same_public_port_get_distinct_loopback_ips() {
        let router = LocalRouter::new();
        let ip1;
        let ip2;
        {
            let mut routes = router.routes.lock();
            let mut state = router.inner.lock();
            ip1 = reserve_domain_route(&mut routes, &mut state, "app1.example.com", 443).unwrap();
            ip2 = reserve_domain_route(&mut routes, &mut state, "app2.example.com", 443).unwrap();
        }

        assert_ne!(ip1, ip2);
        assert_eq!(
            router.route_loopback_ip(443, "app1.example.com"),
            Some(ip1.clone())
        );
        assert_eq!(
            router.route_loopback_ip(443, "app2.example.com"),
            Some(ip2.clone())
        );
    }

    #[test]
    fn first_domain_on_port_uses_loopback_one() {
        let router = LocalRouter::new();
        let ip = {
            let mut routes = router.routes.lock();
            let mut state = router.inner.lock();
            reserve_domain_route(&mut routes, &mut state, "only.example.com", 443).unwrap()
        };
        assert_eq!(ip, "127.0.0.1");
    }

    #[test]
    fn different_ports_can_share_loopback_one() {
        let router = LocalRouter::new();
        let ip443;
        let ip8443;
        {
            let mut routes = router.routes.lock();
            let mut state = router.inner.lock();
            ip443 = reserve_domain_route(&mut routes, &mut state, "a.example.com", 443).unwrap();
            ip8443 = reserve_domain_route(&mut routes, &mut state, "b.example.com", 8443).unwrap();
        }
        assert_eq!(ip443, "127.0.0.1");
        assert_eq!(ip8443, "127.0.0.1");
    }

    #[test]
    fn hostname_with_embedded_port_matches_lookup() {
        let router = LocalRouter::new();
        let ip = {
            let mut routes = router.routes.lock();
            let mut state = router.inner.lock();
            reserve_domain_route(&mut routes, &mut state, "app1.example.com:443", 443).unwrap()
        };
        assert_eq!(router.route_loopback_ip(443, "app1.example.com"), Some(ip));
    }

    #[test]
    fn resolve_normalizes_lookup_hostname() {
        let router = LocalRouter::new();
        let ip = {
            let mut routes = router.routes.lock();
            let mut state = router.inner.lock();
            reserve_domain_route(
                &mut routes,
                &mut state,
                "https://App1.Example.com:443/",
                443,
            )
            .unwrap()
        };
        assert_eq!(
            router.route_loopback_ip(443, "app1.example.com"),
            Some(ip)
        );
    }
}

