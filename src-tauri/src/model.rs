//! Core data model for Tunnelo tunnel profiles and runtime state.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How the SSH client authenticates to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SshAuth {
    /// Public-key auth. `keyPath` points at a private key on disk; the
    /// passphrase (if any) is stored in the OS keyring, never in the profile.
    #[serde(rename_all = "camelCase")]
    Key {
        key_path: String,
        /// True if the key is passphrase-protected (passphrase lives in keyring).
        has_passphrase: bool,
    },
    /// Password auth. The password itself lives in the OS keyring.
    Password,
    /// Use the running ssh-agent.
    Agent,
}

impl Default for SshAuth {
    fn default() -> Self {
        SshAuth::Agent
    }
}

/// SSH server connection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub auth: SshAuth,
}

fn default_ssh_port() -> u16 {
    22
}

fn default_remote_port() -> u16 {
    443
}

/// One remote target reached via SSH local port forward (`-L`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardMapping {
    pub id: String,
    /// Target host as seen from the SSH bastion, e.g. `gitlab.example.com`.
    #[serde(default)]
    pub remote_host: String,
    #[serde(default = "default_remote_port")]
    pub remote_port: u16,
    /// `http` / `https` for web URLs; absent for bare `host:port` (e.g. IP:5432).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_scheme: Option<String>,
    /// Legacy local bind — migrated on load, not persisted.
    #[serde(default, skip_serializing)]
    pub local_host: String,
    #[serde(default, skip_serializing)]
    pub local_port: u16,
    /// Legacy (-R/Caddy): public hostname — migrated into `remote_host`.
    #[serde(default, skip_serializing)]
    pub public_host: String,
    /// Legacy: subdomain under a shared base domain.
    #[serde(default, skip_serializing)]
    pub subdomain: String,
}

impl ForwardMapping {
    pub fn new_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// Public URL the user opens once routing is active (same as remote identity).
    pub fn local_access_url(&self) -> String {
        format_remote_url(
            &self.remote_host,
            self.remote_port,
            self.remote_scheme.as_deref(),
        )
    }
}

fn normalize_remote_host(raw: &str) -> String {
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
            if after.strip_prefix(':').is_some_and(|port| {
                !port.is_empty() && port.chars().all(|c| c.is_ascii_digit())
            }) {
                return host;
            }
            return host;
        }
    }

    if let Some((host, port)) = s.rsplit_once(':') {
        if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            return host.trim_end_matches('.').to_string();
        }
    }

    s.trim_end_matches('.').to_string()
}

pub fn format_remote_url(host: &str, port: u16, remote_scheme: Option<&str>) -> String {
    let host = normalize_remote_host(host);
    if host.is_empty() {
        return String::new();
    }

    match remote_scheme {
        Some("http") => {
            if port == 80 {
                format!("http://{host}")
            } else {
                format!("http://{host}:{port}")
            }
        }
        Some("https") => {
            if port == 443 {
                format!("https://{host}")
            } else {
                format!("https://{host}:{port}")
            }
        }
        _ => format!("{host}:{port}"),
    }
}

fn infer_remote_scheme(host: &str, port: u16, existing: &Option<String>) -> Option<String> {
    if let Some(s) = existing {
        if s == "http" || s == "https" {
            return Some(s.clone());
        }
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return match port {
            80 => Some("http".into()),
            443 => Some("https".into()),
            _ => None,
        };
    }
    match port {
        80 => Some("http".into()),
        _ => Some("https".into()),
    }
}

/// Legacy single-service fields kept only for migration on load.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LocalService {
    host: String,
    port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PublicBinding {
    subdomain: String,
    base_domain: String,
}

/// A complete, persisted tunnel definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelProfile {
    pub id: String,
    pub name: String,
    pub ssh: SshConfig,
    /// One SSH connection; each mapping is a separate `-L` forward.
    #[serde(default)]
    pub mappings: Vec<ForwardMapping>,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    /// Legacy fields — migrated into `mappings` on load.
    #[serde(default, skip_serializing)]
    base_domain: String,
    #[serde(default, skip_serializing)]
    local_service: LocalService,
    #[serde(default, skip_serializing)]
    public: PublicBinding,
    /// Legacy Caddy flag — ignored after pivot to `-L`.
    #[serde(default, skip_serializing, rename = "manageCaddy")]
    _manage_caddy: bool,
}

impl TunnelProfile {
    /// Normalize legacy profiles and ensure mapping ids exist.
    pub fn normalize(mut self) -> Self {
        if self.mappings.is_empty()
            && (!self.public.subdomain.is_empty() || self.local_service.port != 0)
        {
            if self.base_domain.is_empty() {
                self.base_domain = self.public.base_domain.clone();
            }
            let remote_host = if !self.public.subdomain.is_empty() {
                format!("{}.{}", self.public.subdomain, self.public.base_domain)
            } else {
                String::new()
            };
            self.mappings.push(ForwardMapping {
                id: ForwardMapping::new_id(),
                remote_host: remote_host.clone(),
                remote_port: if self.local_service.port != 0 {
                    self.local_service.port
                } else {
                    default_remote_port()
                },
                remote_scheme: None,
                local_host: String::new(),
                local_port: 0,
                public_host: String::new(),
                subdomain: String::new(),
            });
        }
        for m in &mut self.mappings {
            if m.remote_host.is_empty() {
                if !m.public_host.is_empty() {
                    m.remote_host = m.public_host.clone();
                } else if !m.subdomain.is_empty() {
                    if !self.base_domain.is_empty() {
                        m.remote_host = format!("{}.{}", m.subdomain, self.base_domain);
                    } else {
                        m.remote_host = m.subdomain.clone();
                    }
                } else if !m.local_host.is_empty()
                    && m.local_host != "127.0.0.1"
                    && m.local_host != "0.0.0.0"
                {
                    m.remote_host = m.local_host.clone();
                }
            }
            if m.remote_port == 0 {
                m.remote_port = if m.local_port != 0 {
                    m.local_port
                } else {
                    default_remote_port()
                };
            }
            if m.id.is_empty() {
                m.id = ForwardMapping::new_id();
            }
            m.remote_host = normalize_remote_host(&m.remote_host);
            m.remote_scheme = infer_remote_scheme(&m.remote_host, m.remote_port, &m.remote_scheme);
            m.local_host.clear();
            m.local_port = 0;
        }
        self
    }

    pub fn local_urls(&self) -> Vec<String> {
        self.mappings
            .iter()
            .filter(|m| !m.remote_host.is_empty() && m.remote_port > 0)
            .map(|m| m.local_access_url())
            .collect()
    }

    pub fn new_id() -> String {
        Uuid::new_v4().to_string()
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_remote_url_https_default_port() {
        assert_eq!(
            format_remote_url("hrm.mservice.com.vn", 443, Some("https")),
            "https://hrm.mservice.com.vn"
        );
    }

    #[test]
    fn format_remote_url_http_default_port() {
        assert_eq!(
            format_remote_url("nexus.mservice.com.vn", 80, Some("http")),
            "http://nexus.mservice.com.vn"
        );
    }

    #[test]
    fn format_remote_url_https_custom_port() {
        assert_eq!(
            format_remote_url("atlassiansuite.mservice.com.vn", 8443, Some("https")),
            "https://atlassiansuite.mservice.com.vn:8443"
        );
    }

    #[test]
    fn format_remote_url_bare_ip_port() {
        assert_eq!(
            format_remote_url("172.16.54.37", 5432, None),
            "172.16.54.37:5432"
        );
    }

    #[test]
    fn infer_remote_scheme_legacy_profiles() {
        assert_eq!(
            infer_remote_scheme("hrm.mservice.com.vn", 443, &None),
            Some("https".into())
        );
        assert_eq!(
            infer_remote_scheme("nexus.mservice.com.vn", 80, &None),
            Some("http".into())
        );
        assert_eq!(
            infer_remote_scheme("172.16.54.37", 5432, &None),
            None
        );
        assert_eq!(
            infer_remote_scheme("app.example.com", 8443, &None),
            Some("https".into())
        );
    }

    #[test]
    fn round_trip_user_examples_via_format() {
        let cases = [
            ("hrm.mservice.com.vn", 443, Some("https"), "https://hrm.mservice.com.vn"),
            ("nexus.mservice.com.vn", 80, Some("http"), "http://nexus.mservice.com.vn"),
            (
                "atlassiansuite.mservice.com.vn",
                8443,
                Some("https"),
                "https://atlassiansuite.mservice.com.vn:8443",
            ),
            ("172.16.54.37", 5432, None, "172.16.54.37:5432"),
        ];
        for (host, port, scheme, expected) in cases {
            assert_eq!(format_remote_url(host, port, scheme), expected);
        }
    }
}

/// Runtime status of a tunnel, surfaced to the UI via events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelStatus {
    Stopped,
    Connecting,
    Connected,
    Reconnecting,
    Error,
}

impl Default for TunnelStatus {
    fn default() -> Self {
        TunnelStatus::Stopped
    }
}

/// A status snapshot emitted to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelState {
    pub id: String,
    pub status: TunnelStatus,
    /// Local access URLs once connected (one per forward mapping).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_urls: Option<Vec<String>>,
    /// First local URL — kept for backward-compatible UI consumers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_url: Option<String>,
    /// Legacy alias — same as `local_urls` for older frontends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_urls: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    /// Last error message, if status == Error.
    pub error: Option<String>,
    /// Resolved bastion IP when `ssh.host` is a wildcard pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_bastion_host: Option<String>,
}
