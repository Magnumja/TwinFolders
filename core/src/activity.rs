//! Recent file activity in the synced folders, from Syncthing's disk
//! events: `LocalChangeDetected` (changed on this computer) and
//! `RemoteChangeDetected` (changed on another computer; `modifiedBy` is
//! the short ID of the one that changed it).
//!
//! Only what happens inside synced folders is visible here: Syncthing
//! (and so TwinFolders) never looks at the rest of a computer.
//!
//! The log is a JSON-lines file kept by the app, owner-only, capped so it
//! never grows without bound.

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::Event;

/// Event types to request from `/rest/events` (not in the default stream).
pub const EVENT_TYPES: &[&str] = &["LocalChangeDetected", "RemoteChangeDetected"];
/// Entries kept in the log; older ones are dropped when it grows past
/// this by a margin.
pub const KEEP: usize = 5000;
const SLACK: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// RFC 3339 time Syncthing noticed the change.
    pub time: String,
    pub folder_id: String,
    pub folder_label: String,
    /// Path inside the folder.
    pub path: String,
    /// `added`, `modified` or `deleted` (Syncthing reports a new file as
    /// `modified` too, so the UI words both as a change).
    pub action: String,
    /// `file`, `dir` or `symlink`.
    pub kind: String,
    /// Short device ID of the computer that made the change; `None` when
    /// it was this computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

impl Entry {
    /// Identity for de-duplication (Syncthing replays its event buffer
    /// after the app reconnects).
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.time,
            self.folder_id,
            self.path,
            self.action,
            self.by.as_deref().unwrap_or("")
        )
    }
}

fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(1024)
        .collect()
}

fn or(a: String, b: impl FnOnce() -> String) -> String {
    if a.is_empty() {
        b()
    } else {
        a
    }
}

/// One log entry from a disk event; other events, or ones missing the
/// essentials, give `None`.
pub fn from_event(ev: &Event) -> Option<Entry> {
    let remote = match ev.kind.as_str() {
        "LocalChangeDetected" => false,
        "RemoteChangeDetected" => true,
        _ => return None,
    };
    let d = &ev.data;
    let folder_id = or(s(d, "folderID"), || s(d, "folder"));
    let path = s(d, "path");
    let action = s(d, "action");
    if folder_id.is_empty() || path.is_empty() || action.is_empty() {
        return None;
    }
    let by = remote.then(|| s(d, "modifiedBy")).filter(|b| !b.is_empty());
    Some(Entry {
        time: ev.time.clone(),
        folder_label: or(s(d, "label"), || folder_id.clone()),
        folder_id,
        path,
        action,
        kind: or(s(d, "type"), || "file".into()),
        by,
    })
}

/// Reads the whole log (oldest first). A missing or partly corrupt file
/// gives what can be read.
pub fn read(path: &Path) -> Vec<Entry> {
    let Ok(f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    std::io::BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect()
}

fn open_private(path: &Path, append: bool) -> std::io::Result<std::fs::File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut o = std::fs::OpenOptions::new();
    o.create(true);
    if append {
        o.append(true);
    } else {
        o.write(true).truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)
}

/// Appends entries not already in `seen` (updating it), then trims the
/// file to the newest [`KEEP`] when it has grown past the margin.
/// Returns how many were added.
pub fn append(
    path: &Path,
    entries: &[Entry],
    seen: &mut HashSet<String>,
) -> std::io::Result<usize> {
    let fresh: Vec<&Entry> = entries.iter().filter(|e| seen.insert(e.key())).collect();
    if fresh.is_empty() {
        return Ok(0);
    }
    {
        let mut f = open_private(path, true)?;
        for e in &fresh {
            let line = serde_json::to_string(e).map_err(std::io::Error::other)?;
            writeln!(f, "{line}")?;
        }
    }
    let all = read(path);
    if all.len() > KEEP + SLACK {
        let keep = &all[all.len() - KEEP..];
        let tmp = path.with_extension("tmp");
        {
            let mut f = open_private(&tmp, false)?;
            for e in keep {
                let line = serde_json::to_string(e).map_err(std::io::Error::other)?;
                writeln!(f, "{line}")?;
            }
        }
        std::fs::rename(&tmp, path)?;
    }
    Ok(fresh.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(kind: &str, data: Value) -> Event {
        Event {
            id: 1,
            kind: kind.into(),
            time: "2026-11-22T14:02:00-04:00".into(),
            data,
        }
    }

    #[test]
    fn parses_local_and_remote_changes() {
        let local = from_event(&ev(
            "LocalChangeDetected",
            json!({"action":"modified","folder":"proj","folderID":"proj","label":"Projetos","path":"a/b.txt","type":"file"}),
        ))
        .unwrap();
        assert_eq!(local.by, None);
        assert_eq!(local.folder_label, "Projetos");
        let remote = from_event(&ev(
            "RemoteChangeDetected",
            json!({"action":"deleted","folderID":"proj","label":"","path":"x.xlsx","type":"file","modifiedBy":"NOTE1AA"}),
        ))
        .unwrap();
        assert_eq!(remote.by.as_deref(), Some("NOTE1AA"));
        assert_eq!(remote.folder_label, "proj", "label falls back to the ID");
        assert!(from_event(&ev("StateChanged", json!({}))).is_none());
        assert!(from_event(&ev("LocalChangeDetected", json!({"folderID":"p"}))).is_none());
    }

    #[test]
    fn strips_control_characters_from_paths() {
        let e = from_event(&ev(
            "LocalChangeDetected",
            json!({"action":"added","folderID":"p","path":"a\u{1b}[31mb","type":"file"}),
        ))
        .unwrap();
        assert_eq!(e.path, "a[31mb");
    }

    #[test]
    fn append_dedupes_and_caps() {
        let dir = std::env::temp_dir().join(format!("tf-activity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("activity.jsonl");
        let mk = |i: usize| Entry {
            time: format!("t{i:06}"),
            folder_id: "p".into(),
            folder_label: "P".into(),
            path: format!("f{i}"),
            action: "modified".into(),
            kind: "file".into(),
            by: None,
        };
        let mut seen = HashSet::new();
        assert_eq!(append(&path, &[mk(1), mk(1), mk(2)], &mut seen).unwrap(), 2);
        assert_eq!(append(&path, &[mk(2)], &mut seen).unwrap(), 0, "replayed");
        let many: Vec<Entry> = (3..KEEP + SLACK + 10).map(mk).collect();
        append(&path, &many, &mut seen).unwrap();
        let back = read(&path);
        assert_eq!(back.len(), KEEP);
        assert_eq!(back.last().unwrap().path, format!("f{}", KEEP + SLACK + 9));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
