//! Global local routing orchestrator — hosts, SNI/HTTP routers, internal port pool.

use crate::hosts;
use crate::http_router::{HttpRouter, ResolveFn as HttpResolveFn};
use crate::model::ForwardMapping;
use crate::sni_proxy::SniProxy;
use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const PORT_POOL_START: u16 = 49100;
const PORT_POOL_END: u16 = 49999;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct RouteKey {
    hostname: String,
    public_port: u16,
}

#[derive(Debug, Clone)]
struct RouteEntry {
    internal_port: u16,
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
    domain_refs: HashMap<String, u32>,
    tunnel_regs: HashMap<String, Vec<TunnelRegistration>>,
    used_ports: HashSet<u16>,
    sni_proxy: Option<SniProxy>,
    http_routers: HashMap<u16, HttpRouter>,
    resolve: HttpResolveFn,
}

impl Default for LocalRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalRouter {
    pub fn new() -> Self {
        let routes = Arc::new(Mutex::new(HashMap::new()));
        let routes_for_resolve = routes.clone();
        let resolve: HttpResolveFn = Arc::new(move |public_port, hostname| {
            let key = RouteKey {
                hostname: hostname.to_ascii_lowercase(),
                public_port,
            };
            routes_for_resolve
                .lock()
                .get(&key)
                .map(|e: &RouteEntry| e.internal_port)
        });
        Self {
            routes,
            inner: Mutex::new(RouterState {
                domain_refs: HashMap::new(),
                tunnel_regs: HashMap::new(),
                used_ports: HashSet::new(),
                sni_proxy: None,
                http_routers: HashMap::new(),
                resolve,
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
                let access_url = format_access_url("127.0.0.1", mapping.remote_port);
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

            let hostname = remote_host.to_ascii_lowercase();
            let public_port = mapping.remote_port;
            let key = RouteKey {
                hostname: hostname.clone(),
                public_port,
            };

            let internal_port = if let Some(existing) = routes.get(&key) {
                if existing.remote_port != mapping.remote_port {
                    bail!(
                        "domain conflict: {hostname}:{public_port} is already routed to port {}",
                        existing.remote_port
                    );
                }
                let internal_port = existing.internal_port;
                routes.get_mut(&key).unwrap().refs += 1;
                internal_port
            } else {
                let internal_port = state
                    .allocate_port()
                    .context("internal port pool exhausted (49100–49999)")?;
                routes.insert(
                    key.clone(),
                    RouteEntry {
                        internal_port,
                        remote_port: mapping.remote_port,
                        refs: 1,
                    },
                );
                internal_port
            };

            *state.domain_refs.entry(hostname.clone()).or_insert(0) += 1;
            let access_url = format_access_url(&hostname, public_port);

            pending.push(TunnelRegistration { key });
            activated.push(ActivatedMapping {
                mapping_id: mapping.id.clone(),
                bind_host: "127.0.0.1".into(),
                bind_port: internal_port,
                remote_host: remote_host.to_string(),
                remote_port: mapping.remote_port,
                access_url,
            });
        }

        drop(routes);
        state.tunnel_regs.insert(tunnel_id.to_string(), pending);
        state.sync_hosts_locked()?;
        state.ensure_routers_locked(&self.routes)?;

        for m in &activated {
            if !is_ip_address(&m.remote_host) {
                tracing::info!(
                    "routing {} → 127.0.0.1:{}",
                    format_access_url(&m.remote_host.to_ascii_lowercase(), m.remote_port),
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
                    let internal = entry.internal_port;
                    routes.remove(&reg.key);
                    state.used_ports.remove(&internal);
                }
            }
            if let Some(count) = state.domain_refs.get_mut(&reg.key.hostname) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    state.domain_refs.remove(&reg.key.hostname);
                }
            }
        }

        drop(routes);
        let _ = state.sync_hosts_locked();
        state.stop_unused_routers_locked(&self.routes);
    }

    pub fn shutdown_all(&self) {
        let mut state = self.inner.lock();
        self.routes.lock().clear();
        state.tunnel_regs.clear();
        state.domain_refs.clear();
        state.used_ports.clear();
        if let Some(mut proxy) = state.sni_proxy.take() {
            proxy.stop();
        }
        for (_, mut router) in state.http_routers.drain() {
            router.stop();
        }
        // Skip hosts sync on quit — `elevation::write_hosts_file` can block for
        // seconds (UAC / PowerShell) and freezes the window. Orphans are removed
        // on the next launch via `bootstrap()`.
    }
}

impl RouterState {
    fn allocate_port(&mut self) -> Option<u16> {
        for port in PORT_POOL_START..=PORT_POOL_END {
            if !self.used_ports.contains(&port) {
                self.used_ports.insert(port);
                return Some(port);
            }
        }
        None
    }

    fn sync_hosts_locked(&self) -> Result<()> {
        hosts::sync_domains(&self.domain_refs)
    }

    fn ensure_routers_locked(
        &mut self,
        routes: &Arc<Mutex<HashMap<RouteKey, RouteEntry>>>,
    ) -> Result<()> {
        let needs_sni = routes.lock().keys().any(|k| k.public_port == 443);

        if needs_sni && self.sni_proxy.is_none() {
            let resolve = self.resolve.clone();
            self.sni_proxy = Some(SniProxy::start(resolve).context(
                "starting SNI router on 127.0.0.1:443 (is another service using port 443?)",
            )?);
        }

        let http_ports: HashSet<u16> = routes
            .lock()
            .keys()
            .filter(|k| k.public_port != 443)
            .map(|k| k.public_port)
            .collect();

        for port in http_ports {
            if !self.http_routers.contains_key(&port) {
                let resolve = self.resolve.clone();
                let router = HttpRouter::start(port, resolve).with_context(|| {
                    format!("starting HTTP router on 127.0.0.1:{port}")
                })?;
                self.http_routers.insert(port, router);
            }
        }
        Ok(())
    }

    fn stop_unused_routers_locked(
        &mut self,
        routes: &Arc<Mutex<HashMap<RouteKey, RouteEntry>>>,
    ) {
        let routes_guard = routes.lock();
        let needs_sni = routes_guard.keys().any(|k| k.public_port == 443);
        drop(routes_guard);

        if !needs_sni {
            if let Some(mut proxy) = self.sni_proxy.take() {
                proxy.stop();
            }
        }

        let active_http: HashSet<u16> = routes
            .lock()
            .keys()
            .filter(|k| k.public_port != 443)
            .map(|k| k.public_port)
            .collect();

        let stale: Vec<u16> = self
            .http_routers
            .keys()
            .filter(|p| !active_http.contains(p))
            .copied()
            .collect();
        for port in stale {
            if let Some(mut router) = self.http_routers.remove(&port) {
                router.stop();
            }
        }
    }
}

fn is_ip_address(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok()
}

fn format_access_url(host: &str, port: u16) -> String {
    match port {
        443 => format!("https://{host}"),
        80 => format!("http://{host}"),
        _ => format!("https://{host}:{port}"),
    }
}
