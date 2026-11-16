//! Slice 5.3 against a real Syncthing: versioning choices are stored with
//! the right units, other fields survive, and copies archived by the
//! conflict executor are recognised by Syncthing as versions.

mod common;

use std::fs;

use common::{require_syncthing, temp_root, Node};
use twinfolders_core::client::Folder;
use twinfolders_core::conflict::Resolution;
use twinfolders_core::resolve;
use twinfolders_core::versioning::Choice;

const ID: &str = "twin-versions";

#[test]
fn versioning_roundtrip_and_archived_copies_are_listed() {
    let Some(bin) = require_syncthing() else {
        return;
    };
    let root = temp_root("versioning");
    let n = Node::start(&bin, "mac", &root);
    let path = n.folder_path();
    fs::create_dir_all(&path).unwrap();
    n.client
        .put_folder(&Folder::new(ID, "V", path.to_str().unwrap(), &[&n.id]))
        .unwrap();

    let before = n.client.folder(ID).unwrap().versioning;
    for choice in [
        Choice::Staggered { days: 30 },
        Choice::Trashcan { days: 14 },
        Choice::Simple { keep: 7, days: 90 },
        Choice::Off,
        Choice::Staggered { days: 365 },
    ] {
        let cur = n.client.folder(ID).unwrap().versioning;
        n.client
            .set_folder_versioning(ID, &choice.apply(&cur))
            .unwrap();
        let stored = n.client.folder(ID).unwrap().versioning;
        assert_eq!(Choice::from_versioning(&stored), Some(choice), "{choice:?}");
        // Fields we do not manage are kept.
        assert_eq!(
            stored.extra.get("cleanupIntervalS"),
            before.extra.get("cleanupIntervalS")
        );
    }
    let stored = n.client.folder(ID).unwrap().versioning;
    assert_eq!(
        stored.params["maxAge"], "31536000",
        "staggered maxAge in seconds"
    );

    // A copy archived by the conflict executor shows up in Syncthing's
    // version list, i.e. our naming matches Syncthing's.
    fs::write(path.join("nota.txt"), "minha").unwrap();
    fs::write(
        path.join("nota.sync-conflict-20261116-143005-ABCDEF2.txt"),
        "outra",
    )
    .unwrap();
    resolve::resolve(
        &path,
        "nota.sync-conflict-20261116-143005-ABCDEF2.txt",
        Resolution::KeepOther,
        "20261116-150000",
    )
    .unwrap();
    let versions = n.client.folder_versions(ID).unwrap();
    let list = versions
        .get("nota.txt")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("nota.txt not in versions: {versions}"));
    assert_eq!(list.len(), 1);
    eprintln!("EVIDENCE: Syncthing lists archived version: {}", list[0]);
}
