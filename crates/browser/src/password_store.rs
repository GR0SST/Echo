//! Profile-local authenticated password vault. Call only on background workers.
//! The OS credential store owns the encryption key; files contain ciphertext
//! only. Neither credentials nor keys implement Debug or enter session JSON.
use importer::Credential;
use ring::{
    aead, digest,
    rand::{SecureRandom, SystemRandom},
};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use zeroize::Zeroizing;

const HEADER: &[u8] = b"ECHO-PW1";
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_FIELD: usize = 1024 * 1024;
const ERROR: &str = "The local password store could not be opened.";
static ACCESS: Mutex<()> = Mutex::new(());
type Result<T> = std::result::Result<T, String>;

pub(crate) trait Keys {
    fn get(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>>;
    fn set(&self, account: &str, key: &[u8]) -> Result<()>;
}
pub(crate) struct NativeKeys;
impl Keys for NativeKeys {
    fn get(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let entry =
            keyring::Entry::new("dev.echo.browser.password-vault", account).map_err(|_| ERROR)?;
        match entry.get_secret() {
            Ok(key) => Ok(Some(Zeroizing::new(key))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(ERROR.into()),
        }
    }
    fn set(&self, account: &str, key: &[u8]) -> Result<()> {
        keyring::Entry::new("dev.echo.browser.password-vault", account)
            .map_err(|_| ERROR)?
            .set_secret(key)
            .map_err(|_| ERROR.into())
    }
}

pub(crate) struct SavedPassword {
    pub credential: Credential,
    pub modified: i64,
}
pub(crate) struct Vault<K = NativeKeys> {
    file: PathBuf,
    account: String,
    keys: K,
}
impl Vault {
    pub(crate) fn open(root: &Path, profile: &str) -> Result<Self> {
        Self::with_keys(root, profile, NativeKeys)
    }
}
impl<K: Keys> Vault<K> {
    /// Retain passwords imported by older Echo builds. The caller reads only
    /// Echo's previous credential service; Arc's store is never queried.
    pub(crate) fn migrate_if_absent(
        &self,
        read: impl FnOnce() -> Result<Vec<Credential>>,
    ) -> Result<()> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        match std::fs::symlink_metadata(&self.file) {
            Ok(_) => return self.load().map(|_| ()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ERROR.into()),
        }
        let rows = read()?
            .into_iter()
            .map(|credential| SavedPassword {
                credential,
                modified: 0,
            })
            .collect::<Vec<_>>();
        self.save(&rows)
    }
    fn with_keys(root: &Path, profile: &str, keys: K) -> Result<Self> {
        if profile != "default" && engine::StorageKey::new(profile).is_err() {
            return Err(ERROR.into());
        }
        std::fs::create_dir_all(root).map_err(|_| ERROR)?;
        let root = root.canonicalize().map_err(|_| ERROR)?;
        let directory = root.join("Passwords");
        if std::fs::symlink_metadata(&directory).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(ERROR.into());
        }
        std::fs::create_dir_all(&directory).map_err(|_| ERROR)?;
        let identity = format!("{}\0{profile}", root.to_str().ok_or(ERROR)?);
        let account = digest::digest(&digest::SHA256, identity.as_bytes())
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Self {
            file: directory.join(format!("{profile}.vault")),
            account,
            keys,
        })
    }
    fn key(&self, create: bool) -> Result<aead::LessSafeKey> {
        let key = match self.keys.get(&self.account)? {
            Some(key) => key,
            None if create => {
                let mut key = Zeroizing::new(vec![0; 32]);
                SystemRandom::new().fill(&mut key).map_err(|_| ERROR)?;
                self.keys.set(&self.account, &key)?;
                key
            }
            None => return Err(ERROR.into()),
        };
        Ok(aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, &key).map_err(|_| ERROR)?,
        ))
    }
    fn load(&self) -> Result<Vec<SavedPassword>> {
        let metadata = match std::fs::symlink_metadata(&self.file) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(ERROR.into()),
        };
        if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
            return Err(ERROR.into());
        }
        let mut bytes = Zeroizing::new(Vec::new());
        std::fs::File::open(&self.file)
            .map_err(|_| ERROR)?
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ERROR)?;
        if bytes.len() > MAX_BYTES
            || bytes.len() < HEADER.len() + 12 + aead::MAX_TAG_LEN
            || !bytes.starts_with(HEADER)
        {
            return Err(ERROR.into());
        }
        let nonce = aead::Nonce::try_assume_unique_for_key(&bytes[HEADER.len()..HEADER.len() + 12])
            .map_err(|_| ERROR)?;
        let plaintext = self
            .key(false)?
            .open_in_place(
                nonce,
                aead::Aad::from(self.account.as_bytes()),
                &mut bytes[HEADER.len() + 12..],
            )
            .map_err(|_| ERROR)?;
        decode(plaintext)
    }
    fn save(&self, rows: &[SavedPassword]) -> Result<()> {
        // Existing vaults must never silently acquire a replacement key.
        let key = self.key(!self.file.try_exists().map_err(|_| ERROR)?)?;
        let mut payload = encode(rows)?;
        let mut nonce = [0; 12];
        SystemRandom::new().fill(&mut nonce).map_err(|_| ERROR)?;
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(self.account.as_bytes()),
            &mut *payload,
        )
        .map_err(|_| ERROR)?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(self.file.parent().ok_or(ERROR)?).map_err(|_| ERROR)?;
        temporary
            .write_all(HEADER)
            .and_then(|_| temporary.write_all(&nonce))
            .and_then(|_| temporary.write_all(&payload))
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| ERROR)?;
        temporary.persist(&self.file).map_err(|_| ERROR)?;
        Ok(())
    }
    pub(crate) fn list(&self) -> Result<Vec<SavedPassword>> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        self.load()
    }
    pub(crate) fn upsert(&self, credential: &Credential, modified: i64) -> Result<()> {
        validate(credential)?;
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut rows = self.load()?;
        rows.retain(|r| {
            r.credential.origin != credential.origin || r.credential.username != credential.username
        });
        rows.push(SavedPassword {
            credential: Credential {
                origin: credential.origin.clone(),
                username: credential.username.clone(),
                password: credential.password.clone(),
            },
            modified,
        });
        self.save(&rows)
    }
    pub(crate) fn remove(&self, origin: &str, username: &str) -> Result<()> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut rows = self.load()?;
        rows.retain(|r| r.credential.origin != origin || r.credential.username != username);
        self.save(&rows)
    }
    pub(crate) fn clear_since(&self, since: i64) -> Result<()> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut rows = self.load()?;
        rows.retain(|r| r.modified < since);
        self.save(&rows)
    }
}
fn validate(c: &Credential) -> Result<()> {
    let url = url::Url::parse(&c.origin).map_err(|_| ERROR)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.origin().ascii_serialization() != c.origin
        || c.password.is_empty()
        || [&c.origin, &c.username, &c.password]
            .iter()
            .any(|s| s.len() > MAX_FIELD)
    {
        return Err(ERROR.into());
    }
    Ok(())
}
fn encode(rows: &[SavedPassword]) -> Result<Zeroizing<Vec<u8>>> {
    if rows.len() > 10_000 {
        return Err(ERROR.into());
    }
    let mut out = Zeroizing::new(Vec::new());
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for row in rows {
        validate(&row.credential)?;
        out.extend_from_slice(&row.modified.to_le_bytes());
        for field in [
            &row.credential.origin,
            &row.credential.username,
            &row.credential.password,
        ] {
            out.extend_from_slice(&(field.len() as u32).to_le_bytes());
            out.extend_from_slice(field.as_bytes());
        }
        if out.len() > MAX_BYTES - HEADER.len() - 12 - aead::MAX_TAG_LEN {
            return Err(ERROR.into());
        }
    }
    Ok(out)
}
fn decode(mut bytes: &[u8]) -> Result<Vec<SavedPassword>> {
    fn take<'a>(bytes: &mut &'a [u8], size: usize) -> Result<&'a [u8]> {
        if size > bytes.len() {
            return Err(ERROR.into());
        }
        let (head, tail) = bytes.split_at(size);
        *bytes = tail;
        Ok(head)
    }
    fn number(bytes: &mut &[u8]) -> Result<usize> {
        Ok(u32::from_le_bytes(take(bytes, 4)?.try_into().map_err(|_| ERROR)?) as usize)
    }
    fn field(bytes: &mut &[u8]) -> Result<String> {
        let size = number(bytes)?;
        if size > MAX_FIELD {
            return Err(ERROR.into());
        }
        Ok(std::str::from_utf8(take(bytes, size)?)
            .map_err(|_| ERROR)?
            .into())
    }
    let count = number(&mut bytes)?;
    if count > 10_000 {
        return Err(ERROR.into());
    }
    let mut out = Vec::new();
    for _ in 0..count {
        let modified = i64::from_le_bytes(take(&mut bytes, 8)?.try_into().map_err(|_| ERROR)?);
        let origin = field(&mut bytes)?;
        let username = field(&mut bytes)?;
        let password = field(&mut bytes)?;
        let credential = Credential {
            origin,
            username,
            password,
        };
        validate(&credential)?;
        out.push(SavedPassword {
            credential,
            modified,
        });
    }
    if !bytes.is_empty() {
        return Err(ERROR.into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Arc};
    #[derive(Clone, Default)]
    struct MemoryKeys(Arc<Mutex<HashMap<String, Vec<u8>>>>);
    impl Keys for MemoryKeys {
        fn get(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(account)
                .cloned()
                .map(Zeroizing::new))
        }
        fn set(&self, account: &str, key: &[u8]) -> Result<()> {
            self.0.lock().unwrap().insert(account.into(), key.into());
            Ok(())
        }
    }
    fn credential(password: &str) -> Credential {
        Credential {
            origin: "https://echo-password-test.invalid".into(),
            username: "echo-test-user".into(),
            password: password.into(),
        }
    }
    #[test]
    fn password_vault_reopens_updates_and_deletes_without_plaintext_on_disk() {
        let root = tempfile::tempdir().unwrap();
        let keys = MemoryKeys::default();
        let vault = Vault::with_keys(root.path(), "default", keys.clone()).unwrap();
        assert!(vault.list().unwrap().is_empty());
        vault
            .upsert(&credential("test-only-password-1"), 10)
            .unwrap();
        let first = std::fs::read(&vault.file).unwrap();
        for plaintext in [
            "test-only-password-1",
            "echo-test-user",
            "echo-password-test.invalid",
        ] {
            assert!(
                !first
                    .windows(plaintext.len())
                    .any(|s| s == plaintext.as_bytes())
            );
        }
        vault
            .upsert(&credential("test-only-password-2"), 20)
            .unwrap();
        let second = std::fs::read(&vault.file).unwrap();
        assert_ne!(
            &first[HEADER.len()..HEADER.len() + 12],
            &second[HEADER.len()..HEADER.len() + 12]
        );
        let reopened = Vault::with_keys(root.path(), "default", keys).unwrap();
        let rows = reopened.list().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].credential.password, "test-only-password-2");
        assert_eq!(rows[0].modified, 20);
        reopened.clear_since(21).unwrap();
        assert_eq!(reopened.list().unwrap().len(), 1);
        reopened.clear_since(20).unwrap();
        assert!(reopened.list().unwrap().is_empty());
        reopened
            .upsert(&credential("test-only-password-3"), 30)
            .unwrap();
        reopened
            .remove(&credential("").origin, "echo-test-user")
            .unwrap();
        assert!(reopened.list().unwrap().is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&reopened.file)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn password_vault_fails_closed_for_missing_keys_tampering_and_profile_substitution() {
        let root = tempfile::tempdir().unwrap();
        let keys = MemoryKeys::default();
        let vault = Vault::with_keys(root.path(), "default", keys.clone()).unwrap();
        vault.upsert(&credential("test-secret"), 1).unwrap();
        let original = std::fs::read(&vault.file).unwrap();
        let other = Vault::with_keys(root.path(), "profile-2", keys.clone()).unwrap();
        other
            .upsert(&credential("different-test-secret"), 1)
            .unwrap();
        std::fs::copy(&vault.file, &other.file).unwrap();
        assert!(other.list().is_err());
        let missing = Vault::with_keys(root.path(), "default", MemoryKeys::default()).unwrap();
        assert!(missing.list().is_err());
        assert!(missing.upsert(&credential("must-not-replace"), 2).is_err());
        assert_eq!(std::fs::read(&vault.file).unwrap(), original);
        let mut corrupt = original.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        std::fs::write(&vault.file, &corrupt).unwrap();
        assert!(vault.list().is_err());
        assert!(
            vault
                .remove("https://echo-password-test.invalid", "echo-test-user")
                .is_err()
        );
        assert_eq!(std::fs::read(&vault.file).unwrap(), corrupt);
        assert!(Vault::with_keys(root.path(), "../escape", keys).is_err());
    }
    #[test]
    fn password_vault_imports_csv_using_the_same_origin_and_username_identity() {
        let rows = importer::parse_password_csv(b"url,username,password\nhttps://echo-password-test.invalid/login,echo-test-user,test-csv-password\n").unwrap();
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::with_keys(root.path(), "default", MemoryKeys::default()).unwrap();
        vault.upsert(&rows[0], 1).unwrap();
        assert_eq!(
            vault.list().unwrap()[0].credential.origin,
            "https://echo-password-test.invalid"
        );
        let mut invalid = credential("test");
        invalid.origin.push_str("/login");
        assert!(vault.upsert(&invalid, 2).is_err());
        assert_eq!(vault.list().unwrap().len(), 1);
    }
    #[test]
    fn password_vault_migrates_older_echo_imports_once_without_overwriting_saved_changes() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::with_keys(root.path(), "default", MemoryKeys::default()).unwrap();
        assert!(
            vault
                .migrate_if_absent(|| Err("Unavailable test keychain".into()))
                .is_err()
        );
        assert!(!vault.file.exists());
        vault
            .migrate_if_absent(|| Ok(vec![credential("older-test-password")]))
            .unwrap();
        assert_eq!(vault.list().unwrap()[0].modified, 0);
        vault.upsert(&credential("newer-test-password"), 2).unwrap();
        vault
            .migrate_if_absent(|| panic!("Migration must not read the old keychain again"))
            .unwrap();
        assert_eq!(
            vault.list().unwrap()[0].credential.password,
            "newer-test-password"
        );
    }
}

/// Explicit native acceptance fixture: only synthetic credentials, and only in
/// drive.sh's disposable profile. No vault data or key is printed to a log.
#[cfg(all(target_os = "macos", debug_assertions))]
pub(crate) fn native_fixture(phase: &str) -> Result<()> {
    use importer::CredentialStore;
    if std::env::var("BROWSER_PASSWORD_STORE_FIXTURE").as_deref() != Ok("1")
        || std::env::var_os("BROWSER_DATA_DIR").is_none()
        || crate::profile_dir().file_name().and_then(|n| n.to_str())
            != Some("browser-drive-profile")
    {
        return Err(
            "Password fixture requires an explicitly enabled disposable drive profile.".into(),
        );
    }
    let profile = "password-native-check";
    let rows = importer::parse_password_csv(b"url,username,password\nhttps://echo-password-test-69cec731.invalid/login,echo-test-user,echo-native-test-only-password\n")?;
    let legacy = keyring::Entry::new(
        &format!(
            "dev.echo.browser.passwords:{}",
            crate::profile_dir().display()
        ),
        &serde_json::to_string(&(profile, &rows[0].origin, "echo-legacy-user"))
            .map_err(|_| ERROR)?,
    )
    .map_err(|_| ERROR)?;
    match phase {
        "write" => {
            legacy
                .set_secret(b"echo-legacy-test-only-password")
                .map_err(|_| ERROR)?;
            crate::import_credentials::SystemCredentialStore.store(profile, &rows[0])?;
        }
        "verify" => {
            let vault = crate::import_credentials::vault(profile)?;
            let saved = vault.list()?;
            if saved.len() != 2
                || !saved.iter().any(|r| {
                    r.credential.origin == rows[0].origin
                        && r.credential.username == rows[0].username
                        && r.credential.password == rows[0].password
                })
                || !saved.iter().any(|r| {
                    r.credential.origin == rows[0].origin
                        && r.credential.username == "echo-legacy-user"
                        && r.credential.password == "echo-legacy-test-only-password"
                })
            {
                return Err("Password fixture round trip failed.".into());
            }
            if !crate::import_credentials::vault("password-native-other")?
                .list()?
                .is_empty()
            {
                return Err("Password fixture profile separation failed.".into());
            }
            vault.remove(&rows[0].origin, &rows[0].username)?;
            vault.remove(&rows[0].origin, "echo-legacy-user")?;
            legacy.delete_credential().map_err(|_| ERROR)?;
            if !vault.list()?.is_empty() {
                return Err("Password fixture deletion failed.".into());
            }
        }
        _ => return Err("Unknown password fixture phase.".into()),
    }
    Ok(())
}
