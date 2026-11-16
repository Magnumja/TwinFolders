//! Integration tests with two real Syncthing daemons on localhost.
//!
//! Each test starts two isolated daemons (own home dir, GUI port, listen
//! port; discovery, relays and NAT disabled) and pairs them with static
//! `tcp://127.0.0.1:<port>` addresses, simulating the Windows PC and the Mac.
//! "Offline" is simulated by pausing the peer device.
//!
//! Requires the `syncthing` binary on PATH (or `TWINFOLDERS_SYNCTHING`).
//! Locally the tests are skipped with a message if it's missing; in CI
//! (`CI` set) a missing binary is a failure.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::{read, require_syncthing, temp_root, wait_until, Node};
use serde_json::json;
use twinfolders_core::client::{Folder, Versioning};
use twinfolders_core::conflict::{self, Resolution};
use twinfolders_core::{resolve, stignore};

const FOLDER_ID: &str = "twin-test";

/// Two paired daemons sharing FOLDER_ID, with the default .stignore.
struct Pair {
    a: Node,
    b: Node,
    root: PathBuf,
}

impl Drop for Pair {
    fn drop(&mut self) {
        // Nodes are dropped after this runs; kill them first so files are free.
        self.a.stop();
        self.b.stop();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn setup_pair(bin: &str, test: &str) -> Pair {
    let root = temp_root(test);
    let a = Node::start(bin, "mac", &root);
    let b = Node::start(bin, "windows", &root);

    a.client
        .add_device(
            &b.id,
            "windows",
            &[&format!("tcp://127.0.0.1:{}", b.listen_port)],
        )
        .unwrap();
    b.client
        .add_device(
            &a.id,
            "mac",
            &[&format!("tcp://127.0.0.1:{}", a.listen_port)],
        )
        .unwrap();

    for n in [&a, &b] {
        let path = n.folder_path();
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join(".stignore"), stignore::render(&[])).unwrap();
        let mut f = Folder::new(FOLDER_ID, "Test", path.to_str().unwrap(), &[&a.id, &b.id])
            .with_versioning(Versioning::trashcan_days(30));
        f.extra.insert("fsWatcherDelayS".into(), json!(1));
        f.extra.insert("rescanIntervalS".into(), json!(5));
        n.client.put_folder(&f).unwrap();
    }

    wait_until(Duration::from_secs(60), "devices connected", || {
        a.connected_to(&b) && b.connected_to(&a)
    });
    Pair { a, b, root }
}

fn wait_idle(n: &Node) {
    wait_until(Duration::from_secs(60), "folder idle", || {
        n.client
            .folder_status(FOLDER_ID)
            .map(|s| s.state == "idle")
            .unwrap_or(false)
    });
}

#[test]
fn file_created_on_mac_appears_on_windows_and_ignores_apply() {
    let Some(bin) = require_syncthing() else {
        return;
    };
    let p = setup_pair(&bin, "create");

    let src = p.a.folder_path();
    fs::write(src.join("hello.txt"), "olá do Mac").unwrap();
    fs::create_dir_all(src.join("node_modules/pkg")).unwrap();
    fs::write(src.join("node_modules/pkg/index.js"), "ignored").unwrap();
    p.a.client.scan(FOLDER_ID).unwrap();

    let started = Instant::now();
    let dst = p.b.folder_path().join("hello.txt");
    wait_until(Duration::from_secs(60), "hello.txt on windows", || {
        read(&dst).as_deref() == Some("olá do Mac")
    });
    eprintln!(
        "EVIDENCE: Mac -> Windows propagation took {:?}",
        started.elapsed()
    );

    wait_idle(&p.b);
    assert!(
        !p.b.folder_path().join("node_modules").exists(),
        "node_modules must be ignored by the default .stignore"
    );
    let c = p.b.client.completion(FOLDER_ID, Some(&p.a.id)).unwrap();
    eprintln!("EVIDENCE: completion windows->mac view = {}%", c.completion);
}

#[test]
fn edit_while_peer_offline_syncs_after_reconnect() {
    let Some(bin) = require_syncthing() else {
        return;
    };
    let p = setup_pair(&bin, "offline");

    let a_file = p.a.folder_path().join("notes.txt");
    let b_file = p.b.folder_path().join("notes.txt");
    fs::write(&b_file, "v1").unwrap();
    p.b.client.scan(FOLDER_ID).unwrap();
    wait_until(Duration::from_secs(60), "v1 on mac", || {
        read(&a_file).as_deref() == Some("v1")
    });

    // Mac goes "offline": Windows pauses the Mac device.
    p.b.client.pause_device(&p.a.id).unwrap();
    wait_until(Duration::from_secs(30), "disconnected", || {
        !p.a.connected_to(&p.b)
    });

    fs::write(&b_file, "v2 editado no Windows com o Mac offline").unwrap();
    p.b.client.scan(FOLDER_ID).unwrap();
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(
        read(&a_file).as_deref(),
        Some("v1"),
        "must not sync while offline"
    );

    // Mac comes back; no manual action besides reconnecting.
    let started = Instant::now();
    p.b.client.resume_device(&p.a.id).unwrap();
    wait_until(Duration::from_secs(90), "v2 on mac after reconnect", || {
        read(&a_file).as_deref() == Some("v2 editado no Windows com o Mac offline")
    });
    eprintln!(
        "EVIDENCE: resumed sync {:?} after reconnect",
        started.elapsed()
    );
}

#[test]
fn concurrent_offline_edits_create_visible_conflict_without_data_loss() {
    let Some(bin) = require_syncthing() else {
        return;
    };
    let p = setup_pair(&bin, "conflict");

    let a_file = p.a.folder_path().join("plan.txt");
    let b_file = p.b.folder_path().join("plan.txt");
    fs::write(&a_file, "base").unwrap();
    p.a.client.scan(FOLDER_ID).unwrap();
    wait_until(Duration::from_secs(60), "base on windows", || {
        read(&b_file).as_deref() == Some("base")
    });
    wait_idle(&p.a);
    wait_idle(&p.b);

    p.a.client.pause_device(&p.b.id).unwrap();
    p.b.client.pause_device(&p.a.id).unwrap();
    wait_until(Duration::from_secs(30), "disconnected", || {
        !p.a.connected_to(&p.b) && !p.b.connected_to(&p.a)
    });

    fs::write(&a_file, "versão do Mac").unwrap();
    std::thread::sleep(Duration::from_millis(1100)); // distinct mtimes
    fs::write(&b_file, "versão do Windows").unwrap();
    p.a.client.scan(FOLDER_ID).unwrap();
    p.b.client.scan(FOLDER_ID).unwrap();
    wait_idle(&p.a);
    wait_idle(&p.b);

    p.a.client.resume_device(&p.b.id).unwrap();
    p.b.client.resume_device(&p.a.id).unwrap();

    let list = |dir: &Path| -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    };
    let mut found = Vec::new();
    wait_until(
        Duration::from_secs(90),
        "conflict copy on both sides",
        || {
            let a = list(&p.a.folder_path());
            let b = list(&p.b.folder_path());
            let ca = conflict::find_conflicts(a.iter().map(String::as_str));
            let cb = conflict::find_conflicts(b.iter().map(String::as_str));
            if !ca.is_empty() && !cb.is_empty() {
                found = ca;
                true
            } else {
                false
            }
        },
    );

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].original_path, "plan.txt");
    let winner = read(&a_file).unwrap();
    let loser = read(&p.a.folder_path().join(&found[0].conflict_path)).unwrap();
    let mut both = vec![winner, loser];
    both.sort();
    assert_eq!(
        both,
        vec!["versão do Mac".to_string(), "versão do Windows".to_string()],
        "no data lost"
    );
    eprintln!("EVIDENCE: conflict detected: {}", found[0].conflict_path);

    // Slice 5.2: resolve on the Mac with "keep the other version" and check
    // both machines converge with no conflict copy left and nothing lost.
    let r = resolve::scan(&p.a.folder_path(), 10_000).unwrap();
    assert_eq!(r.conflicts.len(), 1);
    let loser_text = read(&p.a.folder_path().join(&found[0].conflict_path)).unwrap();
    let winner_text = read(&a_file).unwrap();
    let out = resolve::resolve(
        &p.a.folder_path(),
        &found[0].conflict_path,
        Resolution::KeepOther,
        "20261116-150000",
    )
    .unwrap();
    assert_eq!(read(&a_file).as_deref(), Some(loser_text.as_str()));
    assert_eq!(
        read(&p.a.folder_path().join(&out.archived[0])).as_deref(),
        Some(winner_text.as_str()),
        "discarded version kept in .stversions"
    );
    p.a.client.scan(FOLDER_ID).unwrap();
    wait_until(
        Duration::from_secs(90),
        "resolution reaches windows",
        || {
            read(&b_file).as_deref() == Some(loser_text.as_str())
                && resolve::scan(&p.b.folder_path(), 10_000)
                    .map(|r| r.conflicts.is_empty())
                    .unwrap_or(false)
        },
    );
    assert!(resolve::scan(&p.a.folder_path(), 10_000)
        .unwrap()
        .conflicts
        .is_empty());
    eprintln!("EVIDENCE: conflict resolved (KeepOther) and converged on both sides");
}
