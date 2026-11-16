//! Finds conflict copies on disk and executes resolution plans safely.
//!
//! Peer-controlled names reach this code, so every step is defensive:
//! - paths are re-derived from the conflict file name (never trusted from
//!   the UI) and must be plain relative paths inside the folder root;
//! - no path component may be a symlink/junction, and the moved item must be
//!   a regular file, so nothing is ever moved outside the root;
//! - renames never replace an existing file (`renamex_np(RENAME_EXCL)` on
//!   macOS, `renameat2(RENAME_NOREPLACE)` on Linux, `MoveFileExW` without
//!   `MOVEFILE_REPLACE_EXISTING` on Windows);
//! - data is never deleted: the discarded version goes to `.stversions`
//!   with Syncthing's own `name~YYYYMMDD-HHMMSS.ext` naming (local time,
//!   supplied by the caller), so it shows up in Syncthing's version list;
//! - if a step fails, completed moves are undone in reverse order.
//!
//! Residual risk (documented): a local process racing between the checks
//! and the rename could swap a directory for a symlink. Syncthing itself
//! does not do this, and the rename still never overwrites.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::conflict::{self, ConflictFile, FsAction, Resolution};

pub const VERSIONS_DIR: &str = ".stversions";
const INTERNAL: &[&str] = &[".stversions", ".stfolder", ".stignore"];

#[derive(Debug)]
pub enum Error {
    NotAConflict(String),
    InvalidTag(String),
    UnsafePath(String),
    NotFound(String),
    Io {
        step: String,
        source: io::Error,
    },
    /// A step failed and undoing an earlier move also failed: `stranded`
    /// lists `(where the file is now, where it was)` so nothing is silent.
    RollbackFailed {
        cause: Box<Error>,
        stranded: Vec<(String, String)>,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotAConflict(p) => write!(f, "não é uma cópia de conflito válida: {p}"),
            Error::InvalidTag(t) => write!(f, "carimbo de versão inválido: {t}"),
            Error::UnsafePath(p) => write!(f, "caminho recusado por segurança: {p}"),
            Error::NotFound(p) => write!(f, "arquivo não encontrado: {p}"),
            Error::Io { step, source } => write!(f, "{step}: {source}"),
            Error::RollbackFailed { cause, stranded } => {
                write!(
                    f,
                    "{cause}. Não foi possível desfazer tudo; mova de volta manualmente:"
                )?;
                for (now, was) in stranded {
                    write!(f, " {now} → {was};")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for Error {}

/// A conflict as shown in the UI.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictEntry {
    pub conflict_path: String,
    pub original_path: String,
    pub device_short_id: String,
    /// `YYYY-MM-DD HH:MM:SS` from the conflict name.
    pub when: String,
    pub conflict_size: u64,
    /// `None` when the original no longer exists.
    pub original_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub conflicts: Vec<ConflictEntry>,
    /// True when the walk stopped at `max_entries`.
    pub truncated: bool,
}

/// What a resolution did, for the UI ("arquivada em ...").
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// Relative paths of archived copies under the folder root.
    pub archived: Vec<String>,
    /// Final relative path of the kept file(s) that moved.
    pub renamed_to: Vec<String>,
}

/// Walks `root` (not following symlinks, skipping Syncthing's internal
/// files) and returns the conflict copies found.
pub fn scan(root: &Path, max_entries: usize) -> io::Result<ScanResult> {
    let mut found = Vec::new();
    let mut seen = 0usize;
    let mut truncated = false;
    let mut stack = vec![(root.to_path_buf(), String::new())];
    'walk: while let Some((dir, rel)) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            seen += 1;
            if seen > max_entries {
                truncated = true;
                break 'walk;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if rel.is_empty() && INTERNAL.iter().any(|i| i.eq_ignore_ascii_case(&name)) {
                continue;
            }
            let ft = entry.file_type()?;
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if ft.is_dir() {
                stack.push((entry.path(), child_rel));
            } else if ft.is_file() && name.contains(".sync-conflict-") {
                found.push(child_rel);
            }
        }
    }
    let conflicts = conflict::find_conflicts(found.iter().map(String::as_str))
        .into_iter()
        .map(|c| {
            let t = &c.timestamp;
            let size = |p: &str| {
                fs::symlink_metadata(root.join(p))
                    .ok()
                    .filter(|m| m.is_file())
                    .map(|m| m.len())
            };
            ConflictEntry {
                conflict_size: size(&c.conflict_path).unwrap_or(0),
                original_size: size(&c.original_path),
                when: format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    t.year, t.month, t.day, t.hour, t.minute, t.second
                ),
                device_short_id: c.device_short_id,
                conflict_path: c.conflict_path,
                original_path: c.original_path,
            }
        })
        .collect();
    Ok(ScanResult {
        conflicts,
        truncated,
    })
}

/// NTFS 8.3 short-name alias such as `REPORT~1.TXT`: on Windows it can
/// point at a different long-named file than the one shown, so the executor
/// refuses such components there.
pub fn is_short_name_alias(component: &str) -> bool {
    let (base, ext) = match component.rfind('.') {
        Some(i) => (&component[..i], Some(&component[i + 1..])),
        None => (component, None),
    };
    let Some(t) = base.rfind('~') else {
        return false;
    };
    let (stem, num) = (&base[..t], &base[t + 1..]);
    !stem.is_empty()
        && !stem.contains(['.', '~'])
        && !num.is_empty()
        && num.bytes().all(|b| b.is_ascii_digit())
        && base.len() <= 8
        && ext.is_none_or(|e| !e.is_empty() && e.len() <= 3 && !e.contains('~'))
}

/// Sets the archived copy's mtime to now: Syncthing's trashcan/simple
/// cleanup ages versions by mtime, and a rename keeps the old one.
fn touch(p: &Path) -> io::Result<()> {
    fs::File::options()
        .write(true)
        .open(p)?
        .set_modified(std::time::SystemTime::now())
}

/// `YYYYMMDD-HHMMSS`, Syncthing's version tag format.
fn valid_tag(tag: &str) -> bool {
    let b = tag.as_bytes();
    b.len() == 15
        && b[8] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 8 || c.is_ascii_digit())
}

/// `dir/name.ext` -> `dir/name~tag.ext` (Syncthing's `TagFilename`).
pub fn tag_filename(rel: &str, tag: &str) -> String {
    let (dir, name) = split_dir(rel);
    let (stem, ext) = split_ext(name);
    format!("{dir}{stem}~{tag}{ext}")
}

fn split_dir(p: &str) -> (&str, &str) {
    match p.rfind('/') {
        Some(i) => p.split_at(i + 1),
        None => ("", p),
    }
}

fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    }
}

/// `dir/name.ext` -> `dir/name 2.ext`, `dir/name 3.ext`, ...
fn numbered(rel: &str, n: u32) -> String {
    let (dir, name) = split_dir(rel);
    let (stem, ext) = split_ext(name);
    format!("{dir}{stem} {n}{ext}")
}

/// Joins a relative `/` path onto `root`, refusing anything that is not a
/// plain relative path or that passes through a symlink/junction. Missing
/// trailing components are allowed (they are checked once created).
fn confined(root: &Path, rel: &str) -> Result<PathBuf, Error> {
    let bad = || Error::UnsafePath(rel.to_string());
    if rel.is_empty() || rel.starts_with('/') || rel.contains(['\\', ':', '\0']) {
        return Err(bad());
    }
    let mut p = root.to_path_buf();
    for c in rel.split('/') {
        if c.is_empty() || c == "." || c == ".." {
            return Err(bad());
        }
        p.push(c);
        match fs::symlink_metadata(&p) {
            Ok(m) if m.file_type().is_symlink() => return Err(bad()),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(Error::Io {
                    step: format!("verificar {rel}"),
                    source: e,
                })
            }
        }
    }
    Ok(p)
}

/// The item at `rel` must exist and be a regular file (not a dir/symlink).
fn regular_file(root: &Path, rel: &str) -> Result<PathBuf, Error> {
    let p = confined(root, rel)?;
    match fs::symlink_metadata(&p) {
        Ok(m) if m.is_file() => Ok(p),
        Ok(_) => Err(Error::UnsafePath(rel.to_string())),
        Err(_) => Err(Error::NotFound(rel.to_string())),
    }
}

/// Creates the parent dirs of `rel` and re-checks them for symlinks.
fn ensure_parent(root: &Path, rel: &str) -> Result<PathBuf, Error> {
    let p = confined(root, rel)?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::Io {
            step: format!("criar pasta para {rel}"),
            source: e,
        })?;
    }
    confined(root, rel)
}

/// Atomic rename that fails with `AlreadyExists` instead of replacing.
pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    imp::rename_noreplace(from, to)
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::CString;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
        let f = CString::new(from.as_os_str().as_bytes())?;
        let t = CString::new(to.as_os_str().as_bytes())?;
        // SAFETY: both pointers are valid NUL-terminated C strings for the
        // duration of the call.
        let r = unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) };
        if r == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::CString;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
        let f = CString::new(from.as_os_str().as_bytes())?;
        let t = CString::new(to.as_os_str().as_bytes())?;
        // SAFETY: valid NUL-terminated strings (absolute paths).
        let r = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                f.as_ptr(),
                libc::AT_FDCWD,
                t.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if r == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
        let (f, t) = (wide(from), wide(to));
        // Flags 0: no MOVEFILE_REPLACE_EXISTING, no copy across volumes.
        // SAFETY: both buffers are NUL-terminated UTF-16 strings.
        let ok = unsafe { MoveFileExW(f.as_ptr(), t.as_ptr(), 0) };
        if ok != 0 {
            Ok(())
        } else {
            let e = io::Error::last_os_error();
            // ERROR_FILE_EXISTS (80) / ERROR_ALREADY_EXISTS (183)
            match e.raw_os_error() {
                Some(80) | Some(183) => Err(io::ErrorKind::AlreadyExists.into()),
                _ => Err(e),
            }
        }
    }
}

/// Moves `from` to `to` (both relative), never replacing. With `unique`,
/// tries `to`, `to 2`, `to 3`... Returns the relative destination used.
fn move_file(root: &Path, from: &str, to: &str, unique: bool) -> Result<String, Error> {
    let src = regular_file(root, from)?;
    for n in 1..=999u32 {
        let candidate = if n == 1 {
            to.to_string()
        } else {
            numbered(to, n)
        };
        let dst = ensure_parent(root, &candidate)?;
        match rename_noreplace(&src, &dst) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && unique => continue,
            Err(e) => {
                return Err(Error::Io {
                    step: format!("mover {from} para {candidate}"),
                    source: e,
                })
            }
        }
    }
    Err(Error::Io {
        step: format!("mover {from}"),
        source: io::ErrorKind::AlreadyExists.into(),
    })
}

/// Resolves the conflict copy at `conflict_path` (relative to `root`).
/// `tag` is the local time `YYYYMMDD-HHMMSS` used to name archived copies.
pub fn resolve(
    root: &Path,
    conflict_path: &str,
    resolution: Resolution,
    tag: &str,
) -> Result<Outcome, Error> {
    let c: ConflictFile = conflict::parse(conflict_path)
        .ok_or_else(|| Error::NotAConflict(conflict_path.to_string()))?;
    if !valid_tag(tag) {
        return Err(Error::InvalidTag(tag.to_string()));
    }
    if cfg!(windows)
        && c.conflict_path
            .split('/')
            .chain(c.original_path.split('/'))
            .any(is_short_name_alias)
    {
        return Err(Error::UnsafePath(conflict_path.to_string()));
    }
    let root = fs::canonicalize(root).map_err(|e| Error::Io {
        step: "abrir a pasta".into(),
        source: e,
    })?;
    let actions = conflict::plan(&c, resolution);
    // Validate every source up front so we fail before touching anything.
    for a in &actions {
        match a {
            FsAction::ArchiveToVersions { path } => {
                regular_file(&root, path)?;
                confined(&root, VERSIONS_DIR)?;
            }
            FsAction::Rename { from, .. } | FsAction::RenameUnique { from, .. } => {
                regular_file(&root, from)?;
            }
        }
    }
    let mut done: Vec<(String, String)> = Vec::new();
    let mut out = Outcome::default();
    for a in &actions {
        let step = match a {
            FsAction::ArchiveToVersions { path } => {
                let dest = format!("{VERSIONS_DIR}/{}", tag_filename(path, tag));
                move_file(&root, path, &dest, true).map(|d| {
                    // Best effort: failing to touch only affects cleanup age.
                    if let Ok(p) = confined(&root, &d) {
                        let _ = touch(&p);
                    }
                    out.archived.push(d.clone());
                    (path.clone(), d)
                })
            }
            FsAction::Rename { from, to } => move_file(&root, from, to, false).map(|d| {
                out.renamed_to.push(d.clone());
                (from.clone(), d)
            }),
            FsAction::RenameUnique { from, to } => move_file(&root, from, to, true).map(|d| {
                out.renamed_to.push(d.clone());
                (from.clone(), d)
            }),
        };
        match step {
            Ok(m) => done.push(m),
            Err(e) => {
                let mut stranded = Vec::new();
                for (orig, moved) in done.iter().rev() {
                    let undone = match (confined(&root, moved), confined(&root, orig)) {
                        (Ok(s), Ok(d)) => rename_noreplace(&s, &d).is_ok(),
                        _ => false,
                    };
                    if !undone {
                        stranded.push((moved.clone(), orig.clone()));
                    }
                }
                return Err(if stranded.is_empty() {
                    e
                } else {
                    Error::RollbackFailed {
                        cause: Box::new(e),
                        stranded,
                    }
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const TAG: &str = "20261116-150000";
    const CF: &str = "docs/report.sync-conflict-20261116-143005-ABCDEF2.txt";

    fn tmp() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "tf-resolve-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("docs")).unwrap();
        d
    }

    fn setup() -> PathBuf {
        let d = tmp();
        fs::write(d.join("docs/report.txt"), "mine").unwrap();
        fs::write(d.join(CF), "other").unwrap();
        d
    }

    fn read(d: &Path, rel: &str) -> String {
        fs::read_to_string(d.join(rel)).unwrap()
    }

    #[test]
    fn tag_filename_matches_syncthing() {
        assert_eq!(tag_filename("a/b.txt", TAG), "a/b~20261116-150000.txt");
        assert_eq!(tag_filename("Makefile", TAG), "Makefile~20261116-150000");
        assert_eq!(tag_filename(".bashrc", TAG), ".bashrc~20261116-150000");
    }

    #[test]
    fn keep_mine_archives_the_conflict_copy() {
        let d = setup();
        let o = resolve(&d, CF, Resolution::KeepMine, TAG).unwrap();
        assert_eq!(read(&d, "docs/report.txt"), "mine");
        assert!(!d.join(CF).exists());
        assert_eq!(
            o.archived,
            vec![
                ".stversions/docs/report.sync-conflict-20261116-143005-ABCDEF2~20261116-150000.txt"
            ]
        );
        assert_eq!(read(&d, &o.archived[0]), "other");
    }

    #[test]
    fn keep_other_archives_mine_and_moves_other_in_place() {
        let d = setup();
        let o = resolve(&d, CF, Resolution::KeepOther, TAG).unwrap();
        assert_eq!(read(&d, "docs/report.txt"), "other");
        assert!(!d.join(CF).exists());
        assert_eq!(
            o.archived,
            vec![".stversions/docs/report~20261116-150000.txt"]
        );
        assert_eq!(read(&d, &o.archived[0]), "mine");
        assert_eq!(o.renamed_to, vec!["docs/report.txt"]);
    }

    #[test]
    fn keep_both_picks_a_unique_name_and_never_overwrites() {
        let d = setup();
        let want = "docs/report (conflito ABCDEF2 2026-11-16 14.30.05).txt";
        fs::write(d.join(want), "already here").unwrap();
        let o = resolve(&d, CF, Resolution::KeepBoth, TAG).unwrap();
        assert_eq!(read(&d, want), "already here");
        let got = "docs/report (conflito ABCDEF2 2026-11-16 14.30.05) 2.txt";
        assert_eq!(o.renamed_to, vec![got]);
        assert_eq!(read(&d, got), "other");
        assert_eq!(read(&d, "docs/report.txt"), "mine");
    }

    #[test]
    fn archive_name_collision_gets_numbered() {
        let d = setup();
        fs::create_dir_all(d.join(".stversions/docs")).unwrap();
        fs::write(d.join(".stversions/docs/report~20261116-150000.txt"), "old").unwrap();
        let o = resolve(&d, CF, Resolution::KeepOther, TAG).unwrap();
        assert_eq!(
            o.archived,
            vec![".stversions/docs/report~20261116-150000 2.txt"]
        );
        assert_eq!(
            read(&d, ".stversions/docs/report~20261116-150000.txt"),
            "old"
        );
    }

    #[test]
    fn rename_noreplace_refuses_existing_target() {
        let d = tmp();
        fs::write(d.join("a"), "a").unwrap();
        fs::write(d.join("b"), "b").unwrap();
        let e = rename_noreplace(&d.join("a"), &d.join("b")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(read(&d, "b"), "b");
        assert_eq!(read(&d, "a"), "a");
    }

    #[test]
    fn rejects_non_conflict_and_traversal_names() {
        let d = setup();
        for bad in [
            "docs/report.txt",
            "../x.sync-conflict-20261116-143005-ABCDEF2.txt",
            "/etc/x.sync-conflict-20261116-143005-ABCDEF2.txt",
            ".stversions/x.sync-conflict-20261116-143005-ABCDEF2.txt",
        ] {
            assert!(
                matches!(
                    resolve(&d, bad, Resolution::KeepMine, TAG),
                    Err(Error::NotAConflict(_))
                ),
                "{bad}"
            );
        }
        assert!(matches!(
            resolve(&d, CF, Resolution::KeepMine, "../../x"),
            Err(Error::InvalidTag(_))
        ));
        assert!(d.join(CF).exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_paths_through_symlinked_dirs() {
        let d = tmp();
        let outside = tmp();
        fs::write(outside.join("report.txt"), "secret").unwrap();
        fs::write(
            outside.join("report.sync-conflict-20261116-143005-ABCDEF2.txt"),
            "x",
        )
        .unwrap();
        std::os::unix::fs::symlink(&outside, d.join("link")).unwrap();
        let cf = "link/report.sync-conflict-20261116-143005-ABCDEF2.txt";
        for r in [
            Resolution::KeepMine,
            Resolution::KeepOther,
            Resolution::KeepBoth,
        ] {
            assert!(
                matches!(resolve(&d, cf, r, TAG), Err(Error::UnsafePath(_))),
                "{r:?}"
            );
        }
        assert_eq!(read(&outside, "report.txt"), "secret");
        assert!(!d.join(".stversions").exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_versions_dir() {
        let d = setup();
        let outside = tmp();
        std::os::unix::fs::symlink(&outside, d.join(".stversions")).unwrap();
        assert!(matches!(
            resolve(&d, CF, Resolution::KeepMine, TAG),
            Err(Error::UnsafePath(_))
        ));
        assert!(d.join(CF).exists());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1); // only "docs"
    }

    #[cfg(unix)]
    #[test]
    fn refuses_when_conflict_copy_is_a_symlink() {
        let d = tmp();
        let outside = tmp();
        fs::write(outside.join("t"), "secret").unwrap();
        fs::write(d.join("docs/report.txt"), "mine").unwrap();
        std::os::unix::fs::symlink(outside.join("t"), d.join(CF)).unwrap();
        assert!(matches!(
            resolve(&d, CF, Resolution::KeepOther, TAG),
            Err(Error::UnsafePath(_))
        ));
        assert_eq!(read(&d, "docs/report.txt"), "mine");
    }

    #[test]
    fn missing_original_fails_before_touching_anything() {
        let d = tmp();
        fs::write(d.join(CF), "other").unwrap();
        assert!(matches!(
            resolve(&d, CF, Resolution::KeepOther, TAG),
            Err(Error::NotFound(_))
        ));
        assert_eq!(read(&d, CF), "other");
    }

    #[test]
    fn scan_finds_conflicts_and_skips_internal_dirs() {
        let d = setup();
        fs::create_dir_all(d.join(".stversions")).unwrap();
        fs::write(
            d.join(".stversions/a.sync-conflict-20261116-143005-ABCDEF2.txt"),
            "v",
        )
        .unwrap();
        fs::write(
            d.join("lonely.sync-conflict-20261116-143005-ABCDEF2.md"),
            "zz",
        )
        .unwrap();
        let r = scan(&d, 10_000).unwrap();
        assert!(!r.truncated);
        let paths: Vec<_> = r
            .conflicts
            .iter()
            .map(|c| c.conflict_path.as_str())
            .collect();
        assert_eq!(
            paths,
            vec![CF, "lonely.sync-conflict-20261116-143005-ABCDEF2.md"]
        );
        assert_eq!(r.conflicts[0].original_size, Some(4));
        assert_eq!(r.conflicts[0].conflict_size, 5);
        assert_eq!(r.conflicts[0].when, "2026-11-16 14:30:05");
        assert_eq!(r.conflicts[1].original_size, None);
    }

    #[test]
    fn scan_reports_truncation() {
        let d = tmp();
        for i in 0..20 {
            fs::write(d.join(format!("f{i}")), "").unwrap();
        }
        assert!(scan(&d, 5).unwrap().truncated);
    }

    #[test]
    fn archived_copy_gets_a_fresh_mtime_so_cleanup_counts_from_now() {
        let d = setup();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 86_400);
        fs::File::options()
            .write(true)
            .open(d.join(CF))
            .unwrap()
            .set_modified(old)
            .unwrap();
        let o = resolve(&d, CF, Resolution::KeepMine, TAG).unwrap();
        let m = fs::metadata(d.join(&o.archived[0]))
            .unwrap()
            .modified()
            .unwrap();
        let age = std::time::SystemTime::now()
            .duration_since(m)
            .unwrap_or_default();
        assert!(age.as_secs() < 60, "archived mtime is {age:?} old");
    }

    #[test]
    fn detects_windows_short_name_aliases() {
        for alias in ["REPORT~1.TXT", "PROGR~12", "a~1.b"] {
            assert!(is_short_name_alias(alias), "{alias}");
        }
        for ok in [
            "report.txt",
            "notas~rascunho.md",
            "file~1x.txt",
            "~1.txt",
            "verylongname~1.txt",
        ] {
            assert!(!is_short_name_alias(ok), "{ok}");
        }
    }

    #[test]
    fn scan_skips_internal_dirs_case_insensitively() {
        let d = tmp();
        fs::create_dir_all(d.join(".StVersions")).unwrap();
        fs::write(
            d.join(".StVersions/a.sync-conflict-20261116-143005-ABCDEF2.txt"),
            "v",
        )
        .unwrap();
        assert!(scan(&d, 1000).unwrap().conflicts.is_empty());
    }
}
