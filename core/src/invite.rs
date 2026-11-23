//! Invites for team computers (office mode).
//!
//! The central computer creates an invite: its device ID, name, optional
//! Tailscale address and a one-time code, valid for a limited time. The
//! team computer pastes it, adds the central and connects. While waiting
//! for approval it advertises its own name with the code (`note-ana
//! [tf:K7Q2-9XPM]`), so the central can tell an invited computer from an
//! unknown one: only someone holding the invite text knows the code.
//! Approval on the central stays an explicit click.
//!
//! The text is `twinfolders-convite:` + base32 of a small JSON object, so
//! it survives being pasted into chat apps and e-mail.

use serde::{Deserialize, Serialize};

use crate::peer;

pub const PREFIX: &str = "twinfolders-convite:";
/// How long an invite stays valid.
pub const VALID_SECS: u64 = 24 * 3600;
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const MAX_NAME: usize = 64;
const MAX_ADDRS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    /// Format version.
    pub v: u8,
    /// Central computer's device ID (canonical form).
    pub hub: String,
    /// Central computer's name, shown to the person joining.
    pub name: String,
    /// `tcp://host:port` addresses of the central (may be empty).
    #[serde(default)]
    pub addrs: Vec<String>,
    /// One-time code, `XXXX-XXXX`.
    pub code: String,
    /// Expiry, Unix seconds.
    pub exp: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    NotAnInvite,
    Damaged,
    Expired,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Error::NotAnInvite => {
                "isso não é um convite do TwinFolders (começa com twinfolders-convite:)"
            }
            Error::Damaged => "o convite está incompleto ou foi alterado; peça um novo à central",
            Error::Expired => "o convite venceu; peça um novo à central",
        })
    }
}

impl std::error::Error for Error {}

/// A fresh one-time code from the OS CSPRNG, e.g. `K7Q2-9XPM`.
pub fn new_code() -> String {
    let mut buf = [0u8; 8];
    getrandom::fill(&mut buf).expect("OS random number generator");
    // 32 symbols: `b % 32` has no modulo bias.
    let s: String = buf
        .iter()
        .map(|b| CODE_ALPHABET[(*b as usize) % CODE_ALPHABET.len()] as char)
        .collect();
    format!("{}-{}", &s[..4], &s[4..])
}

pub fn valid_code(c: &str) -> bool {
    let b = c.as_bytes();
    b.len() == 9
        && b[4] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, x)| i == 4 || CODE_ALPHABET.contains(x))
}

fn clean_name(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME)
        .collect::<String>()
        .trim()
        .to_string()
}

impl Invite {
    pub fn new(hub: &str, name: &str, addrs: Vec<String>, code: String, now: u64) -> Self {
        Self {
            v: 1,
            hub: hub.to_string(),
            name: clean_name(name),
            addrs,
            code,
            exp: now + VALID_SECS,
        }
    }

    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).expect("serializable");
        format!("{PREFIX}{}", base32_encode(&json))
    }

    /// Parses and validates an invite text (any surrounding whitespace or
    /// line breaks from chat apps is ignored). Checks expiry against `now`.
    pub fn decode(text: &str, now: u64) -> Result<Invite, Error> {
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let body = compact.strip_prefix(PREFIX).ok_or(Error::NotAnInvite)?;
        let bytes = base32_decode(body).ok_or(Error::Damaged)?;
        let mut inv: Invite = serde_json::from_slice(&bytes).map_err(|_| Error::Damaged)?;
        if inv.v != 1 || !valid_code(&inv.code) || inv.addrs.len() > MAX_ADDRS {
            return Err(Error::Damaged);
        }
        inv.hub = peer::normalize_device_id(&inv.hub).map_err(|_| Error::Damaged)?;
        inv.name = clean_name(&inv.name);
        let mut addrs = Vec::new();
        for a in &inv.addrs {
            match peer::normalize_address(a, 22000) {
                Ok(Some(n)) => addrs.push(n),
                _ => return Err(Error::Damaged),
            }
        }
        inv.addrs = addrs;
        if now >= inv.exp {
            return Err(Error::Expired);
        }
        Ok(inv)
    }
}

/// Name a joining computer advertises while waiting: `note-ana [tf:CODE]`.
pub fn tag_name(name: &str, code: &str) -> String {
    let base: String = clean_name(name).chars().take(40).collect();
    format!("{} [tf:{code}]", base.trim())
}

/// Splits `note-ana [tf:CODE]` into (`note-ana`, `CODE`). Names without a
/// well-formed tag come back unchanged with no code.
pub fn parse_tag(name: &str) -> (String, Option<String>) {
    if let Some(start) = name.rfind(" [tf:") {
        let rest = &name[start + 5..];
        if let Some(code) = rest.strip_suffix(']') {
            if valid_code(code) {
                return (name[..start].trim().to_string(), Some(code.to_string()));
            }
        }
    }
    (name.to_string(), None)
}

// RFC 4648 base32, no padding.
const B32: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in data {
        buf = (buf << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(B32[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn base32_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() > 4096 {
        return None;
    }
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = B32.iter().position(|&x| x == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub() -> String {
        // A valid (checksummed) device ID, same as in peer.rs tests.
        "ZO5XTSC-KSY4NFT-L47RJH4-MVGAWUN-XGPSAS2-VXP4JSK-6LZNSVP-CISINA2".into()
    }

    fn sample(now: u64) -> Invite {
        Invite::new(
            &hub(),
            "mac-ana",
            vec!["tcp://100.64.0.1:22001".into()],
            "K7Q2-9XPM".into(),
            now,
        )
    }

    #[test]
    fn roundtrip_survives_chat_line_breaks() {
        let inv = sample(1000);
        let text = inv.encode();
        assert!(text.starts_with(PREFIX));
        let mangled = format!("  {}\n{}  ", &text[..30], &text[30..]);
        assert_eq!(Invite::decode(&mangled, 1001).unwrap(), inv);
    }

    #[test]
    fn rejects_expired_damaged_and_foreign_text() {
        let text = sample(1000).encode();
        assert_eq!(
            Invite::decode(&text, 1000 + VALID_SECS),
            Err(Error::Expired)
        );
        assert_eq!(Invite::decode("olá", 0), Err(Error::NotAnInvite));
        let cut = &text[..text.len() - 6];
        assert_eq!(Invite::decode(cut, 1001), Err(Error::Damaged));
        let bad_hub = Invite {
            hub: "AAAAAAA".into(),
            ..sample(1000)
        }
        .encode();
        assert_eq!(Invite::decode(&bad_hub, 1001), Err(Error::Damaged));
        let bad_addr = Invite {
            addrs: vec!["tcp://evil host:1".into()],
            ..sample(1000)
        }
        .encode();
        assert_eq!(Invite::decode(&bad_addr, 1001), Err(Error::Damaged));
        let bad_code = Invite {
            code: "abc".into(),
            ..sample(1000)
        }
        .encode();
        assert_eq!(Invite::decode(&bad_code, 1001), Err(Error::Damaged));
    }

    #[test]
    fn codes_are_random_and_well_formed() {
        let a = new_code();
        let b = new_code();
        assert!(valid_code(&a) && valid_code(&b));
        assert_ne!(a, b);
        assert!(!valid_code("K7Q2-9XP0")); // 0 is not in the alphabet
        assert!(!valid_code("K7Q29XPMM"));
    }

    #[test]
    fn name_tag_roundtrip() {
        let t = tag_name("note-ana", "K7Q2-9XPM");
        assert_eq!(t, "note-ana [tf:K7Q2-9XPM]");
        assert_eq!(parse_tag(&t), ("note-ana".into(), Some("K7Q2-9XPM".into())));
        assert_eq!(parse_tag("note-ana"), ("note-ana".into(), None));
        assert_eq!(parse_tag("x [tf:nope]"), ("x [tf:nope]".into(), None));
        assert!(tag_name("a\u{7}b", "K7Q2-9XPM").starts_with("ab "));
    }
}
