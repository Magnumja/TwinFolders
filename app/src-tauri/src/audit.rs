//! Record of what was done through the app on this computer (pairings,
//! sharing, disconnections, invites, role changes...). Local only, owner-
//! only JSON lines, capped. It answers "who changed this setting, and
//! when?" on the central; it is not tamper-proof against the person who
//! owns this account.

use std::io::{BufRead, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

const FILE: &str = "audit.jsonl";
const KEEP: usize = 2000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// Unix seconds.
    pub at: u64,
    /// Short action key, e.g. `device.approved`.
    pub action: String,
    /// Human detail in pt-BR (names, not secrets).
    pub detail: String,
}

fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(300).collect()
}

fn open(path: &Path, append: bool) -> std::io::Result<std::fs::File> {
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

pub fn read(dir: &Path) -> Vec<AuditEntry> {
    let Ok(f) = std::fs::File::open(dir.join(FILE)) else {
        return Vec::new();
    };
    std::io::BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect()
}

/// Appends one entry; best effort (a full disk must not break the action
/// that was already done).
pub fn record(dir: &Path, at: u64, action: &str, detail: &str) {
    let entry = AuditEntry {
        at,
        action: clean(action),
        detail: clean(detail),
    };
    let path = dir.join(FILE);
    let Ok(line) = serde_json::to_string(&entry) else {
        return;
    };
    if let Ok(mut f) = open(&path, true) {
        let _ = writeln!(f, "{line}");
    }
    let all = read(dir);
    if all.len() > KEEP + KEEP / 4 {
        let tmp = path.with_extension("tmp");
        if let Ok(mut f) = open(&tmp, false) {
            for e in &all[all.len() - KEEP..] {
                if let Ok(l) = serde_json::to_string(e) {
                    let _ = writeln!(f, "{l}");
                }
            }
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_in_order_cleans_and_caps() {
        let d = std::env::temp_dir().join(format!("tf-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        record(&d, 1, "device.approved", "note-ana\u{1b}[31m");
        record(&d, 2, "device.removed", "note-bruno");
        let back = read(&d);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].detail, "note-ana[31m");
        assert_eq!(back[1].action, "device.removed");
        for i in 0..(KEEP + KEEP / 4 + 5) as u64 {
            record(&d, i, "x", "y");
        }
        assert!(read(&d).len() <= KEEP + KEEP / 4);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(d.join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
