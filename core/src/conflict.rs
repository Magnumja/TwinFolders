//! Syncthing conflict-copy detection and resolution planning.
//!
//! Syncthing names conflict copies
//! `<stem>.sync-conflict-<YYYYMMDD>-<HHMMSS>-<SHORTID>[.<ext>]`. "Mine" is the
//! file at `original_path` on this machine; "other" is the conflict copy.

const MARKER: &str = ".sync-conflict-";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictFile {
    pub conflict_path: String,
    pub original_path: String,
    pub timestamp: Timestamp,
    pub device_short_id: String,
}

fn digits(s: &str) -> Option<u32> {
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}

fn is_leap(y: u32) -> bool {
    y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400))
}

fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(y) => 29,
        _ => 28,
    }
}

/// Syncthing's own control files/dirs must never be renamed/archived by a
/// plan. Compared case-insensitively: macOS and Windows file systems are
/// case-insensitive by default.
const CONTROL_NAMES: &[&str] = &[".stignore", ".stfolder", ".stversions"];

/// Defense in depth; the executor must still canonicalize real paths
/// (slice 5). Unicode uppercase catches NTFS-equivalent spellings such as
/// `.ſtignore` (U+017F uppercases to `S`), and `.ST…~N` catches 8.3 short
/// names like `.STIGN~1`.
fn is_control_component(c: &str) -> bool {
    let up = c.to_uppercase();
    CONTROL_NAMES.iter().any(|n| n.to_uppercase() == up)
        || (up.starts_with(".ST") && up.contains('~'))
}

/// Peer-controlled names reach the conflict planner, so only plain relative
/// `/`-separated paths are accepted: no absolute paths, empty/`.`/`..`
/// components, backslashes or colons (traversal/drive letters on Windows),
/// NUL, components ending in `.` or space (Windows silently strips them, so
/// `.stversions.` == `.stversions`), or anything under a control dir.
fn is_safe_relative(path: &str) -> bool {
    !path.starts_with('/')
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|c| {
            !c.is_empty()
                && c != "."
                && c != ".."
                && !c.ends_with(['.', ' '])
                && !is_control_component(c)
        })
}

/// Parses a path as a Syncthing conflict copy; `None` for anything else,
/// including malformed markers.
pub fn parse(path: &str) -> Option<ConflictFile> {
    if !is_safe_relative(path) {
        return None;
    }
    let (dir, name) = match path.rfind('/') {
        Some(i) => path.split_at(i + 1),
        None => ("", path),
    };
    let idx = name.rfind(MARKER)?;
    let stem = &name[..idx];
    // "." / ".." would make original_path point outside the file's directory.
    if stem.is_empty() || stem == "." || stem == ".." {
        return None;
    }
    let rest = &name[idx + MARKER.len()..];
    // YYYYMMDD-HHMMSS-SHORTID = 8 + 1 + 6 + 1 + 7 bytes, all ASCII; checking
    // ASCII first makes the fixed-offset slicing below panic-free.
    let meta = rest.get(..23).filter(|m| m.is_ascii())?;
    let ext = &rest[23..];
    if !ext.is_empty() && (!ext.starts_with('.') || ext.len() == 1) {
        return None;
    }
    let b = meta.as_bytes();
    if b[8] != b'-' || b[15] != b'-' {
        return None;
    }
    let id = &meta[16..];
    if !id
        .bytes()
        .all(|c| c.is_ascii_uppercase() || (b'2'..=b'7').contains(&c))
    {
        return None;
    }
    let year = digits(&meta[0..4])?;
    let month = digits(&meta[4..6])?;
    let day = digits(&meta[6..8])?;
    let hour = digits(&meta[9..11])?;
    let minute = digits(&meta[11..13])?;
    let second = digits(&meta[13..15])?;
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let original_name = format!("{stem}{ext}");
    if is_control_component(&original_name) || original_name.ends_with(['.', ' ']) {
        return None;
    }
    Some(ConflictFile {
        conflict_path: path.to_string(),
        original_path: format!("{dir}{stem}{ext}"),
        timestamp: Timestamp {
            year: year as u16,
            month: month as u8,
            day: day as u8,
            hour: hour as u8,
            minute: minute as u8,
            second: second as u8,
        },
        device_short_id: id.to_string(),
    })
}

/// Returns every conflict copy among `paths`, sorted by original path then
/// timestamp.
pub fn find_conflicts<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<ConflictFile> {
    let mut v: Vec<ConflictFile> = paths.into_iter().filter_map(parse).collect();
    v.sort_by(|a, b| {
        let key = |c: &ConflictFile| {
            let t = &c.timestamp;
            (t.year, t.month, t.day, t.hour, t.minute, t.second)
        };
        a.original_path
            .cmp(&b.original_path)
            .then_with(|| key(a).cmp(&key(b)))
            .then_with(|| a.conflict_path.cmp(&b.conflict_path))
    });
    v
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    KeepMine,
    KeepOther,
    KeepBoth,
}

/// A filesystem step for the app to perform.
///
/// Invariant: there is deliberately no delete variant. Data is only ever
/// archived (the app moves it into Syncthing's `.stversions`), so no plan can
/// lose a file without a recoverable copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsAction {
    ArchiveToVersions { path: String },
    Rename { from: String, to: String },
}

/// Plans the steps to resolve `conflict`. Never produces a delete: every path
/// whose content would disappear is archived first.
pub fn plan(conflict: &ConflictFile, resolution: Resolution) -> Vec<FsAction> {
    match resolution {
        Resolution::KeepMine => vec![FsAction::ArchiveToVersions {
            path: conflict.conflict_path.clone(),
        }],
        Resolution::KeepOther => vec![
            FsAction::ArchiveToVersions {
                path: conflict.original_path.clone(),
            },
            FsAction::Rename {
                from: conflict.conflict_path.clone(),
                to: conflict.original_path.clone(),
            },
        ],
        Resolution::KeepBoth => {
            let t = &conflict.timestamp;
            let p = &conflict.original_path;
            let (dir, name) = match p.rfind('/') {
                Some(i) => p.split_at(i + 1),
                None => ("", p.as_str()),
            };
            let (stem, ext) = match name.rfind('.') {
                Some(i) if i > 0 => name.split_at(i),
                _ => (name, ""),
            };
            vec![FsAction::Rename {
                from: conflict.conflict_path.clone(),
                to: format!(
                    "{dir}{stem} (conflito {} {:04}-{:02}-{:02} {:02}.{:02}.{:02}){ext}",
                    conflict.device_short_id, t.year, t.month, t.day, t.hour, t.minute, t.second
                ),
            }]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Timestamp {
        Timestamp {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }

    fn p(path: &str) -> ConflictFile {
        parse(path).unwrap_or_else(|| panic!("should parse: {path}"))
    }

    #[test]
    fn parses_with_extension() {
        let c = p("report.sync-conflict-20261116-143005-ABCDEF2.docx");
        assert_eq!(c.original_path, "report.docx");
        assert_eq!(
            c.conflict_path,
            "report.sync-conflict-20261116-143005-ABCDEF2.docx"
        );
        assert_eq!(c.timestamp, ts(2026, 11, 16, 14, 30, 5));
        assert_eq!(c.device_short_id, "ABCDEF2");
    }

    #[test]
    fn parses_without_extension() {
        assert_eq!(
            p("Makefile.sync-conflict-20261116-143005-ABCDEF2").original_path,
            "Makefile"
        );
    }

    #[test]
    fn marker_goes_before_last_extension() {
        assert_eq!(
            p("archive.tar.sync-conflict-20261116-143005-ABCDEF2.gz").original_path,
            "archive.tar.gz"
        );
    }

    #[test]
    fn parses_dotfile() {
        assert_eq!(
            p(".bashrc.sync-conflict-20261116-143005-ABCDEF2").original_path,
            ".bashrc"
        );
    }

    #[test]
    fn parses_with_directories() {
        let c = p("a/b/x.sync-conflict-20261116-143005-ABCDEF2.txt");
        assert_eq!(c.original_path, "a/b/x.txt");
    }

    #[test]
    fn accepts_digits_2_to_7_in_id() {
        assert_eq!(
            p("x.sync-conflict-20261116-143005-2345672.txt").device_short_id,
            "2345672"
        );
    }

    #[test]
    fn leap_day_ok_and_invalid_leap_day_rejected() {
        assert!(parse("x.sync-conflict-20240229-000000-ABCDEF2.txt").is_some());
        assert!(parse("x.sync-conflict-20260229-000000-ABCDEF2.txt").is_none());
    }

    #[test]
    fn non_conflict_names_are_none() {
        for n in [
            "report.docx",
            "sync-conflict.txt",
            "sync-conflict-20261116-143005-ABCDEF2.txt",
            "a/b/plain",
            "",
            ".sync-conflict-20261116-143005-ABCDEF2.txt",
        ] {
            assert!(parse(n).is_none(), "{n}");
        }
    }

    #[test]
    fn malformed_markers_are_none() {
        for n in [
            "x.sync-conflict-20261316-143005-ABCDEF2.txt", // month 13
            "x.sync-conflict-20261100-143005-ABCDEF2.txt", // day 0
            "x.sync-conflict-20261132-143005-ABCDEF2.txt", // day 32
            "x.sync-conflict-20261116-243005-ABCDEF2.txt", // hour 24
            "x.sync-conflict-20261116-146005-ABCDEF2.txt", // minute 60
            "x.sync-conflict-20261116-143060-ABCDEF2.txt", // second 60
            "x.sync-conflict-20261116-143005-ABCDEF.txt",  // id too short
            "x.sync-conflict-20261116-143005-ABCDEF22.txt", // id too long
            "x.sync-conflict-20261116-143005-abcdef2.txt", // lowercase
            "x.sync-conflict-20261116-143005-ABCDEF1.txt", // '1' not base32
            "x.sync-conflict-2026116-143005-ABCDEF2.txt",  // short date
            "x.sync-conflict-20261116-14300-ABCDEF2.txt",  // short time
            "x.sync-conflict-20261116-143005-ABCDEF2txt",  // missing dot
            "x.sync-conflict-20261116-143005-ABCDEF2.",    // empty ext
            "x.sync-conflict-2026111a-143005-ABCDEF2.txt", // non digit
        ] {
            assert!(parse(n).is_none(), "{n}");
        }
    }

    #[test]
    fn marker_in_directory_name_only_is_not_conflict() {
        assert!(parse("x.sync-conflict-20261116-143005-ABCDEF2.d/file.txt").is_none());
    }

    #[test]
    fn find_conflicts_filters_and_sorts() {
        let paths = [
            "z.sync-conflict-20261116-143005-ABCDEF2.txt",
            "plain.txt",
            "a/b.sync-conflict-20261117-000000-ABCDEF2.txt",
            "a/b.sync-conflict-20261116-235959-ABCDEF2.txt",
            "a/b.sync-conflict-20261116-235959-ABCDEF2.txt.bak",
        ];
        let got: Vec<_> = find_conflicts(paths.iter().copied())
            .into_iter()
            .map(|c| (c.original_path, c.timestamp.day, c.timestamp.hour))
            .collect();
        assert_eq!(
            got,
            vec![
                ("a/b.txt".to_string(), 16, 23),
                ("a/b.txt".to_string(), 17, 0),
                ("a/b.txt.bak".to_string(), 16, 23),
                ("z.txt".to_string(), 16, 14),
            ]
        );
    }

    #[test]
    fn find_conflicts_empty() {
        assert!(find_conflicts(std::iter::empty::<&str>()).is_empty());
    }

    #[test]
    fn plan_keep_mine_archives_conflict_copy() {
        let c = p("report.sync-conflict-20261116-143005-ABCDEF2.docx");
        assert_eq!(
            plan(&c, Resolution::KeepMine),
            vec![FsAction::ArchiveToVersions {
                path: "report.sync-conflict-20261116-143005-ABCDEF2.docx".into()
            }]
        );
    }

    #[test]
    fn plan_keep_other_archives_original_then_renames() {
        let c = p("d/report.sync-conflict-20261116-143005-ABCDEF2.docx");
        assert_eq!(
            plan(&c, Resolution::KeepOther),
            vec![
                FsAction::ArchiveToVersions {
                    path: "d/report.docx".into()
                },
                FsAction::Rename {
                    from: "d/report.sync-conflict-20261116-143005-ABCDEF2.docx".into(),
                    to: "d/report.docx".into()
                },
            ]
        );
    }

    #[test]
    fn plan_keep_both_renames_with_readable_name() {
        let c = p("report.sync-conflict-20261116-143005-ABCDEF2.docx");
        assert_eq!(
            plan(&c, Resolution::KeepBoth),
            vec![FsAction::Rename {
                from: "report.sync-conflict-20261116-143005-ABCDEF2.docx".into(),
                to: "report (conflito ABCDEF2 2026-11-16 14.30.05).docx".into()
            }]
        );
    }

    #[test]
    fn plan_keep_both_variants() {
        let c = p("a/b/Makefile.sync-conflict-20260102-030405-ABCDEF2");
        assert_eq!(
            plan(&c, Resolution::KeepBoth),
            vec![FsAction::Rename {
                from: c.conflict_path.clone(),
                to: "a/b/Makefile (conflito ABCDEF2 2026-01-02 03.04.05)".into()
            }]
        );
        let c = p("archive.tar.sync-conflict-20260102-030405-ABCDEF2.gz");
        assert!(matches!(&plan(&c, Resolution::KeepBoth)[0],
            FsAction::Rename { to, .. } if to == "archive.tar (conflito ABCDEF2 2026-01-02 03.04.05).gz"));
        let c = p(".bashrc.sync-conflict-20260102-030405-ABCDEF2");
        assert!(matches!(&plan(&c, Resolution::KeepBoth)[0],
            FsAction::Rename { to, .. } if to == ".bashrc (conflito ABCDEF2 2026-01-02 03.04.05)"));
    }

    #[test]
    fn invariant_no_plan_loses_a_path_without_archiving_it() {
        // A path is "lost" if it is a rename source, or a rename target that
        // overwrites an existing path (original). Every such path must be archived
        // first. For KeepMine the conflict copy disappears and must be archived.
        let names = [
            "report.sync-conflict-20261116-143005-ABCDEF2.docx",
            "a/Makefile.sync-conflict-20261116-143005-ABCDEF2",
            ".bashrc.sync-conflict-20261116-143005-ABCDEF2",
        ];
        for n in names {
            let c = p(n);
            for r in [
                Resolution::KeepMine,
                Resolution::KeepOther,
                Resolution::KeepBoth,
            ] {
                let actions = plan(&c, r);
                let mut archived: Vec<&str> = Vec::new();
                for a in &actions {
                    match a {
                        FsAction::ArchiveToVersions { path } => archived.push(path),
                        FsAction::Rename { from, to } => {
                            // overwriting the original destroys its content
                            if *to == c.original_path {
                                assert!(archived.contains(&c.original_path.as_str()), "{n} {r:?}");
                            }
                            assert_ne!(from, to);
                        }
                    }
                }
                // The version that is not kept must be archived.
                match r {
                    Resolution::KeepMine => assert!(archived.contains(&c.conflict_path.as_str())),
                    Resolution::KeepOther => assert!(archived.contains(&c.original_path.as_str())),
                    Resolution::KeepBoth => assert!(archived.is_empty()),
                }
            }
        }
    }

    #[test]
    fn non_ascii_in_marker_does_not_panic() {
        // Multi-byte chars inside the 23-byte metadata window must not panic.
        for name in [
            "x.sync-conflict-2026é1116-143005-ABCDEF2.txt",
            "x.sync-conflict-202é111-143005-ABCDEF2.txt",
            "x.sync-conflict-20261116-14300é-ABCDEF.txt",
            "x.sync-conflict-20261116-1430é5-ABCDEF2",
            "x.sync-conflict-20261116-143005-ABCDÉF2.txt",
            "x.sync-conflict-日本語日本語日本語-ABC",
        ] {
            assert_eq!(parse(name), None, "{name}");
        }
    }

    #[test]
    fn dot_and_dotdot_stems_are_rejected() {
        assert_eq!(parse("..sync-conflict-20261116-143005-ABCDEF2"), None);
        assert_eq!(parse("a/...sync-conflict-20261116-143005-ABCDEF2"), None);
        assert_eq!(parse("a/..sync-conflict-20261116-143005-ABCDEF2.txt"), None);
    }

    #[test]
    fn unsafe_directory_components_are_rejected() {
        let tail = ".sync-conflict-20261116-143005-ABCDEF2";
        for dir in [
            "../", "a/../", "./", "a//", "/abs/", "..\\..\\", "C:/", "a\0/",
        ] {
            let p = format!("{dir}x{tail}.txt");
            assert_eq!(parse(&p), None, "{p:?}");
        }
        // backslash or colon inside the file name itself (traversal on Windows)
        assert_eq!(parse(&format!("..\\..\\x{tail}")), None);
        assert_eq!(parse(&format!("c:x{tail}")), None);
    }

    #[test]
    fn syncthing_control_files_are_never_conflict_targets() {
        let tail = ".sync-conflict-20261116-143005-ABCDEF2";
        assert_eq!(parse(&format!(".stignore{tail}")), None);
        assert_eq!(parse(&format!("sub/.stfolder{tail}")), None);
        assert_eq!(parse(&format!(".stversions/a{tail}.txt")), None);
        assert!(parse(&format!("docs/a{tail}.txt")).is_some());
    }

    #[test]
    fn control_names_are_matched_case_and_trailing_dot_insensitively() {
        let tail = ".sync-conflict-20261116-143005-ABCDEF2";
        for p in [
            format!(".STIGNORE{tail}"),
            format!(".StFolder{tail}"),
            format!(".stversions{tail}"),
            format!(".STVersions/a{tail}.txt"),
            format!(".stversions. /a{tail}.txt"),
            format!("dir./a{tail}.txt"),
            format!("dir /a{tail}.txt"),
        ] {
            assert_eq!(parse(&p), None, "{p:?}");
        }
    }

    #[test]
    fn unicode_and_short_name_spellings_of_control_files_are_rejected() {
        let tail = ".sync-conflict-20261116-143005-ABCDEF2";
        for p in [
            format!(".\u{17f}tignore{tail}"),
            format!(".STIGN~1{tail}"),
            format!(".stver~2/a{tail}.txt"),
        ] {
            assert_eq!(parse(&p), None, "{p:?}");
        }
        assert!(
            parse(&format!("~backup{tail}.txt")).is_some(),
            "ordinary ~ names still fine"
        );
    }
}
