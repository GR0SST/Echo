use std::fmt;
use zeroize::Zeroizing;

/// Never formatted or serialized; callers must explicitly access the secret.
pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialError;
impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("The system credential store is unavailable")
    }
}
impl std::error::Error for CredentialError {}
/// Native implementations must fail closed, never fall back to plaintext files.
/// Run these blocking methods on a worker, not GPUI's UI thread.
pub trait CredentialStore: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<Secret>, CredentialError>;
    fn set(&self, id: &str, secret: &Secret) -> Result<(), CredentialError>;
    fn delete(&self, id: &str) -> Result<(), CredentialError>;
}
#[cfg(feature = "native")]
pub struct NativeCredentialStore;
#[cfg(feature = "native")]
impl NativeCredentialStore {
    fn entry(id: &str) -> Result<keyring::Entry, CredentialError> {
        if id.is_empty() {
            return Err(CredentialError);
        }
        keyring::Entry::new("com.echo.browser.ai", id).map_err(|_| CredentialError)
    }
}
#[cfg(feature = "native")]
impl CredentialStore for NativeCredentialStore {
    fn get(&self, id: &str) -> Result<Option<Secret>, CredentialError> {
        match Self::entry(id)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(CredentialError),
        }
    }
    fn set(&self, id: &str, secret: &Secret) -> Result<(), CredentialError> {
        Self::entry(id)?
            .set_password(secret.expose())
            .map_err(|_| CredentialError)
    }
    fn delete(&self, id: &str) -> Result<(), CredentialError> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(CredentialError),
        }
    }
}
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::{collections::BTreeMap, sync::Mutex};
    #[derive(Default)]
    pub struct MemoryStore(Mutex<BTreeMap<String, Zeroizing<String>>>);
    impl CredentialStore for MemoryStore {
        fn get(&self, id: &str) -> Result<Option<Secret>, CredentialError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(id)
                .map(|v| Secret::new(v.to_string())))
        }
        fn set(&self, id: &str, secret: &Secret) -> Result<(), CredentialError> {
            self.0
                .lock()
                .unwrap()
                .insert(id.into(), Zeroizing::new(secret.expose().into()));
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<(), CredentialError> {
            self.0.lock().unwrap().remove(id);
            Ok(())
        }
    }
    #[test]
    fn credentials_are_explicitly_retrieved_and_debug_output_is_redacted() {
        let store = MemoryStore::default();
        let secret = Secret::new("test-only-secret".into());
        store.set("provider-a", &secret).unwrap();
        assert!(store.get("provider-b").unwrap().is_none());
        assert_eq!(
            store.get("provider-a").unwrap().unwrap().expose(),
            "test-only-secret"
        );
        assert!(!format!("{secret:?}").contains("test-only-secret"));
        store.delete("provider-a").unwrap();
        assert!(store.get("provider-a").unwrap().is_none());
    }
}

#[cfg(all(test, feature = "native"))]
mod native_tests {
    use super::*;
    #[test]
    #[ignore = "writes and removes only an isolated generated OS credential"]
    fn native_store_round_trip() {
        let id = format!(
            "echo-native-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let store = NativeCredentialStore;
        struct Cleanup<'a>(&'a str);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let _ = NativeCredentialStore.delete(self.0);
            }
        }
        let _cleanup = Cleanup(&id);
        store
            .set(
                &id,
                &Secret::new("generated-test-value-not-an-api-key".into()),
            )
            .unwrap();
        assert_eq!(
            store.get(&id).unwrap().unwrap().expose(),
            "generated-test-value-not-an-api-key"
        );
        store.delete(&id).unwrap();
        assert!(store.get(&id).unwrap().is_none());
    }
}
