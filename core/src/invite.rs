//! Invites for team computers (office mode).
//!
//! The central computer creates an invite: its device ID, name, optional
//! Tailscale address, a short code people can read (`K7Q2-9XPM`, a label
//! only) and a random 128-bit key, valid for a limited time. The team
//! computer pastes it, adds the central and connects. While waiting for
//! approval it advertises a *proof* in its name (`note-ana [tf:ABCD-EFGH]`):
//! HMAC-SHA256 of its own device ID keyed with the 128-bit key. The
//! central recomputes it for each pending request, so it can tell an
//! invited computer from an unknown one.
//!
//! Syncthing sends that name to anyone who connects, so the proof must not
//! give the secret away: the key has 128 bits (no offline search recovers
//! it) and the proof is bound to one device ID (overhearing it does not
//! let another computer reuse it). Approval stays an explicit click.
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
    /// Short code shown to people, `XXXX-XXXX` (a label, not the secret).
    pub code: String,
    /// 128-bit random key, base32 (26 chars): the secret behind the proof.
    pub key: String,
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
    pub fn new(
        hub: &str,
        name: &str,
        addrs: Vec<String>,
        code: String,
        key: String,
        now: u64,
    ) -> Self {
        Self {
            v: 1,
            hub: hub.to_string(),
            name: clean_name(name),
            addrs,
            code,
            key,
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
        if inv.v != 1
            || !valid_code(&inv.code)
            || !valid_key(&inv.key)
            || inv.addrs.len() > MAX_ADDRS
        {
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

/// A fresh 128-bit random key (base32, 26 chars).
pub fn new_key() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("OS random number generator");
    base32_encode(&buf)
}

pub fn valid_key(k: &str) -> bool {
    k.len() == 26 && k.bytes().all(|b| B32.contains(&b))
}

fn mac(key: &str, device_id: &str) -> String {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key.as_bytes());
    let tag = ring::hmac::sign(&k, device_id.as_bytes());
    base32_encode(&tag.as_ref()[..5])
}

/// Proof that the computer with `device_id` holds the invite `key`:
/// HMAC-SHA256(key, device_id), first 40 bits as `XXXX-XXXX` (base32).
pub fn proof(key: &str, device_id: &str) -> String {
    let s = mac(key, device_id);
    format!("{}-{}", &s[..4], &s[4..8])
}

fn valid_proof(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() == 9 && b[4] == b'-' && b.iter().enumerate().all(|(i, x)| i == 4 || B32.contains(x))
}

/// Constant-time check of a proof a pending computer advertised. A key
/// that is not a valid 128-bit key never matches.
pub fn proof_matches(key: &str, device_id: &str, advertised: &str) -> bool {
    if !valid_key(key) {
        return false;
    }
    let expected = mac(key, device_id);
    let got: String = advertised.chars().filter(|c| *c != '-').collect();
    expected.len() == got.len()
        && expected
            .bytes()
            .zip(got.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// Name a joining computer advertises while waiting: `note-ana [tf:PROOF]`.
pub fn tag_name(name: &str, key: &str, my_device_id: &str) -> String {
    let base: String = clean_name(name).chars().take(40).collect();
    format!("{} [tf:{}]", base.trim(), proof(key, my_device_id))
}

/// Splits `note-ana [tf:PROOF]` into (`note-ana`, `PROOF`). Names without
/// a well-formed tag come back unchanged with no proof.
pub fn parse_tag(name: &str) -> (String, Option<String>) {
    if let Some(start) = name.rfind(" [tf:") {
        let rest = &name[start + 5..];
        if let Some(p) = rest.strip_suffix(']') {
            if valid_proof(p) {
                return (name[..start].trim().to_string(), Some(p.to_string()));
            }
        }
    }
    (name.to_string(), None)
}

/// Which open invite, given as (code, key), the pending computer
/// `device_id` proves to hold, if exactly one does. Returns its code.
pub fn match_code<'a>(
    invites: &[(&'a str, &str)],
    device_id: &str,
    advertised: &str,
) -> Option<&'a str> {
    let mut hits = invites
        .iter()
        .filter(|(_, key)| proof_matches(key, device_id, advertised));
    match (hits.next(), hits.next()) {
        (Some((code, _)), None) => Some(code),
        _ => None,
    }
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
            KEY.into(),
            now,
        )
    }

    const KEY: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const KEY2: &str = "ZYXWVUTSRQPONMLKJIHGFEDCBA";

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
        let bad_key = Invite {
            key: "short".into(),
            ..sample(1000)
        }
        .encode();
        assert_eq!(Invite::decode(&bad_key, 1001), Err(Error::Damaged));
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
    fn keys_have_128_bits_and_differ() {
        let a = new_key();
        assert!(valid_key(&a));
        assert_ne!(a, new_key());
        assert!(!valid_key("ABC"));
        assert!(!valid_key("abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn name_tag_carries_a_proof_bound_to_the_device_not_the_secret() {
        let me = hub();
        let t = tag_name("note-ana", KEY, &me);
        assert!(!t.contains(KEY) && !t.contains("K7Q2-9XPM"));
        let (clean, p) = parse_tag(&t);
        assert_eq!(clean, "note-ana");
        let p = p.unwrap();
        assert!(proof_matches(KEY, &me, &p));
        // An overheard proof is useless from another device or another key.
        assert!(!proof_matches(KEY, "OTHERDEVICE", &p));
        assert!(!proof_matches(KEY2, &me, &p));
        // Old records without a real key never match.
        assert!(!proof_matches("", &me, &p));
        assert_eq!(parse_tag("note-ana"), ("note-ana".into(), None));
        assert_eq!(parse_tag("x [tf:nope]"), ("x [tf:nope]".into(), None));
        assert!(tag_name("a\u{7}b", KEY, &me).starts_with("ab "));
    }

    #[test]
    fn match_code_needs_exactly_one_invite() {
        let me = hub();
        let p = proof(KEY, &me);
        assert_eq!(
            match_code(&[("AAAA-BBBB", KEY2), ("K7Q2-9XPM", KEY)], &me, &p),
            Some("K7Q2-9XPM")
        );
        assert_eq!(match_code(&[("AAAA-BBBB", KEY2)], &me, &p), None);
        assert_eq!(
            match_code(&[("K7Q2-9XPM", KEY), ("CCCC-DDDD", KEY)], &me, &p),
            None
        );
    }
}
