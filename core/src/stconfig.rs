//! Reading and hardening Syncthing's `config.xml` before the first start.
//!
//! Hardening is done on the file (not via REST after start) so there is no
//! window in which NAT-PMP/UPnP could open a router port or the device could
//! announce itself to public discovery servers (SPEC R8).
//!
//! Edits are scoped to the `<gui>` and `<options>` blocks and every expected
//! element must exist: if a future Syncthing renames one, hardening fails
//! loudly instead of silently leaving a default on.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiConfig {
    pub address: String,
    pub api_key: String,
    pub tls: bool,
    /// Web GUI user; empty means the browser GUI is open to any local user.
    pub user: String,
    /// True when a bcrypt password hash is set.
    pub has_password: bool,
}

impl GuiConfig {
    /// Browser GUI protected by user + password (the REST API always
    /// requires the API key regardless).
    pub fn has_gui_auth(&self) -> bool {
        !self.user.is_empty() && self.has_password
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Xml(String),
    MissingElement(&'static str),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Xml(e) => write!(f, "invalid config.xml: {e}"),
            ConfigError::MissingElement(e) => write!(f, "config.xml has no <{e}> element"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Reads the GUI/REST address and API key.
pub fn read_gui(xml: &str) -> Result<GuiConfig, ConfigError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| ConfigError::Xml(e.to_string()))?;
    let gui = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("gui"))
        .ok_or(ConfigError::MissingElement("gui"))?;
    let text = |tag: &'static str| -> Result<String, ConfigError> {
        gui.children()
            .find(|n| n.has_tag_name(tag))
            .map(|n| n.text().unwrap_or("").trim().to_string())
            .ok_or(ConfigError::MissingElement(tag))
    };
    let opt_text = |tag: &str| {
        gui.children()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    Ok(GuiConfig {
        address: text("address")?,
        api_key: text("apikey")?,
        tls: gui.attribute("tls") == Some("true"),
        user: opt_text("user"),
        has_password: opt_text("password").starts_with("$2"),
    })
}

/// True if a GUI address (`host:port`, possibly with scheme) is loopback.
pub fn is_loopback_address(addr: &str) -> bool {
    let a = addr.split_once("://").map(|(_, r)| r).unwrap_or(addr);
    a.starts_with("127.0.0.1:") || a.starts_with("localhost:") || a.starts_with("[::1]:")
}

/// Byte range of the content between `<tag ...>` and `</tag>` (first match
/// at or after `from`).
fn block(xml: &str, tag: &str, from: usize) -> Option<(usize, usize)> {
    let open = format!("<{tag}");
    let mut i = from;
    loop {
        let start = xml[i..].find(&open)? + i;
        let after = start + open.len();
        // make sure it's `<tag>` or `<tag attr..>`, not `<tagSomething>`
        match xml[after..].chars().next() {
            Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r') => {
                let content_start = xml[after..].find('>')? + after + 1;
                let close = format!("</{tag}>");
                let content_end = xml[content_start..].find(&close)? + content_start;
                return Some((content_start, content_end));
            }
            _ => i = after,
        }
    }
}

/// Replaces the text of the first `<tag>` inside `scope`.
fn set_in(
    xml: &mut String,
    scope: (usize, usize),
    tag: &'static str,
    value: &str,
) -> Result<(), ConfigError> {
    let section = &xml[scope.0..scope.1];
    let (s, e) = block(section, tag, 0).ok_or(ConfigError::MissingElement(tag))?;
    xml.replace_range(scope.0 + s..scope.0 + e, value);
    Ok(())
}

/// Options set by TwinFolders. Local discovery stays on (LAN only).
const HARDENED_OPTIONS: &[(&str, &str)] = &[
    ("globalAnnounceEnabled", "false"), // don't publish device/IP to public discovery
    ("relaysEnabled", "false"),         // Tailscale provides connectivity
    ("natEnabled", "false"),            // never open router ports (UPnP/NAT-PMP)
    ("startBrowser", "false"),
    ("urAccepted", "-1"), // usage reporting declined
    ("crashReportingEnabled", "false"),
    ("autoUpgradeIntervalH", "0"), // upgrades via brew/winget, both sides same version
];

/// Returns `xml` with TwinFolders settings applied: GUI on
/// `127.0.0.1:{gui_port}`, a single TCP listen address on `listen_port`,
/// and the options in [`HARDENED_OPTIONS`]. Idempotent.
pub fn harden(xml: &str, gui_port: u16, listen_port: u16) -> Result<String, ConfigError> {
    // Validate it parses before touching it.
    roxmltree::Document::parse(xml).map_err(|e| ConfigError::Xml(e.to_string()))?;
    let mut out = xml.to_string();

    // GUI over TLS with the pinned certificate (see `guitls`).
    let gui_open = out.find("<gui").ok_or(ConfigError::MissingElement("gui"))?;
    let gui_open_end = out[gui_open..]
        .find('>')
        .ok_or(ConfigError::MissingElement("gui"))?
        + gui_open;
    let tag = out[gui_open..gui_open_end].to_string();
    let new_tag = if tag.contains("tls=\"true\"") {
        tag.clone()
    } else if tag.contains("tls=\"false\"") {
        tag.replace("tls=\"false\"", "tls=\"true\"")
    } else {
        format!("{tag} tls=\"true\"")
    };
    out.replace_range(gui_open..gui_open_end, &new_tag);

    let gui = block(&out, "gui", 0).ok_or(ConfigError::MissingElement("gui"))?;
    set_in(&mut out, gui, "address", &format!("127.0.0.1:{gui_port}"))?;

    for (tag, value) in HARDENED_OPTIONS {
        let opts = block(&out, "options", 0).ok_or(ConfigError::MissingElement("options"))?;
        set_in(&mut out, opts, tag, value)?;
    }

    // Exactly one listen address: drop extras, then set the first.
    let listen = format!("tcp://0.0.0.0:{listen_port}");
    loop {
        let opts = block(&out, "options", 0).ok_or(ConfigError::MissingElement("options"))?;
        let section = &out[opts.0..opts.1];
        let first = block(section, "listenAddress", 0)
            .ok_or(ConfigError::MissingElement("listenAddress"))?;
        let close = "</listenAddress>";
        let after_first = first.1 + close.len();
        match block(section, "listenAddress", after_first) {
            Some((s, e)) => {
                // remove the whole extra element including surrounding whitespace line
                let elem_start = section[..s].rfind("<listenAddress").unwrap();
                let line_start = section[..elem_start]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(elem_start);
                let elem_end = e + close.len();
                let line_end = if section[elem_end..].starts_with('\n') {
                    elem_end + 1
                } else {
                    elem_end
                };
                out.replace_range(opts.0 + line_start..opts.0 + line_end, "");
            }
            None => {
                out.replace_range(opts.0 + first.0..opts.0 + first.1, &listen);
                break;
            }
        }
    }

    // Re-validate the result.
    roxmltree::Document::parse(&out).map_err(|e| ConfigError::Xml(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from a real `syncthing generate` (v2.1.7) output.
    const SAMPLE: &str = r#"<configuration version="52">
    <device id="AAAA" name="mac" compression="metadata">
        <address>dynamic</address>
    </device>
    <gui enabled="true" tls="false" sendBasicAuthPrompt="false">
        <address>127.0.0.1:8384</address>
        <metricsWithoutAuth>false</metricsWithoutAuth>
        <apikey>SECRETKEY123</apikey>
        <theme>default</theme>
    </gui>
    <options>
        <listenAddress>default</listenAddress>
        <listenAddress>quic://0.0.0.0:22000</listenAddress>
        <globalAnnounceServer>default</globalAnnounceServer>
        <globalAnnounceEnabled>true</globalAnnounceEnabled>
        <localAnnounceEnabled>true</localAnnounceEnabled>
        <relaysEnabled>true</relaysEnabled>
        <startBrowser>true</startBrowser>
        <natEnabled>true</natEnabled>
        <urAccepted>0</urAccepted>
        <autoUpgradeIntervalH>12</autoUpgradeIntervalH>
        <crashReportingEnabled>true</crashReportingEnabled>
    </options>
</configuration>
"#;

    fn opt(xml: &str, tag: &str) -> Vec<String> {
        let doc = roxmltree::Document::parse(xml).unwrap();
        let opts = doc
            .root_element()
            .children()
            .find(|n| n.has_tag_name("options"))
            .unwrap();
        opts.children()
            .filter(|n| n.has_tag_name(tag))
            .map(|n| n.text().unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn reads_gui_address_and_key() {
        let g = read_gui(SAMPLE).unwrap();
        assert_eq!(g.address, "127.0.0.1:8384");
        assert_eq!(g.api_key, "SECRETKEY123");
        assert!(!g.tls);
        assert!(!g.has_gui_auth(), "fresh generate has no GUI user/password");
        let with = SAMPLE.replace(
            "<theme>default</theme>",
            "<user>twinfolders</user><password>$2a$10$abcdefghijklmnopqrstuv</password><theme>default</theme>",
        );
        assert!(read_gui(&with).unwrap().has_gui_auth());
    }

    #[test]
    fn harden_sets_all_options_and_ports() {
        let h = harden(SAMPLE, 8385, 22001).unwrap();
        assert_eq!(read_gui(&h).unwrap().address, "127.0.0.1:8385");
        assert!(read_gui(&h).unwrap().tls, "GUI served over TLS");
        assert_eq!(
            read_gui(&h).unwrap().api_key,
            "SECRETKEY123",
            "key untouched"
        );
        assert_eq!(opt(&h, "listenAddress"), vec!["tcp://0.0.0.0:22001"]);
        for (tag, v) in HARDENED_OPTIONS {
            assert_eq!(opt(&h, tag), vec![v.to_string()], "{tag}");
        }
        assert_eq!(
            opt(&h, "localAnnounceEnabled"),
            vec!["true"],
            "LAN discovery kept"
        );
    }

    #[test]
    fn harden_does_not_touch_device_addresses() {
        let h = harden(SAMPLE, 8385, 22001).unwrap();
        assert!(h.contains("<address>dynamic</address>"));
    }

    #[test]
    fn harden_is_idempotent() {
        let once = harden(SAMPLE, 8385, 22001).unwrap();
        assert_eq!(harden(&once, 8385, 22001).unwrap(), once);
    }

    #[test]
    fn missing_option_fails_loudly() {
        let broken = SAMPLE.replace("<natEnabled>true</natEnabled>", "");
        assert_eq!(
            harden(&broken, 1, 2),
            Err(ConfigError::MissingElement("natEnabled"))
        );
    }

    #[test]
    fn invalid_xml_is_rejected() {
        assert!(matches!(
            harden("<configuration>", 1, 2),
            Err(ConfigError::Xml(_))
        ));
        assert!(matches!(read_gui("nope"), Err(ConfigError::Xml(_))));
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_address("127.0.0.1:8385"));
        assert!(is_loopback_address("http://localhost:8385"));
        assert!(is_loopback_address("[::1]:8385"));
        assert!(!is_loopback_address("0.0.0.0:8384"));
        assert!(!is_loopback_address("192.168.1.2:8384"));
        assert!(!is_loopback_address("127.0.0.1.evil:8384"));
    }
}
