//! File operations bind to the captured file stamp, never a refreshed row index.
//! Trash is undoable without retaining file contents in browser memory.
use browser_core::media::MediaStamp;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
pub trait TrashPlatform {
    type Receipt;
    fn move_to_trash(&self, expected: &MediaStamp) -> io::Result<Self::Receipt>;
    fn restore(&self, receipt: &Self::Receipt) -> io::Result<()>;
}
pub struct SystemTrash;
pub struct TrashReceipt {
    original: PathBuf,
    #[cfg(target_os = "macos")]
    trashed: MediaStamp,
    #[cfg(not(target_os = "macos"))]
    trashed: trash::TrashItem,
}
fn vacant_destination(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A file already exists at the original location",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing original directory"))?;
    if fs::canonicalize(parent)? != parent {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The original directory changed",
        ));
    }
    Ok(())
}
// Canonicalize the parent rather than the deleted file. Windows Shell receipts
// may omit the extended path prefix returned by fs::canonicalize; Unix aliases
// such as /var versus /private/var need the same normalization.
#[cfg(any(not(target_os = "macos"), test))]
fn same_original(receipt: &Path, expected: &Path) -> bool {
    receipt.file_name() == expected.file_name()
        && receipt
            .parent()
            .and_then(|parent| fs::canonicalize(parent).ok())
            .as_deref()
            == expected.parent()
}

impl TrashPlatform for SystemTrash {
    type Receipt = TrashReceipt;
    fn move_to_trash(&self, expected: &MediaStamp) -> io::Result<TrashReceipt> {
        crate::validate(expected)?;
        #[cfg(target_os = "macos")]
        {
            let path = crate::macos::trash(&expected.path)
                .ok_or_else(|| io::Error::other("Could not move file to Trash"))?;
            Ok(TrashReceipt {
                original: expected.path.clone(),
                trashed: crate::stamp(&path)?,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let before: std::collections::HashSet<_> = trash::os_limited::list()
                .map_err(io::Error::other)?
                .into_iter()
                .map(|item| item.id)
                .collect();
            trash::delete(&expected.path).map_err(io::Error::other)?;
            let mut matching = trash::os_limited::list()
                .map_err(io::Error::other)?
                .into_iter()
                .filter(|item| {
                    !before.contains(&item.id)
                        && same_original(&item.original_path(), &expected.path)
                });
            let receipt = matching
                .next()
                .ok_or_else(|| io::Error::other("Trash did not return the moved file"))?;
            if matching.next().is_some() {
                return Err(io::Error::other("Trash returned ambiguous file identities"));
            }
            Ok(TrashReceipt {
                original: expected.path.clone(),
                trashed: receipt,
            })
        }
    }
    fn restore(&self, receipt: &TrashReceipt) -> io::Result<()> {
        vacant_destination(&receipt.original)?;
        #[cfg(target_os = "macos")]
        {
            crate::validate(&receipt.trashed)?;
            // Native Trash stays on the same volume. An exclusive hard link preserves
            // attributes and refuses a racing replacement at the original path.
            fs::hard_link(&receipt.trashed.path, &receipt.original)?;
            if let Err(error) = fs::remove_file(&receipt.trashed.path) {
                // The original is already restored. Do not delete it on a cleanup failure.
                return Err(io::Error::other(format!(
                    "File restored, but the Trash entry could not be removed: {error}"
                )));
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            trash::os_limited::restore_all([receipt.trashed.clone()]).map_err(io::Error::other)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Every operation in the test stays below TempDir, including its fake Trash.
    struct FixtureTrash {
        directory: PathBuf,
    }
    impl TrashPlatform for FixtureTrash {
        type Receipt = (MediaStamp, PathBuf);
        fn move_to_trash(&self, expected: &MediaStamp) -> io::Result<Self::Receipt> {
            crate::validate(expected)?;
            let path = self.directory.join("trashed.png");
            fs::rename(&expected.path, &path)?;
            Ok((crate::stamp(&path)?, expected.path.clone()))
        }
        fn restore(&self, receipt: &Self::Receipt) -> io::Result<()> {
            vacant_destination(&receipt.1)?;
            crate::validate(&receipt.0)?;
            fs::hard_link(&receipt.0.path, &receipt.1)?;
            fs::remove_file(&receipt.0.path)
        }
    }
    #[test]
    fn trash_undo_preserves_bytes_and_refuses_to_overwrite_a_new_file() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let path = root.join("fixture.png");
        let store = FixtureTrash {
            directory: root.join("fake-trash"),
        };
        fs::create_dir(&store.directory).unwrap();
        fs::write(&path, b"original fixture").unwrap();
        let stamp = crate::stamp(&path).unwrap();
        let receipt = store.move_to_trash(&stamp).unwrap();
        assert!(!path.exists());
        fs::write(&path, b"replacement fixture").unwrap();
        assert_eq!(
            store.restore(&receipt).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&path).unwrap(), b"replacement fixture");
        fs::remove_file(&path).unwrap();
        store.restore(&receipt).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"original fixture");
        assert!(!receipt.0.path.exists());
        assert!(store.restore(&receipt).is_err());
    }
    #[test]
    fn receipt_matching_normalizes_the_parent_after_the_original_file_is_gone() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("original");
        fs::create_dir(&directory).unwrap();
        let path = directory.join("fixture.png");
        fs::write(&path, b"fixture").unwrap();
        let expected = crate::stamp(&path).unwrap().path;
        fs::remove_file(&path).unwrap();
        assert!(same_original(&path, &expected));
        assert!(!same_original(&directory.join("other.png"), &expected));
        #[cfg(unix)]
        {
            let alias = root.path().join("alias");
            std::os::unix::fs::symlink(&directory, &alias).unwrap();
            assert!(same_original(&alias.join("fixture.png"), &expected));
        }
    }
    #[test]
    fn a_stale_selection_cannot_trash_a_replacement() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("fixture.png");
        fs::write(&path, b"fixture").unwrap();
        let stamp = crate::stamp(&path).unwrap();
        fs::write(&path, b"changed fixture").unwrap();
        let store = FixtureTrash {
            directory: root.path().into(),
        };
        assert!(store.move_to_trash(&stamp).is_err());
        assert_eq!(fs::read(path).unwrap(), b"changed fixture");
    }
}
