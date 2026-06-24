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
        format_remote_url(&self.remote_host, self.remote_port)
    }
}

pub fn format_remote_url(host: &str, port: u16) -> String {
    if host.is_empty() {
        return String::new();
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return match port {
            443 => "https://127.0.0.1".into(),
            80 => "http://127.0.0.1".into(),
            _ => format!("https://127.0.0.1:{port}"),
        };
    }
    match port {
        443 => format!("https://{host}"),
        80 => format!("http://{host}"),
        _ => format!("https://{host}:{port}"),
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
                remote_host,
                remote_port: if self.local_service.port != 0 {
                    self.local_service.port
                } else {
                    default_remote_port()
                },
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
