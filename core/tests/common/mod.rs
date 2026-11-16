//! Shared harness: isolated Syncthing daemons on localhost.
#![allow(dead_code)]

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use twinfolders_core::client::SyncthingClient;

/// Fresh, unique scratch directory for one test.
pub fn temp_root(test: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("twinfolders-it-{test}-{nanos}"));
    fs::create_dir_all(&root).unwrap();
    root
}

pub fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

pub fn syncthing_bin() -> Option<String> {
    if let Ok(p) = std::env::var("TWINFOLDERS_SYNCTHING") {
        return Some(p);
    }
    let ok = Command::new("syncthing")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    ok.then(|| "syncthing".to_string())
}

/// Returns None (and the test should return early) when Syncthing is absent.
pub fn require_syncthing() -> Option<String> {
    match syncthing_bin() {
        Some(b) => Some(b),
        None if std::env::var_os("CI").is_some() => panic!("syncthing binary required in CI"),
        None => {
            eprintln!("SKIPPED: syncthing not found on PATH");
            None
        }
    }
}

pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

pub fn wait_until(timeout: Duration, what: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("timed out after {timeout:?} waiting for: {what}");
}

pub struct Node {
    pub name: &'static str,
    pub root: PathBuf,
    pub listen_port: u16,
    pub client: SyncthingClient,
    pub id: String,
    pub child: Child,
}

impl Node {
    pub fn start(bin: &str, name: &'static str, root: &Path) -> Node {
        let home = root.join(format!("{name}-home"));
        fs::create_dir_all(&home).unwrap();
        let gui_port = free_port();
        let listen_port = free_port();
        let api_key = format!("key-{name}-{gui_port}");
        let child = Command::new(bin)
            .args([
                "serve",
                "--no-browser",
                "--no-restart",
                "--no-upgrade",
                "--log-level=WARN",
            ])
            .arg(format!("--home={}", home.display()))
            .arg(format!("--gui-address=http://127.0.0.1:{gui_port}"))
            .arg(format!("--gui-apikey={api_key}"))
            .stdout(Stdio::null())
            .stderr(fs::File::create(root.join(format!("{name}.log"))).unwrap())
            .spawn()
            .expect("spawn syncthing");
        let client =
            SyncthingClient::new(&format!("http://127.0.0.1:{gui_port}"), &api_key).unwrap();
        wait_until(Duration::from_secs(30), "daemon REST API up", || {
            client.ping().is_ok()
        });
        // Isolate: no internet discovery/relays, fixed loopback listen port.
        client
            .patch_options(&json!({
                "listenAddresses": [format!("tcp://127.0.0.1:{listen_port}")],
                "globalAnnounceEnabled": false,
                "localAnnounceEnabled": false,
                "relaysEnabled": false,
                "natEnabled": false,
                "crashReportingEnabled": false,
                "urAccepted": -1
            }))
            .unwrap();
        let id = client.my_id().unwrap();
        Node {
            name,
            root: root.to_path_buf(),
            listen_port,
            client,
            id,
            child,
        }
    }

    pub fn folder_path(&self) -> PathBuf {
        self.root.join(format!("{}-folder", self.name))
    }

    pub fn connected_to(&self, other: &Node) -> bool {
        self.client
            .connections()
            .map(|c| c.get(&other.id).map(|x| x.connected).unwrap_or(false))
            .unwrap_or(false)
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
