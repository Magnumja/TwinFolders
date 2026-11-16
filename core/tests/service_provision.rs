//! Slice 3: provisioning a fresh Syncthing home with real `syncthing
//! generate`, hardening config.xml, and checking the running daemon really
//! applies it (no launchd/Task Scheduler involved).

mod common;

use std::process::{Command, Stdio};
use std::time::Duration;

use common::{require_syncthing, temp_root, wait_until};
use twinfolders_core::client::{Error, SyncthingClient};
use twinfolders_core::service::{self, Os, ServiceConfig};
use twinfolders_core::stconfig;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
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

    let prov = service::provision_home(&cfg).unwrap();
    let gui = prov.gui.clone();
    assert!(gui.has_gui_auth(), "web GUI protected by user/password");
    assert_eq!(gui.user, "twinfolders");
    assert_eq!(prov.new_gui_password.as_ref().map(|p| p.len()), Some(32));
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
    assert_eq!(again.gui.api_key, gui.api_key);
    assert_eq!(again.new_gui_password, None, "existing password kept");

    // Start exactly like the service would (same args, no key on argv).
    let args = cfg.serve_args(Os::current().unwrap_or(Os::MacOs));
    assert!(!args.iter().any(|a| a.contains(&gui.api_key)));
    let mut child = Command::new(&bin)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    assert!(gui.tls, "GUI served over TLS");
    let c = service::client_for(&cfg, &gui.api_key).unwrap();
    wait_until(Duration::from_secs(30), "daemon up", || c.ping().is_ok());

    // Pinning: an ordinary HTTPS client (system/webpki roots) is rejected,
    // and so is one pinned to a different CA.
    let unpinned = SyncthingClient::new(&cfg.gui_url(), &gui.api_key).unwrap();
    assert!(
        matches!(unpinned.ping(), Err(Error::Transport(_))),
        "unpinned TLS must fail"
    );
    let other = root.join("other");
    twinfolders_core::guitls::ensure(&other).unwrap();
    let other_ca = twinfolders_core::guitls::load_ca_pem(&other).unwrap();
    let wrong = SyncthingClient::with_pinned_ca(&cfg.gui_url(), &gui.api_key, &other_ca).unwrap();
    assert!(
        matches!(wrong.ping(), Err(Error::Transport(_))),
        "wrong CA must fail"
    );
    eprintln!("EVIDENCE: pinned TLS ok; unpinned and wrong-CA clients rejected");
    // The listener-ownership check exists only on the supported desktop OSes.
    if Os::current().is_some() {
        assert!(
            service::listener_is_ours(&cfg),
            "our syncthing owns the GUI port"
        );

        // A squatter on another port is not "ours": the key must not be sent.
        let squatter = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut fake = cfg.clone();
        fake.gui_port = squatter.local_addr().unwrap().port();
        assert!(
            !service::listener_is_ours(&fake),
            "foreign listener rejected"
        );
        eprintln!("EVIDENCE: listener check ours=true squatter=false");
    }

    // Check the live options through REST, not just the file.
    let opts = c.get_value("/rest/config/options").unwrap();
    assert_eq!(opts["natEnabled"], false, "no UPnP/NAT-PMP router mapping");
    assert_eq!(opts["relaysEnabled"], false);
    assert_eq!(opts["globalAnnounceEnabled"], false);
    assert_eq!(opts["crashReportingEnabled"], false);
    assert_eq!(opts["urAccepted"], -1);
    assert_eq!(
        opts["listenAddresses"],
        serde_json::json!([format!("tcp://0.0.0.0:{}", cfg.listen_port)])
    );
    let gui_live = c.get_value("/rest/config/gui").unwrap();
    assert_eq!(gui_live["user"], "twinfolders");
    assert!(
        gui_live["password"].as_str().unwrap().starts_with("$2"),
        "bcrypt hash"
    );
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
