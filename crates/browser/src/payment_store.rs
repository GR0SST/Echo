//! Profile-local authenticated store of payment cards, sealed the same way as
//! the password vault: the OS credential store holds a per-profile key and the
//! file holds AES-256-GCM ciphertext only. Card numbers and security codes
//! never implement Debug or enter session JSON. Call only on background workers.
use crate::password_store::Keys;
use browser_core::payments::{card_digits, valid_card_number, CardInput};
use ring::{
    aead, digest,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

const HEADER: &[u8] = b"ECHO-PC1";
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_CARDS: usize = 1000;
const ERROR: &str = "The local payment methods store could not be opened.";
static ACCESS: Mutex<()> = Mutex::new(());
type Result<T> = std::result::Result<T, String>;
pub(crate) type SharedKeys = Arc<dyn Keys + Send + Sync>;

pub(crate) struct SavedCard {
    pub id: u64,
    pub number: Zeroizing<String>,
    pub month: u8,
    pub year: u16,
    pub name: String,
    pub nickname: String,
    pub cvc: Zeroizing<String>,
    pub modified: i64,
}

#[derive(Serialize)]
struct Stored<'a> {
    id: u64,
    number: &'a str,
    month: u8,
    year: u16,
    name: &'a str,
    nickname: &'a str,
    cvc: &'a str,
    modified: i64,
}
#[derive(Deserialize)]
struct Loaded {
    id: u64,
    number: String,
    month: u8,
    year: u16,
    name: String,
    nickname: String,
    cvc: String,
    modified: i64,
}

pub(crate) struct Vault {
    file: PathBuf,
    account: String,
    keys: SharedKeys,
}

impl Vault {
    pub(crate) fn open(root: &Path, profile: &str) -> Result<Self> {
        Self::with_keys(root, profile, Arc::new(crate::password_store::NativeKeys))
    }
    pub(crate) fn with_keys(root: &Path, profile: &str, keys: SharedKeys) -> Result<Self> {
        if profile != "default" && engine::StorageKey::new(profile).is_err() {
            return Err(ERROR.into());
        }
        std::fs::create_dir_all(root).map_err(|_| ERROR)?;
        let root = root.canonicalize().map_err(|_| ERROR)?;
        let directory = root.join("Payments");
        if std::fs::symlink_metadata(&directory).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(ERROR.into());
        }
        std::fs::create_dir_all(&directory).map_err(|_| ERROR)?;
        // A separate account from the password vault's, so the two never
        // share a key.
        let identity = format!("{}\0payments\0{profile}", root.to_str().ok_or(ERROR)?);
        let account = digest::digest(&digest::SHA256, identity.as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self { file: directory.join(format!("{profile}.vault")), account, keys })
    }
    /// Only for a retired profile; never the Default profile's store.
    pub(crate) fn destroy_retired(&self) -> Result<()> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        if self.file.file_name().is_some_and(|name| name == "default.vault") {
            return Err(ERROR.into());
        }
        match std::fs::symlink_metadata(&self.file) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => std::fs::remove_file(&self.file).map_err(|_| ERROR)?,
            Ok(_) => return Err(ERROR.into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(ERROR.into()),
        }
        self.keys.delete(&self.account)
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
        Ok(aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &key).map_err(|_| ERROR)?))
    }
    fn associated_data(&self) -> String {
        format!("{}\0ECHO-PC1", self.account)
    }
    fn load(&self) -> Result<Vec<SavedCard>> {
        let metadata = match std::fs::symlink_metadata(&self.file) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(ERROR.into()),
        };
        if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
            return Err(ERROR.into());
        }
        let mut bytes = Zeroizing::new(Vec::new());
        std::fs::File::open(&self.file).map_err(|_| ERROR)?.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| ERROR)?;
        if bytes.len() > MAX_BYTES || bytes.len() < HEADER.len() + 12 + aead::MAX_TAG_LEN || !bytes.starts_with(HEADER) {
            return Err(ERROR.into());
        }
        let nonce = aead::Nonce::try_assume_unique_for_key(&bytes[HEADER.len()..HEADER.len() + 12]).map_err(|_| ERROR)?;
        let associated_data = self.associated_data();
        let plaintext = self
            .key(false)?
            .open_in_place(nonce, aead::Aad::from(associated_data.as_bytes()), &mut bytes[HEADER.len() + 12..])
            .map_err(|_| ERROR)?;
        let rows: Vec<Loaded> = serde_json::from_slice(plaintext).map_err(|_| ERROR)?;
        if rows.len() > MAX_CARDS {
            return Err(ERROR.into());
        }
        rows.into_iter()
            .map(|r| {
                let card = SavedCard {
                    id: r.id,
                    number: Zeroizing::new(r.number),
                    month: r.month,
                    year: r.year,
                    name: r.name,
                    nickname: r.nickname,
                    cvc: Zeroizing::new(r.cvc),
                    modified: r.modified,
                };
                validate(&card)?;
                Ok(card)
            })
            .collect()
    }
    fn save(&self, cards: &[SavedCard]) -> Result<()> {
        if cards.len() > MAX_CARDS {
            return Err(ERROR.into());
        }
        // An existing store must never silently acquire a replacement key.
        let key = self.key(!self.file.try_exists().map_err(|_| ERROR)?)?;
        let rows: Vec<Stored> = cards
            .iter()
            .map(|c| Stored { id: c.id, number: &c.number, month: c.month, year: c.year, name: &c.name, nickname: &c.nickname, cvc: &c.cvc, modified: c.modified })
            .collect();
        let mut payload = Zeroizing::new(Vec::new());
        serde_json::to_writer(&mut *payload, &rows).map_err(|_| ERROR)?;
        let mut nonce = [0; 12];
        SystemRandom::new().fill(&mut nonce).map_err(|_| ERROR)?;
        let associated_data = self.associated_data();
        key.seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::from(associated_data.as_bytes()), &mut *payload)
            .map_err(|_| ERROR)?;
        let mut temporary = tempfile::NamedTempFile::new_in(self.file.parent().ok_or(ERROR)?).map_err(|_| ERROR)?;
        temporary
            .write_all(HEADER)
            .and_then(|_| temporary.write_all(&nonce))
            .and_then(|_| temporary.write_all(&payload))
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| ERROR)?;
        temporary.persist(&self.file).map_err(|_| ERROR)?;
        Ok(())
    }
    pub(crate) fn list(&self) -> Result<Vec<SavedCard>> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        self.load()
    }
    /// Adds a card (`id` None) or replaces card `id` in one authenticated
    /// write. `cvc` None keeps the saved security code. A number already
    /// saved on another card is rejected rather than duplicated.
    pub(crate) fn upsert(&self, id: Option<u64>, card: &CardInput, cvc: Option<&str>, modified: i64) -> Result<u64> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut cards = self.load()?;
        let digits = Zeroizing::new(card_digits(&card.number).ok_or(ERROR)?);
        let index = match id {
            Some(id) => Some(cards.iter().position(|c| c.id == id).ok_or(ERROR)?),
            None => None,
        };
        if cards.iter().enumerate().any(|(i, c)| Some(i) != index && card_digits(&c.number).is_some_and(|d| *d == **digits)) {
            return Err(ERROR.into());
        }
        let id = id.unwrap_or_else(|| cards.iter().map(|c| c.id).max().unwrap_or(0) + 1);
        let cvc = match (cvc, index) {
            (Some(cvc), _) => Zeroizing::new(cvc.to_owned()),
            (None, Some(index)) => cards[index].cvc.clone(),
            (None, None) => Zeroizing::new(String::new()),
        };
        let saved = SavedCard {
            id,
            number: Zeroizing::new(digits.to_string()),
            month: card.month,
            year: card.year,
            name: card.name.trim().to_owned(),
            nickname: card.nickname.trim().to_owned(),
            cvc,
            modified,
        };
        validate(&saved)?;
        match index {
            Some(index) => cards[index] = saved,
            None => cards.push(saved),
        }
        self.save(&cards)?;
        Ok(id)
    }
    pub(crate) fn remove(&self, id: u64) -> Result<()> {
        let _lock = ACCESS.lock().map_err(|_| ERROR)?;
        let mut cards = self.load()?;
        let before = cards.len();
        cards.retain(|c| c.id != id);
        if cards.len() == before {
            return Ok(());
        }
        self.save(&cards)
    }
}

fn validate(card: &SavedCard) -> Result<()> {
    if !valid_card_number(&card.number)
        || !(1..=12).contains(&card.month)
        || card.name.len() > 1024
        || card.nickname.len() > 1024
        || card.cvc.len() > 4
        || !card.cvc.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(ERROR.into());
    }
    Ok(())
}

/// The cards the settings page shows: everything but the full number and CVC.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CardSummary {
    pub id: u64,
    pub label: String,
    pub brand: &'static str,
    pub month: u8,
    pub year: u16,
    pub name: String,
    pub nickname: String,
    pub has_cvc: bool,
}
impl CardSummary {
    fn of(card: &SavedCard) -> Self {
        let brand = browser_core::payments::CardBrand::detect(&card.number);
        Self {
            id: card.id,
            label: browser_core::payments::card_label(&card.number, &card.nickname),
            brand: brand.badge(),
            month: card.month,
            year: card.year,
            name: card.name.clone(),
            nickname: card.nickname.clone(),
            has_cvc: !card.cvc.is_empty(),
        }
    }
}

/// The profile stores the page reads and writes; tests use a temporary root
/// and in-memory keys, the app the profile directory and the OS keychain.
pub(crate) struct Storage {
    root: PathBuf,
    keys: SharedKeys,
    #[cfg(test)]
    _temporary: Arc<tempfile::TempDir>,
}
impl gpui::Global for Storage {}
impl Clone for Storage {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            keys: self.keys.clone(),
            #[cfg(test)]
            _temporary: self._temporary.clone(),
        }
    }
}
impl Storage {
    pub(crate) fn get(cx: &mut gpui::App) -> Self {
        if !cx.has_global::<Storage>() {
            #[cfg(not(test))]
            let storage = Storage { root: crate::profile_dir(), keys: Arc::new(crate::password_store::NativeKeys) };
            #[cfg(test)]
            let storage = {
                let temporary = Arc::new(tempfile::tempdir().expect("temporary payments root"));
                Storage { root: temporary.path().to_owned(), keys: Arc::new(tests::MemoryKeys::default()), _temporary: temporary }
            };
            cx.set_global(storage);
        }
        cx.global::<Storage>().clone()
    }
    fn vault(&self, profile: &str) -> Result<Vault> {
        Vault::with_keys(&self.root, profile, self.keys.clone())
    }
    pub(crate) fn list(&self, profile: &str) -> Result<Vec<CardSummary>> {
        Ok(self.vault(profile)?.list()?.iter().map(CardSummary::of).collect())
    }
    /// The saved number and CVC of one card, for the Edit form.
    pub(crate) fn secrets(&self, profile: &str, id: u64) -> Result<(Zeroizing<String>, Zeroizing<String>)> {
        let cards = self.vault(profile)?.list()?;
        let card = cards.iter().find(|c| c.id == id).ok_or(ERROR)?;
        Ok((card.number.clone(), card.cvc.clone()))
    }
    pub(crate) fn upsert(&self, profile: &str, id: Option<u64>, card: &CardInput, cvc: Option<&str>) -> Result<u64> {
        self.vault(profile)?.upsert(id, card, cvc, chrono::Utc::now().timestamp())
    }
    pub(crate) fn remove(&self, profile: &str, id: u64) -> Result<()> {
        self.vault(profile)?.remove(id)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    #[derive(Clone, Default)]
    pub(crate) struct MemoryKeys(Arc<Mutex<HashMap<String, Vec<u8>>>>);
    impl Keys for MemoryKeys {
        fn delete(&self, account: &str) -> Result<()> {
            self.0.lock().unwrap().remove(account);
            Ok(())
        }
        fn get(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
            Ok(self.0.lock().unwrap().get(account).cloned().map(Zeroizing::new))
        }
        fn set(&self, account: &str, key: &[u8]) -> Result<()> {
            self.0.lock().unwrap().insert(account.into(), key.into());
            Ok(())
        }
    }
    fn card(number: &str) -> CardInput {
        CardInput { number: number.into(), month: 2, year: 2028, name: "Echo Test".into(), nickname: String::new(), cvc: String::new() }
    }

    #[test]
    fn payment_vault_round_trips_without_plaintext_and_isolates_profiles() {
        let root = tempfile::tempdir().unwrap();
        let keys: SharedKeys = Arc::new(MemoryKeys::default());
        let vault = Vault::with_keys(root.path(), "default", keys.clone()).unwrap();
        assert!(vault.list().unwrap().is_empty());
        assert!(!vault.file.exists(), "An empty read must not create a store");
        let id = vault.upsert(None, &card("5555 5555 5555 4444"), Some("123"), 1).unwrap();
        let bytes = std::fs::read(&vault.file).unwrap();
        for plaintext in ["5555555555554444", "Echo Test", "123\""] {
            assert!(!bytes.windows(plaintext.len()).any(|w| w == plaintext.as_bytes()), "{plaintext}");
        }
        let other = Vault::with_keys(root.path(), "user-profile-2", keys.clone()).unwrap();
        assert!(other.list().unwrap().is_empty());
        let reopened = Vault::with_keys(root.path(), "default", keys.clone()).unwrap();
        let cards = reopened.list().unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].number.as_str(), "5555555555554444");
        assert_eq!(cards[0].cvc.as_str(), "123");
        // Editing without a CVC keeps the saved one.
        let mut edited = card("5555555555554444");
        edited.month = 3;
        reopened.upsert(Some(id), &edited, None, 2).unwrap();
        let cards = reopened.list().unwrap();
        assert_eq!((cards[0].month, cards[0].cvc.as_str()), (3, "123"));
        // Duplicates, invalid numbers and stale ids never write.
        let before = std::fs::read(&vault.file).unwrap();
        assert!(reopened.upsert(None, &card("5555555555554444"), None, 3).is_err());
        assert!(reopened.upsert(None, &card("4111111111111112"), None, 3).is_err());
        assert!(reopened.upsert(Some(99), &card("4111111111111111"), None, 3).is_err());
        assert_eq!(std::fs::read(&vault.file).unwrap(), before);
        reopened.remove(id).unwrap();
        assert!(reopened.list().unwrap().is_empty());
    }

    #[test]
    fn payment_vault_fails_closed_and_retires_only_other_profiles() {
        let root = tempfile::tempdir().unwrap();
        let keys: SharedKeys = Arc::new(MemoryKeys::default());
        let vault = Vault::with_keys(root.path(), "default", keys.clone()).unwrap();
        vault.upsert(None, &card("4111111111111111"), None, 1).unwrap();
        let missing = Vault::with_keys(root.path(), "default", Arc::new(MemoryKeys::default())).unwrap();
        assert!(missing.list().is_err());
        assert!(missing.upsert(None, &card("378282246310005"), None, 2).is_err());
        let mut corrupt = std::fs::read(&vault.file).unwrap();
        *corrupt.last_mut().unwrap() ^= 1;
        let retired = Vault::with_keys(root.path(), "user-profile-3", keys.clone()).unwrap();
        retired.upsert(None, &card("4111111111111111"), None, 1).unwrap();
        std::fs::copy(&vault.file, &retired.file).unwrap();
        assert!(retired.list().is_err(), "Another profile's file must not authenticate");
        assert!(vault.destroy_retired().is_err());
        retired.destroy_retired().unwrap();
        assert!(!retired.file.exists());
        std::fs::write(&vault.file, corrupt).unwrap();
        assert!(vault.list().is_err());
        assert!(Vault::with_keys(root.path(), "../escape", keys).is_err());
    }
}
