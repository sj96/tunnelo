//! Tauri commands exposed to the React frontend.

use crate::model::TunnelProfile;
use crate::AppState;
use tauri::{AppHandle, State};

/// Convert any error into a string for the IPC boundary.
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

#[tauri::command]
pub fn list_tunnels(state: State<AppState>) -> Vec<TunnelProfile> {
    state.store.list()
}

#[tauri::command]
pub fn get_tunnel(state: State<AppState>, id: String) -> Option<TunnelProfile> {
    state.store.get(&id)
}

#[tauri::command]
pub fn save_tunnel(
    state: State<AppState>,
    profile: TunnelProfile,
) -> Result<TunnelProfile, String> {
    state.store.upsert(profile).map_err(e)
}

#[tauri::command]
pub fn delete_tunnel(state: State<AppState>, id: String) -> Result<(), String> {
    state.tunnels.stop(&id).ok();
    crate::secrets::delete_all(&id);
    state.store.delete(&id).map_err(e)
}

// --- Secrets (OS keyring). `kind` is "password" or "passphrase". ---

#[tauri::command]
pub fn set_secret(id: String, kind: String, value: String) -> Result<(), String> {
    crate::secrets::set(&id, &kind, &value).map_err(e)
}

#[tauri::command]
pub fn delete_secret(id: String, kind: String) -> Result<(), String> {
    crate::secrets::delete(&id, &kind).map_err(e)
}

#[tauri::command]
pub fn has_secret(id: String, kind: String) -> bool {
    crate::secrets::has(&id, &kind)
}

// --- Tunnel control (russh engine). ---

#[tauri::command]
pub fn start_tunnel(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    let profile = state
        .store
        .get(&id)
        .ok_or_else(|| format!("no tunnel with id {id}"))?;
    state.tunnels.start(app, profile)
}

#[tauri::command]
pub fn stop_tunnel(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    state.tunnels.stop(&id)?;
    state.local_router.deactivate_tunnel(&id);
    crate::tunnel::emit_stopped(&app, &id);
    Ok(())
}

#[tauri::command]
pub fn tunnel_running(state: State<AppState>, id: String) -> bool {
    state.tunnels.is_running(&id)
}

// --- SSH host key management (TOFU store). ---

#[tauri::command]
pub fn list_host_keys() -> std::collections::HashMap<String, String> {
    crate::host_keys::list()
}

#[tauri::command]
pub fn forget_host_key(
    state: State<AppState>,
    host: String,
    port: u16,
    tunnel_id: Option<String>,
) -> Result<(), String> {
    crate::host_keys::forget(&host, port).map_err(e)?;
    if let Some(id) = tunnel_id {
        if let Some(ip) = state.tunnels.get_resolved_bastion(&id) {
            let _ = crate::host_keys::forget(&ip, port);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn default_ssh_key_path() -> String {
    crate::platform::default_ssh_key_path()
}
