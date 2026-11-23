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

use crate::setup;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TsPeer {
    pub host_name: String,
    /// MagicDNS name without the trailing dot ("" when MagicDNS is off).
    pub dns_name: String,
    pub os: String,
    pub ips: Vec<String>,
    pub online: bool,
    /// Owner account (`UserID`); 0 when unknown.
    #[serde(skip)]
    pub user_id: i64,
    /// Has ACL tags (e.g. `tag:escritorio`): owned by the tailnet, not a user.
    #[serde(skip)]
    pub tagged: bool,
    /// Shared in from another tailnet (`ShareeNode`).
    #[serde(skip)]
    pub sharee: bool,
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
        user_id: v.get("UserID").and_then(Value::as_i64).unwrap_or(0),
        tagged: v
            .get("Tags")
            .and_then(Value::as_array)
            .is_some_and(|t| !t.is_empty()),
        sharee: v
            .get("ShareeNode")
            .and_then(Value::as_bool)
            .unwrap_or(false),
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
    let me = v.get("Self").and_then(peer);
    // Only this account's own machines, plus this tailnet's tagged machines
    // (office PCs tagged by the admin, e.g. `tag:escritorio`, belong to the
    // tailnet instead of a user). Nodes shared in from other users or other
    // tailnets must never become an SSH target.
    if let Some(owner) = me.as_ref().map(|m| m.user_id).filter(|id| *id != 0) {
        peers.retain(|p| !p.sharee && (p.user_id == owner || p.tagged));
    }
    peers.sort_by(|a, b| {
        b.online
            .cmp(&a.online)
            .then_with(|| a.host_name.cmp(&b.host_name))
    });
    Ok(TsStatus { state, me, peers })
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

/// The tailnet peer that is this Syncthing device, by its addresses only
/// (live connection first, then the configured ones). Never by name: a
/// look-alike name must not decide where an SSH session goes.
pub fn match_device<'a>(peers: &'a [TsPeer], addresses: &[&str]) -> Option<&'a TsPeer> {
    addresses.iter().find_map(|a| peer_for_address(peers, a))
}

/// Program + arguments that open a terminal running `ssh <target>`.
/// `target` must come from [`ssh_target`]; [`setup::terminal_command`]
/// re-checks every word before it reaches AppleScript or `cmd`.
pub fn terminal_command(target: &str) -> Result<(String, Vec<String>), String> {
    setup::terminal_command(&["ssh", target])
}

/// How to show another computer's screen from this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    /// Write this Remote Desktop file and open it (macOS: Windows App).
    RdpFile { name: String, contents: String },
    /// Run this program with these arguments.
    Command { program: String, args: Vec<String> },
}

/// Remote screen for a peer: Remote Desktop to Windows (needs Windows Pro
/// with Remote Desktop on), Screen Sharing (VNC) to a Mac. `host` and
/// `user` are validated here, so nothing else reaches a command line or
/// the `.rdp` file. `local_os` / `peer_os` as in `std::env::consts::OS`
/// and Tailscale's `OS` field (`windows`, `macOS`, `linux`).
pub fn screen_command(
    local_os: &str,
    peer_os: &str,
    host: &str,
    user: &str,
) -> Result<Screen, String> {
    if !valid_host(host) {
        return Err(format!("endereço inválido: {host}"));
    }
    if !valid_user(user) {
        return Err("nome de usuário inválido: use só letras, números, ponto, _ ou -".into());
    }
    match (local_os, peer_os.to_ascii_lowercase().as_str()) {
        ("macos", "windows") => Ok(Screen::RdpFile {
            name: format!("twinfolders-{}.rdp", host.split('.').next().unwrap_or(host)),
            // NLA on (credentials asked before connecting) and a warning if
            // the server's identity cannot be verified.
            contents: format!(
                "full address:s:{host}\r\nusername:s:{user}\r\nprompt for credentials:i:1\r\nauthentication level:i:2\r\nenablecredsspsupport:i:1\r\n"
            ),
        }),
        ("windows", "windows") => Ok(Screen::Command {
            program: "mstsc".into(),
            args: vec![format!("/v:{host}")],
        }),
        ("macos", "macos") => Ok(Screen::Command {
            program: "open".into(),
            args: vec![format!("vnc://{user}@{host}")],
        }),
        ("windows", "macos") => Err("ver a tela de um Mac a partir do Windows precisa de um programa de VNC (por exemplo, o RealVNC Viewer). Use o terminal, ou abra a central pelo próprio Mac.".into()),
        (_, "") => Err("o sistema do outro computador não aparece no Tailscale".into()),
        (_, other) => Err(format!("ver a tela de um computador {other} ainda não é suportado")),
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
        "OS": "macOS", "TailscaleIPs": ["100.101.12.4", "fd7a:115c:a1e0::1"], "Online": true, "UserID": 111
      },
      "CurrentTailnet": {"Name": "voce@example.com", "MagicDNSSuffix": "tail1234.ts.net", "MagicDNSEnabled": true},
      "Peer": {
        "nodekey:aaa": {
          "HostName": "PC-ESCRITORIO", "DNSName": "pc-escritorio.tail1234.ts.net.", "OS": "windows",
          "TailscaleIPs": ["100.92.8.12", "fd7a:115c:a1e0::2"], "Online": true,
          "LastSeen": "0001-01-01T00:00:00Z", "UnknownFutureField": {"x": 1}, "UserID": 111
        },
        "nodekey:bbb": {
          "HostName": "notebook-antigo", "DNSName": "", "OS": "linux",
          "TailscaleIPs": ["100.70.1.9"], "Online": false, "LastSeen": "2026-11-01T10:00:00Z", "UserID": 111
        },
        "nodekey:ddd": {
          "HostName": "pc-escritorio", "DNSName": "pc-escritorio.other-tailnet.ts.net.", "OS": "windows",
          "TailscaleIPs": ["100.99.9.9"], "Online": true, "UserID": 999
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
    fn matches_devices_only_by_address_never_by_name() {
        let s = parse_status(RUNNING).unwrap();
        let m = |addrs: &[&str]| match_device(&s.peers, addrs).map(|p| p.host_name.clone());
        assert_eq!(
            m(&["tcp://100.92.8.12:22001"]).as_deref(),
            Some("PC-ESCRITORIO")
        );
        assert_eq!(
            m(&["dynamic", "tcp://100.70.1.9:22001"]).as_deref(),
            Some("notebook-antigo")
        );
        // A similar name is not enough to pick an SSH host.
        assert_eq!(m(&["dynamic"]), None);
        assert_eq!(m(&[]), None);
    }

    #[test]
    fn nodes_shared_from_other_accounts_are_ignored() {
        let s = parse_status(RUNNING).unwrap();
        assert!(s
            .peers
            .iter()
            .all(|p| p.host_name != "pc-escritorio" || p.dns_name.contains("tail1234")));
        assert_eq!(match_device(&s.peers, &["tcp://100.99.9.9:22001"]), None);
    }

    #[test]
    fn tagged_office_machines_count_but_shared_in_ones_do_not() {
        let json = r#"{
          "BackendState": "Running",
          "Self": {"HostName": "mac-ana", "TailscaleIPs": ["100.64.0.1"], "UserID": 7},
          "Peer": {
            "a": {"HostName": "note-1", "TailscaleIPs": ["100.64.0.2"], "UserID": 99, "Tags": ["tag:escritorio"]},
            "b": {"HostName": "intruso", "TailscaleIPs": ["100.64.0.3"], "UserID": 55},
            "c": {"HostName": "vizinho", "TailscaleIPs": ["100.64.0.4"], "UserID": 99, "Tags": ["tag:x"], "ShareeNode": true},
            "d": {"HostName": "pc-ana", "TailscaleIPs": ["100.64.0.5"], "UserID": 7, "ShareeNode": true},
            "e": {"HostName": "note-2", "TailscaleIPs": ["100.64.0.6"], "UserID": 99, "Tags": []}
          }
        }"#;
        let s = parse_status(json).unwrap();
        let names: Vec<&str> = s.peers.iter().map(|p| p.host_name.as_str()).collect();
        assert_eq!(names, vec!["note-1"]);
    }

    #[test]
    fn terminal_command_runs_ssh_with_the_target_only() {
        let (prog, args) = terminal_command("magnum@pc-escritorio.tail1234.ts.net").unwrap();
        assert!(!prog.is_empty());
        let joined = args.join(" ");
        assert!(
            joined.contains("ssh magnum@pc-escritorio.tail1234.ts.net")
                || args.ends_with(&["ssh".into(), "magnum@pc-escritorio.tail1234.ts.net".into()])
        );
    }

    #[test]
    fn screen_command_per_os_pair() {
        let rdp = screen_command("macos", "windows", "pc-recepcao.tail1234.ts.net", "ana").unwrap();
        match rdp {
            Screen::RdpFile { name, contents } => {
                assert_eq!(name, "twinfolders-pc-recepcao.rdp");
                assert!(contents.starts_with("full address:s:pc-recepcao.tail1234.ts.net\r\n"));
                assert!(contents.contains("username:s:ana\r\n"));
                assert!(contents.contains("authentication level:i:2"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            screen_command("windows", "windows", "100.64.0.5", "ana").unwrap(),
            Screen::Command {
                program: "mstsc".into(),
                args: vec!["/v:100.64.0.5".into()]
            }
        );
        assert_eq!(
            screen_command("macos", "macOS", "mac-ana.tail1234.ts.net", "ana").unwrap(),
            Screen::Command {
                program: "open".into(),
                args: vec!["vnc://ana@mac-ana.tail1234.ts.net".into()]
            }
        );
        assert!(screen_command("windows", "macOS", "h", "u").is_err());
        assert!(screen_command("macos", "", "h", "u").is_err());
    }

    #[test]
    fn screen_command_refuses_injection_into_the_rdp_file() {
        // A newline in host or user would add lines to the .rdp file
        // (e.g. a program to run on connect): both are refused.
        assert!(screen_command("macos", "windows", "pc\r\nalternate shell:s:cmd", "ana").is_err());
        assert!(screen_command(
            "macos",
            "windows",
            "pc",
            "ana\nremoteapplicationprogram:s:x"
        )
        .is_err());
        assert!(screen_command("windows", "windows", "-h", "ana").is_err());
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
