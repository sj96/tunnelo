//! Tunnelo — SSH local port forward (`-L`) manager.

mod bastion_resolve;
mod commands;
mod elevation;
mod host_keys;
mod hosts;
mod local_router;
mod model;
mod platform;
mod secrets;
mod store;
mod tunnel;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use local_router::LocalRouter;
use store::ProfileStore;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, WindowEvent};
use tunnel::TunnelManager;

/// Set when the user explicitly quits so close/exit handlers allow termination.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Shared application state available to all Tauri commands.
pub struct AppState {
    pub store: Arc<ProfileStore>,
    pub tunnels: TunnelManager,
    pub local_router: LocalRouter,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tunnelo=info,warn".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("resolving app data dir");
            host_keys::init(data_dir.clone()).expect("loading host key store");
            let store = ProfileStore::load(data_dir).expect("loading profile store");
            if let Err(e) = LocalRouter::bootstrap() {
                tracing::warn!("hosts orphan cleanup: {e:#}");
            }
            app.manage(AppState {
                store: Arc::new(store),
                tunnels: TunnelManager::default(),
                local_router: LocalRouter::new(),
            });

            setup_tray(app)?;

            show_main_window(app.handle());

            // Auto-start tunnels flagged for launch.
            let handle = app.handle().clone();
            let state = app.state::<AppState>();
            for profile in state.store.list() {
                if profile.auto_start {
                    let _ = state.tunnels.start(handle.clone(), profile);
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                if SHUTTING_DOWN.load(Ordering::SeqCst) {
                    return;
                }
                shutdown_app(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tunnels,
            commands::get_tunnel,
            commands::save_tunnel,
            commands::delete_tunnel,
            commands::start_tunnel,
            commands::stop_tunnel,
            commands::tunnel_running,
            commands::set_secret,
            commands::delete_secret,
            commands::has_secret,
            commands::list_host_keys,
            commands::forget_host_key,
            commands::default_ssh_key_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event {
                // Only allow exit after an explicit shutdown (X button or tray Quit).
                if code.is_none() && !SHUTTING_DOWN.load(Ordering::SeqCst) {
                    api.prevent_exit();
                }
            }
        });
}

/// Tear down background work and terminate the process.
///
/// Must not block the UI thread: `CloseRequested` and tray Quit run on the main
/// loop. Cleanup runs on a background thread; the process exits immediately.
fn shutdown_app(app: &tauri::AppHandle) {
    if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    let app_bg = app.clone();
    std::thread::spawn(move || {
        if let Some(state) = app_bg.try_state::<AppState>() {
            state.tunnels.stop_all();
            state.local_router.shutdown_all();
        }

        #[cfg(desktop)]
        {
            use tauri_plugin_window_state::{AppHandleExt, StateFlags};
            let _ = app_bg.save_window_state(StateFlags::all());
        }
    });

    app.exit(0);
}

/// Build the system tray icon with a Show / Quit menu.
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Tunnelo", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Tunnelo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => shutdown_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
