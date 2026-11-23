//! Slice 9: the office invite against two live Syncthing daemons.
//!
//! The central creates an invite; the team computer decodes it, tags its
//! own name with the code and adds the central. The central must see the
//! pending request carrying that code (that is how it recognises an
//! invited computer), approve it with the clean name, and after the
//! connection the team computer restores its real name.

mod common;

use std::time::Duration;

use common::{require_syncthing, temp_root, wait_until, Node};
use twinfolders_core::client::PendingDevice;
use twinfolders_core::{invite, pairing};

#[test]
fn invited_computer_is_recognised_by_its_code() {
    let Some(bin) = require_syncthing() else {
        return;
    };
    let root = temp_root("invite");
    let hub = Node::start(&bin, "central", &root);
    let note = Node::start(&bin, "note", &root);

    // Central: create the invite (address = its local listen port here).
    let code = invite::new_code();
    let text = invite::Invite::new(
        &hub.id,
        "central",
        vec![format!("tcp://127.0.0.1:{}", hub.listen_port)],
        code.clone(),
        1_000,
    )
    .encode();

    // Team computer: decode, tag own name, add the central.
    let inv = invite::Invite::decode(&text, 1_001).unwrap();
    assert_eq!(inv.hub, hub.id);
    note.client
        .set_device_name(&note.id, &invite::tag_name("note-ana", &inv.code))
        .unwrap();
    note.client
        .add_device(&inv.hub, &inv.name, &[inv.addrs[0].as_str()])
        .unwrap();

    // Central: the pending request carries the code.
    let mut pending: Vec<PendingDevice> = Vec::new();
    wait_until(Duration::from_secs(60), "pending device on central", || {
        pending = hub.client.pending_devices().unwrap_or_default();
        pending.iter().any(|p| p.device_id == note.id)
    });
    let req = pending.iter().find(|p| p.device_id == note.id).unwrap();
    let (clean, got) = invite::parse_tag(&req.name);
    assert_eq!(
        got.as_deref(),
        Some(code.as_str()),
        "name was {:?}",
        req.name
    );
    assert_eq!(clean, "note-ana");

    // Approve with the clean name; they connect.
    let approved = PendingDevice {
        name: clean,
        ..req.clone()
    };
    pairing::approve_device(
        &hub.client,
        &approved,
        Some(&format!("tcp://127.0.0.1:{}", note.listen_port)),
    )
    .unwrap();
    let saved = hub.client.devices().unwrap();
    assert_eq!(
        saved.iter().find(|d| d.device_id == note.id).unwrap().name,
        "note-ana"
    );
    wait_until(Duration::from_secs(60), "connected", || {
        hub.connected_to(&note) && note.connected_to(&hub)
    });

    // Team computer: restore its real name.
    note.client.set_device_name(&note.id, "note-ana").unwrap();
    let mine = note.client.devices().unwrap();
    assert_eq!(
        mine.iter().find(|d| d.device_id == note.id).unwrap().name,
        "note-ana"
    );

    drop(hub);
    drop(note);
    let _ = std::fs::remove_dir_all(&root);
}
