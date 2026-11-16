//! Slice 2: the real pairing UX against two live Syncthing daemons.
//!
//! 1. "windows" adds "mac" and tries to connect.
//! 2. "mac" sees windows as a *pending* device and the user approves it
//!    explicitly (nothing is auto-accepted).
//! 3. "mac" shares a folder; "windows" sees a *pending* folder offer.
//! 4. The user accepts it on windows at a different local path.
//! 5. Files flow both ways and the default .stignore exists on both sides.

mod common;

use std::fs;
use std::time::Duration;

use common::{read, require_syncthing, temp_root, wait_until, Node};
use twinfolders_core::pairing::{self, ShareRequest};
use twinfolders_core::stignore;

#[test]
fn explicit_pairing_then_share_and_accept_folder() {
    let Some(bin) = require_syncthing() else {
        return;
    };
    let root = temp_root("pairing");
    let mac = Node::start(&bin, "mac", &root);
    let win = Node::start(&bin, "windows", &root);

    // Nothing is configured on mac yet: no auto-accept anywhere.
    assert!(mac
        .client
        .devices()
        .unwrap()
        .iter()
        .all(|d| d.device_id != win.id));

    // 1. windows knows mac (user pasted mac's Device ID on windows).
    win.client
        .add_device(
            &mac.id,
            "mac",
            &[&format!("tcp://127.0.0.1:{}", mac.listen_port)],
        )
        .unwrap();

    // 2. mac sees windows as pending and the user approves it.
    let mut pending = Vec::new();
    wait_until(Duration::from_secs(60), "pending device on mac", || {
        pending = mac.client.pending_devices().unwrap_or_default();
        pending.iter().any(|p| p.device_id == win.id)
    });
    let req = pending.iter().find(|p| p.device_id == win.id).unwrap();
    pairing::approve_device(
        &mac.client,
        req,
        Some(&format!("tcp://127.0.0.1:{}", win.listen_port)),
    )
    .unwrap();
    let approved = mac.client.devices().unwrap();
    let w = approved.iter().find(|d| d.device_id == win.id).unwrap();
    assert!(
        !w.auto_accept_folders,
        "approval must not enable auto-accept"
    );
    wait_until(Duration::from_secs(60), "connected", || {
        mac.connected_to(&win) && win.connected_to(&mac)
    });

    // 3. mac shares a folder with a user .stignore line.
    let mac_dir = root.join("Mac Docs");
    let existing: Vec<String> = mac
        .client
        .folders()
        .unwrap()
        .into_iter()
        .map(|f| f.id)
        .collect();
    let folder_id = pairing::folder_id_for("Documentos Pessoais", &existing);
    let shared = pairing::share_folder(
        &mac.client,
        &ShareRequest {
            id: &folder_id,
            label: "Documentos Pessoais",
            path: &mac_dir,
            local_id: &mac.id,
            peer_id: &win.id,
            extra_ignores: &["*.psd"],
        },
    )
    .unwrap();
    assert_eq!(
        shared.versioning.kind, "staggered",
        "versioning on by default"
    );
    let mac_ignore = fs::read_to_string(mac_dir.join(".stignore")).unwrap();
    assert!(stignore::parse(&mac_ignore).contains(&"*.psd".to_string()));

    // 4. windows sees the offer and accepts it at its own path.
    let mut offers = Vec::new();
    wait_until(Duration::from_secs(60), "pending folder on windows", || {
        offers = win.client.pending_folders().unwrap_or_default();
        offers.iter().any(|o| o.folder_id == folder_id)
    });
    let offer = offers.iter().find(|o| o.folder_id == folder_id).unwrap();
    assert_eq!(offer.offered_by, mac.id);
    assert_eq!(offer.label, "Documentos Pessoais");
    let win_dir = root.join("C_Users_me_Documents");
    let accepted = pairing::accept_folder(&win.client, offer, &win.id, &win_dir).unwrap();
    assert_eq!(accepted.id, folder_id);
    assert!(
        win_dir.join(".stignore").exists(),
        "default .stignore on the accepting side too"
    );

    // 5. files flow both ways; ignored patterns stay local.
    fs::write(mac_dir.join("from-mac.txt"), "mac").unwrap();
    fs::write(mac_dir.join("art.psd"), "big").unwrap();
    fs::write(win_dir.join("from-win.txt"), "win").unwrap();
    mac.client.scan(&folder_id).unwrap();
    win.client.scan(&folder_id).unwrap();
    wait_until(Duration::from_secs(60), "files both ways", || {
        read(&win_dir.join("from-mac.txt")).as_deref() == Some("mac")
            && read(&mac_dir.join("from-win.txt")).as_deref() == Some("win")
    });
    assert!(
        !win_dir.join("art.psd").exists(),
        "*.psd ignored on mac side"
    );

    drop(mac);
    drop(win);
    let _ = fs::remove_dir_all(&root);
}
