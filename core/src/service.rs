//! Running Syncthing in the background, started at login, without the app.
//!
//! - macOS: per-user LaunchAgent `dev.twinfolders.syncthing`
//!   (`~/Library/LaunchAgents`), restarted by launchd on crash.
//! - Windows: Scheduled Task `\TwinFolders\Syncthing` at logon of the
//!   current user (least privilege, no time limit, restart on failure).
//!
//! Generators (`launchd_plist`, `windows_task_xml`, `serve_args`) are pure
//! and tested on every OS; `install`/`uninstall`/`status` call the OS tools.
//! The API key is never placed in the plist/XML/argv (see `secrets`).

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::client::{self, SyncthingClient};
use crate::{guitls, secrets, stconfig};

pub const LAUNCHD_LABEL: &str = "dev.twinfolders.syncthing";
pub const WINDOWS_TASK_PATH: &str = "\\TwinFolders\\";
pub const WINDOWS_TASK_NAME: &str = "Syncthing";
pub const DEFAULT_GUI_PORT: u16 = 8385;
pub const DEFAULT_LISTEN_PORT: u16 = 22001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
}

impl Os {
    pub fn current() -> Option<Os> {
        if cfg!(target_os = "macos") {
            Some(Os::MacOs)
        } else if cfg!(windows) {
            Some(Os::Windows)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub syncthing_bin: PathBuf,
    /// Dedicated Syncthing home (config, keys, database).
    pub home: PathBuf,
    pub log_dir: PathBuf,
    pub gui_port: u16,
    pub listen_port: u16,
}

impl ServiceConfig {
    /// Standard per-user locations, separate from any existing Syncthing.
    pub fn for_current_user(syncthing_bin: PathBuf) -> io::Result<Self> {
        let (home, log_dir) = match Os::current() {
            Some(Os::MacOs) => {
                let h = PathBuf::from(env("HOME")?);
                (
                    h.join("Library/Application Support/TwinFolders/syncthing"),
                    h.join("Library/Logs/TwinFolders"),
                )
            }
            Some(Os::Windows) => {
                let base = PathBuf::from(env("LOCALAPPDATA")?).join("TwinFolders");
                (base.join("syncthing"), base.join("logs"))
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "only macOS and Windows",
                ))
            }
        };
        Ok(Self {
            syncthing_bin,
            home,
            log_dir,
            gui_port: DEFAULT_GUI_PORT,
            listen_port: DEFAULT_LISTEN_PORT,
        })
    }

    /// HTTPS with the pinned certificate from [`crate::guitls`].
    pub fn gui_url(&self) -> String {
        format!("https://127.0.0.1:{}", self.gui_port)
    }

    /// Arguments for `syncthing` (after the binary). No secrets.
    pub fn serve_args(&self, os: Os) -> Vec<String> {
        let mut a = vec![
            "serve".to_string(),
            format!("--home={}", self.home.display()),
            format!("--gui-address={}", self.gui_url()),
            "--no-browser".into(),
            "--no-restart".into(),
            "--no-upgrade".into(),
            "--no-port-probing".into(),
            format!(
                "--log-file={}",
                self.log_dir.join("syncthing.log").display()
            ),
            "--log-max-size=10485760".into(),
            "--log-max-old-files=3".into(),
        ];
        if os == Os::Windows {
            a.push("--no-console".into());
        }
        a
    }
}

fn env(name: &str) -> io::Result<String> {
    std::env::var(name)
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, format!("${name} not set")))
}

pub fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c => o.push(c),
        }
    }
    o
}

/// LaunchAgent plist. KeepAlive restarts only on unsuccessful exit, so a
/// clean `POST /rest/system/shutdown` really stops it; restart requests
/// (exit code 3 with `--no-restart`) and crashes are relaunched.
pub fn launchd_plist(cfg: &ServiceConfig) -> String {
    let mut args = format!(
        "        <string>{}</string>\n",
        xml_escape(&cfg.syncthing_bin.to_string_lossy())
    );
    for a in cfg.serve_args(Os::MacOs) {
        args.push_str(&format!("        <string>{}</string>\n", xml_escape(&a)));
    }
    let out = xml_escape(&cfg.log_dir.join("launchd.log").to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCHD_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
{args}    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>ProcessType</key>
    <string>Background</string>
    <key>LowPriorityIO</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{out}</string>
    <key>StandardErrorPath</key>
    <string>{out}</string>
</dict>
</plist>
"#
    )
}

/// Quotes one argument for a Windows command line (CommandLineToArgvW rules).
pub fn windows_quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            c => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// Task Scheduler XML (schema 1.2). `user_id` is `DOMAIN\user`.
/// No XML declaration: Register-ScheduledTask rejects a UTF-16 declaration
/// on a string read as UTF-8.
pub fn windows_task_xml(cfg: &ServiceConfig, user_id: &str) -> String {
    let args: Vec<String> = cfg
        .serve_args(Os::Windows)
        .iter()
        .map(|a| windows_quote_arg(a))
        .collect();
    let user = xml_escape(user_id);
    format!(
        r#"<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>TwinFolders: Syncthing em segundo plano (sincronizacao de pastas)</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>999</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{cmd}</Command>
      <Arguments>{args}</Arguments>
      <WorkingDirectory>{wd}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#,
        cmd = xml_escape(&cfg.syncthing_bin.to_string_lossy()),
        args = xml_escape(&args.join(" ")),
        wd = xml_escape(&cfg.home.to_string_lossy()),
    )
}

// ---------- errors ----------

#[derive(Debug)]
pub enum ServiceError {
    Io(io::Error),
    Config(stconfig::ConfigError),
    Client(client::Error),
    Secrets(String),
    Command {
        cmd: String,
        code: Option<i32>,
        stderr: String,
    },
    Unsupported,
    NotReady(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::Io(e) => write!(f, "{e}"),
            ServiceError::Config(e) => write!(f, "{e}"),
            ServiceError::Client(e) => write!(f, "{e}"),
            ServiceError::Secrets(e) => write!(f, "{e}"),
            ServiceError::Command { cmd, code, stderr } => {
                write!(f, "`{cmd}` failed (exit {code:?}): {}", stderr.trim())
            }
            ServiceError::Unsupported => write!(f, "only macOS and Windows are supported"),
            ServiceError::NotReady(e) => write!(f, "Syncthing did not become ready: {e}"),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<io::Error> for ServiceError {
    fn from(e: io::Error) -> Self {
        ServiceError::Io(e)
    }
}
impl From<stconfig::ConfigError> for ServiceError {
    fn from(e: stconfig::ConfigError) -> Self {
        ServiceError::Config(e)
    }
}
impl From<client::Error> for ServiceError {
    fn from(e: client::Error) -> Self {
        ServiceError::Client(e)
    }
}

pub type Result<T> = std::result::Result<T, ServiceError>;

/// Runs a command; `Err` on non-zero exit. Arguments never contain secrets.
fn run(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd).args(args).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(ServiceError::Command {
            cmd: format!("{cmd} {}", args.join(" ")),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

// ---------- provisioning (shared) ----------

/// Web GUI user created by TwinFolders.
pub const GUI_USER: &str = "twinfolders";

/// Result of [`provision_home`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provisioned {
    pub gui: stconfig::GuiConfig,
    /// Set when a new random GUI password was created (caller stores it in
    /// the OS credential store). Never logged.
    pub new_gui_password: Option<String>,
}

/// Runs `syncthing generate` for `cfg.home`, setting GUI user/password.
/// The password goes through stdin (`--gui-password=-`), never argv. On an
/// existing home Syncthing keeps the device keys and only updates the GUI
/// credentials (verified with v2.1.7).
fn generate_with_gui_auth(cfg: &ServiceConfig, password: &str) -> Result<()> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(&cfg.syncthing_bin)
        .arg("generate")
        .arg(format!("--home={}", cfg.home.display()))
        .arg("--no-port-probing")
        .arg(format!("--gui-user={GUI_USER}"))
        .arg("--gui-password=-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(password.as_bytes())?;
    let out = child.wait_with_output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(ServiceError::Command {
            cmd: "syncthing generate --gui-user=twinfolders --gui-password=-".into(),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// Creates the Syncthing home on first run, protects the web GUI with a
/// random password if it has none, and applies [`stconfig::harden`].
/// Must run while Syncthing is NOT running.
pub fn provision_home(cfg: &ServiceConfig) -> Result<Provisioned> {
    std::fs::create_dir_all(&cfg.log_dir)?;
    std::fs::create_dir_all(&cfg.home)?;
    let config = cfg.home.join("config.xml");
    let needs_auth = match std::fs::read_to_string(&config) {
        Ok(xml) => !stconfig::read_gui(&xml)?.has_gui_auth(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => true,
        Err(e) => return Err(e.into()),
    };
    guitls::ensure(&cfg.home)?;
    let new_gui_password = if needs_auth {
        let pw = secrets::random_password();
        generate_with_gui_auth(cfg, &pw)?;
        Some(pw)
    } else {
        None
    };
    let xml = std::fs::read_to_string(&config)?;
    let hardened = stconfig::harden(&xml, cfg.gui_port, cfg.listen_port)?;
    if hardened != xml {
        // fs::write on an existing file keeps its 0600 mode.
        std::fs::write(&config, &hardened)?;
    }
    let gui = stconfig::read_gui(&hardened)?;
    if !gui.has_gui_auth() {
        return Err(ServiceError::NotReady(
            "web GUI has no user/password after provisioning".into(),
        ));
    }
    Ok(Provisioned {
        gui,
        new_gui_password,
    })
}

/// True only if every process listening on the GUI port is our Syncthing
/// binary run by this user. The API key is sent only after this check, so
/// a program that grabbed the port first never receives it.
pub fn listener_is_ours(cfg: &ServiceConfig) -> bool {
    match Os::current() {
        Some(Os::MacOs) => {
            let port = format!("-iTCP:{}", cfg.gui_port);
            let Ok(out) = run("lsof", &["-nP", &port, "-sTCP:LISTEN", "-t"]) else {
                return false;
            };
            let Ok(me) = uid() else { return false };
            let pids: Vec<&str> = out.split_whitespace().collect();
            !pids.is_empty()
                && pids.iter().all(|pid| {
                    run("ps", &["-o", "uid=,comm=", "-p", pid])
                        .map(|l| {
                            let l = l.trim();
                            let (u, comm) = l.split_once(char::is_whitespace).unwrap_or(("", ""));
                            u == me && same_bin(Path::new(comm.trim()), &cfg.syncthing_bin)
                        })
                        .unwrap_or(false)
                })
        }
        Some(Os::Windows) => {
            let script = format!(
                // Win32_Process.GetOwner works without elevation (unlike
                // Get-Process -IncludeUserName). One line per PID: path|DOMAIN\user
                "$c = Get-NetTCPConnection -LocalPort {} -State Listen -ErrorAction SilentlyContinue; if (-not $c) {{ exit 1 }}; $c.OwningProcess | Sort-Object -Unique | ForEach-Object {{ $p = Get-CimInstance Win32_Process -Filter \"ProcessId=$_\"; $o = Invoke-CimMethod -InputObject $p -MethodName GetOwner; \"$($p.ExecutablePath)|$($o.Domain)\\$($o.User)\" }}",
                cfg.gui_port
            );
            let Ok(out) = powershell(&script) else {
                return false;
            };
            let Ok(me) = windows_user() else {
                return false;
            };
            let rows: Vec<&str> = out
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            !rows.is_empty()
                && rows.iter().all(|r| match r.rsplit_once('|') {
                    Some((path, owner)) => {
                        owner.eq_ignore_ascii_case(&me)
                            && same_bin(Path::new(path), &cfg.syncthing_bin)
                    }
                    None => false,
                })
        }
        None => false,
    }
}

/// Canonical path of a binary; bare names (e.g. `syncthing`) are looked up
/// in PATH. Symlinks (Homebrew) are resolved so both sides compare equal.
fn canonical_bin(p: &Path) -> Option<PathBuf> {
    if p.components().count() == 1 {
        let exe = if cfg!(windows) && p.extension().is_none() {
            p.with_extension("exe")
        } else {
            p.to_path_buf()
        };
        let found = std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|d| d.join(&exe))
            .find(|c| c.is_file())?;
        return std::fs::canonicalize(found).ok();
    }
    std::fs::canonicalize(p).ok()
}

fn same_bin(a: &Path, b: &Path) -> bool {
    match (canonical_bin(a), canonical_bin(b)) {
        (Some(x), Some(y)) if cfg!(windows) => {
            x.to_string_lossy().to_lowercase() == y.to_string_lossy().to_lowercase()
        }
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Client that trusts only the pinned GUI CA in the Syncthing home.
pub fn client_for(cfg: &ServiceConfig, key: &str) -> Result<SyncthingClient> {
    let ca = guitls::load_ca_pem(&cfg.home)?;
    Ok(SyncthingClient::with_pinned_ca(&cfg.gui_url(), key, &ca)?)
}

/// Waits until the REST API answers with the right key.
pub fn wait_ready(cfg: &ServiceConfig, key: &str, timeout: Duration) -> Result<String> {
    let c = client_for(cfg, key)?;
    let start = Instant::now();
    let mut last = String::from("no listener on the GUI port yet");
    while start.elapsed() < timeout {
        if !listener_is_ours(cfg) {
            std::thread::sleep(Duration::from_millis(300));
            continue;
        }
        match c.my_id() {
            Ok(id) => return Ok(id),
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err(ServiceError::NotReady(last))
}

/// API key of the service instance, read from its owner-only `config.xml`
/// (the source of truth). Not from the OS credential store: on macOS the
/// Keychain item is tied to the binary that created it, so a rebuilt CLI or
/// the app would block on a permission prompt.
pub fn api_key_from_config(cfg: &ServiceConfig) -> Option<String> {
    let xml = std::fs::read_to_string(cfg.home.join("config.xml")).ok()?;
    let key = crate::stconfig::read_gui(&xml).ok()?.api_key;
    (!key.is_empty()).then_some(key)
}

/// Asks a running instance to exit cleanly (exit 0: not restarted). Skipped
/// unless the listener is verified to be ours; `platform_stop` follows.
fn request_shutdown(cfg: &ServiceConfig) {
    if !listener_is_ours(cfg) {
        return;
    }
    if let Some(key) = api_key_from_config(cfg) {
        if let Ok(c) = client_for(cfg, &key) {
            let _ = c.shutdown();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub installed: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub device_id: Option<String>,
    pub gui_url: String,
}

/// Installs (or reinstalls) and starts the background service, then waits
/// for the REST API. Returns the local device ID.
pub fn install(cfg: &ServiceConfig) -> Result<String> {
    let os = Os::current().ok_or(ServiceError::Unsupported)?;
    // Stop a running instance before touching its config.
    request_shutdown(cfg);
    platform_stop(os)?;
    let p = provision_home(cfg)?;
    secrets::store_api_key(&p.gui.api_key).map_err(ServiceError::Secrets)?;
    if let Some(pw) = &p.new_gui_password {
        secrets::store_gui_password(pw).map_err(ServiceError::Secrets)?;
    }
    platform_register_and_start(os, cfg)?;
    wait_ready(cfg, &p.gui.api_key, Duration::from_secs(45))
}

/// Stops and removes the service. Never deletes the Syncthing home or any
/// synced folder.
pub fn uninstall(cfg: &ServiceConfig) -> Result<()> {
    let os = Os::current().ok_or(ServiceError::Unsupported)?;
    request_shutdown(cfg);
    platform_stop(os)?;
    platform_unregister(os)?;
    secrets::delete_api_key().map_err(ServiceError::Secrets)?;
    // Keep the GUI password: the home (and its bcrypt hash) is kept too.
    Ok(())
}

pub fn status(cfg: &ServiceConfig) -> Result<ServiceStatus> {
    let os = Os::current().ok_or(ServiceError::Unsupported)?;
    let (installed, running, pid) = platform_status(os)?;
    let device_id = match api_key_from_config(cfg) {
        Some(k) if running && listener_is_ours(cfg) => client_for(cfg, &k)?.my_id().ok(),
        _ => None,
    };
    Ok(ServiceStatus {
        installed,
        running,
        pid,
        device_id,
        gui_url: cfg.gui_url(),
    })
}

// ---------- macOS ----------

pub fn launchd_plist_path() -> io::Result<PathBuf> {
    Ok(PathBuf::from(env("HOME")?)
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

fn uid() -> Result<String> {
    Ok(run("id", &["-u"])?.trim().to_string())
}

fn launchd_target() -> Result<String> {
    Ok(format!("gui/{}/{LAUNCHD_LABEL}", uid()?))
}

/// Parses `launchctl print` output: (running, pid).
pub fn parse_launchctl_print(out: &str) -> (bool, Option<u32>) {
    let mut running = false;
    let mut pid = None;
    for line in out.lines().map(str::trim) {
        if line == "state = running" {
            running = true;
        } else if let Some(p) = line.strip_prefix("pid = ") {
            pid = p.trim().parse().ok();
        }
    }
    (running, pid)
}

// ---------- Windows ----------

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn powershell(script: &str) -> Result<String> {
    run(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
}

fn windows_user() -> io::Result<String> {
    Ok(format!("{}\\{}", env("USERDOMAIN")?, env("USERNAME")?))
}

// ---------- dispatch ----------

fn platform_stop(os: Os) -> Result<()> {
    match os {
        Os::MacOs => {
            // Not loaded is fine.
            let _ = run("launchctl", &["bootout", &launchd_target()?]);
            Ok(())
        }
        Os::Windows => {
            let script = format!(
                "$t = Get-ScheduledTask -TaskPath {p} -TaskName {n} -ErrorAction SilentlyContinue; if ($t) {{ Stop-ScheduledTask -InputObject $t }}; exit 0",
                p = ps_quote(WINDOWS_TASK_PATH),
                n = ps_quote(WINDOWS_TASK_NAME)
            );
            powershell(&script).map(|_| ())
        }
    }
}

fn platform_register_and_start(os: Os, cfg: &ServiceConfig) -> Result<()> {
    match os {
        Os::MacOs => {
            let path = launchd_plist_path()?;
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, launchd_plist(cfg))?;
            let domain = format!("gui/{}", uid()?);
            run(
                "launchctl",
                &["bootstrap", &domain, &path.to_string_lossy()],
            )?;
            Ok(())
        }
        Os::Windows => {
            let xml_path = cfg.log_dir.join("task.xml");
            std::fs::write(&xml_path, windows_task_xml(cfg, &windows_user()?))?;
            let script = format!(
                "$x = Get-Content -Raw -LiteralPath {f}; Register-ScheduledTask -TaskPath {p} -TaskName {n} -Xml $x -Force | Out-Null; Start-ScheduledTask -TaskPath {p} -TaskName {n}",
                f = ps_quote(&xml_path.to_string_lossy()),
                p = ps_quote(WINDOWS_TASK_PATH),
                n = ps_quote(WINDOWS_TASK_NAME)
            );
            powershell(&script).map(|_| ())
        }
    }
}

fn platform_unregister(os: Os) -> Result<()> {
    match os {
        Os::MacOs => {
            let path = launchd_plist_path()?;
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            }
        }
        Os::Windows => {
            let script = format!(
                "Unregister-ScheduledTask -TaskPath {p} -TaskName {n} -Confirm:$false -ErrorAction SilentlyContinue; exit 0",
                p = ps_quote(WINDOWS_TASK_PATH),
                n = ps_quote(WINDOWS_TASK_NAME)
            );
            powershell(&script).map(|_| ())
        }
    }
}

fn platform_status(os: Os) -> Result<(bool, bool, Option<u32>)> {
    match os {
        Os::MacOs => {
            let installed = launchd_plist_path()?.exists();
            match run("launchctl", &["print", &launchd_target()?]) {
                Ok(out) => {
                    let (running, pid) = parse_launchctl_print(&out);
                    Ok((installed, running, pid))
                }
                Err(_) => Ok((installed, false, None)),
            }
        }
        Os::Windows => {
            let script = format!(
                "$t = Get-ScheduledTask -TaskPath {p} -TaskName {n} -ErrorAction SilentlyContinue; if ($t) {{ $t.State }} else {{ 'Missing' }}; exit 0",
                p = ps_quote(WINDOWS_TASK_PATH),
                n = ps_quote(WINDOWS_TASK_NAME)
            );
            let state = powershell(&script)?.trim().to_string();
            Ok((state != "Missing", state == "Running", None))
        }
    }
}

/// True if `p` looks like a usable Syncthing binary path.
pub fn is_executable(p: &Path) -> bool {
    p.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_mac() -> ServiceConfig {
        ServiceConfig {
            syncthing_bin: "/opt/homebrew/bin/syncthing".into(),
            home: "/Users/a b/Library/Application Support/TwinFolders/syncthing".into(),
            log_dir: "/Users/a b/Library/Logs/TwinFolders".into(),
            gui_port: 8385,
            listen_port: 22001,
        }
    }

    fn cfg_win() -> ServiceConfig {
        ServiceConfig {
            syncthing_bin: r"C:\Program Files\Syncthing\syncthing.exe".into(),
            home: r"C:\Users\Zé & Co\AppData\Local\TwinFolders\syncthing".into(),
            log_dir: r"C:\Users\Zé & Co\AppData\Local\TwinFolders\logs".into(),
            gui_port: 8385,
            listen_port: 22001,
        }
    }

    #[test]
    fn serve_args_are_loopback_and_secret_free() {
        let a = cfg_mac().serve_args(Os::MacOs);
        assert_eq!(a[0], "serve");
        assert!(a.contains(&"--gui-address=https://127.0.0.1:8385".to_string()));
        for flag in [
            "--no-browser",
            "--no-restart",
            "--no-upgrade",
            "--no-port-probing",
        ] {
            assert!(a.contains(&flag.to_string()), "{flag}");
        }
        assert!(
            !a.iter().any(|x| x.contains("apikey")),
            "no API key on argv"
        );
        assert!(!a.contains(&"--no-console".to_string()));
        assert!(cfg_win()
            .serve_args(Os::Windows)
            .contains(&"--no-console".to_string()));
    }

    #[test]
    fn plist_has_label_args_and_keepalive_on_failure_only() {
        let p = launchd_plist(&cfg_mac());
        assert!(p.contains("<string>dev.twinfolders.syncthing</string>"));
        assert!(p.contains("<string>/opt/homebrew/bin/syncthing</string>"));
        assert!(p.contains("--home=/Users/a b/Library/Application Support/TwinFolders/syncthing"));
        assert!(p.contains("<key>SuccessfulExit</key>\n        <false/>"));
        assert!(p.contains("<key>RunAtLoad</key>\n    <true/>"));
        roxmltree::Document::parse_with_options(
            &p,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .expect("well-formed XML");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn plist_passes_plutil_lint() {
        let f = std::env::temp_dir().join(format!("twinfolders-lint-{}.plist", std::process::id()));
        std::fs::write(&f, launchd_plist(&cfg_mac())).unwrap();
        let out = Command::new("plutil")
            .arg("-lint")
            .arg(&f)
            .output()
            .unwrap();
        std::fs::remove_file(&f).unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }

    #[test]
    fn task_xml_is_escaped_and_well_formed() {
        let x = windows_task_xml(&cfg_win(), r"PC\Zé");
        let doc = roxmltree::Document::parse(&x).expect("well-formed XML");
        let ns = "http://schemas.microsoft.com/windows/2004/02/mit/task";
        let text = |tag: &str| {
            doc.descendants()
                .find(|n| n.has_tag_name((ns, tag)))
                .and_then(|n| n.text())
                .unwrap_or("")
                .to_string()
        };
        assert_eq!(text("Command"), r"C:\Program Files\Syncthing\syncthing.exe");
        assert_eq!(text("ExecutionTimeLimit"), "PT0S");
        assert_eq!(text("RunLevel"), "LeastPrivilege");
        assert_eq!(text("DisallowStartIfOnBatteries"), "false");
        let args = text("Arguments");
        assert!(
            args.contains(r#""--home=C:\Users\Zé & Co\AppData\Local\TwinFolders\syncthing""#),
            "{args}"
        );
        assert!(args.contains("--no-console"));
        assert!(!x.contains("<?xml"), "no XML declaration");
    }

    #[test]
    fn windows_arg_quoting_follows_argv_rules() {
        assert_eq!(windows_quote_arg("plain"), "plain");
        assert_eq!(windows_quote_arg("a b"), r#""a b""#);
        assert_eq!(windows_quote_arg(""), r#""""#);
        assert_eq!(windows_quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
        // trailing backslashes before the closing quote are doubled
        assert_eq!(
            windows_quote_arg(r"C:\dir with space\"),
            r#""C:\dir with space\\""#
        );
        // backslashes not followed by a quote are literal
        assert_eq!(windows_quote_arg(r"C:\a b\c"), r#""C:\a b\c""#);
    }

    #[test]
    fn launchctl_print_parsing() {
        let out = "gui/501/dev.twinfolders.syncthing = {\n\tactive count = 1\n\tstate = running\n\tpid = 4242\n}";
        assert_eq!(parse_launchctl_print(out), (true, Some(4242)));
        assert_eq!(
            parse_launchctl_print("state = not running\n"),
            (false, None)
        );
    }

    /// A missing task (first install) must not fail the best-effort
    /// PowerShell steps: `-ErrorAction SilentlyContinue` still leaves `$?`
    /// false, so each tolerant script ends with `exit 0` (seen in CI).
    #[test]
    fn tolerant_powershell_scripts_exit_zero() {
        let src = include_str!("service.rs");
        for marker in [
            "Stop-ScheduledTask -InputObject $t }}; exit 0",
            "SilentlyContinue; exit 0\",",
            "'Missing' }}; exit 0",
        ] {
            assert!(src.contains(marker), "missing: {marker}");
        }
    }

    #[test]
    fn powershell_single_quote_escaping() {
        assert_eq!(ps_quote(r"C:\it's"), r"'C:\it''s'");
    }
}
