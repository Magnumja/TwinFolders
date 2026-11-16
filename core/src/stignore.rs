//! Generate and edit Syncthing `.stignore` files.

pub const HEADER: &str = "// TwinFolders default";
const MERGE_HEADER: &str = "// TwinFolders defaults (added)";

pub fn default_patterns() -> &'static [&'static str] {
    &[
        "(?d).DS_Store",
        "(?d)Thumbs.db",
        "(?d)desktop.ini",
        "(?d)._*",
        "node_modules",
        ".git",
        "dist",
        "build",
        "target",
        "*.tmp",
        "~$*",
    ]
}

/// Header, defaults, then trimmed extras; exact duplicates dropped (first wins).
pub fn render(extra: &[&str]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = String::from(HEADER);
    out.push('\n');
    for line in default_patterns()
        .iter()
        .copied()
        .chain(extra.iter().map(|e| e.trim()))
    {
        if line.is_empty() || seen.contains(&line) {
            continue;
        }
        seen.push(line);
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Effective pattern lines: trimmed, without blank lines or `//` comments.
pub fn parse(content: &str) -> Vec<String> {
    content
        .trim_start_matches('\u{feff}') // UTF-8 BOM from Windows Notepad
        .lines() // handles both \n and \r\n
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .map(str::to_owned)
        .collect()
}

/// Append missing defaults at the end; user lines are never reordered or removed.
pub fn merge_with_defaults(existing: &str) -> String {
    let present = parse(existing);
    let missing: Vec<&str> = default_patterns()
        .iter()
        .copied()
        .filter(|d| !present.iter().any(|p| p == d))
        .collect();
    if missing.is_empty() {
        return existing.to_owned();
    }
    let mut out = existing.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(MERGE_HEADER);
    out.push('\n');
    for m in missing {
        out.push_str(m);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: [&str; 11] = [
        "(?d).DS_Store",
        "(?d)Thumbs.db",
        "(?d)desktop.ini",
        "(?d)._*",
        "node_modules",
        ".git",
        "dist",
        "build",
        "target",
        "*.tmp",
        "~$*",
    ];

    #[test]
    fn defaults_exact_and_ordered() {
        assert_eq!(default_patterns(), &DEFAULTS[..]);
    }

    #[test]
    fn render_empty_is_header_plus_defaults() {
        let out = render(&[]);
        let mut expected = String::from("// TwinFolders default\n");
        for d in DEFAULTS {
            expected.push_str(d);
            expected.push('\n');
        }
        assert_eq!(out, expected);
    }

    #[test]
    fn render_appends_extras_after_defaults_trimmed() {
        let out = render(&["  foo  ", "", "   ", "bar/"]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], HEADER);
        assert_eq!(&lines[1..12], &DEFAULTS[..]);
        assert_eq!(&lines[12..], &["foo", "bar/"]);
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn render_dedupes_extras_keeping_first() {
        let out = render(&["a", "b", "a", "node_modules", "b"]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(&lines[12..], &["a", "b"]);
        assert_eq!(lines.iter().filter(|l| **l == "node_modules").count(), 1);
    }

    #[test]
    fn parse_skips_blank_and_comments() {
        let c = "// c\n\n  \nfoo\n  // indented comment\nbar\n";
        assert_eq!(parse(c), vec!["foo", "bar"]);
    }

    #[test]
    fn parse_handles_crlf() {
        assert_eq!(parse("a\r\n\r\n// x\r\nb\r\n"), vec!["a", "b"]);
    }

    #[test]
    fn parse_keeps_include_and_negation() {
        let c = "#include other.stignore\n!keep\n(?d)x\n";
        assert_eq!(parse(c), vec!["#include other.stignore", "!keep", "(?d)x"]);
    }

    #[test]
    fn parse_render_roundtrip() {
        let p = parse(&render(&["x"]));
        assert_eq!(p.len(), 12);
        assert_eq!(p[11], "x");
    }

    #[test]
    fn merge_noop_when_complete() {
        let full = render(&["mine"]);
        assert_eq!(merge_with_defaults(&full), full);
    }

    #[test]
    fn merge_appends_missing_preserving_user_order() {
        let existing = "zzz\n!dist\nnode_modules\n";
        let out = merge_with_defaults(existing);
        assert!(out.starts_with(existing));
        let tail = &out[existing.len()..];
        let lines: Vec<&str> = tail.lines().collect();
        assert_eq!(lines[0], "// TwinFolders defaults (added)");
        let added = &lines[1..];
        assert!(!added.contains(&"node_modules"));
        // `!dist` is not the same line as `dist`; dist still gets added
        assert!(added.contains(&"dist"));
        assert_eq!(added.len(), 10);
        assert_eq!(added[0], "(?d).DS_Store");
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn merge_adds_newline_when_existing_lacks_one() {
        let out = merge_with_defaults("foo");
        assert!(out.starts_with("foo\n"));
        assert!(out.contains("// TwinFolders defaults (added)\n"));
    }

    #[test]
    fn merge_handles_crlf_existing() {
        let existing = render(&[]).replace('\n', "\r\n");
        assert_eq!(merge_with_defaults(&existing), existing);
    }

    #[test]
    fn merge_is_idempotent() {
        for existing in ["", "foo\n", "foo", "a\r\nnode_modules\r\n", "!x\n.git\n"] {
            let once = merge_with_defaults(existing);
            let twice = merge_with_defaults(&once);
            assert_eq!(once, twice, "input {existing:?}");
        }
    }

    #[test]
    fn merge_empty_existing_gives_defaults_block() {
        let out = merge_with_defaults("");
        assert_eq!(
            parse(&out),
            DEFAULTS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn parse_strips_utf8_bom() {
        assert_eq!(
            parse("\u{feff}(?d)Thumbs.db\r\nnode_modules"),
            vec!["(?d)Thumbs.db", "node_modules"]
        );
    }
}
