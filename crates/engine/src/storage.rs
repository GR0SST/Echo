//! Filesystem policy and ownership-free persistent context registry.
//!
//! Default uses `root/Default`; named keys use the direct child
//! `root/profile-<key>`. CEF 154 cannot persist profiles at `root/Profiles/<key>`.
//! Cache-path equality only validates configuration; full process-restart cookie
//! and localStorage verification must establish runtime persistence separately.
//!
//! No profile deletion API is provided. Removing a profile in the application
//! must only remove its metadata for now. Physical deletion needs a future
//! barrier proving all pages, inspectors, forks, handles and CEF background
//! users have released storage; dropping a handle is not that barrier. CEF is
//! never shut down/reinitialized to switch profiles.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// CEF's initialization callback only signals this flag. It holds no Runtime,
/// engine, native parent or application borrow, and never pumps or creates pages.
#[derive(Clone, Default)]
pub(crate) struct ContextReady(Arc<AtomicBool>);

impl ContextReady {
    pub(crate) fn initialized() -> Self {
        let ready = Self::default();
        ready.mark_initialized();
        ready
    }

    pub(crate) fn mark_initialized(&self) {
        self.0.store(true, Ordering::Release);
    }
    fn is_ready(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Sidecar keyed by the exact existing Rc lease. No context types or shutdown
/// ownership change: entries are weak, including private contexts. Callback
/// flags may outlive this registry without retaining the initialized runtime.
pub(crate) struct ContextReadiness<T>(RefCell<Vec<(Weak<T>, ContextReady)>>);

impl<T> Default for ContextReadiness<T> {
    fn default() -> Self {
        Self(RefCell::new(Vec::new()))
    }
}

impl<T> ContextReadiness<T> {
    pub(crate) fn track(&self, context: &Rc<T>, ready: ContextReady) {
        let mut entries = self.0.borrow_mut();
        entries.retain(|(context, _)| context.strong_count() != 0);
        let weak = Rc::downgrade(context);
        // Registry hits must retain the ORIGINAL callback flag, rather than
        // reset readiness with a fresh flag from a factory that was not called.
        if !entries
            .iter()
            .any(|(context, _)| Weak::ptr_eq(context, &weak))
        {
            entries.push((weak, ready));
        }
    }

    pub(crate) fn is_ready(&self, context: &Rc<T>) -> bool {
        let weak = Rc::downgrade(context);
        self.0
            .borrow()
            .iter()
            .any(|(context, ready)| Weak::ptr_eq(context, &weak) && ready.is_ready())
    }
}

/// Immutable on-disk identity, independent of profile display names or core IDs.
/// Use a stable generated key: 1–128 lowercase ASCII letters/digits/`-`/`_`,
/// starting with a letter or digit. `default` and Windows device names are
/// reserved. Keys cannot be renamed; changing one selects different storage.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StorageKey(String);

impl StorageKey {
    pub fn new(key: &str) -> Result<Self, String> {
        let valid = !key.is_empty()
            && key.len() <= 128
            && key.as_bytes()[0].is_ascii_alphanumeric()
            && key
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_');
        let reserved = matches!(key, "default" | "con" | "prn" | "aux" | "nul")
            || (key.len() == 4
                && (key.starts_with("com") || key.starts_with("lpt"))
                && matches!(key.as_bytes()[3], b'1'..=b'9'));
        if !valid || reserved {
            return Err("Invalid or reserved Chromium storage key".into());
        }
        Ok(Self(key.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) fn utf8_path(path: &Path) -> Result<&str, String> {
    let value = path
        .to_str()
        .ok_or("Chromium storage path must be valid UTF-8")?;
    if value.is_empty() || value.contains('\0') {
        return Err("Chromium storage path is empty or contains NUL".into());
    }
    Ok(value)
}

/// Run even when CEF has already initialized. A relative root is resolved once
/// against the caller's working directory; CEF only ever sees canonical paths.
pub(crate) fn canonical_root(path: &Path) -> Result<PathBuf, String> {
    utf8_path(path)?;
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Chromium storage root must not contain parent traversal".into());
    }
    std::fs::create_dir_all(path)
        .map_err(|e| format!("Cannot create Chromium storage root: {e}"))?;
    let root = std::fs::canonicalize(path)
        .map_err(|e| format!("Cannot resolve Chromium storage root: {e}"))?;
    utf8_path(&root)?;
    Ok(root)
}

pub(crate) fn require_same_root(expected: &Path, actual: &Path) -> Result<(), String> {
    if expected != actual {
        return Err("CEF is already initialized with a different storage root".into());
    }
    Ok(())
}

/// Reject aliases as well as escapes below the canonical root. This prevents
/// two keys (or Default and a key) from accidentally sharing a cache directory.
fn child_dir(parent: &Path, name: &str) -> Result<PathBuf, String> {
    // Recheck the canonical parent before creating anything below it. The root
    // may have been replaced since initialization.
    let resolved_parent = std::fs::canonicalize(parent).map_err(|e| e.to_string())?;
    if resolved_parent != parent {
        return Err("Chromium storage parent resolves to an alias or escapes its root".into());
    }
    let path = parent.join(name);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("Chromium storage directories must not be symlinks".into());
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match std::fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("Cannot create Chromium storage directory: {e}")),
        },
        Err(e) => return Err(format!("Cannot inspect Chromium storage directory: {e}")),
    }
    let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Chromium storage path must be a directory, not a symlink".into());
    }
    let canonical = std::fs::canonicalize(&path).map_err(|e| e.to_string())?;
    utf8_path(&canonical)?;
    if canonical != path {
        return Err("Chromium storage directory resolves to an alias or escapes its root".into());
    }
    Ok(canonical)
}

pub(crate) fn default_path(root: &Path) -> Result<PathBuf, String> {
    child_dir(root, "Default")
}

pub(crate) fn named_path(root: &Path, key: &StorageKey) -> Result<PathBuf, String> {
    // CEF 154 ChromeBrowserContext::InitializeAsync only loads a disk profile
    // when cache_path is user_data_dir itself or its direct child. A nested
    // Profiles/<key> path silently selects OffTheRecord despite GetCachePath
    // still returning the configured path. Preserve Default separately.
    child_dir(root, &format!("profile-{}", key.as_str()))
}

/// Check CEF's configured cache path against the exact canonical path supplied.
/// An empty or mismatched value is rejected, but equality is NOT proof of runtime
/// persistence: CEF 154 can fall back to OffTheRecord while retaining this value.
/// Full process-restart cookie/localStorage verification is required before
/// accepting persistent-profile behavior, even with the direct-child policy.
pub(crate) fn verify_cache_path(expected: &Path, effective: &str) -> Result<(), String> {
    if effective.is_empty() || Path::new(effective) != expected {
        return Err("CEF configured cache path differs from the requested persistent path".into());
    }
    Ok(())
}

/// Values deliberately have no back-reference to Runtime. Public handles own
/// Runtime separately, so strong persistent retention cannot form an Rc cycle.
pub(crate) struct PersistentContexts<T>(RefCell<HashMap<PathBuf, Rc<T>>>);

impl<T> Default for PersistentContexts<T> {
    fn default() -> Self {
        Self(RefCell::new(HashMap::new()))
    }
}

impl<T> PersistentContexts<T> {
    pub(crate) fn snapshot(&self) -> Vec<(PathBuf, Rc<T>)> {
        self.0
            .borrow()
            .iter()
            .map(|(path, context)| (path.clone(), context.clone()))
            .collect()
    }

    pub(crate) fn clear(&self) {
        // Drop native handles outside the registry borrow.
        let contexts = std::mem::take(&mut *self.0.borrow_mut());
        drop(contexts);
    }

    pub(crate) fn resolve(
        &self,
        path: PathBuf,
        factory: impl FnOnce() -> Result<T, String>,
    ) -> Result<Rc<T>, String> {
        if let Some(context) = self.0.borrow().get(&path) {
            return Ok(context.clone());
        }
        // Do not hold a registry borrow across CEF calls, which may re-enter.
        // Failed creation never publishes a context or falls back to Default.
        let context = Rc::new(factory()?);
        Ok(self.0.borrow_mut().entry(path).or_insert(context).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn readiness_requires_callback_and_uses_the_exact_shared_lease() {
        let registry = ContextReadiness::default();
        let context = Rc::new("work");
        let unrelated = Rc::new("work");
        let callback = ContextReady::default();
        registry.track(&context, callback.clone());
        assert!(!registry.is_ready(&context));
        assert!(!registry.is_ready(&unrelated));
        let fork_lease = context.clone();
        callback.mark_initialized();
        assert!(registry.is_ready(&context));
        assert!(registry.is_ready(&fork_lease));
        assert!(!registry.is_ready(&unrelated));
        // Repeated lookup must not reset an already initialized context.
        registry.track(&context, ContextReady::default());
        assert!(registry.is_ready(&context));
    }

    #[test]
    fn cached_pending_lookup_does_not_replace_the_original_callback() {
        let registry = ContextReadiness::default();
        let context = Rc::new("work");
        let original = ContextReady::default();
        let unused_factory_flag = ContextReady::default();
        registry.track(&context, original.clone());
        registry.track(&context, unused_factory_flag.clone());
        unused_factory_flag.mark_initialized();
        assert!(!registry.is_ready(&context));
        original.mark_initialized();
        assert!(registry.is_ready(&context));
    }

    #[test]
    fn readiness_does_not_retain_private_context_or_let_late_callback_ready_its_replacement() {
        let registry = ContextReadiness::default();
        let private = Rc::new("private");
        let private_weak = Rc::downgrade(&private);
        let late_callback = ContextReady::default();
        registry.track(&private, late_callback.clone());
        drop(private);
        assert!(private_weak.upgrade().is_none());
        let replacement = Rc::new("private");
        let replacement_callback = ContextReady::default();
        registry.track(&replacement, replacement_callback.clone());
        late_callback.mark_initialized();
        assert!(!registry.is_ready(&replacement));
        replacement_callback.mark_initialized();
        assert!(registry.is_ready(&replacement));
        // CEF callbacks can outlive the sidecar without owning Runtime.
        drop(registry);
        late_callback.mark_initialized();
    }

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "echo-engine-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            // Fail rather than reuse preexisting data, even after a crashed test.
            std::fs::create_dir(&path).unwrap();
            Self(std::fs::canonicalize(path).unwrap())
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn storage_keys_are_immutable_portable_components() {
        for key in ["a", "0", "profile-123_abc", &"a".repeat(128)] {
            let key = StorageKey::new(key).unwrap();
            assert_eq!(StorageKey::new(key.as_str()).unwrap(), key.clone());
        }
        for key in [
            "",
            ".",
            "..",
            "Default",
            "default",
            "DEFAULT",
            "a/b",
            "a\\b",
            "/root",
            "../x",
            "x/..",
            " x",
            "x ",
            "x.",
            "a:b",
            "a\0b",
            "é",
            "A",
            "_a",
            "-a",
            "con",
            "prn",
            "aux",
            "nul",
            "com1",
            "com9",
            "lpt1",
            "lpt9",
            &"a".repeat(129),
        ] {
            assert!(StorageKey::new(key).is_err(), "accepted {key:?}");
        }
    }

    #[test]
    fn canonical_root_and_legacy_default_keep_their_layout() {
        let temp = TempRoot::new();
        let root = canonical_root(&temp.0.join("Chromium")).unwrap();
        assert!(root.is_absolute());
        assert_eq!(root, temp.0.join("Chromium"));
        assert_eq!(default_path(&root).unwrap(), root.join("Default"));
        let named = named_path(&root, &StorageKey::new("work").unwrap()).unwrap();
        assert_eq!(named, root.join("profile-work"));
        // Match CEF 154's disk-profile selection condition, not merely its
        // broader settings validation rule allowing arbitrary descendants.
        assert_eq!(named.parent(), Some(root.as_path()));
        assert_ne!(named, default_path(&root).unwrap());
        assert!(!root.join("Profiles").exists());
        assert_eq!(canonical_root(&root).unwrap(), root);
    }

    #[test]
    fn invalid_roots_and_different_initialized_roots_fail() {
        let temp = TempRoot::new();
        assert!(canonical_root(Path::new("")).is_err());
        assert!(canonical_root(Path::new("nul\0root")).is_err());
        assert!(canonical_root(&temp.0.join("../escape")).is_err());
        let file = temp.0.join("file");
        std::fs::write(&file, "not a directory").unwrap();
        assert!(canonical_root(&file).is_err());
        assert!(require_same_root(&temp.0, &temp.0).is_ok());
        assert!(require_same_root(&temp.0, &temp.0.join("different")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_roots_fail_without_lossy_replacement_or_creation() {
        use std::os::unix::ffi::OsStringExt;
        let temp = TempRoot::new();
        let path = temp.0.join(std::ffi::OsString::from_vec(vec![0xff]));
        assert!(canonical_root(&path).is_err());
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn canonical_root_must_also_be_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let temp = TempRoot::new();
        let target = temp.0.join(std::ffi::OsString::from_vec(vec![0xff]));
        if let Err(error) = std::fs::create_dir(&target) {
            // Some Unix filesystems (including macOS) reject non-UTF8 names.
            // There is then no symlink target to construct. Still assert our
            // deliberate input rejection, and exercise a valid canonical root.
            assert!(
                matches!(canonical_root(&target), Err(message) if message == "Chromium storage path must be valid UTF-8"),
                "filesystem rejected fixture: {error}"
            );
            assert!(!target.exists());
            let valid = temp.0.join("valid-target");
            assert_eq!(canonical_root(&valid).unwrap(), valid);
            return;
        }
        let alias = temp.0.join("alias");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        assert!(canonical_root(&alias).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn default_and_named_profile_symlinks_cannot_escape_or_alias_storage() {
        use std::os::unix::fs::symlink;
        let temp = TempRoot::new();
        let outside = TempRoot::new();
        symlink(&outside.0, temp.0.join("Default")).unwrap();
        assert!(default_path(&temp.0).is_err());
        symlink(&outside.0, temp.0.join("profile-work")).unwrap();
        let key = StorageKey::new("work").unwrap();
        assert!(named_path(&temp.0, &key).is_err());
        assert!(!outside.0.join("profile-work").exists());
        std::fs::remove_file(temp.0.join("profile-work")).unwrap();
        let first = named_path(&temp.0, &key).unwrap();
        symlink(&first, temp.0.join("profile-alias")).unwrap();
        assert!(named_path(&temp.0, &StorageKey::new("alias").unwrap()).is_err());
        std::fs::remove_dir(&first).unwrap();
        symlink(&outside.0, &first).unwrap();
        // The same lookup is validated again, even after initial resolution.
        assert!(named_path(&temp.0, &key).is_err());
    }

    #[test]
    fn files_cannot_be_used_as_profile_directories() {
        let temp = TempRoot::new();
        std::fs::write(temp.0.join("Default"), "file").unwrap();
        assert!(default_path(&temp.0).is_err());
        std::fs::write(temp.0.join("profile-work"), "file").unwrap();
        assert!(named_path(&temp.0, &StorageKey::new("work").unwrap()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn replaced_root_is_rejected_before_creating_profiles_outside_it() {
        let temp = TempRoot::new();
        let outside = TempRoot::new();
        let root = canonical_root(&temp.0.join("Chromium")).unwrap();
        std::fs::rename(&root, temp.0.join("original")).unwrap();
        std::os::unix::fs::symlink(&outside.0, &root).unwrap();
        assert!(named_path(&root, &StorageKey::new("work").unwrap()).is_err());
        assert!(!outside.0.join("profile-work").exists());
        assert!(require_same_root(&root, &canonical_root(&root).unwrap()).is_err());
    }

    #[test]
    fn configured_cache_paths_reject_empty_or_mismatched_values() {
        let temp = TempRoot::new();
        let expected = named_path(&temp.0, &StorageKey::new("work").unwrap()).unwrap();
        assert!(verify_cache_path(&expected, "").is_err());
        assert!(verify_cache_path(&expected, "relative/path").is_err());
        assert!(
            verify_cache_path(
                &expected,
                utf8_path(&default_path(&temp.0).unwrap()).unwrap()
            )
            .is_err()
        );
        // This only accepts the reported configuration. It cannot observe
        // CEF's actual Profile::IsOffTheRecord or prove disk persistence.
        assert!(verify_cache_path(&expected, utf8_path(&expected).unwrap()).is_ok());
    }

    #[derive(Default)]
    struct FakeContext {
        cookies: RefCell<HashMap<String, String>>,
    }

    #[test]
    fn registry_reuses_context_and_keeps_different_profiles_isolated() {
        let temp = TempRoot::new();
        let registry = PersistentContexts::default();
        let work_path = named_path(&temp.0, &StorageKey::new("work").unwrap()).unwrap();
        let personal_path = named_path(&temp.0, &StorageKey::new("personal").unwrap()).unwrap();
        let work = registry
            .resolve(work_path.clone(), || Ok(FakeContext::default()))
            .unwrap();
        let personal = registry
            .resolve(personal_path, || Ok(FakeContext::default()))
            .unwrap();
        work.cookies
            .borrow_mut()
            .insert("example.test".into(), "work-session".into());
        let reused = registry
            .resolve(work_path.clone(), || panic!("must reuse existing context"))
            .unwrap();
        assert!(Rc::ptr_eq(&work, &reused));
        assert!(!Rc::ptr_eq(&work, &personal));
        assert!(personal.cookies.borrow().is_empty());
        drop(work);
        drop(reused);
        let reopened = registry
            .resolve(work_path, || {
                panic!("registry must retain persistent storage")
            })
            .unwrap();
        assert_eq!(reopened.cookies.borrow()["example.test"], "work-session");
    }

    #[test]
    fn failed_factory_and_invalid_cache_configuration_never_publish_a_context() {
        let temp = TempRoot::new();
        let registry = PersistentContexts::<String>::default();
        let path = named_path(&temp.0, &StorageKey::new("work").unwrap()).unwrap();
        let default = registry
            .resolve(default_path(&temp.0).unwrap(), || Ok("global".into()))
            .unwrap();
        assert!(
            registry
                .resolve(path.clone(), || Err("CEF creation failed".into()))
                .is_err()
        );
        assert!(
            registry
                .resolve(path.clone(), || {
                    verify_cache_path(&path, "")?;
                    Ok("in-memory".into())
                })
                .is_err()
        );
        let context = registry.resolve(path, || Ok("work".into())).unwrap();
        assert_eq!(context.as_str(), "work");
        assert_eq!(default.as_str(), "global");
        assert!(!Rc::ptr_eq(&context, &default));
    }

    #[test]
    fn factory_can_reenter_registry_without_a_borrow_panic() {
        let registry = PersistentContexts::default();
        let context = registry
            .resolve(PathBuf::from("work"), || {
                registry.resolve(PathBuf::from("other"), || Ok("other"))?;
                registry.resolve(PathBuf::from("work"), || Ok("reentered"))?;
                Ok("outer")
            })
            .unwrap();
        assert_eq!(*context, "reentered");
    }

    #[test]
    fn registry_has_no_retention_cycle_and_a_fork_lease_outlives_its_source() {
        struct Tracked(Rc<Cell<usize>>);
        impl Drop for Tracked {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let registry = PersistentContexts::default();
        let source = registry
            .resolve(PathBuf::from("work"), || Ok(Tracked(drops.clone())))
            .unwrap();
        let fork_lease = source.clone();
        drop(source);
        drop(registry);
        assert_eq!(drops.get(), 0);
        drop(fork_lease);
        assert_eq!(drops.get(), 1);
    }
}
