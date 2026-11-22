//! First-run setup: finding Syncthing, the OS package manager that can
//! install it, and opening a visible terminal for commands the person runs
//! (installing software, SSH). Nothing here installs anything by itself.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// `name` in each PATH directory, then the given well-known locations.
/// Apps opened from Finder get a minimal PATH, hence the fixed fallbacks.
fn locate(name: &str, fallbacks: &[PathBuf]) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join(name))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter()
        .chain(fallbacks.iter().cloned())
        .find(|p| p.is_file())
}

fn local_app_data(rel: &str) -> Vec<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|l| vec![PathBuf::from(l).join(rel)])
        .unwrap_or_default()
}

/// The Syncthing binary: PATH, Homebrew (Apple Silicon and Intel), winget.
pub fn find_syncthing() -> Option<PathBuf> {
    if cfg!(windows) {
        locate(
            "syncthing.exe",
            &local_app_data("Microsoft\\WinGet\\Links\\syncthing.exe"),
        )
    } else {
        locate(
            "syncthing",
            &[
                "/opt/homebrew/bin/syncthing".into(),
                "/usr/local/bin/syncthing".into(),
            ],
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PackageManager {
    Brew,
    Winget,
}

/// The package manager that installs Syncthing on this OS, with its path.
pub fn package_manager() -> Option<(PackageManager, PathBuf)> {
    if cfg!(windows) {
        locate(
            "winget.exe",
            &local_app_data("Microsoft\\WindowsApps\\winget.exe"),
        )
        .map(|p| (PackageManager::Winget, p))
    } else if cfg!(target_os = "macos") {
        // Homebrew's own locations first: they never contain spaces, which
        // `terminal_command` would refuse.
        ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
            .or_else(|| locate("brew", &[]))
            .map(|p| (PackageManager::Brew, p))
    } else {
        None
    }
}

/// The command that installs Syncthing, as shown to the person and run in
/// a visible terminal. `bin` is the package manager found on disk; winget
/// is called by name (it is on PATH, and the user folder may have spaces).
pub fn syncthing_install_words(pm: PackageManager, bin: &Path) -> Vec<String> {
    match pm {
        PackageManager::Brew => vec![
            bin.to_string_lossy().into_owned(),
            "install".into(),
            "syncthing".into(),
        ],
        PackageManager::Winget => vec![
            "winget".into(),
            "install".into(),
            "--id".into(),
            "Syncthing.Syncthing".into(),
            "--exact".into(),
            "--source".into(),
            "winget".into(),
        ],
    }
}

/// Characters allowed in a word passed to [`terminal_command`]: enough for
/// paths, `user@host` and options, nothing an AppleScript string, a shell
/// or `cmd` would interpret (no spaces, quotes, `;`, `&`, `|`, `$`, `%`, `^`).
fn safe_word(w: &str) -> bool {
    !w.is_empty()
        && w.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/' | '\\' | ':' | '@')
        })
}

/// Program + arguments that open a new terminal window running `words`.
/// Every word is checked by [`safe_word`], so the result can't inject
/// commands into AppleScript (macOS) or `cmd /C start` (Windows).
pub fn terminal_command(words: &[&str]) -> Result<(String, Vec<String>), String> {
    if words.is_empty() || !words.iter().all(|w| safe_word(w)) {
        return Err("comando recusado: caracteres não permitidos".into());
    }
    if cfg!(target_os = "macos") {
        Ok((
            "osascript".into(),
            vec![
                "-e".into(),
                "tell application \"Terminal\" to activate".into(),
                "-e".into(),
                format!(
                    "tell application \"Terminal\" to do script \"{}\"",
                    words.join(" ")
                ),
            ],
        ))
    } else if cfg!(windows) {
        let mut args: Vec<String> = vec!["/C".into(), "start".into(), "TwinFolders".into()];
        args.extend(words.iter().map(|w| w.to_string()));
        Ok(("cmd".into(), args))
    } else {
        let mut args: Vec<String> = vec!["-e".into()];
        args.extend(words.iter().map(|w| w.to_string()));
        Ok(("x-terminal-emulator".into(), args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_words_that_could_break_out() {
        for bad in [
            "a b", "x\"y", "x'y", "a;b", "a&b", "a|b", "$(x)", "%PATH%", "a^b", "a\nb", "", "`x`",
        ] {
            assert!(terminal_command(&["ssh", bad]).is_err(), "{bad:?}");
        }
        assert!(terminal_command(&[]).is_err());
    }

    #[test]
    fn accepts_paths_targets_and_options() {
        let (_, args) =
            terminal_command(&["/opt/homebrew/bin/brew", "install", "syncthing"]).unwrap();
        assert!(args.join(" ").contains("brew install syncthing"));
        assert!(terminal_command(&["ssh", "mag@pc.tail1234.ts.net"]).is_ok());
        assert!(terminal_command(&["C:\\Users\\a\\winget.exe", "--id"]).is_ok());
    }

    #[test]
    fn install_words_are_all_safe() {
        for (pm, bin) in [
            (PackageManager::Brew, "/opt/homebrew/bin/brew"),
            (
                PackageManager::Winget,
                "C:\\Users\\Ana\\AppData\\Local\\Microsoft\\WindowsApps\\winget.exe",
            ),
        ] {
            let words = syncthing_install_words(pm, Path::new(bin));
            let refs: Vec<&str> = words.iter().map(String::as_str).collect();
            assert!(terminal_command(&refs).is_ok(), "{words:?}");
        }
    }
}
