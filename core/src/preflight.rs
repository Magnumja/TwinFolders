//! Pre-share scan (SPEC R9): warn about things that break Mac <-> Windows
//! syncing instead of failing silently.
//!
//! [`scan`] walks a folder (iteratively, never following symlinks) and builds
//! a [`Report`] with: names Windows cannot store, case-only collisions,
//! cloud-synced location, cloud-only placeholder files and symlinks.
//! Directories ignored by the default `.stignore` (`.git`, `node_modules`,
//! `.stversions`, `.stfolder`) are listed but not descended into.
//!
//! Placeholder detection ([`is_placeholder`]) is a heuristic based on
//! platform file flags/attributes and may miss providers that do not set them.

use std::fs::Metadata;
use std::path::Path;

use serde::Serialize;

use crate::names::{self, NameIssue};

/// Stop walking after this many entries.
pub const MAX_ENTRIES: usize = 50_000;
/// Cap on list sizes in the report.
pub const MAX_REPORTED: usize = 50;

/// Directories not descended into (ignored by the default `.stignore`).
const SKIP_DIRS: [&str; 4] = [".git", "node_modules", ".stversions", ".stfolder"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudProvider {
    ICloud,
    OneDrive,
    Dropbox,
    GoogleDrive,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameWarning {
    pub path: String,
    /// Human-readable problem (pt-BR).
    pub problem: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub entries_scanned: usize,
    /// Hit [`MAX_ENTRIES`] before finishing.
    pub truncated: bool,
    /// Capped at [`MAX_REPORTED`].
    pub invalid_names: Vec<NameWarning>,
    pub invalid_names_total: usize,
    /// Capped at [`MAX_REPORTED`] groups.
    pub case_collisions: Vec<Vec<String>>,
    pub cloud: Option<CloudProvider>,
    pub cloud_only_files: usize,
    pub symlinks: usize,
}

impl Report {
    /// Invalid names or case collisions: the sync would lose or clash files.
    pub fn has_blocking_issues(&self) -> bool {
        self.invalid_names_total > 0 || !self.case_collisions.is_empty()
    }

    /// Nothing at all to warn about.
    pub fn is_clean(&self) -> bool {
        !self.has_blocking_issues()
            && !self.truncated
            && self.cloud.is_none()
            && self.cloud_only_files == 0
            && self.symlinks == 0
    }
}

/// Cloud-synced location detected purely from the path text (any OS).
pub fn cloud_provider_for(path: &Path) -> Option<CloudProvider> {
    let norm = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let with_slash = format!("{}/", norm.trim_end_matches('/'));

    if with_slash.contains("/library/mobile documents/") {
        return Some(CloudProvider::ICloud);
    }
    if let Some(i) = with_slash.find("/library/cloudstorage/") {
        let rest = &with_slash[i + "/library/cloudstorage/".len()..];
        let first = rest.split('/').next().unwrap_or("");
        return Some(if first.starts_with("onedrive") {
            CloudProvider::OneDrive
        } else if first.starts_with("dropbox") {
            CloudProvider::Dropbox
        } else if first.starts_with("googledrive") {
            CloudProvider::GoogleDrive
        } else {
            CloudProvider::Other
        });
    }
    for comp in norm.split('/') {
        if comp.starts_with("onedrive") {
            return Some(CloudProvider::OneDrive);
        }
        match comp {
            "dropbox" => return Some(CloudProvider::Dropbox),
            "google drive" | "my drive" => return Some(CloudProvider::GoogleDrive),
            "iclouddrive" => return Some(CloudProvider::ICloud),
            _ => {}
        }
    }
    None
}

fn describe(issue: &NameIssue) -> String {
    match issue {
        NameIssue::InvalidChar(c) if (*c as u32) < 0x20 => format!(
            "caractere de controle (U+{:04X}) não é permitido no Windows",
            *c as u32
        ),
        NameIssue::InvalidChar(c) => format!("caractere \"{c}\" não é permitido no Windows"),
        NameIssue::ReservedName(n) => format!("\"{n}\" é um nome reservado do Windows"),
        NameIssue::TrailingDotOrSpace => "termina com ponto ou espaço (o Windows remove)".into(),
        NameIssue::PathTooLong(n) => format!("caminho longo demais ({n} caracteres)"),
    }
}

/// Pure report builder from `/`-separated relative paths (files and dirs).
/// `cloud`, `cloud_only_files` and `symlinks` are left for the caller.
fn build_report(rel_paths: &[String], truncated: bool) -> Report {
    let mut invalid_names = Vec::new();
    let mut total = 0;
    for rel in rel_paths {
        for pi in names::validate_path(rel) {
            // Issues on a parent prefix are reported by that parent's own entry.
            if pi.path != *rel {
                continue;
            }
            total += 1;
            if invalid_names.len() < MAX_REPORTED {
                invalid_names.push(NameWarning {
                    path: pi.path,
                    problem: describe(&pi.issue),
                });
            }
        }
    }
    let mut case_collisions = names::case_collisions(rel_paths.iter().map(String::as_str));
    case_collisions.truncate(MAX_REPORTED);
    Report {
        entries_scanned: rel_paths.len(),
        truncated,
        invalid_names,
        invalid_names_total: total,
        case_collisions,
        cloud: None,
        cloud_only_files: 0,
        symlinks: 0,
    }
}

/// Heuristic: file content is not stored locally (cloud placeholder).
/// macOS: `SF_DATALESS`. Windows: recall/offline attributes.
#[cfg(target_os = "macos")]
fn is_placeholder(meta: &Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    const SF_DATALESS: u32 = 0x4000_0000;
    meta.st_flags() & SF_DATALESS != 0
}

#[cfg(windows)]
fn is_placeholder(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x0004_0000;
    meta.file_attributes() & (RECALL_ON_DATA_ACCESS | OFFLINE | RECALL_ON_OPEN) != 0
}

#[cfg(not(any(target_os = "macos", windows)))]
fn is_placeholder(_meta: &Metadata) -> bool {
    false
}

fn scan_with_limit(root: &Path, limit: usize) -> std::io::Result<Report> {
    let mut rels: Vec<String> = Vec::new();
    let mut symlinks = 0;
    let mut cloud_only = 0;
    let mut truncated = false;

    // A root error propagates; subdirectory errors are skipped.
    let mut first = Some(std::fs::read_dir(root)?);
    let mut stack: Vec<(std::path::PathBuf, String)> = vec![(root.to_path_buf(), String::new())];

    'walk: while let Some((dir, prefix)) = stack.pop() {
        let iter = match first.take() {
            Some(it) => it,
            None => match std::fs::read_dir(&dir) {
                Ok(it) => it,
                Err(_) => continue,
            },
        };
        for entry in iter {
            let Ok(entry) = entry else { continue };
            if rels.len() >= limit {
                truncated = true;
                break 'walk;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            rels.push(rel.clone());
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            let ft = meta.file_type();
            if ft.is_symlink() {
                symlinks += 1;
            } else if ft.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push((entry.path(), rel));
                }
            } else if is_placeholder(&meta) {
                cloud_only += 1;
            }
        }
    }

    let mut report = build_report(&rels, truncated);
    report.cloud = cloud_provider_for(root).or_else(|| {
        root.canonicalize()
            .ok()
            .and_then(|p| cloud_provider_for(&p))
    });
    report.cloud_only_files = cloud_only;
    report.symlinks = symlinks;
    Ok(report)
}

/// Scan `root`. A missing/unreadable root is an error; unreadable
/// subdirectories are skipped silently.
pub fn scan(root: &Path) -> std::io::Result<Report> {
    scan_with_limit(root, MAX_ENTRIES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn p(s: &str) -> Option<CloudProvider> {
        cloud_provider_for(Path::new(s))
    }

    #[test]
    fn cloud_icloud_mac() {
        assert_eq!(
            p("/Users/me/Library/Mobile Documents/com~apple~CloudDocs/Work"),
            Some(CloudProvider::ICloud)
        );
    }

    #[test]
    fn cloud_file_provider_mac() {
        assert_eq!(
            p("/Users/me/Library/CloudStorage/OneDrive-Empresa/x"),
            Some(CloudProvider::OneDrive)
        );
        assert_eq!(
            p("/Users/me/Library/CloudStorage/Dropbox/x"),
            Some(CloudProvider::Dropbox)
        );
        assert_eq!(
            p("/Users/me/Library/CloudStorage/GoogleDrive-a@b.com/My Drive"),
            Some(CloudProvider::GoogleDrive)
        );
        assert_eq!(
            p("/Users/me/Library/CloudStorage/Box-Box/x"),
            Some(CloudProvider::Other)
        );
    }

    #[test]
    fn cloud_windows_style() {
        assert_eq!(
            p("C:\\Users\\me\\OneDrive\\Docs"),
            Some(CloudProvider::OneDrive)
        );
        assert_eq!(
            p("C:\\Users\\me\\OneDrive - Empresa\\Docs"),
            Some(CloudProvider::OneDrive)
        );
        assert_eq!(p("C:\\Users\\me\\Dropbox\\a"), Some(CloudProvider::Dropbox));
        assert_eq!(
            p("C:\\Users\\me\\Google Drive\\a"),
            Some(CloudProvider::GoogleDrive)
        );
        assert_eq!(p("G:\\My Drive\\a"), Some(CloudProvider::GoogleDrive));
        assert_eq!(
            p("C:\\Users\\me\\iCloudDrive\\a"),
            Some(CloudProvider::ICloud)
        );
        assert_eq!(
            p("c:\\users\\me\\onedrive\\a"),
            Some(CloudProvider::OneDrive)
        );
    }

    #[test]
    fn cloud_negatives() {
        assert_eq!(p("/Users/me/Documents"), None);
        assert_eq!(p("C:\\Users\\me\\Desktop"), None);
        assert_eq!(p("/Users/me/Library/Application Support"), None);
        assert_eq!(p("/Users/me/notdropbox/a"), None);
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn build_report_clean() {
        let r = build_report(&s(&["a", "a/b.txt", "c.md"]), false);
        assert_eq!(r.entries_scanned, 3);
        assert!(r.invalid_names.is_empty());
        assert_eq!(r.invalid_names_total, 0);
        assert!(r.case_collisions.is_empty());
        assert!(r.is_clean());
        assert!(!r.has_blocking_issues());
    }

    #[test]
    fn build_report_invalid_names_text() {
        let r = build_report(&s(&["a:b.txt", "CON.txt", "trail. ", "x/y\u{1}z"]), false);
        let find = |path: &str| {
            r.invalid_names
                .iter()
                .find(|w| w.path == path)
                .unwrap_or_else(|| panic!("missing {path}"))
                .problem
                .clone()
        };
        assert_eq!(
            find("a:b.txt"),
            "caractere \":\" não é permitido no Windows"
        );
        assert_eq!(find("CON.txt"), "\"CON\" é um nome reservado do Windows");
        assert_eq!(
            find("trail. "),
            "termina com ponto ou espaço (o Windows remove)"
        );
        assert!(find("x/y\u{1}z").starts_with("caractere"));
        assert_eq!(r.invalid_names_total, 4);
        assert!(r.has_blocking_issues());
        assert!(!r.is_clean());
    }

    #[test]
    fn build_report_dir_issue_not_duplicated_for_children() {
        let r = build_report(&s(&["bad?", "bad?/ok.txt", "bad?/deep/ok2.txt"]), false);
        assert_eq!(r.invalid_names_total, 1);
        assert_eq!(r.invalid_names[0].path, "bad?");
    }

    #[test]
    fn build_report_path_too_long() {
        let long = "a".repeat(250);
        let r = build_report(std::slice::from_ref(&long), false);
        assert_eq!(r.invalid_names_total, 1);
        assert_eq!(r.invalid_names[0].path, long);
        assert_eq!(
            r.invalid_names[0].problem,
            "caminho longo demais (250 caracteres)"
        );
    }

    #[test]
    fn build_report_case_collisions() {
        let r = build_report(&s(&["Dir", "Dir/a.txt", "dir", "dir/b.txt", "x"]), false);
        assert_eq!(
            r.case_collisions,
            vec![vec!["Dir".to_string(), "dir".into()]]
        );
        assert!(r.has_blocking_issues());
        assert!(!r.is_clean());
    }

    #[test]
    fn build_report_caps_lists_but_keeps_totals() {
        let bad: Vec<String> = (0..MAX_REPORTED + 10).map(|i| format!("f{i}?")).collect();
        let r = build_report(&bad, false);
        assert_eq!(r.invalid_names.len(), MAX_REPORTED);
        assert_eq!(r.invalid_names_total, MAX_REPORTED + 10);

        let mut paths = Vec::new();
        for i in 0..MAX_REPORTED + 5 {
            paths.push(format!("A{i}"));
            paths.push(format!("a{i}"));
        }
        let r = build_report(&paths, false);
        assert_eq!(r.case_collisions.len(), MAX_REPORTED);
    }

    #[test]
    fn report_flags() {
        let base = build_report(&[], false);
        assert!(base.is_clean());
        let mut r = base.clone();
        r.truncated = true;
        assert!(!r.is_clean());
        assert!(!r.has_blocking_issues());
        let mut r = base.clone();
        r.cloud = Some(CloudProvider::Dropbox);
        assert!(!r.is_clean());
        assert!(!r.has_blocking_issues());
        let mut r = base.clone();
        r.cloud_only_files = 1;
        assert!(!r.is_clean());
        let mut r = base;
        r.symlinks = 1;
        assert!(!r.is_clean());
        assert!(!r.has_blocking_issues());
    }

    #[test]
    fn report_serializes_camel_case() {
        let mut r = build_report(&s(&["a"]), false);
        r.cloud = Some(CloudProvider::GoogleDrive);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["entriesScanned"], 1);
        assert_eq!(v["invalidNamesTotal"], 0);
        assert_eq!(v["cloudOnlyFiles"], 0);
        assert_eq!(v["cloud"], "googleDrive");
    }

    // ---- filesystem tests ----

    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Tmp {
            static N: AtomicUsize = AtomicUsize::new(0);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let d = std::env::temp_dir().join(format!(
                "twinfolders-preflight-{}-{}-{}",
                std::process::id(),
                nanos,
                N.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&d).unwrap();
            Tmp(d)
        }
        fn touch(&self, rel: &str) {
            let f = self.0.join(rel);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b"x").unwrap();
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn scan_missing_root_is_err() {
        let t = Tmp::new();
        assert!(scan(&t.0.join("nope")).is_err());
    }

    #[test]
    fn scan_clean_tree() {
        let t = Tmp::new();
        t.touch("ok.txt");
        t.touch("sub/inner.txt");
        let r = scan(&t.0).unwrap();
        assert_eq!(r.entries_scanned, 3);
        assert!(!r.truncated);
        assert!(r.is_clean(), "{r:?}");
    }

    #[cfg(unix)]
    #[test]
    fn scan_reports_bad_names() {
        let t = Tmp::new();
        t.touch("clean.txt");
        t.touch("bad?.txt");
        t.touch("CON.txt");
        t.touch("trailing. ");
        t.touch("dir/star*.txt");
        let r = scan(&t.0).unwrap();
        let mut paths: Vec<&str> = r.invalid_names.iter().map(|w| w.path.as_str()).collect();
        paths.sort();
        assert_eq!(
            paths,
            vec!["CON.txt", "bad?.txt", "dir/star*.txt", "trailing. "]
        );
        assert_eq!(r.invalid_names_total, 4);
        assert!(r.has_blocking_issues());
    }

    #[cfg(unix)]
    #[test]
    fn scan_skips_ignored_dirs() {
        let t = Tmp::new();
        t.touch("node_modules/bad?.txt");
        t.touch(".git/bad?.txt");
        t.touch(".stversions/bad?.txt");
        t.touch(".stfolder/bad?.txt");
        t.touch("ok.txt");
        let r = scan(&t.0).unwrap();
        assert_eq!(r.invalid_names_total, 0, "{r:?}");
        assert!(r.invalid_names.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn scan_counts_symlinks_without_following() {
        let t = Tmp::new();
        t.touch("real/bad?.txt");
        std::os::unix::fs::symlink(t.0.join("real"), t.0.join("link")).unwrap();
        std::os::unix::fs::symlink(t.0.join("missing"), t.0.join("dangling")).unwrap();
        let r = scan(&t.0).unwrap();
        assert_eq!(r.symlinks, 2);
        // link/bad?.txt must not be visited: only real/bad?.txt reported
        assert_eq!(r.invalid_names_total, 1);
        assert_eq!(r.invalid_names[0].path, "real/bad?.txt");
        assert!(!r.is_clean());
    }

    #[test]
    fn scan_case_collision_when_fs_is_case_sensitive() {
        let t = Tmp::new();
        t.touch("Readme.md");
        if t.0.join("README.md").exists() {
            eprintln!("case-insensitive filesystem: skipping");
            return;
        }
        t.touch("README.md");
        let r = scan(&t.0).unwrap();
        assert_eq!(
            r.case_collisions,
            vec![vec!["README.md".to_string(), "Readme.md".into()]]
        );
    }

    #[test]
    fn scan_respects_limit_and_sets_truncated() {
        let t = Tmp::new();
        for i in 0..10 {
            t.touch(&format!("f{i}.txt"));
        }
        let r = scan_with_limit(&t.0, 4).unwrap();
        assert!(r.truncated);
        assert_eq!(r.entries_scanned, 4);
        let r = scan_with_limit(&t.0, 10).unwrap();
        assert!(!r.truncated);
        assert_eq!(r.entries_scanned, 10);
    }

    #[cfg(unix)]
    #[test]
    fn scan_skips_unreadable_subdir() {
        use std::os::unix::fs::PermissionsExt;
        let t = Tmp::new();
        t.touch("ok.txt");
        t.touch("locked/a.txt");
        let locked = t.0.join("locked");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let r = scan(&t.0);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(r.is_ok());
    }

    #[test]
    fn scan_sets_cloud_from_root_path() {
        let t = Tmp::new();
        let root = t.0.join("Dropbox");
        std::fs::create_dir_all(&root).unwrap();
        let r = scan(&root).unwrap();
        assert_eq!(r.cloud, Some(CloudProvider::Dropbox));
    }
}
