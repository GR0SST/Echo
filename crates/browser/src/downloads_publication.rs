//! Dormant, engine-independent filesystem publication. No service/model wiring.
//!
//! TRUST BOUNDARY: only the single genuine event adapter may supply observations;
//! use (DownloadToken, full DownloadProvenance) as K. Completion is never inferred
//! from bytes, destination resolution, a file's existence, or a shutdown guess.
//!
//! Activation gates: directory-relative race-safe operations and crash recovery
//! are NOT implemented. Callers must ensure exclusive control of staging and no
//! concurrent replacement of the destination directory/ancestors during an effect.
//! Revalidation catches replacement between effects, not adversarial TOCTOU.
//! Never reconstruct this object after a crash and retry automatically.
//! See crates/browser/DOWNLOAD_PUBLICATION.md.

use std::{
    ffi::OsStr,
    fs::{self, File},
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

/// Minted by reserve; cannot be constructed from a path, serialized ID or token.
/// Clone only for the same registered transfer; K must include full provenance.
pub(crate) struct Registration<K> {
    key: K,
    nonce: Arc<()>,
}
impl<K: Clone> Clone for Registration<K> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            nonce: self.nonce.clone(),
        }
    }
}

/// Adapter vocabulary only, not an engine state converter or a completion oracle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Observation {
    DestinationRequested,
    UpdatedPending,
    UpdatedInProgress,
    UpdatedPaused,
    UpdatedInterrupted,
    UpdatedCanceled,
    UpdatedComplete,
}

#[derive(Debug)]
pub(crate) enum Error {
    InvalidFolder,
    InvalidCandidate,
    EmptyCandidates,
    IdentityMismatch,
    PathMismatch,
    Detached,
    Replaced,
    NotRegularFile,
    CandidatesExhausted,
    /// A syscall was attempted but success cannot safely be acknowledged/retried.
    /// Bytes may be at `attempted_path`, the stage path, or both. Never delete either.
    Ambiguous,
    Io(io::Error),
}
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Handles are retained to prevent identity reuse while checking replacement.
struct Lease {
    _file: File,
    id: FileId,
}
impl Lease {
    fn open(path: &Path, directory: bool) -> Result<Self, Error> {
        check_kind(path, directory)?;
        let file = open_identity(path)?;
        let metadata = file.metadata()?;
        if metadata.is_dir() != directory || (!directory && !metadata.is_file()) {
            return Err(Error::Replaced);
        }
        let lease = Self {
            id: file_id(&file)?,
            _file: file,
        };
        lease.check(path, directory)?;
        Ok(lease)
    }
    fn check(&self, path: &Path, directory: bool) -> Result<(), Error> {
        check_kind(path, directory)?;
        let current = open_identity(path)?;
        if file_id(&current)? != self.id {
            return Err(Error::Replaced);
        }
        Ok(())
    }
}
fn check_kind(path: &Path, directory: bool) -> Result<(), Error> {
    let m = fs::symlink_metadata(path)?;
    if m.file_type().is_symlink() {
        return Err(Error::Replaced);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Includes junctions and other reparse points, not just symbolic links.
        if m.file_attributes() & 0x400 != 0 {
            return Err(Error::Replaced);
        }
    }
    if (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return Err(Error::NotRegularFile);
    }
    Ok(())
}

struct Directory {
    path: PathBuf,
    ancestors: Vec<(PathBuf, Lease)>,
}
impl Directory {
    fn capture(path: &Path) -> Result<Self, Error> {
        if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(Error::InvalidFolder);
        }
        // Require caller to resolve legitimate platform aliases before reserving.
        // No symlink/junction ancestor is accepted, even if canonicalize succeeds.
        let mut ancestors = Vec::new();
        for p in path.ancestors() {
            if !p.as_os_str().is_empty() {
                ancestors.push((p.to_owned(), Lease::open(p, true)?));
            }
        }
        if fs::canonicalize(path)? != path {
            return Err(Error::InvalidFolder);
        }
        let dir = Self {
            path: path.to_owned(),
            ancestors,
        };
        dir.check()?;
        Ok(dir)
    }
    fn check(&self) -> Result<(), Error> {
        for (path, lease) in &self.ancestors {
            lease.check(path, true)?;
        }
        Ok(())
    }
}

struct Reservation {
    folder: Directory,
    stage_dir: PathBuf,
    stage_lease: Lease,
    stage_path: PathBuf,
    candidates: Vec<PathBuf>,
    nonce: Arc<()>,
}
impl Reservation {
    fn check(&self) -> Result<(), Error> {
        self.folder.check()?;
        self.stage_lease.check(&self.stage_dir, true)
    }
}

/// Retained by the future caller until genuine Complete. Drop deletes nothing.
pub(crate) struct Staging<K> {
    registration: Registration<K>,
    reservation: Option<Reservation>,
}
impl<K: Clone + Eq> Staging<K> {
    /// No Downloads resolver or suffix/extension policy. Candidates are injected
    /// in exact order, either leaf names or absolute immediate children of folder.
    /// This filesystem effect must run outside App/service/CEF borrows.
    pub(crate) fn reserve(key: K, folder: &Path, candidates: Vec<PathBuf>) -> Result<Self, Error> {
        let folder = Directory::capture(folder)?;
        if candidates.is_empty() {
            return Err(Error::EmptyCandidates);
        }
        let candidates = candidates
            .into_iter()
            .map(|p| candidate(&folder.path, &p))
            .collect::<Result<Vec<_>, _>>()?;
        for (i, p) in candidates.iter().enumerate() {
            if candidates[..i].contains(p) {
                return Err(Error::InvalidCandidate);
            }
        }
        folder.check()?;
        // Random fresh exclusive directory on the destination filesystem. No
        // final-target reservation, and payload does NOT exist when given to CEF.
        // Cleanup disabled at construction, including all subsequent error paths.
        let mut builder = tempfile::Builder::new();
        builder
            .prefix(".echo-download-")
            .rand_bytes(20)
            .disable_cleanup(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o700));
        }
        let dir = builder.tempdir_in(&folder.path)?;
        let stage_dir = dir.path().to_owned();
        let stage_lease = Lease::open(&stage_dir, true)?;
        let nonce = Arc::new(());
        let reservation = Reservation {
            stage_path: stage_dir.join("payload"),
            stage_dir,
            stage_lease,
            folder,
            candidates,
            nonce: nonce.clone(),
        };
        reservation.check()?;
        require_absent(&reservation.stage_path)?;
        Ok(Self {
            registration: Registration { key, nonce },
            reservation: Some(reservation),
        })
    }
    pub(crate) fn registration(&self) -> Registration<K> {
        self.registration.clone()
    }
    pub(crate) fn stage_path(&self) -> Result<&Path, Error> {
        Ok(&self.reservation.as_ref().ok_or(Error::Detached)?.stage_path)
    }
    /// Revalidate immediately before the future outside-borrows CEF Continue.
    /// Even this cannot make a path-based CEF handoff adversarial-race-free.
    pub(crate) fn validate_handoff(&self) -> Result<&Path, Error> {
        let r = self.reservation.as_ref().ok_or(Error::Detached)?;
        r.check()?;
        require_absent(&r.stage_path)?;
        Ok(&r.stage_path)
    }
    /// PURE detach; no stat/open/native/model calls under the caller's borrow.
    /// Validate variant, exact native path, opaque registration AND original key.
    /// All other observations leave the intent in place and authorize no writes.
    pub(crate) fn observe(
        &mut self,
        registration: &Registration<K>,
        event_key: &K,
        observation: Observation,
        exact_full_path: &Path,
    ) -> Result<Option<Publication>, Error> {
        if !Arc::ptr_eq(&registration.nonce, &self.registration.nonce)
            || registration.key != self.registration.key
            || event_key != &self.registration.key
        {
            return Err(Error::IdentityMismatch);
        }
        if observation != Observation::UpdatedComplete {
            return Ok(None);
        }
        let r = self.reservation.as_ref().ok_or(Error::Detached)?;
        // OsStr comparison deliberately avoids Path's normalization of /./.
        if exact_full_path.as_os_str() != r.stage_path.as_os_str() {
            return Err(Error::PathMismatch);
        }
        Ok(Some(Publication {
            reservation: self.reservation.take().unwrap(),
            source: None,
            state: State::Ready { next: 0 },
        }))
    }
}
fn candidate(folder: &Path, input: &Path) -> Result<PathBuf, Error> {
    let name = if input.is_absolute() {
        if input.parent() != Some(folder) {
            return Err(Error::InvalidCandidate);
        }
        input.file_name().ok_or(Error::InvalidCandidate)?
    } else {
        input.as_os_str()
    };
    let mut parts = Path::new(name).components();
    let Some(Component::Normal(leaf)) = parts.next() else {
        return Err(Error::InvalidCandidate);
    };
    // components() normalizes a trailing separator or /.; a candidate must
    // already be spelled exactly as its accepted leaf, without normalization.
    if name != leaf
        || parts.next().is_some()
        || name == OsStr::new("")
        || input
            .components()
            .any(|p| matches!(p, Component::ParentDir))
    {
        return Err(Error::InvalidCandidate);
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        // Containment validation, not a naming policy: disallow alternate data
        // stream syntax, NUL and Win32 normalization aliases. The folder must be
        // canonical (verbatim absolute on Windows); output always joins it.
        let units: Vec<u16> = name.encode_wide().collect();
        if units.iter().any(|u| *u == 0 || *u == b':' as u16)
            || matches!(units.last(), Some(0x20 | 0x2e))
        {
            return Err(Error::InvalidCandidate);
        }
    }
    let accepted = folder.join(leaf);
    // file_name()/parent() also normalize absolute inputs. Compare native
    // spelling losslessly so absolute trailing separators/dots cannot bypass it.
    if input.is_absolute() && input.as_os_str() != accepted.as_os_str() {
        return Err(Error::InvalidCandidate);
    }
    Ok(accepted)
}
fn require_absent(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
        Ok(_) => Err(Error::Replaced), // Includes dangling symlinks.
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Ready {
        next: usize,
    },
    /// Installed BEFORE invoking persist_noclobber. Any uncertain outcome blocks
    /// retry; an executor must keep this object rather than minting another.
    Attempting {
        candidate: PathBuf,
    },
    Published {
        path: PathBuf,
    },
    Exhausted,
}

/// Owned effect; execute only after releasing ALL App/service/CEF borrows.
/// No Clone, serialization, resume constructor, automatic cleanup or Drop logic.
pub(crate) struct Publication {
    reservation: Reservation,
    source: Option<Lease>,
    state: State,
}

/// Only a successful, identity-checked Publication can construct this proof.
/// Process-local capability, not a promise that users cannot later move the file.
/// The future canonical writer must accept this type and validate it immediately;
/// it must not persist the stage path or invent a proof from a path string.
pub(crate) struct PublishedPath {
    path: PathBuf,
    nonce: Arc<()>,
    id: FileId,
}
impl PublishedPath {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
impl Publication {
    pub(crate) fn state(&self) -> &State {
        &self.state
    }
    pub(crate) fn stage_path(&self) -> &Path {
        &self.reservation.stage_path
    }
    pub(crate) fn execute(&mut self) -> Result<PublishedPath, Error> {
        self.execute_with(|_, _| {})
    }
    // Hook is private; tests create a collision after validation, before syscall.
    fn execute_with(
        &mut self,
        mut before: impl FnMut(&Path, &Path),
    ) -> Result<PublishedPath, Error> {
        self.reservation.check()?;
        if let State::Published { path } = &self.state {
            return self.proof(path.clone());
        }
        if matches!(self.state, State::Attempting { .. }) {
            return Err(Error::Ambiguous);
        }
        if matches!(self.state, State::Exhausted) {
            return Err(Error::CandidatesExhausted);
        }
        if self.source.is_none() {
            self.source = Some(Lease::open(&self.reservation.stage_path, false)?);
        }
        loop {
            let next = match self.state {
                State::Ready { next } => next,
                _ => unreachable!(),
            };
            let Some(target) = self.reservation.candidates.get(next).cloned() else {
                self.state = State::Exhausted;
                return Err(Error::CandidatesExhausted);
            };
            self.reservation.check()?;
            self.source
                .as_ref()
                .unwrap()
                .check(self.stage_path(), false)?;
            // No exists()+rename. TempPath does not create/open/reserve anything.
            let mut temporary = tempfile::TempPath::try_from_path(self.stage_path())?;
            temporary.disable_cleanup(true); // BEFORE any fallible operation/drop.
            self.state = State::Attempting {
                candidate: target.clone(),
            };
            before(self.stage_path(), &target);
            match temporary.persist_noclobber(&target) {
                Ok(()) => {
                    // May leave the owned source hardlink behind. Do not unlink it;
                    // it is retained for explicit recovery/cleanup work in the future.
                    let proof = self.proof(target.clone())?;
                    self.state = State::Published { path: target };
                    return Ok(proof);
                }
                Err(failure) => {
                    let kind = failure.error.kind();
                    // Dropping this error MUST retain the staged bytes.
                    drop(failure.path);
                    if kind != io::ErrorKind::AlreadyExists {
                        // EXDEV/unsupported/permissions are NOT collisions. Never
                        // copy, overwrite, or try another candidate on these errors.
                        return Err(Error::Io(failure.error));
                    }
                    self.reservation.check()?;
                    self.source
                        .as_ref()
                        .unwrap()
                        .check(self.stage_path(), false)?;
                    // A target with source identity means prior/ambiguous publish,
                    // not a normal collision. Never create a second final link.
                    if let Ok(existing) = Lease::open(&target, false) {
                        if existing.id == self.source.as_ref().unwrap().id {
                            return Err(Error::Ambiguous);
                        }
                    }
                    self.state = State::Ready { next: next + 1 };
                }
            }
        }
    }
    fn proof(&self, target: PathBuf) -> Result<PublishedPath, Error> {
        self.reservation.check()?;
        let source = self.source.as_ref().ok_or(Error::Ambiguous)?;
        source.check(&target, false)?;
        // Source may have been removed by rename or retained by hardlink fallback.
        match fs::symlink_metadata(self.stage_path()) {
            Ok(_) => source.check(self.stage_path(), false)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        Ok(PublishedPath {
            path: target,
            nonce: self.reservation.nonce.clone(),
            id: source.id,
        })
    }
    pub(crate) fn validate_proof(&self, proof: &PublishedPath) -> Result<(), Error> {
        if !Arc::ptr_eq(&proof.nonce, &self.reservation.nonce)
            || self.source.as_ref().map(|s| s.id) != Some(proof.id)
            || self.state
                != (State::Published {
                    path: proof.path.clone(),
                })
        {
            return Err(Error::IdentityMismatch);
        }
        self.proof(proof.path.clone()).map(|_| ())
    }
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
compile_error!("downloads_publication needs a platform file identity implementation");

#[cfg(test)]
#[path = "downloads_publication_tests.rs"]
mod tests;
