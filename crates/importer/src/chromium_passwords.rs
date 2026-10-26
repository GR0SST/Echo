//! Chromium browsers (Chrome, Edge, Brave, Arc) keep saved passwords in
//! `Login Data`, encrypted with a key the OS holds: on macOS the login
//! Keychain's "<Browser> Safe Storage" item, whose first read shows the
//! system's consent prompt. Linux builds without a keyring use a fixed key.
use crate::{BrowserKind, Credential, Result, database};
use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use zeroize::Zeroize;

pub struct Key([u8; 16]);
impl Drop for Key {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
impl Key {
    pub fn derive(secret: &[u8], iterations: u32) -> Self {
        let mut key = [0; 16];
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(secret, b"saltysalt", iterations, &mut key);
        Self(key)
    }
}

/// The OS-held key that decrypts `browser`'s saved passwords.
pub fn key(browser: BrowserKind) -> Result<Key> {
    #[cfg(target_os = "macos")]
    {
        let (service, account) = match browser {
            BrowserKind::Chrome => ("Chrome Safe Storage", "Chrome"),
            BrowserKind::Edge => ("Microsoft Edge Safe Storage", "Microsoft Edge"),
            BrowserKind::Brave => ("Brave Safe Storage", "Brave"),
            BrowserKind::Arc => ("Arc Safe Storage", "Arc"),
            _ => return Err(format!("{} passwords need a CSV export.", browser.name())),
        };
        let mut secret = keyring::Entry::new(service, account)
            .and_then(|entry| entry.get_password())
            .map_err(|_| format!("Echo wasn't allowed to read {}'s saved passwords.", browser.name()))?;
        let key = Key::derive(secret.as_bytes(), 1003);
        secret.zeroize();
        Ok(key)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = browser;
        Ok(Key::derive(b"peanuts", 1))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err(format!(
            "{} protects its saved passwords on this system; export them to a CSV file instead.",
            browser.name()
        ))
    }
}

/// Decrypts one `password_value`: "v10"/"v11", AES-128-CBC with a space IV.
fn decrypt(value: &[u8], key: &Key) -> Option<String> {
    let body = value.strip_prefix(b"v10").or_else(|| value.strip_prefix(b"v11"))?;
    let mut buffer = body.to_vec();
    let plain = cbc::Decryptor::<aes::Aes128>::new(&key.0.into(), &[b' '; 16].into())
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .ok()
        .map(|plain| String::from_utf8(plain.to_vec()));
    buffer.zeroize();
    plain?.ok()
}

/// Saved logins of one profile, skipping never-save entries and any row the
/// key cannot decrypt (e.g. one written by another OS key).
pub fn read(profile: &Path, key: &Key, cancel: &AtomicBool) -> Result<Vec<Credential>> {
    let path = profile.join("Login Data");
    if !path.exists() {
        return Ok(vec![]);
    }
    let db = database(&path)?;
    let mut query = db
        .prepare("SELECT origin_url, username_value, password_value FROM logins WHERE blacklisted_by_user = 0")
        .map_err(|e| e.to_string())?;
    let mut rows = query.query([]).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        if cancel.load(Ordering::Relaxed) {
            return Err("Import cancelled.".into());
        }
        let origin: String = row.get(0).map_err(|e| e.to_string())?;
        let Ok(parsed) = url::Url::parse(&origin) else { continue };
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            continue;
        }
        let username: String = row.get::<_, Option<String>>(1).map_err(|e| e.to_string())?.unwrap_or_default();
        let mut value: Vec<u8> = row.get::<_, Option<Vec<u8>>>(2).map_err(|e| e.to_string())?.unwrap_or_default();
        let password = decrypt(&value, key);
        value.zeroize();
        let Some(password) = password.filter(|p| !p.is_empty()) else { continue };
        out.push(Credential { origin: parsed.origin().ascii_serialization(), username, password });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockEncryptMut;

    fn encrypt(plain: &str, key: &Key) -> Vec<u8> {
        let mut buffer = vec![0u8; plain.len() + 16];
        let cipher = cbc::Encryptor::<aes::Aes128>::new(&key.0.into(), &[b' '; 16].into())
            .encrypt_padded_b2b_mut::<Pkcs7>(plain.as_bytes(), &mut buffer)
            .unwrap()
            .to_vec();
        [b"v10".as_slice(), &cipher].concat()
    }

    #[test]
    fn decrypts_chromium_logins_and_skips_never_save_and_foreign_rows() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::derive(b"fixture-safe-storage", 1003);
        let other = Key::derive(b"another-key", 1003);
        let db = rusqlite::Connection::open(dir.path().join("Login Data")).unwrap();
        db.execute_batch("CREATE TABLE logins(origin_url TEXT, username_value TEXT, password_value BLOB, blacklisted_by_user INTEGER);").unwrap();
        for (origin, user, value, never) in [
            ("https://site.example/login", "me", encrypt("s3cret, with comma", &key), 0),
            ("https://never.example/", "", encrypt("x", &key), 1),
            ("https://foreign.example/", "you", encrypt("hidden", &other), 0),
            ("chrome://settings", "x", encrypt("y", &key), 0),
        ] {
            db.execute("INSERT INTO logins VALUES (?1, ?2, ?3, ?4)", rusqlite::params![origin, user, value, never]).unwrap();
        }
        drop(db);
        let credentials = read(dir.path(), &key, &AtomicBool::new(false)).unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].origin, "https://site.example");
        assert_eq!(credentials[0].username, "me");
        assert_eq!(credentials[0].password, "s3cret, with comma");
    }
}
