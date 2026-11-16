//! Windows file-name compatibility checks for macOS -> Windows syncing.
//!
//! Before syncing a folder from macOS to Windows we warn about names that
//! Windows cannot store, instead of failing silently. Rules follow
//! <https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file>:
//!
//! * **Invalid characters** ([`NameIssue::InvalidChar`]): `< > : " | ? * \`
//!   and ASCII control characters `0x00..=0x1F`. `/` is the separator on the
//!   macOS side and never appears inside a component. Each distinct character
//!   is reported once, in first-occurrence order.
//! * **Reserved device names** ([`NameIssue::ReservedName`]): `CON PRN AUX NUL`,
//!   `COM1`-`COM9`, `LPT1`-`LPT9`, plus the superscript forms `COM¹ COM² COM³`
//!   and `LPT¹ LPT² LPT³`. Matching is case-insensitive and applies to the part
//!   of the name before the first `.` (so `nul.txt` and `Com1.tar.gz` are
//!   reserved, while `CONSOLE` is not). The payload is the upper-cased device
//!   name (e.g. `"COM1"`).
//! * **Trailing dot or space** ([`NameIssue::TrailingDotOrSpace`]): Windows
//!   strips them, so `file.` would silently become `file`. The special
//!   components `.` and `..` are exempt.
//! * **Path length** ([`NameIssue::PathTooLong`]): Windows `MAX_PATH` is 260
//!   UTF-16 units including the root and terminator. The sync root's length is
//!   unknown here, so [`MAX_PATH_WARN`] (240) is used as a heuristic limit on
//!   the *relative* path, measured in UTF-16 code units.
//!
//! # Limitation
//!
//! Unicode normalization (NFC vs NFD, as used by macOS/APFS/HFS+) is **not**
//! handled: [`case_collisions`] only folds case via `to_lowercase()`.

use std::collections::{BTreeMap, BTreeSet};

/// A reason a name cannot be stored on Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameIssue {
    /// `< > : " | ? * \` or an ASCII control character.
    InvalidChar(char),
    /// Reserved device name (upper-cased, e.g. `"NUL"`).
    ReservedName(String),
    /// Name ends in `.` or a space.
    TrailingDotOrSpace,
    /// Relative path length in UTF-16 units exceeds [`MAX_PATH_WARN`].
    PathTooLong(usize),
}

/// Heuristic limit for the relative path length (UTF-16 units). Real limit is
/// 259 once joined to the sync root; 240 leaves room for a short root.
pub const MAX_PATH_WARN: usize = 240;

/// Issues for a single path component.
pub fn validate_component(name: &str) -> Vec<NameIssue> {
    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();
    for c in name.chars() {
        let bad = (c as u32) < 0x20 || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\');
        if bad && seen.insert(c) {
            issues.push(NameIssue::InvalidChar(c));
        }
    }
    if let Some(device) = reserved_device(name) {
        issues.push(NameIssue::ReservedName(device));
    }
    if name != "." && name != ".." && (name.ends_with('.') || name.ends_with(' ')) {
        issues.push(NameIssue::TrailingDotOrSpace);
    }
    issues
}

/// Upper-cased device name if the part before the first `.` is reserved.
fn reserved_device(name: &str) -> Option<String> {
    let stem = name.split('.').next().unwrap_or("").trim_end_matches(' ');
    let up = stem.to_uppercase();
    let reserved = match up.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => {
            let mut it = up.chars();
            let prefix: String = it.by_ref().take(3).collect();
            let rest: Vec<char> = it.collect();
            (prefix == "COM" || prefix == "LPT")
                && rest.len() == 1
                && matches!(rest[0], '1'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}')
        }
    };
    reserved.then_some(up)
}

/// An issue attached to the (relative) path that triggered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathIssue {
    pub path: String,
    pub issue: NameIssue,
}

/// Validate a `/`-separated relative path.
///
/// Each component issue is attached to the path prefix ending at that
/// component; a length issue is attached to the whole path. Empty components
/// (from `//` or a leading `/`) are skipped.
pub fn validate_path(rel_path: &str) -> Vec<PathIssue> {
    let mut out = Vec::new();
    let mut prefix = String::new();
    for (i, comp) in rel_path.split('/').enumerate() {
        if i > 0 {
            prefix.push('/');
        }
        prefix.push_str(comp);
        if comp.is_empty() {
            continue;
        }
        for issue in validate_component(comp) {
            out.push(PathIssue {
                path: prefix.clone(),
                issue,
            });
        }
    }
    let len = rel_path.encode_utf16().count();
    if len > MAX_PATH_WARN {
        out.push(PathIssue {
            path: rel_path.to_string(),
            issue: NameIssue::PathTooLong(len),
        });
    }
    out
}

/// Groups of paths that collide on a case-insensitive filesystem.
///
/// Reports (a) groups of distinct full paths equal under `to_lowercase()` and
/// (b) groups of distinct parent-directory prefixes equal under
/// `to_lowercase()` (e.g. `Docs/a.txt` and `docs/b.txt` yield `["Docs",
/// "docs"]`). Exact duplicates are ignored, only groups of size >= 2 are
/// returned, and groups and members are sorted for deterministic output.
///
/// Limitation: Unicode normalization (NFC vs NFD) is not handled.
pub fn case_collisions<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<Vec<String>> {
    let full: BTreeSet<&str> = paths.into_iter().collect();
    let mut dirs: BTreeSet<&str> = BTreeSet::new();
    for p in &full {
        for (i, b) in p.bytes().enumerate() {
            if b == b'/' {
                dirs.insert(&p[..i]);
            }
        }
    }
    let mut groups: BTreeSet<Vec<String>> = BTreeSet::new();
    for set in [&full, &dirs] {
        let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for p in set.iter() {
            by_key
                .entry(p.to_lowercase())
                .or_default()
                .push((*p).to_string());
        }
        // BTreeSet iteration is sorted, so members are already sorted.
        groups.extend(by_key.into_values().filter(|g| g.len() >= 2));
    }
    groups.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pi(path: &str, issue: NameIssue) -> PathIssue {
        PathIssue {
            path: path.to_string(),
            issue,
        }
    }

    #[test]
    fn clean_names_have_no_issues() {
        for n in [
            "file.txt",
            ".hidden",
            "CONSOLE",
            "console.txt",
            "COM10",
            "LPT0",
            "COM",
            "a b",
            "日本語.md",
            "📁 emoji 🎉.txt",
            "nul_",
            "x.nul",
        ] {
            assert_eq!(validate_component(n), vec![], "{n:?}");
        }
    }

    #[test]
    fn dot_components_are_not_issues() {
        assert_eq!(validate_component("."), vec![]);
        assert_eq!(validate_component(".."), vec![]);
        assert_eq!(
            validate_component("..."),
            vec![NameIssue::TrailingDotOrSpace]
        );
    }

    #[test]
    fn each_forbidden_char_detected() {
        for c in ['<', '>', ':', '"', '|', '?', '*', '\\'] {
            let n = format!("a{c}b");
            assert_eq!(validate_component(&n), vec![NameIssue::InvalidChar(c)]);
        }
    }

    #[test]
    fn control_chars_detected() {
        assert_eq!(
            validate_component("a\u{0}b"),
            vec![NameIssue::InvalidChar('\u{0}')]
        );
        assert_eq!(
            validate_component("a\tb"),
            vec![NameIssue::InvalidChar('\t')]
        );
        assert_eq!(
            validate_component("a\u{1f}b"),
            vec![NameIssue::InvalidChar('\u{1f}')]
        );
        assert_eq!(validate_component("a\u{20}b"), vec![]);
    }

    #[test]
    fn invalid_chars_reported_once_in_first_occurrence_order() {
        assert_eq!(
            validate_component("?a:b?c:*"),
            vec![
                NameIssue::InvalidChar('?'),
                NameIssue::InvalidChar(':'),
                NameIssue::InvalidChar('*')
            ]
        );
    }

    #[test]
    fn reserved_names_case_insensitive() {
        for n in [
            "CON", "con", "Prn", "AUX", "nUl", "COM1", "com9", "LPT1", "lpt9",
        ] {
            assert_eq!(
                validate_component(n),
                vec![NameIssue::ReservedName(n.to_uppercase())],
                "{n}"
            );
        }
    }

    #[test]
    fn reserved_names_with_extensions() {
        assert_eq!(
            validate_component("con.txt"),
            vec![NameIssue::ReservedName("CON".into())]
        );
        assert_eq!(
            validate_component("nul.txt"),
            vec![NameIssue::ReservedName("NUL".into())]
        );
        assert_eq!(
            validate_component("Com1.tar.gz"),
            vec![NameIssue::ReservedName("COM1".into())]
        );
    }

    #[test]
    fn console_is_not_reserved() {
        assert_eq!(validate_component("CONSOLE"), vec![]);
        assert_eq!(validate_component("CONSOLE.txt"), vec![]);
        assert_eq!(validate_component("COM0"), vec![]);
    }

    #[test]
    fn superscript_digit_reserved() {
        assert_eq!(
            validate_component("COM¹"),
            vec![NameIssue::ReservedName("COM¹".into())]
        );
        assert_eq!(
            validate_component("com².txt"),
            vec![NameIssue::ReservedName("COM²".into())]
        );
        assert_eq!(
            validate_component("LPT³"),
            vec![NameIssue::ReservedName("LPT³".into())]
        );
        assert_eq!(
            validate_component("LPT¹.x"),
            vec![NameIssue::ReservedName("LPT¹".into())]
        );
    }

    #[test]
    fn trailing_dot_or_space() {
        assert_eq!(
            validate_component("file."),
            vec![NameIssue::TrailingDotOrSpace]
        );
        assert_eq!(
            validate_component("file "),
            vec![NameIssue::TrailingDotOrSpace]
        );
        assert_eq!(validate_component(" file"), vec![]);
    }

    #[test]
    fn multiple_issue_kinds_combine() {
        assert_eq!(
            validate_component("con?."),
            vec![NameIssue::InvalidChar('?'), NameIssue::TrailingDotOrSpace]
        );
        assert_eq!(
            validate_component("nul."),
            vec![
                NameIssue::ReservedName("NUL".into()),
                NameIssue::TrailingDotOrSpace
            ]
        );
    }

    #[test]
    fn path_clean() {
        assert_eq!(validate_path("a/b/c.txt"), vec![]);
        assert_eq!(validate_path("./a/../b"), vec![]);
        assert_eq!(validate_path(""), vec![]);
    }

    #[test]
    fn path_reports_offending_prefix() {
        assert_eq!(
            validate_path("ok/con/x?.txt"),
            vec![
                pi("ok/con", NameIssue::ReservedName("CON".into())),
                pi("ok/con/x?.txt", NameIssue::InvalidChar('?')),
            ]
        );
    }

    #[test]
    fn path_backslash_in_component() {
        assert_eq!(
            validate_path("a/b\\c"),
            vec![pi("a/b\\c", NameIssue::InvalidChar('\\'))]
        );
    }

    #[test]
    fn path_control_char_and_trailing() {
        assert_eq!(
            validate_path("dir./a\u{7}"),
            vec![
                pi("dir.", NameIssue::TrailingDotOrSpace),
                pi("dir./a\u{7}", NameIssue::InvalidChar('\u{7}')),
            ]
        );
    }

    #[test]
    fn path_length_threshold() {
        let ok = "a".repeat(MAX_PATH_WARN);
        assert_eq!(validate_path(&ok), vec![]);
        let long = "a".repeat(MAX_PATH_WARN + 1);
        assert_eq!(
            validate_path(&long),
            vec![pi(&long, NameIssue::PathTooLong(MAX_PATH_WARN + 1))]
        );
    }

    #[test]
    fn path_length_counts_utf16_units() {
        // each emoji is 2 UTF-16 units: 121 * 2 = 242 > 240 though only 121 chars
        let p = "😀".repeat(121);
        assert_eq!(validate_path(&p), vec![pi(&p, NameIssue::PathTooLong(242))]);
        let p = "😀".repeat(120);
        assert_eq!(validate_path(&p), vec![]);
    }

    #[test]
    fn collisions_none() {
        assert_eq!(
            case_collisions(["a.txt", "b.txt", "d/a.txt"]),
            Vec::<Vec<String>>::new()
        );
        assert_eq!(
            case_collisions(std::iter::empty::<&str>()),
            Vec::<Vec<String>>::new()
        );
        // exact duplicates are not collisions
        assert_eq!(
            case_collisions(["a.txt", "a.txt"]),
            Vec::<Vec<String>>::new()
        );
    }

    #[test]
    fn collisions_full_path() {
        assert_eq!(
            case_collisions(["b.txt", "README.md", "readme.md", "Readme.MD"]),
            vec![vec![
                "README.md".to_string(),
                "Readme.MD".into(),
                "readme.md".into()
            ]]
        );
    }

    #[test]
    fn collisions_parent_dir_differs_in_case() {
        assert_eq!(
            case_collisions(["Docs/a.txt", "docs/b.txt"]),
            vec![vec!["Docs".to_string(), "docs".into()]]
        );
    }

    #[test]
    fn collisions_full_and_dir_groups_combined_and_sorted() {
        assert_eq!(
            case_collisions(["z/X", "Z/x", "a/F", "A/f", "q"]),
            vec![
                vec!["A".to_string(), "a".into()],
                vec!["A/f".to_string(), "a/F".into()],
                vec!["Z".to_string(), "z".into()],
                vec!["Z/x".to_string(), "z/X".into()],
            ]
        );
    }

    #[test]
    fn collisions_deterministic_regardless_of_input_order() {
        let a = case_collisions(["Docs/a", "docs/b", "DOCS/c"]);
        let b = case_collisions(["DOCS/c", "docs/b", "Docs/a"]);
        assert_eq!(a, b);
        assert_eq!(
            a,
            vec![vec!["DOCS".to_string(), "Docs".into(), "docs".into()]]
        );
    }
}
