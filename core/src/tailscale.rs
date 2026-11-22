//! Reading the tailnet (`tailscale status --json`) and building safe SSH
//! targets for "open a terminal on the other computer".
//!
//! Format per tailscale/ipn/ipnstate (see docs/research.md §2.1). Tailscale
//! warns it may change, so parsing is defensive: unknown/missing fields are
//! tolerated, peers are keyed by public key (ignored), `DNSName` has a
//! trailing dot.
//!
//! SSH itself is plain OpenSSH (Remote Login on macOS, OpenSSH Server on
//! Windows) over the tailnet: Tailscale SSH can't be a server on Windows
//! nor in the macOS App Store/Standalone apps.

use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TsPeer {
    pub host_name: String,
    /// MagicDNS name without the trailing dot ("" when MagicDNS is off).
    pub dns_name: String,
    pub os: String,
    pub ips: Vec<String>,
    pub online: bool,
}

impl TsPeer {
    /// Best name to connect to: MagicDNS name, else the first IPv4.
    pub fn host(&self) -> Option<String> {
        if !self.dns_name.is_empty() && valid_host(&self.dns_name) {
            return Some(self.dns_name.clone());
        }
        self.ips.iter().find(|ip| ip.contains('.')).cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TsStatus {
    /// `BackendState` (`Running`, `NeedsLogin`, `Stopped`, ...).
    pub state: String,
    pub me: Option<TsPeer>,
    pub peers: Vec<TsPeer>,
}

impl TsStatus {
    pub fn running(&self) -> bool {
        self.state == "Running"
    }
}

fn peer(v: &Value) -> Option<TsPeer> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let ips: Vec<String> = v
        .get("TailscaleIPs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let host_name = s("HostName");
    if host_name.is_empty() && ips.is_empty() {
        return None;
    }
    Some(TsPeer {
        dns_name: s("DNSName").trim_end_matches('.').to_string(),
        host_name,
        os: s("OS"),
        ips,
        online: v.get("Online").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Parses `tailscale status --json`.
pub fn parse_status(json: &str) -> Result<TsStatus, String> {
    let v: Value =
        serde_json::from_str(json).map_err(|e| format!("resposta inesperada do Tailscale: {e}"))?;
    let state = v
        .get("BackendState")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut peers: Vec<TsPeer> = v
        .get("Peer")
        .and_then(Value::as_object)
        .map(|m| m.values().filter_map(peer).collect())
        .unwrap_or_default();
    peers.sort_by(|a, b| {
        b.online
            .cmp(&a.online)
            .then_with(|| a.host_name.cmp(&b.host_name))
    });
    Ok(TsStatus {
        state,
        me: v.get("Self").and_then(peer),
        peers,
    })
}

/// The tailnet peer behind a Syncthing address such as
/// `tcp://100.92.8.12:22001` or `tcp://pc.tail1234.ts.net:22001`.
pub fn peer_for_address<'a>(peers: &'a [TsPeer], address: &str) -> Option<&'a TsPeer> {
    let rest = address.split_once("://").map_or(address, |(_, r)| r);
    let host = if let Some(v6) = rest.strip_prefix('[') {
        v6.split_once(']').map_or(v6, |(h, _)| h)
    } else {
        match rest.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') && p.bytes().all(|b| b.is_ascii_digit()) => h,
            _ => rest,
        }
    }
    .trim_end_matches('.')
    .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    peers.iter().find(|p| {
        p.ips.iter().any(|ip| ip.eq_ignore_ascii_case(&host))
            || p.dns_name.eq_ignore_ascii_case(&host)
            || p.host_name.eq_ignore_ascii_case(&host)
            || p.dns_name
                .split('.')
                .next()
                .is_some_and(|short| !short.is_empty() && short.eq_ignore_ascii_case(&host))
    })
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The tailnet peer that is this Syncthing device: by any of its addresses
/// (live connection first), else by name ("PC-ESCRITORIO" ~ "pc escritório").
pub fn match_device<'a>(peers: &'a [TsPeer], name: &str, addresses: &[&str]) -> Option<&'a TsPeer> {
    addresses
        .iter()
        .find_map(|a| peer_for_address(peers, a))
        .or_else(|| {
            let n = squash(name);
            (!n.is_empty())
                .then(|| peers.iter().find(|p| squash(&p.host_name) == n))
                .flatten()
        })
}

/// Program + arguments that open a terminal running `ssh <target>`.
/// `target` must come from [`ssh_target`], whose character set makes it
/// safe inside an AppleScript string and a `cmd /C start` command line.
pub fn terminal_command(target: &str) -> (String, Vec<String>) {
    if cfg!(target_os = "macos") {
        (
            "osascript".into(),
            vec![
                "-e".into(),
                "tell application \"Terminal\" to activate".into(),
                "-e".into(),
                format!("tell application \"Terminal\" to do script \"ssh {target}\""),
            ],
        )
    } else if cfg!(windows) {
        (
            "cmd".into(),
            vec![
                "/C".into(),
                "start".into(),
                "TwinFolders SSH".into(),
                "ssh".into(),
                target.into(),
            ],
        )
    } else {
        (
            "x-terminal-emulator".into(),
            vec!["-e".into(), "ssh".into(), target.into()],
        )
    }
}

/// Host names/IPs safe to put in a terminal command line.
pub fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && !h.starts_with('-')
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

/// Account names safe for `ssh user@host`: no spaces, quotes or shell
/// characters, never starting with '-' (would read as an ssh option).
pub fn valid_user(u: &str) -> bool {
    !u.is_empty()
        && u.len() <= 64
        && !u.starts_with('-')
        && u.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `user@host`, validated so it can be passed to a terminal safely.
pub fn ssh_target(user: &str, host: &str) -> Result<String, String> {
    if !valid_user(user) {
        return Err(
            "nome de usuário inválido: use só letras, números, ponto, _ ou - (sem espaços)".into(),
        );
    }
    if !valid_host(host) {
        return Err(format!("endereço inválido: {host}"));
    }
    Ok(format!("{user}@{host}"))
}

/// Where the Tailscale CLI lives on each OS (docs/research.md §2.4).
pub fn cli_candidates() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from("tailscale")];
    if cfg!(target_os = "macos") {
        v.push("/Applications/Tailscale.app/Contents/MacOS/Tailscale".into());
        v.push("/usr/local/bin/tailscale".into());
        v.push("/opt/homebrew/bin/tailscale".into());
    } else if cfg!(windows) {
        v.push(r"C:\Program Files\Tailscale\tailscale.exe".into());
    }
    v
}

/// Runs `tailscale status --json`. `Err` explains what to do in plain words.
pub fn status() -> Result<TsStatus, String> {
    for bin in cli_candidates() {
        let Ok(out) = Command::new(&bin).args(["status", "--json"]).output() else {
            continue; // not at this path
        };
        if let Ok(s) = parse_status(&String::from_utf8_lossy(&out.stdout)) {
            return Ok(s);
        }
        if !out.status.success() {
            return Err("o Tailscale está instalado, mas não está conectado. Abra o app do Tailscale e entre na sua conta.".into());
        }
    }
    Err("o Tailscale não está instalado neste computador.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped after ipnstate.Status (docs/research.md §2.1).
    const RUNNING: &str = r#"{
      "Version": "1.104.7",
      "BackendState": "Running",
      "TailscaleIPs": ["100.101.12.4", "fd7a:115c:a1e0::1"],
      "Self": {
        "ID": "n1", "HostName": "MacBook-Pro", "DNSName": "macbook-pro.tail1234.ts.net.",
        "OS": "macOS", "TailscaleIPs": ["100.101.12.4", "fd7a:115c:a1e0::1"], "Online": true
      },
      "CurrentTailnet": {"Name": "voce@example.com", "MagicDNSSuffix": "tail1234.ts.net", "MagicDNSEnabled": true},
      "Peer": {
        "nodekey:aaa": {
          "HostName": "PC-ESCRITORIO", "DNSName": "pc-escritorio.tail1234.ts.net.", "OS": "windows",
          "TailscaleIPs": ["100.92.8.12", "fd7a:115c:a1e0::2"], "Online": true,
          "LastSeen": "0001-01-01T00:00:00Z", "UnknownFutureField": {"x": 1}
        },
        "nodekey:bbb": {
          "HostName": "notebook-antigo", "DNSName": "", "OS": "linux",
          "TailscaleIPs": ["100.70.1.9"], "Online": false, "LastSeen": "2026-11-01T10:00:00Z"
        },
        "nodekey:ccc": {"Tags": ["tag:server"]}
      }
    }"#;

    #[test]
    fn parses_self_and_peers_defensively() {
        let s = parse_status(RUNNING).unwrap();
        assert!(s.running());
        assert_eq!(
            s.me.as_ref().unwrap().dns_name,
            "macbook-pro.tail1234.ts.net"
        );
        // malformed peer (no name, no IPs) is skipped; online peers first
        assert_eq!(s.peers.len(), 2);
        assert_eq!(s.peers[0].host_name, "PC-ESCRITORIO");
        assert!(s.peers[0].online);
        assert_eq!(s.peers[0].dns_name, "pc-escritorio.tail1234.ts.net");
        assert_eq!(s.peers[1].host_name, "notebook-antigo");
        assert!(!s.peers[1].online);
    }

    #[test]
    fn host_prefers_magicdns_then_ipv4() {
        let s = parse_status(RUNNING).unwrap();
        assert_eq!(
            s.peers[0].host().as_deref(),
            Some("pc-escritorio.tail1234.ts.net")
        );
        assert_eq!(s.peers[1].host().as_deref(), Some("100.70.1.9"));
    }

    #[test]
    fn logged_out_and_garbage() {
        let s = parse_status(r#"{"BackendState":"NeedsLogin","Self":null,"Peer":null}"#).unwrap();
        assert!(!s.running());
        assert!(s.me.is_none() && s.peers.is_empty());
        assert!(parse_status("not json").is_err());
    }

    #[test]
    fn finds_the_peer_behind_a_syncthing_address() {
        let s = parse_status(RUNNING).unwrap();
        let p = |a| peer_for_address(&s.peers, a).map(|p| p.host_name.as_str());
        assert_eq!(p("tcp://100.92.8.12:22001"), Some("PC-ESCRITORIO"));
        assert_eq!(
            p("tcp://pc-escritorio.tail1234.ts.net:22001"),
            Some("PC-ESCRITORIO")
        );
        assert_eq!(p("tcp://pc-escritorio:22001"), Some("PC-ESCRITORIO"));
        assert_eq!(p("tcp://[fd7a:115c:a1e0::2]:22001"), Some("PC-ESCRITORIO"));
        assert_eq!(p("tcp://192.168.1.4:22001"), None);
        assert_eq!(p("dynamic"), None);
        assert_eq!(p(""), None);
    }

    #[test]
    fn matches_devices_by_address_then_name() {
        let s = parse_status(RUNNING).unwrap();
        let m =
            |name, addrs: &[&str]| match_device(&s.peers, name, addrs).map(|p| p.host_name.clone());
        assert_eq!(
            m("Windows PC", &["tcp://100.92.8.12:22001"]).as_deref(),
            Some("PC-ESCRITORIO")
        );
        assert_eq!(
            m("PC Escritorio", &["dynamic"]).as_deref(),
            Some("PC-ESCRITORIO")
        );
        assert_eq!(
            m("Notebook-Antigo", &[]).as_deref(),
            Some("notebook-antigo")
        );
        assert_eq!(m("Outro", &["dynamic"]), None);
        assert_eq!(m("", &[]), None);
    }

    #[test]
    fn terminal_command_runs_ssh_with_the_target_only() {
        let (prog, args) = terminal_command("magnum@pc-escritorio.tail1234.ts.net");
        assert!(!prog.is_empty());
        let joined = args.join(" ");
        assert!(
            joined.contains("ssh magnum@pc-escritorio.tail1234.ts.net")
                || args.ends_with(&["ssh".into(), "magnum@pc-escritorio.tail1234.ts.net".into()])
        );
    }

    #[test]
    fn ssh_targets_are_validated() {
        assert_eq!(
            ssh_target("magnum", "pc-escritorio.tail1234.ts.net").unwrap(),
            "magnum@pc-escritorio.tail1234.ts.net"
        );
        assert_eq!(
            ssh_target("ana.paula", "100.92.8.12").unwrap(),
            "ana.paula@100.92.8.12"
        );
        for bad in [
            "",
            "-oProxyCommand=x",
            "ana paula",
            "a;rm",
            "a\"b",
            "a$(x)",
            "a`x`",
            "ação",
        ] {
            assert!(ssh_target(bad, "host").is_err(), "{bad}");
        }
        for bad in ["", "-oProxyCommand=x", "a b", "h;x", "h$(x)", "h/x"] {
            assert!(ssh_target("u", bad).is_err(), "{bad}");
        }
    }
}
