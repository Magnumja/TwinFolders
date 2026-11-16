//! Pairing and folder sharing on top of [`crate::client`].
//!
//! Policy encoded here (see SPEC R1/R2/R6):
//! - devices are only added after an explicit user approval, and never with
//!   `autoAcceptFolders`;
//! - every shared/accepted folder gets the default `.stignore` (existing
//!   user files are merged, never overwritten);
//! - versioning is on by default so deletions are recoverable.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use crate::client::{self, Folder, PendingDevice, PendingFolder, SyncthingClient, Versioning};
use crate::stignore;

/// Days to keep old versions by default (staggered versioning).
pub const DEFAULT_VERSIONING_DAYS: u32 = 30;
const ID_PREFIX: &str = "twin-";
const MAX_SLUG: usize = 40;

#[derive(Debug)]
pub enum Error {
    Client(client::Error),
    Io(io::Error),
    InvalidPath(String),
    /// A folder with this ID already exists locally; sharing/accepting
    /// never overwrites an existing folder's config.
    AlreadyExists(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Client(e) => write!(f, "{e}"),
            Error::Io(e) => write!(f, "file system error: {e}"),
            Error::InvalidPath(p) => write!(f, "invalid folder path: {p}"),
            Error::AlreadyExists(id) => write!(f, "folder {id:?} already exists"),
        }
    }
}

impl std::error::Error for Error {}

impl From<client::Error> for Error {
    fn from(e: client::Error) -> Self {
        Error::Client(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn fold_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        other => other,
    }
}

/// Stable, human-readable folder ID from a label, unique among `existing`.
/// e.g. "Documentos Pessoais" -> "twin-documentos-pessoais".
pub fn folder_id_for(label: &str, existing: &[String]) -> String {
    let mut slug = String::new();
    for c in label.to_lowercase().chars().map(fold_accent) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let mut slug: String = slug.trim_end_matches('-').chars().take(MAX_SLUG).collect();
    slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        slug = "pasta".into();
    }
    let base = format!("{ID_PREFIX}{slug}");
    if !existing.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !existing.contains(id))
        .expect("infinite iterator")
}

/// Adds a pending device after the user approved it. `address` is the
/// peer's Tailscale address (e.g. `tcp://100.64.0.2:22000`) or `None` for
/// dynamic discovery.
pub fn approve_device(
    c: &SyncthingClient,
    pending: &PendingDevice,
    address: Option<&str>,
) -> Result<()> {
    let name = if pending.name.is_empty() {
        pending.device_id.chars().take(7).collect()
    } else {
        pending.name.clone()
    };
    // Idempotent: re-approving must not reset an existing device's settings.
    if c.devices()?
        .iter()
        .any(|d| d.device_id == pending.device_id)
    {
        return Ok(());
    }
    let addrs: Vec<&str> = address.into_iter().collect();
    c.add_device(&pending.device_id, &name, &addrs)?;
    Ok(())
}

/// Creates the folder (if needed) and writes the default `.stignore`, or
/// merges defaults and `extra` patterns into an existing one without
/// touching the user's lines.
pub fn prepare_folder_dir(path: &Path, extra: &[&str]) -> Result<()> {
    if !path.is_absolute() {
        return Err(Error::InvalidPath(format!(
            "{} is not absolute",
            path.display()
        )));
    }
    fs::create_dir_all(path)?;
    let file = path.join(".stignore");
    let new = match fs::read_to_string(&file) {
        Ok(existing) => {
            let mut merged = stignore::merge_with_defaults(&existing);
            let have = stignore::parse(&merged);
            let missing: Vec<&str> = extra
                .iter()
                .map(|e| e.trim())
                .filter(|e| !e.is_empty() && !have.iter().any(|h| h == e))
                .collect();
            if !missing.is_empty() {
                if !merged.ends_with('\n') {
                    merged.push('\n');
                }
                for m in missing {
                    merged.push_str(m);
                    merged.push('\n');
                }
            }
            if merged == existing {
                return Ok(());
            }
            merged
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => stignore::render(extra),
        Err(e) => return Err(e.into()),
    };
    fs::write(file, new)?;
    Ok(())
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| Error::InvalidPath(format!("{} is not valid UTF-8", path.display())))
}

/// Validates the ID and refuses IDs that already exist, so a retry or a
/// hostile offer can never replace an existing folder's config.
fn ensure_new_folder(c: &SyncthingClient, id: &str) -> Result<()> {
    client::validate_folder_id(id)?;
    if c.folders()?.iter().any(|f| f.id == id) {
        return Err(Error::AlreadyExists(id.to_string()));
    }
    Ok(())
}

pub struct ShareRequest<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub path: &'a Path,
    pub local_id: &'a str,
    pub peer_id: &'a str,
    pub extra_ignores: &'a [&'a str],
}

/// Shares a local folder with the peer (send-receive, versioned).
pub fn share_folder(c: &SyncthingClient, r: &ShareRequest<'_>) -> Result<Folder> {
    let path = path_str(r.path)?;
    ensure_new_folder(c, r.id)?;
    prepare_folder_dir(r.path, r.extra_ignores)?;
    let folder = Folder::new(r.id, r.label, path, &[r.local_id, r.peer_id])
        .with_versioning(Versioning::staggered_days(DEFAULT_VERSIONING_DAYS));
    c.put_folder(&folder)?;
    Ok(folder)
}

/// Accepts a folder the peer offered, storing it at `path` on this machine.
pub fn accept_folder(
    c: &SyncthingClient,
    offer: &PendingFolder,
    local_id: &str,
    path: &Path,
) -> Result<Folder> {
    let path_s = path_str(path)?;
    ensure_new_folder(c, &offer.folder_id)?;
    prepare_folder_dir(path, &[])?;
    let label = if offer.label.is_empty() {
        &offer.folder_id
    } else {
        &offer.label
    };
    let folder = Folder::new(
        &offer.folder_id,
        label,
        path_s,
        &[local_id, &offer.offered_by],
    )
    .with_versioning(Versioning::staggered_days(DEFAULT_VERSIONING_DAYS));
    c.put_folder(&folder)?;
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp(name: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("twinfolders-unit-{name}-{n}"))
    }

    #[test]
    fn id_from_label_folds_accents_and_spaces() {
        assert_eq!(
            folder_id_for("Documentos Pessoais", &[]),
            "twin-documentos-pessoais"
        );
        assert_eq!(
            folder_id_for("  Música & Vídeos!! ", &[]),
            "twin-musica-videos"
        );
        assert_eq!(folder_id_for("Ação/2026", &[]), "twin-acao-2026");
    }

    #[test]
    fn id_falls_back_when_label_has_no_ascii() {
        assert_eq!(folder_id_for("日本", &[]), "twin-pasta");
        assert_eq!(folder_id_for("", &[]), "twin-pasta");
    }

    #[test]
    fn id_is_unique_among_existing() {
        let existing = vec!["twin-docs".to_string(), "twin-docs-2".to_string()];
        assert_eq!(folder_id_for("Docs", &existing), "twin-docs-3");
    }

    #[test]
    fn id_is_length_limited() {
        let id = folder_id_for(&"a".repeat(200), &[]);
        assert_eq!(id.len(), ID_PREFIX.len() + MAX_SLUG);
    }

    #[test]
    fn prepare_creates_dir_with_default_ignore() {
        let d = tmp("new");
        prepare_folder_dir(&d, &["*.psd"]).unwrap();
        let s = fs::read_to_string(d.join(".stignore")).unwrap();
        let p = stignore::parse(&s);
        assert!(p.contains(&"node_modules".to_string()));
        assert!(p.contains(&"*.psd".to_string()));
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn prepare_preserves_user_lines_first_and_is_idempotent() {
        let d = tmp("existing");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(".stignore"), "!keep.log\n*.log\n").unwrap();
        prepare_folder_dir(&d, &["*.bak"]).unwrap();
        let once = fs::read_to_string(d.join(".stignore")).unwrap();
        assert!(
            once.starts_with("!keep.log\n*.log\n"),
            "user order first: {once}"
        );
        assert!(stignore::parse(&once).contains(&"*.bak".to_string()));
        prepare_folder_dir(&d, &["*.bak"]).unwrap();
        assert_eq!(fs::read_to_string(d.join(".stignore")).unwrap(), once);
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn prepare_rejects_relative_paths() {
        assert!(matches!(
            prepare_folder_dir(Path::new("relative/dir"), &[]),
            Err(Error::InvalidPath(_))
        ));
    }
}
