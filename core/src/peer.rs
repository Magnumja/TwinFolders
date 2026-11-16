//! Adding the other computer by hand: device ID and address as a person
//! would paste them.
//!
//! Device IDs are 52 base32 characters split into four 13-character chunks,
//! each followed by a Luhn mod-32 check character (Syncthing's
//! `lib/protocol/luhn.go`), shown as 8 groups of 7 joined by dashes.
//! Checking the Luhn digits catches typos before anything is sent.

use std::fmt;

use crate::client::{self, SyncthingClient};

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Wrong length or characters outside base32.
    Malformed,
    /// Right shape, but a check character does not match (a typo).
    Checksum,
    /// The address cannot be used.
    BadAddress(String),
    /// The ID is this computer's own.
    IsSelf,
    /// Already paired.
    AlreadyPaired,
    Client(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Malformed => write!(
                f,
                "isso não parece um ID: são 8 blocos de 7 letras e números"
            ),
            Error::Checksum => write!(
                f,
                "o ID tem um erro de digitação; copie de novo no outro computador"
            ),
            Error::BadAddress(a) => write!(f, "endereço inválido: {a}"),
            Error::IsSelf => write!(f, "esse é o ID deste computador; cole o ID do outro"),
            Error::AlreadyPaired => write!(f, "esse computador já está pareado"),
            Error::Client(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

fn luhn32(chunk: &[u8]) -> u8 {
    let n = 32u32;
    let mut factor = 1u32;
    let mut sum = 0u32;
    for &c in chunk {
        let cp = ALPHABET.iter().position(|&a| a == c).unwrap_or(0) as u32;
        let mut addend = factor * cp;
        factor = if factor == 2 { 1 } else { 2 };
        addend = addend / n + addend % n;
        sum += addend;
    }
    ALPHABET[((n - sum % n) % n) as usize]
}

/// Accepts an ID as pasted (any case, spaces, with or without dashes) and
/// returns the canonical `XXXXXXX-XXXXXXX-…` form.
pub fn normalize_device_id(input: &str) -> Result<String, Error> {
    let raw: Vec<u8> = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .map(|c| u8::try_from(c as u32).unwrap_or(0))
        .collect();
    if raw.len() != 56 || !raw.iter().all(|c| ALPHABET.contains(c)) {
        return Err(Error::Malformed);
    }
    for chunk in raw.chunks(14) {
        if luhn32(&chunk[..13]) != chunk[13] {
            return Err(Error::Checksum);
        }
    }
    Ok(raw
        .chunks(7)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("-"))
}

/// Turns what a person types into a Syncthing address:
/// empty -> `None` (find it on the local network), `windows-pc` ->
/// `tcp://windows-pc:<port>`, `100.92.8.12:22000` -> `tcp://100.92.8.12:22000`.
pub fn normalize_address(input: &str, default_port: u16) -> Result<Option<String>, Error> {
    let s = input.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let bad = || Error::BadAddress(s.to_string());
    let rest = match s.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("tcp") => rest,
        Some(_) => return Err(bad()),
        None => s,
    };
    let rest = rest.trim_end_matches('/');
    // host[:port] | [v6][:port]
    let (host, port) = if let Some(v6) = rest.strip_prefix('[') {
        let (h, after) = v6.split_once(']').ok_or_else(bad)?;
        let p = match after.strip_prefix(':') {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err(bad()),
        };
        (format!("[{h}]"), p)
    } else {
        match rest.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') => (h.to_string(), Some(p)),
            Some(_) => return Err(bad()), // bare IPv6 needs brackets
            None => (rest.to_string(), None),
        }
    };
    let inner = host.trim_start_matches('[').trim_end_matches(']');
    let host_ok = !inner.is_empty()
        && inner.len() <= 253
        && inner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'));
    if !host_ok {
        return Err(bad());
    }
    let port = match port {
        Some(p) => p.parse::<u16>().ok().filter(|p| *p > 0).ok_or_else(bad)?,
        None => default_port,
    };
    Ok(Some(format!("tcp://{host}:{port}")))
}

/// Adds the other computer. The other side then sees a pairing request and
/// must approve it; nothing is shared until folders are offered and accepted.
pub fn add_peer(
    c: &SyncthingClient,
    id_input: &str,
    name: &str,
    address_input: &str,
    default_port: u16,
) -> Result<String, Error> {
    let id = normalize_device_id(id_input)?;
    let address = normalize_address(address_input, default_port)?;
    let ce = |e: client::Error| Error::Client(e.to_string());
    if c.my_id().map_err(ce)? == id {
        return Err(Error::IsSelf);
    }
    if c.devices().map_err(ce)?.iter().any(|d| d.device_id == id) {
        return Err(Error::AlreadyPaired);
    }
    let name = name.trim();
    let name = if name.is_empty() { &id[..7] } else { name };
    let addrs: Vec<&str> = address.as_deref().into_iter().collect();
    c.add_device(&id, name, &addrs).map_err(ce)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real IDs from throwaway Syncthing homes.
    const A: &str = "ZO5XTSC-KSY4NFT-L47RJH4-MVGAWUN-XGPSAS2-VXP4JSK-6LZNSVP-CISINA2";
    const B: &str = "HHI5PAB-J5EJ7WD-4TUXQXE-OG7GERX-PJKGPLY-D7KUYMC-5KBMGDN-FVNAEQM";
    const C: &str = "4DJCJDH-CNW2LLC-SBFRBCJ-5E6LLEV-AIOWSPP-IWVJIS2-GWDBZPH-JPP6FQT";

    #[test]
    fn accepts_real_ids_as_pasted() {
        for id in [A, B, C] {
            assert_eq!(normalize_device_id(id).unwrap(), id);
        }
        assert_eq!(normalize_device_id(&A.to_lowercase()).unwrap(), A);
        assert_eq!(normalize_device_id(&A.replace('-', "")).unwrap(), A);
        assert_eq!(
            normalize_device_id(&format!("  {}\n", A.replace('-', " "))).unwrap(),
            A
        );
    }

    #[test]
    fn catches_typos_with_the_check_characters() {
        let typo = A.replacen("ZO5XTSC", "ZO5XTSD", 1);
        assert_eq!(normalize_device_id(&typo), Err(Error::Checksum));
        let swapped = B.replacen("HHI5PAB", "HHI5APB", 1);
        assert_eq!(normalize_device_id(&swapped), Err(Error::Checksum));
    }

    #[test]
    fn rejects_wrong_shapes() {
        assert_eq!(normalize_device_id(""), Err(Error::Malformed));
        assert_eq!(normalize_device_id("ZO5XTSC"), Err(Error::Malformed));
        assert_eq!(
            normalize_device_id(&A.replace('Z', "1")),
            Err(Error::Malformed)
        );
        assert_eq!(normalize_device_id(&format!("{A}X")), Err(Error::Malformed));
        assert_eq!(normalize_device_id(&"Ç".repeat(56)), Err(Error::Malformed));
    }

    #[test]
    fn addresses_from_what_people_type() {
        let n = |s| normalize_address(s, 22001);
        assert_eq!(n(""), Ok(None));
        assert_eq!(n("   "), Ok(None));
        assert_eq!(n("windows-pc"), Ok(Some("tcp://windows-pc:22001".into())));
        assert_eq!(
            n("windows-pc.tail1234.ts.net"),
            Ok(Some("tcp://windows-pc.tail1234.ts.net:22001".into()))
        );
        assert_eq!(n("100.92.8.12"), Ok(Some("tcp://100.92.8.12:22001".into())));
        assert_eq!(
            n("100.92.8.12:22000"),
            Ok(Some("tcp://100.92.8.12:22000".into()))
        );
        assert_eq!(
            n("tcp://192.168.1.4:22001/"),
            Ok(Some("tcp://192.168.1.4:22001".into()))
        );
        assert_eq!(
            n("[fd7a:115c:a1e0::1]"),
            Ok(Some("tcp://[fd7a:115c:a1e0::1]:22001".into()))
        );
        assert_eq!(n("[fd7a::1]:9000"), Ok(Some("tcp://[fd7a::1]:9000".into())));
    }

    #[test]
    fn refuses_unusable_addresses() {
        let n = |s| normalize_address(s, 22001);
        for bad in [
            "http://x",
            "relay://x",
            "a b",
            "x:0",
            "x:99999",
            "x:abc",
            "fd7a::1",
            "[fd7a::1",
            "x;rm",
            "@host",
        ] {
            assert!(matches!(n(bad), Err(Error::BadAddress(_))), "{bad}");
        }
    }
}
