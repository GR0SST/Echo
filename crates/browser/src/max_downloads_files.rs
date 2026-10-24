//! Narrow no-overwrite file moves. Retained handles prevent identity reuse;
//! changed/replaced paths and ambiguous outcomes are never cleaned up/retried.
use browser_core::downloads::DownloadRenamePlan;
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameError {
    Io(io::ErrorKind),
    Changed,
    Ambiguous,
    Collisions,
}
impl From<io::Error> for RenameError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.kind())
    }
}
#[derive(Clone)]
pub struct FileProof {
    file: Arc<File>,
    parent: Arc<File>,
    id: FileId,
    parent_id: FileId,
    folder: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}
impl FileProof {
    pub fn capture(path: &Path) -> Result<Self, RenameError> {
        check_kind(path, false)?;
        let folder = path.parent().ok_or(RenameError::Changed)?.to_owned();
        check_kind(&folder, true)?;
        let file = open_identity(path)?;
        let parent = open_identity(&folder)?;
        check_open_kind(&file, false)?;
        check_open_kind(&parent, true)?;
        let meta = file.metadata()?;
        let proof = Self {
            id: file_id(&file)?,
            parent_id: file_id(&parent)?,
            file: Arc::new(file),
            parent: Arc::new(parent),
            folder,
            len: meta.len(),
            modified: meta.modified().ok(),
        };
        proof.check(path, true)?;
        Ok(proof)
    }
    fn check(&self, path: &Path, contents: bool) -> Result<(), RenameError> {
        if path.parent() != Some(self.folder.as_path()) {
            return Err(RenameError::Changed);
        }
        check_kind(&self.folder, true)?;
        check_kind(path, false)?;
        let file = open_identity(path)?;
        let parent = open_identity(&self.folder)?;
        check_open_kind(&file, false)?;
        check_open_kind(&parent, true)?;
        if file_id(&file)? != self.id
            || file_id(&self.file)? != self.id
            || file_id(&parent)? != self.parent_id
            || file_id(&self.parent)? != self.parent_id
        {
            return Err(RenameError::Changed);
        }
        let meta = file.metadata()?;
        if contents && (meta.len() != self.len || meta.modified().ok() != self.modified) {
            return Err(RenameError::Changed);
        }
        Ok(())
    }
    pub fn rename(&self, plan: &DownloadRenamePlan) -> Result<PathBuf, RenameError> {
        for i in 0..=99 {
            self.check(plan.expected.final_path(), true)?;
            let target = plan.candidate(i);
            match self.move_exact(plan.expected.final_path(), &target, true) {
                Ok(()) => return Ok(target),
                Err(RenameError::Io(io::ErrorKind::AlreadyExists)) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(RenameError::Collisions)
    }
    pub fn move_exact(
        &self,
        source: &Path,
        target: &Path,
        contents: bool,
    ) -> Result<(), RenameError> {
        self.check(source, contents)?;
        if target.parent() != Some(self.folder.as_path()) || source == target {
            return Err(RenameError::Changed);
        }
        let mut temporary = tempfile::TempPath::try_from_path(source)?;
        // A failed persist must NEVER delete a user's completed download.
        temporary.disable_cleanup(true);
        temporary.persist_noclobber(target).map_err(|error| {
            let kind = error.error.kind();
            drop(error.path);
            RenameError::Io(kind)
        })?;
        self.check(target, false)
            .map_err(|_| RenameError::Ambiguous)?;
        // An unsupported-platform hard-link fallback may retain the source.
        // Keep both names, report ambiguity, and never unlink/retry on a guess.
        if source.try_exists()? {
            if let Ok(original) = open_identity(source) {
                if file_id(&original)? == self.id {
                    return Err(RenameError::Ambiguous);
                }
            }
        }
        Ok(())
    }
}
fn check_kind(path: &Path, directory: bool) -> Result<(), RenameError> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink()
        || meta.is_dir() != directory
        || (!directory && !meta.is_file())
    {
        return Err(RenameError::Changed);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(RenameError::Changed);
        }
    }
    Ok(())
}
fn check_open_kind(file: &File, directory: bool) -> Result<(), RenameError> {
    let meta = file.metadata()?;
    if meta.is_dir() != directory || (!directory && !meta.is_file()) {
        return Err(RenameError::Changed);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(RenameError::Changed);
        }
    }
    Ok(())
}
// Minimal isolated identity shim. Unsupported identity filesystems fail closed.
// Windows uses full 128-bit IDs (not same-file's legacy 64-bit ReFS key).
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileId {
    device: u64,
    inode: u64,
}
#[cfg(unix)]
fn file_id(file: &File) -> io::Result<FileId> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata()?;
    if m.ino() == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Missing stable inode identity",
        ));
    }
    Ok(FileId {
        device: m.dev(),
        inode: m.ino(),
    })
}
#[cfg(unix)]
fn open_identity(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileId {
    volume: u64,
    file: [u8; 16],
}
#[cfg(windows)]
fn file_id(file: &File) -> io::Result<FileId> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO,
    };
    let mut info = FILE_ID_INFO::default();
    // SAFETY: live owned file handle and a correctly sized, writable FILE_ID_INFO.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.FileId.Identifier == [0; 16] {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Missing stable file identity",
        ));
    }
    Ok(FileId {
        volume: info.VolumeSerialNumber,
        file: info.FileId.Identifier,
    })
}
#[cfg(windows)]
fn open_identity(path: &Path) -> io::Result<File> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    };
    OpenOptions::new()
        .read(true)
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(1 | 2 | 4)
        .open(path)
}
#[cfg(not(any(unix, windows)))]
compile_error!("Max downloads need a platform file identity implementation");

#[cfg(test)]
mod tests {
    use super::*;
    use browser_core::{
        downloads::{DownloadCompletion, DownloadTime},
        Browser,
    };
    fn fixture() -> (tempfile::TempDir, DownloadRenamePlan) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("123.pdf");
        fs::write(&path, b"original contents").unwrap();
        let mut browser = Browser::new();
        let profile = browser.default_user_profile_id();
        let id = browser
            .record_completed_download(
                profile,
                DownloadCompletion {
                    final_path: path,
                    source_url: "https://example.test/file".into(),
                    original_url: "https://example.test/file".into(),
                    mime_type: "application/pdf".into(),
                    finished_at: DownloadTime::Unknown,
                },
            )
            .unwrap();
        let plan =
            DownloadRenamePlan::new(browser.completed_download(id).unwrap().clone(), "Report")
                .unwrap();
        (dir, plan)
    }
    #[test]
    fn rename_undo_preserve_content_and_never_overwrite_collisions() {
        let (_dir, plan) = fixture();
        let original = plan.expected.final_path();
        let proof = FileProof::capture(original).unwrap();
        fs::write(plan.candidate(0), b"existing file").unwrap();
        let renamed = proof.rename(&plan).unwrap();
        assert_eq!(renamed, plan.candidate(1));
        assert_eq!(fs::read(plan.candidate(0)).unwrap(), b"existing file");
        assert!(!original.exists());
        fs::write(original, b"another download").unwrap();
        assert_eq!(
            proof.move_exact(&renamed, original, false),
            Err(RenameError::Io(io::ErrorKind::AlreadyExists))
        );
        assert_eq!(fs::read(original).unwrap(), b"another download");
        fs::remove_file(original).unwrap();
        fs::write(&renamed, b"user edited contents").unwrap();
        proof.move_exact(&renamed, original, false).unwrap();
        assert_eq!(fs::read(original).unwrap(), b"user edited contents");
    }
    #[test]
    fn changed_or_replaced_download_is_never_moved() {
        let (_dir, plan) = fixture();
        let original = plan.expected.final_path();
        let proof = FileProof::capture(original).unwrap();
        fs::write(original, b"changed").unwrap();
        assert_eq!(proof.rename(&plan), Err(RenameError::Changed));
        assert!(!plan.candidate(0).exists());
        fs::remove_file(original).unwrap();
        fs::write(original, b"replacement").unwrap();
        assert_eq!(
            proof.move_exact(original, &plan.candidate(0), false),
            Err(RenameError::Changed)
        );
        assert_eq!(fs::read(original).unwrap(), b"replacement");
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_and_replaced_parent_directories_are_rejected() {
        let (dir, plan) = fixture();
        let original = plan.expected.final_path();
        let proof = FileProof::capture(original).unwrap();
        let held = dir.path().with_extension("retained");
        fs::rename(dir.path(), &held).unwrap();
        fs::create_dir(dir.path()).unwrap();
        fs::write(original, b"new folder file").unwrap();
        assert_eq!(proof.rename(&plan), Err(RenameError::Changed));
        let link = dir.path().join("link.pdf");
        std::os::unix::fs::symlink(original, &link).unwrap();
        assert!(matches!(
            FileProof::capture(&link),
            Err(RenameError::Changed)
        ));
        fs::remove_dir_all(held).unwrap();
    }
}
