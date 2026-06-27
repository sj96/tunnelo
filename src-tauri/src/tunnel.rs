//! SSH tunnel engine built on russh.
//!
//! Each running tunnel opens ONE SSH session and spawns one local port forward
//! (`-L`) per mapping: bind on the client, `channel_open_direct_tcpip` to the
//! remote target as seen from the bastion.

use crate::bastion_resolve;
use crate::host_keys;
use crate::local_router::ActivatedMapping;
use crate::model::{format_local_access_url, SshAuth, TunnelProfile, TunnelState, TunnelStatus};
use crate::secrets;
use crate::AppState;
use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use russh::client::{self, Handle};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};

/// Outcome of a single connection attempt.
enum Outcome {
    /// User asked the tunnel to stop.
    Shutdown,
    /// The session dropped after being established (candidate for reconnect).
    Lost,
}

/// A live tunnel: signalling `shutdown` (or aborting) tears the session down.
struct RunningTunnel {
    shutdown: watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Default)]
pub struct TunnelManager {
    running: Mutex<HashMap<String, RunningTunnel>>,
    resolved_bastion: Mutex<HashMap<String, String>>,
}

impl TunnelManager {
    pub fn is_running(&self, id: &str) -> bool {
        self.running.lock().contains_key(id)
    }

    pub fn get_resolved_bastion(&self, id: &str) -> Option<String> {
        self.resolved_bastion.lock().get(id).cloned()
    }

    /// Spawn a tunnel. Status transitions are emitted to the frontend.
    pub fn start(&self, app: AppHandle, profile: TunnelProfile) -> Result<(), String> {
        let id = profile.id.clone();
        if self.running.lock().contains_key(&id) {
            return Err("tunnel already running".into());
        }

        let (tx, rx) = watch::channel(false);
        let app2 = app.clone();
        let id2 = id.clone();
        let task = tauri::async_runtime::spawn(async move {
            supervise(&app2, &profile, rx).await;
            let state = app2.state::<AppState>();
            state.local_router.deactivate_tunnel(&id2);
            state
                .tunnels
                .running
                .lock()
                .remove(&id2);
        });

        self.running
            .lock()
            .insert(id, RunningTunnel { shutdown: tx, task });
        Ok(())
    }

    /// Stop every running tunnel (app quit).
    pub fn stop_all(&self) {
        let tunnels: Vec<RunningTunnel> = self.running.lock().drain().map(|(_, rt)| rt).collect();
        self.resolved_bastion.lock().clear();
        for rt in tunnels {
            let _ = rt.shutdown.send(true);
            rt.task.abort();
        }
    }

    /// Signal a tunnel to shut down. The task emits `Stopped` when it unwinds.
    pub fn stop(&self, id: &str) -> Result<(), String> {
        let Some(rt) = self.running.lock().remove(id) else {
            return Ok(());
        };
        self.resolved_bastion.lock().remove(id);
        let _ = rt.shutdown.send(true);
        let task = rt.task;
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(8)).await;
            task.abort();
        });
        Ok(())
    }
}

/// Notify the UI that a tunnel has stopped (also used when stop is invoked from IPC).
pub fn emit_stopped(app: &AppHandle, id: &str) {
    emit_state(app, id, TunnelStatus::Stopped, None, None, None, None);
}

/// Minimal client handler — host key verification only.
struct ClientHandler {
    app: AppHandle,
    id: String,
    ssh_host: String,
    ssh_port: u16,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        match host_keys::verify(&self.ssh_host, self.ssh_port, server_public_key) {
            Ok(()) => Ok(true),
            Err(e) => {
                emit_log(
                    &self.app,
                    &self.id,
                    LogLevel::Error,
                    &format!("Host key verification failed: {e:#}"),
                );
                emit_state(
                    &self.app,
                    &self.id,
                    TunnelStatus::Error,
                    None,
                    Some(format!("{e:#}")),
                    None,
                    None,
                );
                Ok(false)
            }
        }
    }
}

/// User-initiated stop while a blocking operation is in progress.
#[derive(Debug)]
struct ShutdownRequested;

impl std::fmt::Display for ShutdownRequested {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shutdown requested")
    }
}

impl std::error::Error for ShutdownRequested {}

fn is_shutdown(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|e| e.downcast_ref::<ShutdownRequested>().is_some())
}

fn is_benign_forward_close(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::UnexpectedEof
    ) || matches!(err.raw_os_error(), Some(10053) | Some(10054))
}

/// Relay bytes between local TCP and SSH channel; shutdown write-half when one side EOFs.
async fn relay_forward(
    socket: &mut TcpStream,
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
) -> std::io::Result<()> {
    let (mut cr, mut cw) = socket.split();
    let (mut sr, mut sw) = tokio::io::split(stream);
    tokio::select! {
        r = tokio::io::copy(&mut cr, &mut sw) => {
            r?;
            let _ = sw.shutdown().await;
        }
        r = tokio::io::copy(&mut sr, &mut cw) => {
            r?;
            let _ = cw.shutdown().await;
        }
    }
    Ok(())
}

fn log_forward_close(app: &AppHandle, id: &str, e: std::io::Error) {
    if is_benign_forward_close(&e) {
        tracing::debug!(tunnel_id = %id, "Forward closed: {e}");
    } else {
        emit_log(
            app,
            id,
            LogLevel::Warn,
            &format!("Forward closed: {e}"),
        );
    }
}

const RETRY_DELAY_SECS: u64 = 5;
/// Max wait for bastion to open the SSH direct-tcpip channel to the remote target.
const REMOTE_FORWARD_TIMEOUT: Duration = Duration::from_secs(15);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Supervises one tunnel across (re)connections with fixed-delay retry on
/// failed reconnect attempts. Emits status transitions to the UI.
async fn supervise(app: &AppHandle, profile: &TunnelProfile, mut shutdown: watch::Receiver<bool>) {
    let id = &profile.id;
    let state = app.state::<AppState>();

    let bindings = match state
        .local_router
        .activate_tunnel(id, &profile.mappings)
    {
        Ok(b) => b,
        Err(e) => {
            emit_log(
                app,
                id,
                LogLevel::Error,
                &format!("Routing setup failed: {e:#}"),
            );
            emit_state(
                app,
                id,
                TunnelStatus::Error,
                None,
                Some(format!("{e:#}")),
                None,
                None,
            );
            return;
        }
    };

    let mut reconnecting = false;
    loop {
        if *shutdown.borrow() {
            break;
        }
        emit_state(
            app,
            id,
            if reconnecting {
                TunnelStatus::Reconnecting
            } else {
                TunnelStatus::Connecting
            },
            None,
            None,
            None,
            None,
        );

        match run_tunnel(app, profile, &bindings, &mut shutdown).await {
            Ok(Outcome::Shutdown) => break,
            Ok(Outcome::Lost) => {
                if !profile.auto_reconnect || *shutdown.borrow() {
                    break;
                }
                emit_log(app, id, LogLevel::Warn, "Connection lost — reconnecting");
                emit_state(
                    app,
                    id,
                    TunnelStatus::Reconnecting,
                    None,
                    None,
                    None,
                    None,
                );
                reconnecting = true;
                continue;
            }
            Err(e) => {
                if *shutdown.borrow() {
                    break;
                }
                if reconnecting {
                    emit_log(
                        app,
                        id,
                        LogLevel::Warn,
                        &format!("Reconnect failed: {e:#}"),
                    );
                    if wait_retry(app, id, &mut shutdown, RETRY_DELAY_SECS).await {
                        break;
                    }
                    continue;
                }
                emit_log(app, id, LogLevel::Error, &format!("Error: {e:#}"));
                emit_state(
                    app,
                    id,
                    TunnelStatus::Error,
                    None,
                    Some(format!("{e:#}")),
                    None,
                    None,
                );
                return;
            }
        }
    }
    emit_state(app, id, TunnelStatus::Stopped, None, None, None, None);
}

/// Emit reconnecting state with countdown, wait, return true if shutdown requested.
async fn wait_retry(
    app: &AppHandle,
    id: &str,
    shutdown: &mut watch::Receiver<bool>,
    secs: u64,
) -> bool {
    let retry_at = now_ms() + secs * 1000;
    emit_state(
        app,
        id,
        TunnelStatus::Reconnecting,
        None,
        None,
        None,
        Some(retry_at),
    );
    wait_or_shutdown(shutdown, Duration::from_secs(secs)).await
}

/// Wait up to `duration`, returning true if shutdown was requested.
async fn wait_or_shutdown(shutdown: &mut watch::Receiver<bool>, duration: Duration) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(duration) => false,
        res = shutdown.changed() => res.is_err() || *shutdown.borrow(),
    }
}

/// Establish the session, set up `-L` forwards, then run until shutdown or loss.
async fn run_tunnel(
    app: &AppHandle,
    profile: &TunnelProfile,
    bindings: &[ActivatedMapping],
    shutdown: &mut watch::Receiver<bool>,
) -> Result<Outcome> {
    let id = &profile.id;
    if bindings.is_empty() {
        bail!("no port forwards configured");
    }

    let session = match connect_session(app, profile, shutdown).await {
        Ok(handle) => Arc::new(handle),
        Err(e) if is_shutdown(&e) => return Ok(Outcome::Shutdown),
        Err(e) => return Err(e),
    };
    let user = &profile.ssh.user;
    let state = app.state::<AppState>();
    let bastion_host = state
        .tunnels
        .get_resolved_bastion(id)
        .unwrap_or_else(|| profile.ssh.host.clone());

    let (forward_shutdown, forward_shutdown_rx) = watch::channel(false);
    let mut forward_tasks = Vec::with_capacity(bindings.len());
    let mut local_urls = Vec::with_capacity(bindings.len());

    for mapping in bindings {
        let allow_port_fallback = mapping.remote_host.parse::<std::net::IpAddr>().is_ok();
        let (task, bind_port) = spawn_local_forward(
            app,
            id.clone(),
            session.clone(),
            shutdown.clone(),
            forward_shutdown_rx.clone(),
            mapping.bind_host.clone(),
            mapping.bind_port,
            mapping.remote_host.clone(),
            mapping.remote_port,
            allow_port_fallback,
        )
        .await
        .with_context(|| {
            format!(
                "binding local {}:{}",
                mapping.bind_host, mapping.bind_port
            )
        })?;

        if bind_port != mapping.bind_port {
            emit_log(
                app,
                id,
                LogLevel::Warn,
                &format!(
                    "Port {} in use locally — using {}:{} instead (connect to this address)",
                    mapping.bind_port, mapping.bind_host, bind_port
                ),
            );
        }

        local_urls.push(if bind_port == mapping.bind_port {
            mapping.access_url.clone()
        } else {
            format_local_access_url(
                &mapping.remote_host,
                bind_port,
                None,
                &mapping.bind_host,
            )
        });

        forward_tasks.push(task);
        emit_log(
            app,
            id,
            LogLevel::Forward,
            &format!(
                "{}:{} → {}:{}",
                mapping.bind_host, bind_port, mapping.remote_host, mapping.remote_port
            ),
        );
    }

    let n = bindings.len();
    let forward_word = if n == 1 { "forward" } else { "forwards" };
    emit_log(
        app,
        id,
        LogLevel::Ready,
        &format!("Ready — {n} {forward_word} active via {bastion_host} ({user})"),
    );

    let resolved_bastion = if bastion_resolve::is_wildcard_host(&profile.ssh.host) {
        state.tunnels.get_resolved_bastion(id)
    } else {
        None
    };
    let urls = local_urls;
    emit_state(
        app,
        id,
        TunnelStatus::Connected,
        Some(urls),
        None,
        resolved_bastion,
        None,
    );

    let mut tick = tokio::time::interval(Duration::from_secs(5));
    let outcome = loop {
        tokio::select! {
            res = shutdown.changed() => {
                if res.is_err() || *shutdown.borrow() {
                    break Outcome::Shutdown;
                }
            }
            _ = tick.tick() => {
                if session.is_closed() {
                    emit_log(app, id, LogLevel::Warn, "Session closed by server");
                    break Outcome::Lost;
                }
            }
        }
    };

    if matches!(outcome, Outcome::Shutdown) {
        emit_log(app, id, LogLevel::Info, "Shutting down");
    }
    let _ = forward_shutdown.send(true);
    for task in forward_tasks {
        let _ = task.await;
    }
    Ok(outcome)
}

/// Connect and authenticate. Cancellable while the TCP/handshake is in flight.
async fn connect_session(
    app: &AppHandle,
    profile: &TunnelProfile,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<Handle<ClientHandler>> {
    loop {
        if *shutdown.borrow() {
            bail!(ShutdownRequested);
        }
        let mut shutdown_scan = shutdown.clone();
        tokio::select! {
            biased;
            res = shutdown.changed() => {
                if res.is_err() || *shutdown.borrow() {
                    bail!(ShutdownRequested);
                }
            }
            res = connect_with_bastion_resolve(app, profile, &mut shutdown_scan) => return res,
        }
    }
}

async fn connect_with_bastion_resolve(
    app: &AppHandle,
    profile: &TunnelProfile,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<Handle<ClientHandler>> {
    let state = app.state::<AppState>();
    let is_wildcard = bastion_resolve::is_wildcard_host(&profile.ssh.host);
    let had_cache = is_wildcard && state.tunnels.get_resolved_bastion(&profile.id).is_some();

    let resolved =
        resolve_bastion_host(app, profile, &state.tunnels, shutdown, false).await?;
    match connect_session_inner(app, profile, &resolved).await {
        Ok(handle) => Ok(handle),
        Err(e) if is_shutdown(&e) => Err(e),
        Err(_e) if is_wildcard && had_cache => {
            let resolved =
                resolve_bastion_host(app, profile, &state.tunnels, shutdown, true).await?;
            connect_session_inner(app, profile, &resolved).await
        }
        Err(e) => Err(e),
    }
}

async fn resolve_bastion_host(
    app: &AppHandle,
    profile: &TunnelProfile,
    manager: &TunnelManager,
    shutdown: &mut watch::Receiver<bool>,
    force_rescan: bool,
) -> Result<String> {
    let pattern = &profile.ssh.host;
    if !bastion_resolve::is_wildcard_host(pattern) {
        return Ok(pattern.clone());
    }

    let id = &profile.id;
    if !force_rescan {
        if let Some(cached) = manager.get_resolved_bastion(id) {
            return Ok(cached);
        }
    } else if let Some(cached) = manager.get_resolved_bastion(id) {
        emit_log(
            app,
            id,
            LogLevel::Scan,
            &format!("Cached {cached} unreachable — rescanning"),
        );
        manager.resolved_bastion.lock().remove(id);
    }

    let candidates = bastion_resolve::expand_wildcard(pattern)
        .with_context(|| format!("invalid bastion pattern `{pattern}`"))?;
    emit_log(
        app,
        id,
        LogLevel::Scan,
        &format!("Scanning {pattern} — {n} hosts", n = candidates.len()),
    );

    let ip = match bastion_resolve::scan_ssh_port(candidates, profile.ssh.port, shutdown).await? {
        Some(ip) => ip,
        None if *shutdown.borrow() => bail!(ShutdownRequested),
        None => bail!("no bastion found for pattern {pattern}"),
    };

    let ip_str = ip.to_string();
    emit_log(app, id, LogLevel::Scan, &format!("Found bastion {ip_str}"));
    manager
        .resolved_bastion
        .lock()
        .insert(id.clone(), ip_str.clone());
    Ok(ip_str)
}

async fn connect_session_inner(
    app: &AppHandle,
    profile: &TunnelProfile,
    bastion_host: &str,
) -> Result<Handle<ClientHandler>> {
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        ..Default::default()
    });
    let handler = ClientHandler {
        app: app.clone(),
        id: profile.id.clone(),
        ssh_host: bastion_host.to_string(),
        ssh_port: profile.ssh.port,
    };
    let addr = format!("{}:{}", bastion_host, profile.ssh.port);
    emit_log(app, &profile.id, LogLevel::Connect, &format!("Connecting to {addr}"));
    let mut session = client::connect(config, addr.as_str(), handler)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    authenticate(&mut session, profile).await?;
    let user = &profile.ssh.user;
    emit_log(
        app,
        &profile.id,
        LogLevel::Auth,
        &format!("Authenticated as {user}"),
    );
    Ok(session)
}

/// Bind a local port; tunnel each accepted connection to `remote_host:remote_port`
/// via `channel_open_direct_tcpip` on the SSH session.
/// Returns the listener task and the actual bound port (may differ when `allow_port_fallback`).
async fn spawn_local_forward(
    app: &AppHandle,
    id: String,
    session: Arc<Handle<ClientHandler>>,
    mut tunnel_shutdown: watch::Receiver<bool>,
    mut session_shutdown: watch::Receiver<bool>,
    local_host: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
    allow_port_fallback: bool,
) -> Result<(JoinHandle<()>, u16)> {
    let (listener, bind_port) =
        bind_local_listener(&local_host, local_port, allow_port_fallback).await?;
    let app = app.clone();
    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                changed = tunnel_shutdown.changed() => {
                    if changed.is_err() || *tunnel_shutdown.borrow() {
                        break;
                    }
                }
                changed = session_shutdown.changed() => {
                    if changed.is_err() || *session_shutdown.borrow() {
                        break;
                    }
                }
                accept = listener.accept() => {
                    let Ok((mut socket, peer)) = accept else { break };
                    let _ = socket.set_nodelay(true);
                    let session = session.clone();
                    let rh = remote_host.clone();
                    let rp = remote_port;
                    let app = app.clone();
                    let id = id.clone();
                    let origin_host = peer.ip().to_string();
                    let origin_port = peer.port();
                    tokio::spawn(async move {
                        let open = tokio::time::timeout(
                            REMOTE_FORWARD_TIMEOUT,
                            session.channel_open_direct_tcpip(
                                &rh,
                                rp as u32,
                                &origin_host,
                                origin_port as u32,
                            ),
                        )
                        .await;
                        match open {
                            Ok(Ok(channel)) => {
                                let mut stream = channel.into_stream();
                                if let Err(e) = relay_forward(&mut socket, &mut stream).await {
                                    log_forward_close(&app, &id, e);
                                }
                            }
                            Ok(Err(e)) => {
                                emit_log(
                                    &app,
                                    &id,
                                    LogLevel::Error,
                                    &format!("Cannot reach {rh}:{rp} via bastion — {e}"),
                                );
                                let _ = socket.shutdown().await;
                            }
                            Err(_) => {
                                emit_log(
                                    &app,
                                    &id,
                                    LogLevel::Error,
                                    &format!(
                                        "Timed out connecting to {rh}:{rp} via bastion — check routing/firewall from bastion"
                                    ),
                                );
                                let _ = socket.shutdown().await;
                            }
                        }
                    });
                }
            }
        }
    });
    Ok((handle, bind_port))
}

/// Bind `local_host:preferred_port`, falling back to an ephemeral port for IP/TCP forwards.
async fn bind_local_listener(
    local_host: &str,
    preferred_port: u16,
    allow_port_fallback: bool,
) -> Result<(TcpListener, u16)> {
    match TcpListener::bind((local_host, preferred_port)).await {
        Ok(listener) => {
            let port = if preferred_port == 0 {
                listener
                    .local_addr()
                    .context("reading bind port")?
                    .port()
            } else {
                preferred_port
            };
            Ok((listener, port))
        }
        Err(e) if allow_port_fallback && e.kind() == std::io::ErrorKind::AddrInUse => {
            let listener = TcpListener::bind((local_host, 0))
                .await
                .with_context(|| format!("binding ephemeral port on {local_host}"))?;
            let port = listener
                .local_addr()
                .context("reading ephemeral bind port")?
                .port();
            Ok((listener, port))
        }
        Err(e) => Err(e).with_context(|| format!("binding {local_host}:{preferred_port}")),
    }
}

/// Try the configured auth method.
async fn authenticate(
    session: &mut Handle<ClientHandler>,
    profile: &TunnelProfile,
) -> Result<()> {
    let user = &profile.ssh.user;
    let ok = match &profile.ssh.auth {
        SshAuth::Key {
            key_path,
            has_passphrase,
        } => {
            let path = crate::platform::resolve_key_path(key_path);
            let path_display = path.display().to_string();
            let passphrase = if *has_passphrase {
                Some(
                    secrets::get(&profile.id, secrets::KIND_PASSPHRASE)
                        .ok_or_else(|| anyhow::anyhow!("no key passphrase stored for this tunnel"))?,
                )
            } else {
                None
            };
            let key = load_secret_key(&path, passphrase.as_deref())
                .with_context(|| format!("loading private key {path_display}"))?;
            session
                .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), None))
                .await
                .context("public-key auth")?
                .success()
        }
        SshAuth::Password => {
            let pass = secrets::get(&profile.id, secrets::KIND_PASSWORD)
                .ok_or_else(|| anyhow::anyhow!("no password stored for this tunnel"))?;
            session
                .authenticate_password(user, pass)
                .await
                .context("password auth")?
                .success()
        }
        SshAuth::Agent => authenticate_agent(session, user).await?,
    };
    if !ok {
        bail!("authentication failed (server rejected credentials)");
    }
    Ok(())
}

#[cfg(windows)]
async fn authenticate_agent(
    session: &mut Handle<ClientHandler>,
    user: &str,
) -> Result<bool> {
    use russh::keys::agent::client::AgentClient;
    let mut agent = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
        .await
        .context("connecting to OpenSSH agent (run «Start-Service ssh-agent» on Windows)")?;
    let identities = agent
        .request_identities()
        .await
        .context("requesting agent identities")?;
    if identities.is_empty() {
        bail!(
            "ssh-agent has no keys loaded. On Windows: run «Get-Service ssh-agent | Set-Service -StartupType Automatic; Start-Service ssh-agent» then «ssh-add $env:USERPROFILE\\.ssh\\id_ed25519»"
        );
    }
    for key in identities {
        if session
            .authenticate_publickey_with(user, key, None, &mut agent)
            .await
            .context("agent auth")?
            .success()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(not(windows))]
async fn authenticate_agent(
    session: &mut Handle<ClientHandler>,
    user: &str,
) -> Result<bool> {
    use russh::keys::agent::client::AgentClient;
    let mut agent = AgentClient::connect_env()
        .await
        .context("connecting to ssh-agent (SSH_AUTH_SOCK)")?;
    let identities = agent.request_identities().await.context("agent identities")?;
    if identities.is_empty() {
        bail!("ssh-agent has no identities loaded");
    }
    for key in identities {
        if session
            .authenticate_publickey_with(user, key, None, &mut agent)
            .await
            .context("agent auth")?
            .success()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn emit_state(
    app: &AppHandle,
    id: &str,
    status: TunnelStatus,
    local_urls: Option<Vec<String>>,
    error: Option<String>,
    resolved_bastion_host: Option<String>,
    retry_at_ms: Option<u64>,
) {
    let local_url = local_urls.as_ref().and_then(|u| u.first().cloned());
    let _ = app.emit(
        "tunnel://state",
        TunnelState {
            id: id.to_string(),
            status,
            local_urls: local_urls.clone(),
            local_url: local_url.clone(),
            public_urls: local_urls,
            public_url: local_url,
            error,
            resolved_bastion_host,
            retry_at_ms,
        },
    );
}

#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum LogLevel {
    Scan,
    Connect,
    Auth,
    Forward,
    Ready,
    Warn,
    Error,
    Info,
}

fn emit_log(app: &AppHandle, id: &str, level: LogLevel, message: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let _ = app.emit(
        "tunnel://log",
        serde_json::json!({ "id": id, "level": level, "message": message, "ts": ts }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_local_listener_uses_preferred_port_when_free() {
        let probe = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("probe bind");
        let free_port = probe.local_addr().unwrap().port();
        drop(probe);

        let (listener, port) = bind_local_listener("127.0.0.1", free_port, false)
            .await
            .expect("bind preferred");
        assert_eq!(port, free_port);
        drop(listener);
    }

    #[tokio::test]
    async fn bind_local_listener_falls_back_when_port_in_use() {
        let blocker = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("blocker bind");
        let blocked_port = blocker.local_addr().unwrap().port();

        let (listener, port) = bind_local_listener("127.0.0.1", blocked_port, true)
            .await
            .expect("fallback bind");
        assert_ne!(port, blocked_port);
        drop(listener);
        drop(blocker);
    }

    #[test]
    fn is_benign_forward_close_detects_expected_kinds() {
        use std::io::{Error, ErrorKind};

        assert!(is_benign_forward_close(&Error::from(
            ErrorKind::ConnectionReset,
        )));
        assert!(is_benign_forward_close(&Error::from(
            ErrorKind::ConnectionAborted,
        )));
        assert!(is_benign_forward_close(&Error::new(
            ErrorKind::ConnectionAborted,
            "aborted",
        )));
        assert!(is_benign_forward_close(&Error::from(
            ErrorKind::BrokenPipe,
        )));
        assert!(is_benign_forward_close(&Error::from(
            ErrorKind::UnexpectedEof,
        )));
        assert!(!is_benign_forward_close(&Error::new(
            ErrorKind::TimedOut,
            "timeout",
        )));
        assert!(!is_benign_forward_close(&Error::from(
            ErrorKind::TimedOut,
        )));
        assert!(!is_benign_forward_close(&Error::from(
            ErrorKind::AddrInUse,
        )));
        assert!(!is_benign_forward_close(&Error::new(
            ErrorKind::AddrInUse,
            "addr in use",
        )));
    }

    #[test]
    fn is_benign_forward_close_detects_windows_wsa_codes() {
        assert!(is_benign_forward_close(&std::io::Error::from_raw_os_error(10053)));
        assert!(is_benign_forward_close(&std::io::Error::from_raw_os_error(10054)));
        assert!(!is_benign_forward_close(&std::io::Error::from_raw_os_error(995)));
    }

    #[tokio::test]
    async fn bind_local_listener_errors_when_port_in_use_and_no_fallback() {
        let blocker = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("blocker bind");
        let blocked_port = blocker.local_addr().unwrap().port();

        let err = bind_local_listener("127.0.0.1", blocked_port, false)
            .await
            .expect_err("should fail without fallback");
        assert!(format!("{err:#}").contains(&blocked_port.to_string()));
        drop(blocker);
    }
}
