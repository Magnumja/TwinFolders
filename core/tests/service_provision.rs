//! Slice 3: provisioning a fresh Syncthing home with real `syncthing
//! generate`, hardening config.xml, and checking the running daemon really
//! applies it (no launchd/Task Scheduler involved).

mod common;

use std::process::{Command, Stdio};
use std::time::Duration;

use common::{require_syncthing, temp_root, wait_until};
use twinfolders_core::client::SyncthingClient;
use twinfolders_core::service::{self, Os, ServiceConfig};
use twinfolders_core::stconfig;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// GET a REST path; key goes in a header, never on a command line.
fn get_json(base: &str, key: &str, path: &str) -> serde_json::Value {
    ureq::get(format!("{base}{path}"))
        .header("X-API-Key", key)
        .call()
        .unwrap()
        .body_mut()
        .read_json()
        .unwrap()
}

#[test]
fn provisioned_home_is_hardened_and_daemon_applies_it() {
    let Some(bin) = require_syncthing() else {
        return;
    };
    let root = temp_root("provision");
    let cfg = ServiceConfig {
        syncthing_bin: bin.clone().into(),
        home: root.join("home with space"),
        log_dir: root.join("logs"),
        gui_port: free_port(),
        listen_port: free_port(),
    };

    let gui = service::provision_home(&cfg).unwrap();
    assert_eq!(gui.address, format!("127.0.0.1:{}", cfg.gui_port));
    assert!(gui.api_key.len() >= 20, "generated key");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(cfg.home.join("config.xml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "config.xml keeps 0600 after hardening");
    }

    // Idempotent: second provisioning keeps the same identity and key.
    let again = service::provision_home(&cfg).unwrap();
    assert_eq!(again.api_key, gui.api_key);

    // Start exactly like the service would (same args, no key on argv).
    let args = cfg.serve_args(Os::current().unwrap_or(Os::MacOs));
    assert!(!args.iter().any(|a| a.contains(&gui.api_key)));
    let mut child = Command::new(&bin)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let c = SyncthingClient::new(&cfg.gui_url(), &gui.api_key).unwrap();
    wait_until(Duration::from_secs(30), "daemon up", || c.ping().is_ok());

    // Check the live options through REST, not just the file.
    let opts = get_json(&cfg.gui_url(), &gui.api_key, "/rest/config/options");
    assert_eq!(opts["natEnabled"], false, "no UPnP/NAT-PMP router mapping");
    assert_eq!(opts["relaysEnabled"], false);
    assert_eq!(opts["globalAnnounceEnabled"], false);
    assert_eq!(opts["crashReportingEnabled"], false);
    assert_eq!(opts["urAccepted"], -1);
    assert_eq!(
        opts["listenAddresses"],
        serde_json::json!([format!("tcp://0.0.0.0:{}", cfg.listen_port)])
    );
    let gui_live = get_json(&cfg.gui_url(), &gui.api_key, "/rest/config/gui");
    assert!(stconfig::is_loopback_address(
        gui_live["address"].as_str().unwrap()
    ));
    eprintln!(
        "EVIDENCE: live options nat={} relays={} globalAnnounce={} listen={}",
        opts["natEnabled"],
        opts["relaysEnabled"],
        opts["globalAnnounceEnabled"],
        opts["listenAddresses"]
    );

    // Log file goes where the service config says.
    wait_until(Duration::from_secs(10), "log file", || {
        cfg.log_dir.join("syncthing.log").exists()
    });

    // Clean shutdown via REST exits with code 0 (launchd won't restart it).
    c.shutdown().unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0), "clean shutdown exit code");
    eprintln!("EVIDENCE: clean shutdown exit code = {:?}", status.code());
    let _ = std::fs::remove_dir_all(&root);
}
