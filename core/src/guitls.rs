//! Pinned TLS for the local Syncthing GUI/REST API.
//!
//! Closes the check-then-connect race of a port-ownership check: the client
//! trusts only a private CA created here, so a process that grabbed the GUI
//! port cannot complete the TLS handshake and never sees the API key.
//!
//! - A one-off CA signs a leaf certificate for `127.0.0.1` / `localhost`.
//! - The CA private key is never written to disk (dropped after signing), so
//!   nobody can mint another certificate the client would accept.
//! - Syncthing serves the leaf from `https-cert.pem` / `https-key.pem` in its
//!   home; the key file is created with mode 0600 (owner only).
//! - The CA certificate (public) is kept as `twinfolders-gui-ca.pem` for the
//!   client to pin.

use std::fs;
use std::io;
use std::path::Path;

use rcgen::{
    date_time_ymd, BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};

pub const CA_FILE: &str = "twinfolders-gui-ca.pem";
pub const CERT_FILE: &str = "https-cert.pem";
pub const KEY_FILE: &str = "https-key.pem";

fn rc(e: rcgen::Error) -> io::Error {
    io::Error::other(format!("certificate generation: {e}"))
}

/// Writes a file readable only by the owner (0600 on Unix). On Windows the
/// per-user profile directory ACL already restricts access.
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    let _ = fs::remove_file(path);
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    opts.open(path)?.write_all(data)
}

/// Ensures the pinned CA and Syncthing's GUI cert/key exist in `home`.
/// Returns `true` if new material was generated. Must run while Syncthing
/// is stopped (it reads the cert at startup).
pub fn ensure(home: &Path) -> io::Result<bool> {
    let (ca_path, cert_path, key_path) = (
        home.join(CA_FILE),
        home.join(CERT_FILE),
        home.join(KEY_FILE),
    );
    if ca_path.exists() && cert_path.exists() && key_path.exists() {
        return Ok(false);
    }
    fs::create_dir_all(home)?;

    let ca_key = KeyPair::generate().map_err(rc)?;
    let mut ca = CertificateParams::new(Vec::<String>::new()).map_err(rc)?;
    ca.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    ca.distinguished_name
        .push(DnType::CommonName, "TwinFolders local GUI CA");
    ca.key_usages = vec![KeyUsagePurpose::KeyCertSign];
    ca.not_before = date_time_ymd(2025, 1, 1);
    ca.not_after = date_time_ymd(2045, 1, 1);
    let ca_cert = ca.self_signed(&ca_key).map_err(rc)?;
    let issuer = Issuer::from_params(&ca, &ca_key);

    let leaf_key = KeyPair::generate().map_err(rc)?;
    let mut leaf = CertificateParams::new(vec!["127.0.0.1".to_string(), "localhost".to_string()])
        .map_err(rc)?;
    leaf.distinguished_name
        .push(DnType::CommonName, "TwinFolders Syncthing GUI");
    leaf.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    leaf.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    leaf.not_before = date_time_ymd(2025, 1, 1);
    leaf.not_after = date_time_ymd(2045, 1, 1);
    let leaf_cert = leaf.signed_by(&leaf_key, &issuer).map_err(rc)?;

    write_private(&key_path, leaf_key.serialize_pem().as_bytes())?;
    fs::write(&cert_path, leaf_cert.pem())?;
    fs::write(&ca_path, ca_cert.pem())?;
    // ca_key is dropped here and never persisted.
    Ok(true)
}

/// The pinned CA certificate (PEM) for [`crate::client::SyncthingClient::with_pinned_ca`].
pub fn load_ca_pem(home: &Path) -> io::Result<Vec<u8>> {
    fs::read(home.join(CA_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp() -> std::path::PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("twinfolders-guitls-{n}"))
    }

    #[test]
    fn generates_material_once_and_keeps_it() {
        let d = tmp();
        assert!(ensure(&d).unwrap());
        let ca = fs::read_to_string(d.join(CA_FILE)).unwrap();
        let cert = fs::read_to_string(d.join(CERT_FILE)).unwrap();
        let key = fs::read_to_string(d.join(KEY_FILE)).unwrap();
        assert!(ca.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(cert.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(key.contains("PRIVATE KEY"));
        assert_ne!(ca, cert, "leaf is distinct from the CA");
        assert!(!ensure(&d).unwrap(), "idempotent");
        assert_eq!(fs::read_to_string(d.join(CERT_FILE)).unwrap(), cert);
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn regenerates_if_ca_missing() {
        let d = tmp();
        ensure(&d).unwrap();
        let cert1 = fs::read_to_string(d.join(CERT_FILE)).unwrap();
        fs::remove_file(d.join(CA_FILE)).unwrap();
        assert!(ensure(&d).unwrap());
        assert_ne!(fs::read_to_string(d.join(CERT_FILE)).unwrap(), cert1);
        fs::remove_dir_all(d).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn private_key_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp();
        ensure(&d).unwrap();
        let mode = fs::metadata(d.join(KEY_FILE)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_dir_all(d).unwrap();
    }
}
