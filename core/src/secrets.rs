//! Syncthing API key in the OS credential store (macOS Keychain / Windows
//! Credential Manager), service `twinfolders`, account `syncthing-api-key`.
//!
//! The key also lives in Syncthing's own `config.xml` (mode 0600, written by
//! Syncthing). TwinFolders reads it from there once and keeps it in the OS
//! store, so the app's own settings never contain secrets and the key never
//! appears in a plist, task XML or process command line.

use std::path::Path;

use crate::stconfig;

pub const SERVICE: &str = "twinfolders";
pub const API_KEY_ACCOUNT: &str = "syncthing-api-key";
/// Random password protecting the Syncthing web GUI (user `twinfolders`).
pub const GUI_PASSWORD_ACCOUNT: &str = "syncthing-gui-password";

fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| format!("credential store: {e}"))
}

pub fn store_api_key(key: &str) -> Result<(), String> {
    store(API_KEY_ACCOUNT, key)
}

pub fn load_api_key() -> Result<Option<String>, String> {
    load(API_KEY_ACCOUNT)
}

pub fn delete_api_key() -> Result<(), String> {
    delete(API_KEY_ACCOUNT)
}

pub fn store_gui_password(pw: &str) -> Result<(), String> {
    store(GUI_PASSWORD_ACCOUNT, pw)
}

pub fn delete_gui_password() -> Result<(), String> {
    delete(GUI_PASSWORD_ACCOUNT)
}

/// 32 random characters from the OS CSPRNG (alphabet without look-alikes).
pub fn random_password() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
    let mut buf = [0u8; 64];
    getrandom::fill(&mut buf).expect("OS random number generator");
    // Rejection sampling avoids modulo bias.
    let limit = 256 - (256 % ALPHABET.len());
    buf.iter()
        .filter(|&&b| (b as usize) < limit)
        .take(32)
        .map(|&b| ALPHABET[b as usize % ALPHABET.len()] as char)
        .collect()
}

fn store(account: &str, value: &str) -> Result<(), String> {
    entry(account)?
        .set_password(value)
        .map_err(|e| format!("credential store: {e}"))
}

fn load(account: &str) -> Result<Option<String>, String> {
    match entry(account)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("credential store: {e}")),
    }
}

fn delete(account: &str) -> Result<(), String> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("credential store: {e}")),
    }
}

/// API key for the Syncthing instance in `home`: from the OS store, or read
/// from `config.xml` and stored. If the stored key no longer matches the
/// config (e.g. home re-generated), the config wins and the store is updated.
pub fn resolve_api_key(home: &Path) -> Result<String, String> {
    let xml = std::fs::read_to_string(home.join("config.xml"))
        .map_err(|e| format!("reading config.xml: {e}"))?;
    let from_config = stconfig::read_gui(&xml).map_err(|e| e.to_string())?.api_key;
    if from_config.is_empty() {
        return Err("config.xml has an empty API key".into());
    }
    if load_api_key()?.as_deref() != Some(from_config.as_str()) {
        store_api_key(&from_config)?;
    }
    Ok(from_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_passwords_are_long_and_distinct() {
        let a = random_password();
        let b = random_password();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    /// Touches the real OS credential store, so it only runs when
    /// `TWINFOLDERS_KEYCHAIN_TEST=1`. Uses a throwaway account and cleans up.
    #[test]
    fn roundtrip_in_real_credential_store() {
        if std::env::var("TWINFOLDERS_KEYCHAIN_TEST").as_deref() != Ok("1") {
            eprintln!("SKIPPED: set TWINFOLDERS_KEYCHAIN_TEST=1 to touch the OS credential store");
            return;
        }
        let acct = "test-roundtrip";
        delete(acct).unwrap();
        assert_eq!(load(acct).unwrap(), None);
        store(acct, "value-1").unwrap();
        assert_eq!(load(acct).unwrap().as_deref(), Some("value-1"));
        store(acct, "value-2").unwrap();
        assert_eq!(load(acct).unwrap().as_deref(), Some("value-2"));
        delete(acct).unwrap();
        assert_eq!(load(acct).unwrap(), None);
        eprintln!("EVIDENCE: OS credential store roundtrip ok");
    }
}
