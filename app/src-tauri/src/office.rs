//! Office mode state kept by the app (not by Syncthing): this computer's
//! role, the central a team computer belongs to, and the invites a central
//! has handed out. Stored as JSON in the app's config folder, owner-only.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const WORKSPACE: &str = "workspace.json";
const INVITES: &str = "invites.json";

/// How this computer is used: a personal pair, the office's central
/// computer (sees and orchestrates the team's), or a team member's.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    #[default]
    Personal,
    Hub,
    Member,
}

/// A team computer that pasted an invite and waits for the central.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Joining {
    pub code: String,
    /// This computer's device name before it was tagged with the code.
    pub original_name: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    #[serde(default)]
    pub role: Role,
    /// Team computer: the central it belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joining: Option<Joining>,
}

/// An invite handed out by the central.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InviteRecord {
    pub code: String,
    pub created: u64,
    pub expires: u64,
    /// Folders offered to the computer once it is approved.
    #[serde(default)]
    pub folders: Vec<String>,
    /// Device that used it (single use).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_by: Option<String>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Writes `value` readable only by this account (invite codes are bearer
/// secrets while they are valid).
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        use std::io::Write;
        let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
        f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn read_workspace(dir: &Path) -> Workspace {
    read_json(&dir.join(WORKSPACE))
}

pub fn write_workspace(dir: &Path, ws: &Workspace) -> Result<(), String> {
    write_json(&dir.join(WORKSPACE), ws)
}

pub fn read_invites(dir: &Path) -> Vec<InviteRecord> {
    read_json(&dir.join(INVITES))
}

/// Saves the list, dropping invites that expired more than a week ago.
pub fn write_invites(dir: &Path, list: &[InviteRecord]) -> Result<(), String> {
    let cutoff = now().saturating_sub(7 * 24 * 3600);
    let keep: Vec<&InviteRecord> = list.iter().filter(|r| r.expires > cutoff).collect();
    write_json(&dir.join(INVITES), &keep)
}

/// An invite that can still be used: unused and not expired.
pub fn open_invite<'a>(list: &'a [InviteRecord], code: &str, now: u64) -> Option<&'a InviteRecord> {
    list.iter()
        .find(|r| r.code == code && r.used_by.is_none() && now < r.expires)
}

/// A team computer only pairs and shares with its central. This guards
/// against mistakes in the app; it is not a boundary against the person
/// at that computer, who can change the role in Preferences.
pub fn member_guard(ws: &Workspace, device_ids: &[&str]) -> Result<(), String> {
    if ws.role != Role::Member {
        return Ok(());
    }
    let Some(hub) = ws.hub_id.as_deref() else {
        return Ok(());
    };
    if device_ids.iter().all(|d| *d == hub) {
        return Ok(());
    }
    Err(format!(
        "este é um computador do time: ele só se conecta à central{}. Para mudar, troque o papel em Preferências.",
        ws.hub_name
            .as_deref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("tf-office-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn rec(code: &str, created: u64, expires: u64, used: Option<&str>) -> InviteRecord {
        InviteRecord {
            code: code.into(),
            created,
            expires,
            folders: vec![],
            used_by: used.map(str::to_string),
        }
    }

    #[test]
    fn workspace_roundtrip_and_defaults() {
        let d = tmpdir("ws");
        assert_eq!(read_workspace(&d), Workspace::default());
        let ws = Workspace {
            role: Role::Member,
            hub_id: Some("HUB".into()),
            hub_name: Some("mac-ana".into()),
            joining: Some(Joining {
                code: "K7Q2-9XPM".into(),
                original_name: "note-ana".into(),
            }),
        };
        write_workspace(&d, &ws).unwrap();
        assert_eq!(read_workspace(&d), ws);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(d.join(WORKSPACE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "owner-only");
        }
        // Older files with only the role still load.
        std::fs::write(d.join(WORKSPACE), r#"{"role":"hub"}"#).unwrap();
        assert_eq!(read_workspace(&d).role, Role::Hub);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn invites_are_single_use_and_expire() {
        let d = tmpdir("inv");
        let t = now();
        let mut list = vec![
            rec("AAAA-BBBB", t, t + 100, None),
            rec("CCCC-DDDD", t, t + 100, Some("X")),
            rec("EEEE-FFFF", t - 200, t - 100, None),
            rec("GGGG-HHHH", 0, 1, None),
        ];
        assert!(open_invite(&list, "AAAA-BBBB", t).is_some());
        assert!(open_invite(&list, "CCCC-DDDD", t).is_none(), "used");
        assert!(open_invite(&list, "EEEE-FFFF", t).is_none(), "expired");
        assert!(open_invite(&list, "ZZZZ-ZZZZ", t).is_none());
        list[0].used_by = Some("NOTE".into());
        write_invites(&d, &list).unwrap();
        let back = read_invites(&d);
        assert_eq!(back.len(), 3, "long-expired invites are dropped");
        assert!(open_invite(&back, "AAAA-BBBB", t).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn team_computers_only_talk_to_their_central() {
        let mut ws = Workspace {
            role: Role::Member,
            hub_id: Some("HUB".into()),
            hub_name: Some("mac-ana".into()),
            joining: None,
        };
        assert!(member_guard(&ws, &["HUB"]).is_ok());
        let e = member_guard(&ws, &["HUB", "OTHER"]).unwrap_err();
        assert!(e.contains("mac-ana"));
        ws.role = Role::Hub;
        assert!(member_guard(&ws, &["OTHER"]).is_ok());
        ws.role = Role::Member;
        ws.hub_id = None;
        assert!(member_guard(&ws, &["OTHER"]).is_ok(), "not joined yet");
    }
}
