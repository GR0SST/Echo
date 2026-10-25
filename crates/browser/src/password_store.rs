//! Profile-local authenticated password vault. Call only on background workers.
//! The OS credential store owns the encryption key; files contain ciphertext
//! only. Neither credentials nor keys implement Debug or enter session JSON.
use importer::Credential;
use ring::{
    aead, digest,
    rand::{SecureRandom, SystemRandom},
};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use zeroize::Zeroizing;

const HEADER: &[u8] = b"ECHO-PW2";
const LEGACY_HEADER: &[u8] = b"ECHO-PW1";
const MAX_NOTE_UTF16: usize = 1000;
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
    pub note: Zeroizing<String>,
}
#[derive(Default)]
struct VaultData {
    passwords: Vec<SavedPassword>,
    blocked: BTreeSet<String>,
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
                note: Zeroizing::new(String::new()),
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return Ok(());
        }
        self.save(&VaultData {
            passwords: rows,
            blocked: BTreeSet::new(),
        })
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
    fn associated_data(&self, legacy: bool) -> String {
        if legacy {
            self.account.clone()
        } else {
            format!("{}\0ECHO-PW2", self.account)
        }
    }
    fn load(&self) -> Result<VaultData> {
        let metadata = match std::fs::symlink_metadata(&self.file) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(VaultData::default()),
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
            || !(bytes.starts_with(HEADER) || bytes.starts_with(LEGACY_HEADER))
        {
            return Err(ERROR.into());
        }
        let legacy = bytes.starts_with(LEGACY_HEADER);
        let associated_data = self.associated_data(legacy);
        let nonce = aead::Nonce::try_assume_unique_for_key(&bytes[HEADER.len()..HEADER.len() + 12])
            .map_err(|_| ERROR)?;
        let plaintext = self
            .key(false)?
            .open_in_place(
                nonce,
                aead::Aad::from(associated_data.as_bytes()),
                &mut bytes[HEADER.len() + 12..],
            )
            .map_err(|_| ERROR)?;
        decode(plaintext, !legacy)
    }
    fn save(&self, data: &VaultData) -> Result<()> {
        // Existing vaults must never silently acquire a replacement key.
        let key = self.key(!self.file.try_exists().map_err(|_| ERROR)?)?;
        let mut payload = encode(data)?;
        let mut nonce = [0; 12];
        SystemRandom::new().fill(&mut nonce).map_err(|_| ERROR)?;
        let associated_data = self.associated_data(false);
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(associated_data.as_bytes()),
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
    pub(crate) fn lookup(&self, origin: &str) -> Result<(bool, Vec<SavedPassword>)> {
        validate_origin(origin)?;
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let data = self.load()?;
        Ok((
            data.blocked.contains(origin),
            data.passwords
                .into_iter()
                .filter(|row| row.credential.origin == origin)
                .collect(),
        ))
    }
    pub(crate) fn list(&self) -> Result<Vec<SavedPassword>> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        self.load().map(|data| data.passwords)
    }
    /// Imports and login updates preserve the user's existing encrypted note.
    pub(crate) fn upsert(&self, credential: &Credential, modified: i64) -> Result<()> {
        self.upsert_note(credential, None, modified)
    }
    pub(crate) fn upsert_with_note(
        &self,
        credential: &Credential,
        note: &str,
        modified: i64,
    ) -> Result<()> {
        self.upsert_note(credential, Some(note), modified)
    }
    fn upsert_note(
        &self,
        credential: &Credential,
        note: Option<&str>,
        modified: i64,
    ) -> Result<()> {
        validate(credential)?;
        if let Some(note) = note {
            validate_note(note)?;
        }
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut data = self.load()?;
        let rows = &mut data.passwords;
        let note = match note {
            Some(note) => Zeroizing::new(note.to_owned()),
            None => rows
                .iter()
                .find(|r| same_identity(&r.credential, credential))
                .map(|r| r.note.clone())
                .unwrap_or_default(),
        };
        rows.retain(|r| !same_identity(&r.credential, credential));
        rows.push(SavedPassword {
            credential: copy_credential(credential),
            modified,
            note,
        });
        self.save(&data)
    }
    pub(crate) fn insert(&self, credential: &Credential, note: &str, modified: i64) -> Result<()> {
        validate(credential)?;
        validate_note(note)?;
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut data = self.load()?;
        if data
            .passwords
            .iter()
            .any(|row| same_identity(&row.credential, credential))
        {
            return Err(ERROR.into());
        }
        data.passwords.push(SavedPassword {
            credential: copy_credential(credential),
            note: Zeroizing::new(note.into()),
            modified,
        });
        self.save(&data)
    }
    /// Editing an identity is one authenticated write. A stale selection or a
    /// conflicting identity never deletes or overwrites another credential.
    pub(crate) fn replace(
        &self,
        old_origin: &str,
        old_username: &str,
        credential: &Credential,
        note: &str,
        modified: i64,
    ) -> Result<()> {
        validate(credential)?;
        validate_note(note)?;
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut data = self.load()?;
        let rows = &mut data.passwords;
        let index = rows
            .iter()
            .position(|r| {
                r.credential.origin == old_origin && r.credential.username == old_username
            })
            .ok_or(ERROR)?;
        if rows
            .iter()
            .enumerate()
            .any(|(i, r)| i != index && same_identity(&r.credential, credential))
        {
            return Err(ERROR.into());
        }
        rows[index] = SavedPassword {
            credential: copy_credential(credential),
            modified,
            note: Zeroizing::new(note.to_owned()),
        };
        self.save(&data)
    }
    pub(crate) fn remove(&self, origin: &str, username: &str) -> Result<()> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut data = self.load()?;
        let rows = &mut data.passwords;
        rows.retain(|r| r.credential.origin != origin || r.credential.username != username);
        self.save(&data)
    }
    pub(crate) fn is_blocked(&self, origin: &str) -> Result<bool> {
        validate_origin(origin)?;
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        Ok(self.load()?.blocked.contains(origin))
    }
    pub(crate) fn block(&self, origin: &str) -> Result<()> {
        self.set_blocked(origin, true)
    }
    pub(crate) fn unblock(&self, origin: &str) -> Result<()> {
        self.set_blocked(origin, false)
    }
    fn set_blocked(&self, origin: &str, blocked: bool) -> Result<()> {
        validate_origin(origin)?;
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut data = self.load()?;
        if blocked {
            data.blocked.insert(origin.to_owned());
        } else {
            data.blocked.remove(origin);
        }
        self.save(&data)
    }
    pub(crate) fn clear_since(&self, since: i64) -> Result<()> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut data = self.load()?;
        let rows = &mut data.passwords;
        rows.retain(|r| r.modified < since);
        self.save(&data)
    }
}
fn same_identity(a: &Credential, b: &Credential) -> bool {
    a.origin == b.origin && a.username == b.username
}
fn copy_credential(c: &Credential) -> Credential {
    Credential {
        origin: c.origin.clone(),
        username: c.username.clone(),
        password: c.password.clone(),
    }
}
fn validate_note(note: &str) -> Result<()> {
    // Arc's shared password-manager resource uses JavaScript string.length.
    if note.encode_utf16().count() > MAX_NOTE_UTF16 {
        return Err(ERROR.into());
    }
    Ok(())
}
fn validate_origin(origin: &str) -> Result<()> {
    let url = url::Url::parse(origin).map_err(|_| ERROR)?;
    if origin.len() > MAX_FIELD
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.origin().ascii_serialization() != origin
    {
        return Err(ERROR.into());
    }
    Ok(())
}
fn validate(c: &Credential) -> Result<()> {
    validate_origin(&c.origin)?;
    if c.password.is_empty() || c.username.len() > MAX_FIELD || c.password.len() > MAX_FIELD {
        return Err(ERROR.into());
    }
    Ok(())
}
fn encode(data: &VaultData) -> Result<Zeroizing<Vec<u8>>> {
    let rows = &data.passwords;
    if rows.len() > 10_000 {
        return Err(ERROR.into());
    }
    let mut out = Zeroizing::new(Vec::new());
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for row in rows {
        validate(&row.credential)?;
        validate_note(&row.note)?;
        out.extend_from_slice(&row.modified.to_le_bytes());
        for field in [
            &row.credential.origin,
            &row.credential.username,
            &row.credential.password,
            &*row.note,
        ] {
            out.extend_from_slice(&(field.len() as u32).to_le_bytes());
            out.extend_from_slice(field.as_bytes());
        }
        if out.len() > MAX_BYTES - HEADER.len() - 12 - aead::MAX_TAG_LEN {
            return Err(ERROR.into());
        }
    }
    if data.blocked.len() > 10_000 {
        return Err(ERROR.into());
    }
    out.extend_from_slice(&(data.blocked.len() as u32).to_le_bytes());
    for origin in &data.blocked {
        validate_origin(origin)?;
        out.extend_from_slice(&(origin.len() as u32).to_le_bytes());
        out.extend_from_slice(origin.as_bytes());
        if out.len() > MAX_BYTES - HEADER.len() - 12 - aead::MAX_TAG_LEN {
            return Err(ERROR.into());
        }
    }
    Ok(out)
}
fn decode(mut bytes: &[u8], has_notes: bool) -> Result<VaultData> {
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
        let note = Zeroizing::new(if has_notes {
            field(&mut bytes)?
        } else {
            String::new()
        });
        validate_note(&note)?;
        validate(&credential)?;
        out.push(SavedPassword {
            credential,
            modified,
            note,
        });
    }
    let mut blocked = BTreeSet::new();
    if has_notes {
        let count = number(&mut bytes)?;
        if count > 10_000 {
            return Err(ERROR.into());
        }
        for _ in 0..count {
            let origin = field(&mut bytes)?;
            validate_origin(&origin)?;
            blocked.insert(origin);
        }
    }
    if !bytes.is_empty() {
        return Err(ERROR.into());
    }
    Ok(VaultData {
        passwords: out,
        blocked,
    })
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
            assert!(!first
                .windows(plaintext.len())
                .any(|s| s == plaintext.as_bytes()));
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
        assert!(vault
            .remove("https://echo-password-test.invalid", "echo-test-user")
            .is_err());
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
    fn password_manager_insert_rejects_duplicates_without_replacing_password_or_note() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::with_keys(root.path(), "default", MemoryKeys::default()).unwrap();
        vault
            .insert(&credential("first-password"), "first-note", 1)
            .unwrap();
        assert!(vault
            .insert(&credential("replacement-password"), "replacement-note", 2)
            .is_err());
        let rows = vault.list().unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].credential.password == "first-password");
        assert!(rows[0].note.as_str() == "first-note");
    }
    #[test]
    fn password_vault_empty_lookups_do_not_create_a_key_or_persist_a_vault() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::with_keys(root.path(), "default", MemoryKeys::default()).unwrap();
        vault.migrate_if_absent(|| Ok(Vec::new())).unwrap();
        assert!(!vault.file.exists());
        let (blocked, rows) = vault.lookup("https://echo-password-test.invalid").unwrap();
        assert!(!blocked);
        assert!(rows.is_empty());
        assert!(!vault.file.exists());
    }
    #[test]
    fn password_vault_migrates_older_echo_imports_once_without_overwriting_saved_changes() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::with_keys(root.path(), "default", MemoryKeys::default()).unwrap();
        assert!(vault
            .migrate_if_absent(|| Err("Unavailable test keychain".into()))
            .is_err());
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
    #[test]
    fn password_notes_are_encrypted_bounded_and_retained_by_imports() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::with_keys(root.path(), "default", MemoryKeys::default()).unwrap();
        let note = "🗝".repeat(500);
        vault
            .upsert_with_note(&credential("test-original"), &note, 1)
            .unwrap();
        let bytes = std::fs::read(&vault.file).unwrap();
        assert!(!bytes.windows(note.len()).any(|w| w == note.as_bytes()));
        assert!(vault
            .upsert_with_note(&credential("must-not-save"), &"🗝".repeat(501), 2)
            .is_err());
        assert_eq!(std::fs::read(&vault.file).unwrap(), bytes);
        vault.upsert(&credential("test-import-update"), 3).unwrap();
        let rows = vault.list().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].credential.password, "test-import-update");
        assert_eq!(rows[0].note.as_str(), note);
        assert_eq!(rows[0].modified, 3);
    }
    #[test]
    fn password_edit_is_atomic_and_cannot_overwrite_a_different_identity() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::with_keys(root.path(), "default", MemoryKeys::default()).unwrap();
        let original = credential("original-test-password");
        vault
            .upsert_with_note(&original, "original-test-note", 1)
            .unwrap();
        let mut neighbor = credential("neighbor-test-password");
        neighbor.username = "neighbor".into();
        vault.upsert(&neighbor, 2).unwrap();
        let bytes = std::fs::read(&vault.file).unwrap();
        assert!(vault
            .replace(
                &original.origin,
                &original.username,
                &neighbor,
                "conflict",
                3
            )
            .is_err());
        assert!(vault
            .replace(&original.origin, "missing", &original, "stale", 3)
            .is_err());
        assert_eq!(std::fs::read(&vault.file).unwrap(), bytes);
        let mut edited = credential("edited-test-password");
        edited.username = "renamed-test-user".into();
        vault
            .replace(
                &original.origin,
                &original.username,
                &edited,
                "edited-test-note",
                4,
            )
            .unwrap();
        let rows = vault.list().unwrap();
        assert_eq!(rows.len(), 2);
        let changed = rows
            .iter()
            .find(|r| r.credential.username == edited.username)
            .unwrap();
        assert_eq!(changed.note.as_str(), "edited-test-note");
        assert_eq!(changed.modified, 4);
        assert!(!rows
            .iter()
            .any(|r| r.credential.username == original.username));
        assert_eq!(
            rows.iter()
                .find(|r| r.credential.username == "neighbor")
                .unwrap()
                .credential
                .password,
            "neighbor-test-password"
        );
    }
    #[test]
    fn password_v1_files_upgrade_without_changing_keys_and_versions_are_authenticated() {
        let root = tempfile::tempdir().unwrap();
        let keys = MemoryKeys::default();
        let vault = Vault::with_keys(root.path(), "default", keys.clone()).unwrap();
        let c = credential("legacy-v1-test-password");
        let mut payload = Zeroizing::new(Vec::new());
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&5i64.to_le_bytes());
        for field in [&c.origin, &c.username, &c.password] {
            payload.extend_from_slice(&(field.len() as u32).to_le_bytes());
            payload.extend_from_slice(field.as_bytes());
        }
        let nonce = [7u8; 12];
        vault
            .key(true)
            .unwrap()
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(vault.account.as_bytes()),
                &mut *payload,
            )
            .unwrap();
        let mut file = LEGACY_HEADER.to_vec();
        file.extend_from_slice(&nonce);
        file.extend_from_slice(&payload);
        std::fs::write(&vault.file, file).unwrap();
        let key = keys.get(&vault.account).unwrap().unwrap();
        let rows = vault.list().unwrap();
        assert_eq!(rows[0].credential.password, c.password);
        assert!(rows[0].note.is_empty());
        vault.upsert_with_note(&c, "upgraded-test-note", 6).unwrap();
        assert!(keys.get(&vault.account).unwrap().unwrap().as_slice() == key.as_slice());
        let mut bytes = std::fs::read(&vault.file).unwrap();
        assert!(bytes.starts_with(HEADER));
        assert_eq!(vault.list().unwrap()[0].note.as_str(), "upgraded-test-note");
        bytes[..LEGACY_HEADER.len()].copy_from_slice(LEGACY_HEADER);
        std::fs::write(&vault.file, bytes).unwrap();
        assert!(
            vault.list().is_err(),
            "Changing only the format version must fail authentication"
        );
    }
    #[test]
    fn never_save_policy_is_encrypted_profile_owned_and_survives_password_edits() {
        let root = tempfile::tempdir().unwrap();
        let keys = MemoryKeys::default();
        let vault = Vault::with_keys(root.path(), "default", keys.clone()).unwrap();
        let c = credential("test-policy-password");
        vault.block(&c.origin).unwrap();
        vault.upsert_with_note(&c, "test-policy-note", 1).unwrap();
        vault.upsert(&c, 2).unwrap();
        vault
            .replace(&c.origin, &c.username, &c, "updated-test-policy-note", 3)
            .unwrap();
        assert!(vault.is_blocked(&c.origin).unwrap());
        let reopened = Vault::with_keys(root.path(), "default", keys.clone()).unwrap();
        assert!(reopened.is_blocked(&c.origin).unwrap());
        assert_eq!(reopened.list().unwrap().len(), 1);
        let other = Vault::with_keys(root.path(), "profile-2", keys).unwrap();
        assert!(!other.is_blocked(&c.origin).unwrap());
        let bytes = std::fs::read(&vault.file).unwrap();
        assert!(!bytes
            .windows(c.origin.len())
            .any(|w| w == c.origin.as_bytes()));
        assert!(vault
            .block("https://echo-password-test.invalid/path")
            .is_err());
        assert_eq!(bytes, std::fs::read(&vault.file).unwrap());
        vault.remove(&c.origin, &c.username).unwrap();
        vault.clear_since(0).unwrap();
        assert!(vault.list().unwrap().is_empty());
        assert!(vault.is_blocked(&c.origin).unwrap());
        vault.unblock(&c.origin).unwrap();
        assert!(!vault.is_blocked(&c.origin).unwrap());
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
    if phase == "frames-seed" {
        for (origin, user, password) in [
            ("http://127.0.0.1:53861", "echo-test-alpha", "test-only-alpha-53861"),
            ("http://127.0.0.1:53861", "echo-test-beta", "test-only-beta-53861"),
            ("http://127.0.0.1:53862", "echo-frame-test", "synthetic-frame-only"),
            ("http://127.0.0.1:53863", "echo-test-alpha", "test-only-alpha-53861"),
            ("http://127.0.0.1:53863", "echo-test-beta", "test-only-beta-53861"),
        ] {
            crate::import_credentials::SystemCredentialStore.store("default", &importer::Credential {
                origin: origin.into(), username: user.into(), password: password.into(),
            })?;
        }
        return Ok(());
    }
    if phase == "manager-seed" {
        let imported=importer::parse_password_csv(b"url,username,password\nhttp://127.0.0.1:53849/login,echo-password-test-69cec731,echo-native-manager-seed-only\n")?;
        crate::import_credentials::SystemCredentialStore.store("default", &imported[0])?;
        return Ok(());
    }
    if phase == "manager-verify" || phase == "manager-after-delete" {
        let rows = crate::import_credentials::vault("default")?.list()?;
        let seed = rows.iter().any(|r| {
            r.credential.origin == "http://127.0.0.1:53849"
                && r.credential.username == "echo-password-test-69cec731"
                && r.credential.password == "echo-native-manager-seed-only"
        });
        let added = rows.iter().any(|r| {
            r.credential.origin == "https://echo-password-manager-added.invalid"
                && r.credential.username == "manager-added-user"
                && r.credential.password == "echo-native-manager-added-only"
                && r.note.as_str() == "native manager note"
        });
        let expected = if phase == "manager-verify" { 2 } else { 1 };
        if !seed || rows.len() != expected || added != (phase == "manager-verify") {
            return Err("Manager native fixture state differs from its synthetic records.".into());
        }
        return Ok(());
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
            crate::import_credentials::vault(profile)?.upsert_with_note(
                &rows[0],
                "echo-encrypted-native-note",
                chrono::Utc::now().timestamp(),
            )?;
            // Reimporting a password must keep a note written by the manager.
            crate::import_credentials::SystemCredentialStore.store(profile, &rows[0])?;
            crate::import_credentials::vault(profile)?
                .block("https://echo-password-never-test-69cec731.invalid")?;
        }
        "verify" => {
            let vault = crate::import_credentials::vault(profile)?;
            if !vault.is_blocked("https://echo-password-never-test-69cec731.invalid")?
                || crate::import_credentials::vault("password-native-other")?
                    .is_blocked("https://echo-password-never-test-69cec731.invalid")?
            {
                return Err("Password policy profile separation failed.".into());
            }
            let saved = vault.list()?;
            if saved.len() != 2
                || !saved.iter().any(|r| {
                    r.credential.origin == rows[0].origin
                        && r.credential.username == rows[0].username
                        && r.credential.password == rows[0].password
                        && r.note.as_str() == "echo-encrypted-native-note"
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
            vault.unblock("https://echo-password-never-test-69cec731.invalid")?;
            legacy.delete_credential().map_err(|_| ERROR)?;
            if !vault.list()?.is_empty() {
                return Err("Password fixture deletion failed.".into());
            }
        }
        _ => return Err("Unknown password fixture phase.".into()),
    }
    Ok(())
}
