//! TwinFolders desktop app: menu-bar/tray icon + status panel.
//!
//! The app is only a viewer/controller: syncing runs in the background
//! service (slice 3) whether or not the app is open. Closing the window
//! hides it; "Sair" in the tray menu quits.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use twinfolders_core::client::SyncthingClient;
use twinfolders_core::overview::{self, Overview, RateSample};
use twinfolders_core::service::{self, ServiceConfig};
use twinfolders_core::{secrets, stconfig};

/// Events that change what the panel shows.
const WATCHED_EVENTS: &[&str] = &[
    "StateChanged",
    "FolderCompletion",
    "FolderSummary",
    "FolderErrors",
    "DeviceConnected",
    "DeviceDisconnected",
    "DevicePaused",
    "DeviceResumed",
    "ConfigSaved",
    "PendingDevicesChanged",
    "PendingFoldersChanged",
];

#[derive(Default)]
struct AppState {
    client: Mutex<Option<SyncthingClient>>,
    rates: Mutex<Option<RateSample>>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum Snapshot {
    Ok { overview: Overview },
    NotInstalled { message: String },
    NotRunning { message: String },
}

/// Syncthing home: the service's standard location, or `TWINFOLDERS_HOME`
/// for development against a throwaway instance (key then read from its
/// config.xml instead of the OS credential store).
fn home() -> Result<(PathBuf, bool), String> {
    if let Some(h) = std::env::var_os("TWINFOLDERS_HOME") {
        return Ok((PathBuf::from(h), true));
    }
    ServiceConfig::for_current_user(PathBuf::new())
        .map(|c| (c.home, false))
        .map_err(|e| e.to_string())
}

fn connect() -> Result<SyncthingClient, Snapshot> {
    let not_installed = |message: String| Snapshot::NotInstalled { message };
    let (home, dev) = home().map_err(not_installed)?;
    let xml = std::fs::read_to_string(home.join("config.xml")).map_err(|_| {
        not_installed(format!(
            "O serviço em segundo plano não está instalado ({} não existe). Rode `twinfolders service install`.",
            home.display()
        ))
    })?;
    let gui = stconfig::read_gui(&xml).map_err(|e| not_installed(e.to_string()))?;
    let gui_port = gui
        .address
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .ok_or_else(|| {
            not_installed(format!("endereço inválido no config.xml: {}", gui.address))
        })?;
    let key = if dev {
        gui.api_key.clone()
    } else {
        secrets::resolve_api_key(&home).map_err(not_installed)?
    };
    let cfg = ServiceConfig {
        syncthing_bin: PathBuf::new(),
        home,
        log_dir: PathBuf::new(),
        gui_port,
        listen_port: 0,
    };
    service::client_for(&cfg, &key).map_err(|e| not_installed(e.to_string()))
}

fn cached_client(st: &AppState) -> Result<SyncthingClient, Snapshot> {
    let mut slot = st.client.lock().unwrap();
    if let Some(c) = slot.as_ref() {
        return Ok(c.clone());
    }
    let c = connect()?;
    *slot = Some(c.clone());
    Ok(c)
}

fn snapshot(st: &AppState) -> Snapshot {
    let client = match cached_client(st) {
        Ok(c) => c,
        Err(s) => return s,
    };
    let prev = st.rates.lock().unwrap().clone();
    match overview::collect(&client, prev.as_ref()) {
        Ok((ov, sample)) => {
            *st.rates.lock().unwrap() = Some(sample);
            Snapshot::Ok { overview: ov }
        }
        Err(e) => {
            // Reconnect next time (daemon restarted, key rotated, ...).
            *st.client.lock().unwrap() = None;
            *st.rates.lock().unwrap() = None;
            Snapshot::NotRunning {
                message: format!("Não foi possível falar com o Syncthing local: {e}"),
            }
        }
    }
}

#[tauri::command]
async fn get_overview(state: State<'_, Arc<AppState>>) -> Result<Snapshot, String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || snapshot(&st))
        .await
        .map_err(|e| e.to_string())
}

/// Long-polls Syncthing events and tells the UI to refresh. Survives daemon
/// restarts: on error it reconnects and restarts from event 0, and if IDs go
/// backwards (new daemon process) it resynchronizes.
fn watch_events(app: AppHandle) {
    std::thread::spawn(move || {
        let mut since: u64 = 0;
        loop {
            let client = match connect() {
                Ok(c) => c,
                Err(_) => {
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            loop {
                match client.events(since, 30, WATCHED_EVENTS) {
                    Ok(evs) => {
                        if let Some(last) = evs.last() {
                            since = last.id;
                            let _ = app.emit("sync-changed", ());
                        }
                    }
                    Err(_) => {
                        since = 0;
                        let _ = app.emit("sync-changed", ());
                        std::thread::sleep(Duration::from_secs(3));
                        break;
                    }
                }
            }
        }
    });
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn main() {
    let state = Arc::new(AppState::default());
    tauri::Builder::default()
        // Must be first: a second launch focuses the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(state)
        .invoke_handler(tauri::generate_handler![get_overview])
        .setup(|app| {
            let open = MenuItem::with_id(app, "open", "Abrir painel", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Sair do TwinFolders", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(true)
                .tooltip("TwinFolders")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = e
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            watch_events(app.handle().clone());
            Ok(())
        })
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                // Keep running in the tray; syncing never depends on the app.
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running TwinFolders");
}
