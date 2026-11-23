//! TwinFolders desktop app: menu-bar/tray icon + status panel.
//!
//! The app is only a viewer/controller: syncing runs in the background
//! service (slice 3) whether or not the app is open. Closing the window
//! hides it; "Sair" in the tray menu quits.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audit;
mod office;
mod security;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use office::{Role, Workspace};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use twinfolders_core::client::{Ignores, PendingDevice, PendingFolder, SyncthingClient};
use twinfolders_core::conflict::Resolution;
use twinfolders_core::overview::{self, Overview, RateSample};
use twinfolders_core::pairing::{self, ShareRequest};
use twinfolders_core::preflight;
use twinfolders_core::resolve::{self, ConflictEntry, Outcome};
use twinfolders_core::service::{self, ServiceConfig};
use twinfolders_core::versioning::Choice;
use twinfolders_core::{activity, invite, setup, stconfig, tailscale};

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

/// Locks a mutex, recovering from poisoning: one panicked command must not
/// make every later command panic too.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Max directory entries walked per folder when looking for conflicts.
const CONFLICT_SCAN_LIMIT: usize = 200_000;
const CONFLICT_POLL: Duration = Duration::from_secs(60);

#[derive(Default)]
struct AppState {
    client: Mutex<Option<SyncthingClient>>,
    rates: Mutex<Option<RateSample>>,
    /// `folder_id/conflict_path` already notified, so each conflict
    /// produces one notification.
    notified: Mutex<BTreeSet<String>>,
    /// Last overview and when it was taken. Polls and event-driven refreshes
    /// can land milliseconds apart; reusing a fresh result avoids transfer
    /// rates computed over a near-zero interval (spikes or zeros).
    last: Mutex<Option<(Instant, Overview)>>,
    /// The app's config folder (role, invites, SSH users), set at startup.
    config_dir: Mutex<Option<PathBuf>>,
}

impl AppState {
    fn dir(&self) -> Result<PathBuf, String> {
        lock(&self.config_dir)
            .clone()
            .ok_or_else(|| "pasta de configuração indisponível".into())
    }

    /// Records an action done through the app (best effort).
    fn audit(&self, action: &str, detail: &str) {
        if let Ok(dir) = self.dir() {
            audit::record(&dir, office::now(), action, detail);
        }
    }

    fn workspace(&self) -> Workspace {
        self.dir()
            .map(|d| office::read_workspace(&d))
            .unwrap_or_default()
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FolderConflicts {
    folder_id: String,
    label: String,
    conflicts: Vec<ConflictEntry>,
    truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersioningView {
    /// `None` = a type the app does not manage (e.g. external): shown
    /// read-only, never overwritten by accident.
    choice: Option<Choice>,
    kind: String,
}

/// Folder root from Syncthing's own config (never from the UI), with
/// Syncthing's `~` expansion.
fn folder_root(c: &SyncthingClient, folder_id: &str) -> Result<PathBuf, String> {
    let f = c.folder(folder_id).map_err(|e| e.to_string())?;
    let p = &f.path;
    let rest = if p == "~" {
        Some("")
    } else {
        p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\"))
    };
    if let Some(rest) = rest {
        // Same variable Syncthing uses for `~` on each OS (Git Bash may
        // set HOME on Windows to something else).
        let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let home = std::env::var_os(var).ok_or("pasta pessoal desconhecida")?;
        return Ok(PathBuf::from(home).join(rest));
    }
    Ok(PathBuf::from(p))
}

/// Peer-offered folder labels end up in OS notifications: drop control and
/// invisible/bidi characters and cap the length.
fn clean_label(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(c, '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
        })
        .collect();
    let mut out: String = cleaned.trim().chars().take(80).collect();
    if cleaned.trim().chars().count() > 80 {
        out.push('…');
    }
    out
}

fn scan_all_conflicts(c: &SyncthingClient) -> Result<Vec<FolderConflicts>, String> {
    let mut out = Vec::new();
    for f in c.folders().map_err(|e| e.to_string())? {
        let Ok(root) = folder_root(c, &f.id) else {
            continue;
        };
        // A folder whose path is missing/unreadable is reported by the
        // overview as an error; skip it here.
        let Ok(r) = resolve::scan(&root, CONFLICT_SCAN_LIMIT) else {
            continue;
        };
        out.push(FolderConflicts {
            label: if f.label.is_empty() {
                f.id.clone()
            } else {
                f.label.clone()
            },
            folder_id: f.id,
            conflicts: r.conflicts,
            truncated: r.truncated,
        });
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum Snapshot {
    Ok { overview: Box<Overview> },
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
    let (home, _dev) = home().map_err(not_installed)?;
    let xml = std::fs::read_to_string(home.join("config.xml")).map_err(|_| {
        not_installed(format!(
            "O serviço em segundo plano ainda não foi ligado neste computador ({} não existe).",
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
    // config.xml (owner-only, written by the service install) is the source
    // of truth for the key; the OS credential store only mirrors it. Reading
    // the store from the app would trigger a Keychain prompt on macOS (the
    // item belongs to the CLI binary that created it) and block the first
    // connection until someone answers it, without adding any protection.
    let key = gui.api_key.clone();
    if key.is_empty() {
        return Err(not_installed(
            "config.xml sem chave da API; reinstale o serviço com `twinfolders service install`"
                .into(),
        ));
    }
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
    let mut slot = lock(&st.client);
    if let Some(c) = slot.as_ref() {
        return Ok(c.clone());
    }
    let c = connect()?;
    *slot = Some(c.clone());
    Ok(c)
}

const MIN_SNAPSHOT_GAP: Duration = Duration::from_secs(1);

fn snapshot(st: &AppState) -> Snapshot {
    // Held for the whole snapshot: serializes concurrent refreshes.
    let mut last = lock(&st.last);
    if let Some((at, ov)) = last.as_ref() {
        if at.elapsed() < MIN_SNAPSHOT_GAP {
            return Snapshot::Ok {
                overview: Box::new(ov.clone()),
            };
        }
    }
    let client = match cached_client(st) {
        Ok(c) => c,
        Err(s) => return s,
    };
    let prev = lock(&st.rates).clone();
    match overview::collect(&client, prev.as_ref()) {
        Ok((ov, sample)) => {
            finish_join(st, &client, &ov);
            *lock(&st.rates) = Some(sample);
            *last = Some((Instant::now(), ov.clone()));
            Snapshot::Ok {
                overview: Box::new(ov),
            }
        }
        Err(e) => {
            // Reconnect next time (daemon restarted, key rotated, ...).
            *lock(&st.client) = None;
            *lock(&st.rates) = None;
            *last = None;
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

/// Runs `f` with the connected client on a worker thread.
async fn with_client<T: Send + 'static>(
    state: &Arc<AppState>,
    f: impl FnOnce(&SyncthingClient) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let st = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let c = cached_client(&st).map_err(|s| match s {
            Snapshot::NotInstalled { message } | Snapshot::NotRunning { message } => message,
            Snapshot::Ok { .. } => unreachable!(),
        })?;
        f(&c)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    /// OS account name, for the greeting/avatar. Never leaves this machine.
    user: String,
    os: &'static str,
    version: String,
}

#[tauri::command]
fn get_profile(app: AppHandle) -> Profile {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    Profile {
        user,
        os: std::env::consts::OS,
        version: app.package_info().version.to_string(),
    }
}

/* ---------- first-run setup ---------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Setup {
    os: &'static str,
    /// Where Syncthing was found, if anywhere.
    syncthing: Option<String>,
    /// Package manager that can install it (`brew` / `winget`), if present.
    package_manager: Option<setup::PackageManager>,
    /// The install command, shown so the person can also run it themself.
    install_command: Option<String>,
    /// `None` when Tailscale is running; otherwise what to do, in pt-BR.
    tailscale_problem: Option<String>,
}

#[tauri::command]
async fn get_setup() -> Result<Setup, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let pm = setup::package_manager();
        Setup {
            os: std::env::consts::OS,
            syncthing: setup::find_syncthing().map(|p| p.display().to_string()),
            package_manager: pm.as_ref().map(|(k, _)| *k),
            install_command: pm
                .as_ref()
                .map(|(k, bin)| setup::syncthing_install_words(*k, bin).join(" ")),
            tailscale_problem: match tailscale::status() {
                Ok(s) if s.running() => None,
                Ok(_) => Some("o Tailscale está instalado, mas não está conectado.".into()),
                Err(e) => Some(e),
            },
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Opens a terminal running the package manager's "install Syncthing"
/// command, so the person sees the progress and any password prompt.
/// The command is built here from fixed words, never taken from the UI.
#[tauri::command]
async fn install_syncthing() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let (pm, bin) = setup::package_manager().ok_or(
            "nenhum instalador de programas encontrado (Homebrew no Mac, winget no Windows)",
        )?;
        let words = setup::syncthing_install_words(pm, &bin);
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let (prog, args) = setup::terminal_command(&refs)?;
        let mut child = std::process::Command::new(prog)
            .args(args)
            .spawn()
            .map_err(|e| format!("não foi possível abrir o terminal: {e}"))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Installs and starts the background service (same as
/// `twinfolders service install`): per-user, no administrator rights.
/// Returns this computer's device ID.
#[tauri::command]
async fn install_service(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let bin = setup::find_syncthing()
            .ok_or("o Syncthing ainda não está instalado neste computador")?;
        let cfg = ServiceConfig::for_current_user(bin).map_err(|e| e.to_string())?;
        let id = service::install(&cfg).map_err(|e| e.to_string())?;
        // Next snapshot connects to the new instance.
        *lock(&st.client) = None;
        *lock(&st.last) = None;
        Ok(id)
    })
    .await
    .map_err(|e| e.to_string())?
}

/* ---------- remote access (SSH over Tailscale) ---------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteMachine {
    device_id: String,
    name: String,
    /// MagicDNS name or tailnet IP; `None` when not found in the tailnet.
    host: Option<String>,
    online: bool,
    os: String,
    /// Account to log in as on that computer (saved, or this account's name).
    user: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteAccess {
    /// Plain-language problem with Tailscale here, if any.
    problem: Option<String>,
    machines: Vec<RemoteMachine>,
}

fn remote_file(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("remote.json"))
}

fn load_users(app: &AppHandle) -> std::collections::BTreeMap<String, String> {
    remote_file(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn local_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

fn remote_access(c: &SyncthingClient, app: &AppHandle) -> Result<RemoteAccess, String> {
    let users = load_users(app);
    let my_id = c.my_id().map_err(|e| e.to_string())?;
    let conns = c.connections().map_err(|e| e.to_string())?;
    let (problem, peers) = match tailscale::status() {
        Ok(s) if s.running() => (None, s.peers),
        Ok(_) => (
            Some("o Tailscale está instalado, mas desconectado. Abra o app do Tailscale e entre na sua conta.".into()),
            Vec::new(),
        ),
        Err(e) => (Some(e), Vec::new()),
    };
    let machines = c
        .devices()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|d| d.device_id != my_id)
        .map(|d| {
            let live = conns
                .get(&d.device_id)
                .map(|x| x.address.clone())
                .unwrap_or_default();
            let mut addrs: Vec<&str> = vec![live.as_str()];
            addrs.extend(d.addresses.iter().map(String::as_str));
            let peer = tailscale::match_device(&peers, &addrs);
            RemoteMachine {
                user: users.get(&d.device_id).cloned().unwrap_or_else(local_user),
                host: peer.and_then(|p| p.host()),
                online: peer.is_some_and(|p| p.online),
                os: peer.map(|p| p.os.clone()).unwrap_or_default(),
                name: d.name,
                device_id: d.device_id,
            }
        })
        .collect();
    Ok(RemoteAccess { problem, machines })
}

#[tauri::command]
async fn get_remote_access(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<RemoteAccess, String> {
    with_client(state.inner(), move |c| remote_access(c, &app)).await
}

/// Saves which account to use on the other computer.
#[tauri::command]
fn set_ssh_user(app: AppHandle, device_id: String, user: String) -> Result<(), String> {
    let user = user.trim().to_string();
    if !tailscale::valid_user(&user) {
        return Err(
            "nome de usuário inválido: use só letras, números, ponto, _ ou - (sem espaços)".into(),
        );
    }
    let path = remote_file(&app).ok_or("pasta de configuração do app indisponível")?;
    let mut users = load_users(&app);
    users.insert(device_id, user);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&users).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// Opens the system terminal running `ssh user@host` for a paired computer.
/// Host and user are resolved here (tailnet + saved settings), never taken
/// from the UI, and validated before reaching the command line.
#[tauri::command]
async fn open_terminal(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    device_id: String,
) -> Result<(), String> {
    with_client(state.inner(), move |c| {
        let access = remote_access(c, &app)?;
        if let Some(p) = access.problem {
            return Err(p);
        }
        let m = access
            .machines
            .into_iter()
            .find(|m| m.device_id == device_id)
            .ok_or("computador não encontrado")?;
        let host = m.host.ok_or_else(|| {
            format!(
                "{} não aparece no seu Tailscale. Instale e entre com a mesma conta nos dois.",
                m.name
            )
        })?;
        let target = tailscale::ssh_target(&m.user, &host)?;
        let (prog, args) = tailscale::terminal_command(&target)?;
        let mut child = std::process::Command::new(prog)
            .args(args)
            .spawn()
            .map_err(|e| format!("não foi possível abrir o terminal: {e}"))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })
    .await
}

/// Adds the other computer by its ID (as pasted) and optional address.
/// The other side then gets a pairing request to approve. Returns the
/// canonical ID.
#[tauri::command]
async fn add_device(
    state: State<'_, Arc<AppState>>,
    device_id: String,
    name: String,
    address: String,
) -> Result<String, String> {
    if let Ok(id) = twinfolders_core::peer::normalize_device_id(&device_id) {
        office::member_guard(&state.workspace(), &[&id])?;
    }
    with_client(state.inner(), move |c| {
        twinfolders_core::peer::add_peer(
            c,
            &device_id,
            &name,
            &address,
            service::DEFAULT_LISTEN_PORT,
        )
        .map_err(|e| e.to_string())
    })
    .await
}

/// Explicit user approval of a device that asked to pair.
#[tauri::command]
async fn approve_device(
    state: State<'_, Arc<AppState>>,
    device_id: String,
    name: String,
) -> Result<(), String> {
    let note = format!("{} ({})", invite::parse_tag(&name).0, short(&device_id));
    let ws = state.workspace();
    office::member_guard(&ws, &[&device_id])?;
    let dir = state.dir()?;
    with_client(state.inner(), move |c| {
        // An invited computer shows `name [tf:CODE]`: save the clean name,
        // use up the invite and offer the folders chosen for it.
        let (clean, proof) = invite::parse_tag(&name);
        let pending = PendingDevice {
            device_id: device_id.clone(),
            name: clean,
            address: String::new(),
            time: String::new(),
        };
        pairing::approve_device(c, &pending, None).map_err(|e| e.to_string())?;
        let Some(proof) = proof else { return Ok(()) };
        let mut list = office::read_invites(&dir);
        let now = office::now();
        let codes = office::open_codes(&list, now);
        let Some(code) = invite::match_code(&codes, &device_id, &proof).map(str::to_string) else {
            return Ok(());
        };
        let Some(folders) = office::open_invite(&list, &code, now).map(|r| r.folders.clone())
        else {
            return Ok(());
        };
        if let Some(r) = list.iter_mut().find(|r| r.code == code) {
            r.used_by = Some(device_id.clone());
        }
        office::write_invites(&dir, &list)?;
        let my_id = c.my_id().map_err(|e| e.to_string())?;
        for f in folders {
            let Ok(folder) = c.folder(&f) else { continue };
            let mut peers: Vec<String> = folder
                .devices
                .into_iter()
                .map(|d| d.device_id)
                .filter(|d| *d != my_id)
                .collect();
            if !peers.contains(&device_id) {
                peers.push(device_id.clone());
            }
            let refs: Vec<&str> = peers.iter().map(String::as_str).collect();
            pairing::set_folder_peers(c, &f, &my_id, &refs).map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await
    .inspect(|_| state.audit("Computador aprovado", &note))
}

/// Pre-share check of a local folder (Windows-invalid names, case
/// collisions, cloud-only placeholders, symlinks).
#[tauri::command]
async fn scan_folder(path: String) -> Result<preflight::Report, String> {
    tauri::async_runtime::spawn_blocking(move || {
        preflight::scan(std::path::Path::new(&path)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Shares a local folder with one paired device. Returns the folder ID.
#[tauri::command]
async fn share_folder(
    state: State<'_, Arc<AppState>>,
    path: String,
    label: String,
    peer_ids: Vec<String>,
    extra_ignores: Vec<String>,
) -> Result<String, String> {
    let note = format!("\"{label}\" com {} computador(es)", peer_ids.len());
    let refs: Vec<&str> = peer_ids.iter().map(String::as_str).collect();
    office::member_guard(&state.workspace(), &refs)?;
    with_client(state.inner(), move |c| {
        let local_id = c.my_id().map_err(|e| e.to_string())?;
        let existing: Vec<String> = c
            .folders()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|f| f.id)
            .collect();
        let id = pairing::folder_id_for(&label, &existing);
        let peers: Vec<&str> = peer_ids.iter().map(String::as_str).collect();
        let extras: Vec<&str> = extra_ignores.iter().map(String::as_str).collect();
        pairing::share_folder(
            c,
            &ShareRequest {
                id: &id,
                label: &label,
                path: std::path::Path::new(&path),
                local_id: &local_id,
                peer_ids: &peers,
                extra_ignores: &extras,
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    })
    .await
    .inspect(|_| state.audit("Pasta compartilhada", &note))
}

/// Sets which paired computers a folder is shared with.
#[tauri::command]
async fn set_folder_peers(
    state: State<'_, Arc<AppState>>,
    folder_id: String,
    peer_ids: Vec<String>,
) -> Result<(), String> {
    let note = format!("{folder_id}: agora com {} computador(es)", peer_ids.len());
    let refs: Vec<&str> = peer_ids.iter().map(String::as_str).collect();
    office::member_guard(&state.workspace(), &refs)?;
    with_client(state.inner(), move |c| {
        let local_id = c.my_id().map_err(|e| e.to_string())?;
        let peers: Vec<&str> = peer_ids.iter().map(String::as_str).collect();
        pairing::set_folder_peers(c, &folder_id, &local_id, &peers).map_err(|e| e.to_string())
    })
    .await
    .inspect(|_| state.audit("Compartilhamento alterado", &note))
}

/// Disconnects a computer: unshares every folder with it and removes it.
/// Its files stay where they are.
#[tauri::command]
async fn remove_device(state: State<'_, Arc<AppState>>, device_id: String) -> Result<(), String> {
    let note = short(&device_id);
    if state.workspace().role == Role::Member {
        return Err("num computador do time, só a central desconecta computadores".into());
    }
    with_client(state.inner(), move |c| {
        pairing::remove_device(c, &device_id).map_err(|e| e.to_string())
    })
    .await
    .inspect(|_| state.audit("Computador desconectado", &note))
}

/* ---------- role of this computer and office invites ---------- */

#[tauri::command]
fn get_workspace(state: State<'_, Arc<AppState>>) -> Workspace {
    state.workspace()
}

#[tauri::command]
fn set_role(state: State<'_, Arc<AppState>>, role: Role) -> Result<(), String> {
    let dir = state.dir()?;
    let mut ws = office::read_workspace(&dir);
    ws.role = role;
    if role != Role::Member {
        // Leaving the team: forget the central and any pending join.
        ws.hub_id = None;
        ws.hub_name = None;
        ws.joining = None;
    }
    office::write_workspace(&dir, &ws)?;
    state.audit("Papel alterado", &format!("{role:?}"));
    Ok(())
}

/// This computer's name as the other side sees it.
fn my_device_name(c: &SyncthingClient, my_id: &str) -> String {
    c.devices()
        .ok()
        .and_then(|ds| ds.into_iter().find(|d| d.device_id == my_id))
        .map(|d| d.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| my_id.chars().take(7).collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenInvite {
    code: String,
    expires: u64,
    folders: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Invites {
    open: Vec<OpenInvite>,
    /// Pending device ID -> code of the open invite it carries.
    matches: std::collections::BTreeMap<String, String>,
}

/// Creates a one-time invite (24 h) for a team computer. `folders` are
/// offered to it as soon as the central approves it. Returns the text to
/// send to the person at that computer.
#[tauri::command]
async fn create_invite(
    state: State<'_, Arc<AppState>>,
    folders: Vec<String>,
) -> Result<String, String> {
    let note = format!("{} pasta(s) oferecida(s)", folders.len());
    let dir = state.dir()?;
    with_client(state.inner(), move |c| {
        let my_id = c.my_id().map_err(|e| e.to_string())?;
        let known: Vec<String> = c
            .folders()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|f| f.id)
            .collect();
        if let Some(f) = folders.iter().find(|f| !known.contains(f)) {
            return Err(format!("pasta desconhecida: {f}"));
        }
        // The central's Tailscale address helps a computer on another
        // network find it; on the same network discovery does it alone.
        let addrs: Vec<String> = tailscale::status()
            .ok()
            .and_then(|s| s.me)
            .and_then(|me| me.ips.into_iter().find(|ip| ip.contains('.')))
            .map(|ip| vec![format!("tcp://{ip}:{}", service::DEFAULT_LISTEN_PORT)])
            .unwrap_or_default();
        let now = office::now();
        let code = invite::new_code();
        let key = invite::new_key();
        let inv = invite::Invite::new(
            &my_id,
            &my_device_name(c, &my_id),
            addrs,
            code.clone(),
            key.clone(),
            now,
        );
        let mut list = office::read_invites(&dir);
        list.push(office::InviteRecord {
            code,
            key,
            created: now,
            expires: inv.exp,
            folders,
            used_by: None,
        });
        office::write_invites(&dir, &list)?;
        Ok(inv.encode())
    })
    .await
    .inspect(|_| state.audit("Convite criado", &note))
}

#[tauri::command]
async fn list_invites(state: State<'_, Arc<AppState>>) -> Result<Invites, String> {
    let dir = state.dir()?;
    with_client(state.inner(), move |c| {
        let now = office::now();
        let list = office::read_invites(&dir);
        let codes = office::open_codes(&list, now);
        let mut matches = std::collections::BTreeMap::new();
        for p in c.pending_devices().map_err(|e| e.to_string())? {
            if let (_, Some(proof)) = invite::parse_tag(&p.name) {
                if let Some(code) = invite::match_code(&codes, &p.device_id, &proof) {
                    matches.insert(p.device_id, code.to_string());
                }
            }
        }
        // Two requests claiming the same invite: mark neither as invited.
        let mut seen = std::collections::BTreeMap::<String, usize>::new();
        for code in matches.values() {
            *seen.entry(code.clone()).or_default() += 1;
        }
        matches.retain(|_, code| seen.get(code) == Some(&1));
        let open = list
            .iter()
            .filter(|r| r.used_by.is_none() && now < r.expires)
            .map(|r| OpenInvite {
                code: r.code.clone(),
                expires: r.expires,
                folders: r.folders.clone(),
            })
            .collect();
        Ok(Invites { open, matches })
    })
    .await
}

#[tauri::command]
fn cancel_invite(state: State<'_, Arc<AppState>>, code: String) -> Result<(), String> {
    let dir = state.dir()?;
    let list: Vec<office::InviteRecord> = office::read_invites(&dir)
        .into_iter()
        .filter(|r| r.code != code)
        .collect();
    office::write_invites(&dir, &list)?;
    state.audit("Convite cancelado", &code);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InvitePreview {
    hub_id: String,
    hub_name: String,
    expires: u64,
}

/// Reads an invite without doing anything, so the person can confirm
/// which central they are joining.
#[tauri::command]
fn read_invite(text: String) -> Result<InvitePreview, String> {
    let inv = invite::Invite::decode(&text, office::now()).map_err(|e| e.to_string())?;
    Ok(InvitePreview {
        hub_id: inv.hub,
        hub_name: inv.name,
        expires: inv.exp,
    })
}

/// Team computer: joins the central from an invite. Adds the central,
/// marks this computer as a team member and advertises the invite code in
/// its name until the central approves it.
#[tauri::command]
async fn join_with_invite(state: State<'_, Arc<AppState>>, text: String) -> Result<String, String> {
    let dir = state.dir()?;
    with_client(state.inner(), move |c| {
        let inv = invite::Invite::decode(&text, office::now()).map_err(|e| e.to_string())?;
        let my_id = c.my_id().map_err(|e| e.to_string())?;
        if inv.hub == my_id {
            return Err(
                "este convite foi criado neste computador; use-o no computador do time".into(),
            );
        }
        let mut ws = office::read_workspace(&dir);
        let original = match &ws.joining {
            Some(j) => j.original_name.clone(),
            None => my_device_name(c, &my_id),
        };
        c.set_device_name(&my_id, &invite::tag_name(&original, &inv.key, &my_id))
            .map_err(|e| e.to_string())?;
        let addr = inv.addrs.first().cloned().unwrap_or_default();
        if !c
            .devices()
            .map_err(|e| e.to_string())?
            .iter()
            .any(|d| d.device_id == inv.hub)
        {
            twinfolders_core::peer::add_peer(
                c,
                &inv.hub,
                &inv.name,
                &addr,
                service::DEFAULT_LISTEN_PORT,
            )
            .map_err(|e| e.to_string())?;
        }
        ws.role = Role::Member;
        ws.hub_id = Some(inv.hub.clone());
        ws.hub_name = Some(inv.name.clone());
        ws.joining = Some(office::Joining {
            code: inv.code,
            original_name: original,
        });
        office::write_workspace(&dir, &ws)?;
        Ok(inv.name)
    })
    .await
    .inspect(|r| state.audit("Entrou no time", &format!("central {r}")))
}

/// Once the central approved this team computer (it is connected), put
/// its real name back and forget the invite.
fn finish_join(st: &AppState, c: &SyncthingClient, ov: &Overview) {
    let Ok(dir) = st.dir() else { return };
    let mut ws = office::read_workspace(&dir);
    let (Some(j), Some(hub)) = (ws.joining.clone(), ws.hub_id.clone()) else {
        return;
    };
    if ov.devices.iter().any(|d| d.id == hub && d.connected)
        && c.set_device_name(&ov.my_id, &j.original_name).is_ok()
    {
        ws.joining = None;
        let _ = office::write_workspace(&dir, &ws);
    }
}

/// Accepts a folder offered by a paired device, stored at `path` here.
#[tauri::command]
async fn accept_folder(
    state: State<'_, Arc<AppState>>,
    folder_id: String,
    offered_by: String,
    label: String,
    path: String,
) -> Result<(), String> {
    let note = format!("\"{label}\" de {}", short(&offered_by));
    office::member_guard(&state.workspace(), &[&offered_by])?;
    with_client(state.inner(), move |c| {
        let local_id = c.my_id().map_err(|e| e.to_string())?;
        let offer = PendingFolder {
            folder_id,
            offered_by,
            label,
            time: String::new(),
        };
        pairing::accept_folder(c, &offer, &local_id, std::path::Path::new(&path))
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .inspect(|_| state.audit("Pasta aceita", &note))
}

/// Stops syncing a folder. Files on disk are kept.
#[tauri::command]
async fn remove_folder(state: State<'_, Arc<AppState>>, folder_id: String) -> Result<(), String> {
    let note = folder_id.clone();
    with_client(state.inner(), move |c| {
        c.remove_folder(&folder_id).map_err(|e| e.to_string())
    })
    .await
    .inspect(|_| state.audit("Parou de sincronizar", &note))
}

/// Pauses or resumes syncing one folder.
#[tauri::command]
async fn set_folder_paused(
    state: State<'_, Arc<AppState>>,
    folder_id: String,
    paused: bool,
) -> Result<(), String> {
    let note = folder_id.clone();
    with_client(state.inner(), move |c| {
        c.set_folder_paused(&folder_id, paused)
            .map_err(|e| e.to_string())
    })
    .await
    .inspect(|_| {
        state.audit(
            if paused {
                "Pasta pausada"
            } else {
                "Pasta retomada"
            },
            &note,
        )
    })
}

/// Opens a synced folder in Finder / Explorer. The path comes from
/// Syncthing's config (never from the UI) and must be an existing directory.
#[tauri::command]
async fn reveal_folder(state: State<'_, Arc<AppState>>, folder_id: String) -> Result<(), String> {
    with_client(state.inner(), move |c| {
        let root = folder_root(c, &folder_id)?;
        // Absolute + canonical: a path can never start with '-' and be read
        // as an option by the opener.
        if !root.is_absolute() {
            return Err(format!(
                "caminho da pasta não é absoluto: {}",
                root.display()
            ));
        }
        let root = std::fs::canonicalize(&root)
            .ok()
            .filter(|p| p.is_dir())
            .ok_or_else(|| format!("a pasta não existe neste computador: {}", root.display()))?;
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        };
        // On macOS `open` LAUNCHES a folder named like a bundle (`X.app`);
        // for any folder with an extension, only reveal it in Finder.
        let reveal_only = cfg!(target_os = "macos") && root.extension().is_some();
        // The path is a single argument (no shell), so spaces and quotes
        // in folder names are passed through untouched.
        let mut child = std::process::Command::new(opener)
            .args(reveal_only.then_some("-R"))
            .arg(&root)
            .spawn()
            .map_err(|e| format!("não foi possível abrir a pasta: {e}"))?;
        // Reap it so no zombie is left (explorer.exe exits 1 even on success,
        // so the status is ignored).
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })
    .await
}

#[tauri::command]
async fn get_ignores(
    state: State<'_, Arc<AppState>>,
    folder_id: String,
) -> Result<Ignores, String> {
    with_client(state.inner(), move |c| {
        c.ignores(&folder_id).map_err(|e| e.to_string())
    })
    .await
}

/// Saves `.stignore` text; Syncthing validates it (see `error` in result).
#[tauri::command]
async fn set_ignores(
    state: State<'_, Arc<AppState>>,
    folder_id: String,
    text: String,
) -> Result<Ignores, String> {
    with_client(state.inner(), move |c| {
        let lines: Vec<&str> = text.lines().collect();
        c.set_ignores(&folder_id, &lines).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn list_conflicts(state: State<'_, Arc<AppState>>) -> Result<Vec<FolderConflicts>, String> {
    with_client(state.inner(), scan_all_conflicts).await
}

/// Resolves one conflict copy. `tag` is the local time `YYYYMMDD-HHMMSS`
/// (from the UI clock) used to name the archived copy like Syncthing does.
#[tauri::command]
async fn resolve_conflict(
    state: State<'_, Arc<AppState>>,
    folder_id: String,
    conflict_path: String,
    resolution: Resolution,
    tag: String,
) -> Result<Outcome, String> {
    let note = format!("{folder_id}: {conflict_path}");
    with_client(state.inner(), move |c| {
        let root = folder_root(c, &folder_id)?;
        let out =
            resolve::resolve(&root, &conflict_path, resolution, &tag).map_err(|e| e.to_string())?;
        // Pick the change up now instead of waiting for the watcher.
        let _ = c.scan(&folder_id);
        Ok(out)
    })
    .await
    .inspect(|_| state.audit("Conflito resolvido", &note))
}

#[tauri::command]
async fn get_versioning(
    state: State<'_, Arc<AppState>>,
    folder_id: String,
) -> Result<VersioningView, String> {
    with_client(state.inner(), move |c| {
        let v = c.folder(&folder_id).map_err(|e| e.to_string())?.versioning;
        Ok(VersioningView {
            choice: Choice::from_versioning(&v),
            kind: v.kind,
        })
    })
    .await
}

#[tauri::command]
async fn set_versioning(
    state: State<'_, Arc<AppState>>,
    folder_id: String,
    choice: Choice,
) -> Result<(), String> {
    let note = folder_id.clone();
    with_client(state.inner(), move |c| {
        let cur = c.folder(&folder_id).map_err(|e| e.to_string())?.versioning;
        // A type configured outside the app (e.g. external) is shown
        // read-only; never overwrite it from here.
        if Choice::from_versioning(&cur).is_none() {
            return Err(format!(
                "esta pasta usa um versionamento personalizado ({}); altere-o no painel do Syncthing",
                cur.kind
            ));
        }
        c.set_folder_versioning(&folder_id, &choice.apply(&cur))
            .map_err(|e| e.to_string())
    })
    .await
    .inspect(|_| state.audit("Versionamento alterado", &note))
}

/// Periodically looks for new conflict copies; notifies once per conflict
/// and tells the UI to refresh its list.
fn watch_conflicts(app: AppHandle, st: Arc<AppState>) {
    std::thread::spawn(move || loop {
        if let Ok(c) = cached_client(&st) {
            if let Ok(all) = scan_all_conflicts(&c) {
                let key = |f: &FolderConflicts, x: &ConflictEntry| {
                    format!("{}/{}", f.folder_id, x.conflict_path)
                };
                let scanned: BTreeSet<&str> = all.iter().map(|f| f.folder_id.as_str()).collect();
                let current: BTreeSet<String> = all
                    .iter()
                    .flat_map(|f| f.conflicts.iter().map(move |x| key(f, x)))
                    .collect();
                let mut notified = lock(&st.notified);
                let fresh: Vec<(&FolderConflicts, &ConflictEntry)> = all
                    .iter()
                    .flat_map(|f| f.conflicts.iter().map(move |x| (f, x)))
                    .filter(|(f, x)| !notified.contains(&key(f, x)))
                    .collect();
                let changed = notified
                    .iter()
                    .filter(|k| scanned.iter().any(|id| k.starts_with(&format!("{id}/"))))
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    != current;
                if let Some((f, x)) = fresh.first() {
                    let body = if fresh.len() == 1 {
                        format!(
                            "Possível conflito em \"{}\" ({}). Abra o TwinFolders para escolher qual versão manter.",
                            clean_label(&x.original_path),
                            clean_label(&f.label)
                        )
                    } else {
                        format!(
                            "{} novos possíveis conflitos: escolha qual versão manter.",
                            fresh.len()
                        )
                    };
                    let _ = app
                        .notification()
                        .builder()
                        .title("Possível conflito de sincronização")
                        .body(body)
                        .show();
                }
                // Keep entries of folders that could not be scanned this
                // round, so their conflicts do not notify again later.
                notified.retain(|k| !scanned.iter().any(|id| k.starts_with(&format!("{id}/"))));
                notified.extend(current.iter().cloned());
                let count = current.len();
                drop(notified);
                if changed || !fresh.is_empty() {
                    let _ = app.emit("conflicts-changed", count);
                }
            }
        }
        std::thread::sleep(CONFLICT_POLL);
    });
}

/// Long-polls Syncthing events and tells the UI to refresh. Survives daemon
/// restarts: on error it reconnects and restarts from event 0. A restart that
/// does not break the connection would leave `since` ahead of the new IDs
/// until they catch up; the UI's 3 s poll covers that gap.
fn short(id: &str) -> String {
    id.chars().take(7).collect()
}

/* ---------- activity, action log and security checks ---------- */

/// Newest first, at most `limit`.
#[tauri::command]
fn get_activity(
    state: State<'_, Arc<AppState>>,
    limit: usize,
) -> Result<Vec<activity::Entry>, String> {
    let dir = state.dir()?;
    let mut all = activity::read(&dir.join("activity.jsonl"));
    all.reverse();
    all.truncate(limit.clamp(1, activity::KEEP));
    Ok(all)
}

#[tauri::command]
fn get_audit(state: State<'_, Arc<AppState>>) -> Result<Vec<audit::AuditEntry>, String> {
    let mut all = audit::read(&state.dir()?);
    all.reverse();
    Ok(all)
}

#[tauri::command]
async fn get_security() -> Result<Vec<security::Check>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let gui = home()
            .ok()
            .and_then(|(h, _)| std::fs::read_to_string(h.join("config.xml")).ok())
            .and_then(|xml| stconfig::read_gui(&xml).ok());
        security::build(&security::Inputs {
            os: std::env::consts::OS,
            disk_encrypted: security::disk_encrypted(),
            gui_loopback: gui.as_ref().map(|g| security::is_loopback(&g.address)),
            gui_protected: gui
                .as_ref()
                .map(|g| g.tls && g.has_password && !g.user.is_empty()),
            tailscale_running: match tailscale::status() {
                Ok(s) => Some(s.running()),
                Err(e) if e.contains("não está conectado") => Some(false),
                Err(_) => None,
            },
        })
    })
    .await
    .map_err(|e| e.to_string())
}

/// Keeps the activity log: Syncthing's disk events (changes in synced
/// folders, here and on the other computers) appended to activity.jsonl.
/// On (re)connect it replays Syncthing's buffer; duplicates are skipped.
fn watch_activity(app: AppHandle, st: Arc<AppState>) {
    std::thread::spawn(move || {
        let Ok(dir) = st.dir() else { return };
        let path = dir.join("activity.jsonl");
        let mut seen: std::collections::HashSet<String> = activity::read(&path)
            .iter()
            .map(activity::Entry::key)
            .collect();
        loop {
            let Ok(client) = connect() else {
                std::thread::sleep(Duration::from_secs(5));
                continue;
            };
            let mut since = 0;
            loop {
                match client.events(since, 50, activity::EVENT_TYPES) {
                    Ok(evs) => {
                        if let Some(last) = evs.last() {
                            since = last.id;
                        }
                        let entries: Vec<activity::Entry> =
                            evs.iter().filter_map(activity::from_event).collect();
                        if matches!(activity::append(&path, &entries, &mut seen), Ok(n) if n > 0) {
                            let _ = app.emit("activity-changed", ());
                        }
                    }
                    Err(_) => {
                        std::thread::sleep(Duration::from_secs(3));
                        break;
                    }
                }
            }
        }
    });
}

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
        .manage(state.clone())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_overview,
            get_profile,
            get_setup,
            install_syncthing,
            install_service,
            get_remote_access,
            set_ssh_user,
            open_terminal,
            approve_device,
            add_device,
            scan_folder,
            share_folder,
            set_folder_peers,
            remove_device,
            get_workspace,
            set_role,
            create_invite,
            list_invites,
            cancel_invite,
            read_invite,
            join_with_invite,
            get_activity,
            get_audit,
            get_security,
            accept_folder,
            remove_folder,
            set_folder_paused,
            reveal_folder,
            get_ignores,
            set_ignores,
            list_conflicts,
            resolve_conflict,
            get_versioning,
            set_versioning
        ])
        .setup(move |app| {
            *lock(&state.config_dir) = app.path().app_config_dir().ok();
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
            watch_conflicts(app.handle().clone(), state.clone());
            watch_activity(app.handle().clone(), state.clone());
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
