//! Local-only media discovery. Constructing the service performs no filesystem
//! access. A system-folder approval is required before scanning or watching.
pub mod file_actions;
#[cfg(target_os = "macos")]
mod macos;
pub mod thumbnails;
use browser_core::media::{MediaItem, MediaKind, MediaSource, MediaStamp};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, SystemTime},
};

/// Shared platform boundary. `dirs` resolves Known Folders on Windows and XDG
/// user directories on Linux, rather than assuming English names under $HOME.
pub trait FolderPlatform {
    fn media_folders(&self) -> Vec<PathBuf>;
}
pub struct SystemFolders;
impl FolderPlatform for SystemFolders {
    fn media_folders(&self) -> Vec<PathBuf> {
        [
            dirs::document_dir(),
            dirs::desktop_dir(),
            dirs::download_dir(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Only create after the user approves paths in the native system picker.
/// A cancelled or denied picker must not create this capability.
#[derive(Clone, Debug)]
pub struct ApprovedFolders {
    paths: Vec<PathBuf>,
}
impl ApprovedFolders {
    pub fn from_system_selection(paths: Vec<PathBuf>) -> Option<Self> {
        let mut seen = HashSet::new();
        let paths: Vec<_> = paths
            .into_iter()
            .filter(|p| p.is_absolute() && seen.insert(p.clone()))
            .collect();
        (!paths.is_empty()).then_some(Self { paths })
    }
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub items: Vec<MediaItem>,
    pub denied: Vec<PathBuf>,
    pub unavailable: Vec<PathBuf>,
    pub truncated: bool,
    pub watch_error: Option<String>,
}
#[derive(Clone, Copy)]
pub struct ScanLimits {
    pub entries: usize,
    pub depth: usize,
}
impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            entries: 20_000,
            depth: 8,
        }
    }
}
fn hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}
fn identity(_path: &Path, meta: &fs::Metadata) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((meta.dev(), meta.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let file = fs::File::open(_path).ok()?;
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } == 0 {
            return None;
        }
        Some((
            info.dwVolumeSerialNumber as u64,
            ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = meta;
        None
    }
}
pub fn stamp(path: &Path) -> io::Result<MediaStamp> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Media is no longer a regular file",
        ));
    }
    Ok(MediaStamp {
        path: fs::canonicalize(path)?,
        bytes: meta.len(),
        modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        identity: identity(path, &meta),
    })
}
pub fn validate(expected: &MediaStamp) -> io::Result<()> {
    if stamp(&expected.path)? != *expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Media changed since it was selected",
        ));
    }
    // Reject swapped parent symlinks too, including a newly redirected root.
    if fs::canonicalize(&expected.path)? != expected.path {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Media location changed",
        ));
    }
    Ok(())
}
/// Bounded recursive scan, skipping links, hidden files and package directories.
/// Only approved roots are touched; an inaccessible root does not hide others.
pub fn scan(approved: &ApprovedFolders, limits: ScanLimits, cancelled: &AtomicBool) -> Snapshot {
    let mut result = Snapshot::default();
    let mut seen = HashSet::new();
    for root in &approved.paths {
        let mut budget = limits.entries;
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let meta = match fs::symlink_metadata(root) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => m,
            Ok(_) => continue,
            Err(e) => {
                record_error(&mut result, root, e);
                continue;
            }
        };
        let _ = meta;
        let canonical = match fs::canonicalize(root) {
            Ok(p) => p,
            Err(e) => {
                record_error(&mut result, root, e);
                continue;
            }
        };
        let mut queue = vec![(canonical.clone(), 0)];
        while let Some((dir, depth)) = queue.pop() {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            // A queued directory may have been replaced since enumeration.
            // Never traverse a newly redirected parent outside the approval.
            if !matches!(fs::canonicalize(&dir), Ok(ref current) if current == &dir && current.starts_with(&canonical))
            {
                continue;
            }
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) => {
                    record_error(&mut result, &dir, e);
                    continue;
                }
            };
            for entry in entries {
                if budget == 0 {
                    result.truncated = true;
                    break;
                }
                budget -= 1;
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(entry) = entry else { continue };
                let path = entry.path();
                if hidden(&path) {
                    continue;
                }
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    if depth < limits.depth
                        && ![
                            "app",
                            "bundle",
                            "photoslibrary",
                            "photolibrary",
                            "rtfd",
                            "pages",
                            "key",
                            "numbers",
                        ]
                        .iter()
                        .any(|ext| {
                            path.extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
                        })
                    {
                        queue.push((path, depth + 1));
                    }
                } else if kind.is_file() {
                    let Some(kind) = MediaKind::from_path(&path) else {
                        continue;
                    };
                    if !seen.insert(path.clone()) {
                        continue;
                    }
                    let Ok(stamp) = stamp(&path) else { continue };
                    if !stamp.path.starts_with(&canonical) {
                        continue;
                    }
                    result.items.push(MediaItem {
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        stamp,
                        kind,
                        source: MediaSource::Folder,
                    });
                }
            }
            if budget == 0 {
                break;
            }
        }
        if budget == 0 {
            result.truncated = true;
        }
    }
    result.items.sort_by(|a, b| {
        b.stamp
            .modified
            .cmp(&a.stamp.modified)
            .then_with(|| a.stamp.path.cmp(&b.stamp.path))
    });
    result
}
/// Captures are application-owned files. Read only the active profile's
/// canonical model entries, without enumerating sibling profile directories.
/// They remain available when the user declines access to external folders.
pub fn capture_items(
    directory: &Path,
    profile: browser_core::UserProfileId,
    captures: &browser_core::captures::Captures,
) -> Vec<MediaItem> {
    captures
        .items()
        .filter_map(|capture| {
            let stamp = stamp(&directory.join(capture.filename())).ok()?;
            Some(MediaItem {
                stamp,
                kind: MediaKind::Image,
                source: MediaSource::Capture {
                    profile,
                    id: capture.id,
                },
                name: capture.name.clone(),
            })
        })
        .collect()
}

fn record_error(result: &mut Snapshot, path: &Path, error: io::Error) {
    if error.kind() == io::ErrorKind::PermissionDenied {
        result.denied.push(path.into());
    } else {
        result.unavailable.push(path.into());
    }
}

/// One worker per consent session. Events are coalesced into a bounded channel;
/// snapshots replace one shared value instead of accumulating in a UI queue.
pub struct Discovery {
    latest: Arc<Mutex<Snapshot>>,
    generation: Arc<AtomicU64>,
    cancelled: Arc<AtomicBool>,
    wake: mpsc::SyncSender<()>,
}
impl Discovery {
    pub fn start(approved: ApprovedFolders) -> Self {
        Self::start_sources(Some(approved), None)
    }
    pub fn start_sources(
        approved: Option<ApprovedFolders>,
        captures: Option<(
            PathBuf,
            browser_core::UserProfileId,
            browser_core::captures::Captures,
        )>,
    ) -> Self {
        let latest = Arc::new(Mutex::new(Snapshot::default()));
        let generation = Arc::new(AtomicU64::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (wake, events) = mpsc::sync_channel(1);
        let this = Self {
            latest: latest.clone(),
            generation: generation.clone(),
            cancelled: cancelled.clone(),
            wake: wake.clone(),
        };
        std::thread::Builder::new()
            .name("library-media".into())
            .spawn(move || {
                let callback = move |event: notify::Result<notify::Event>| {
                    if event
                        .as_ref()
                        .map_or(true, |e| !matches!(e.kind, notify::EventKind::Access(_)))
                    {
                        let _ = wake.try_send(());
                    }
                };
                let mut watcher = RecommendedWatcher::new(
                    callback,
                    Config::default().with_follow_symlinks(false),
                );
                let mut watch_error = None;
                if let Ok(watcher) = &mut watcher {
                    for root in approved
                        .as_ref()
                        .map(|a| a.paths())
                        .unwrap_or(&[])
                        .iter()
                        .chain(captures.as_ref().map(|c| &c.0))
                    {
                        if let Err(error) = watcher.watch(root, RecursiveMode::Recursive) {
                            watch_error = Some(error.to_string());
                        }
                    }
                } else if let Err(error) = &watcher {
                    watch_error = Some(error.to_string());
                }
                loop {
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut snapshot = approved
                        .as_ref()
                        .map(|a| scan(a, ScanLimits::default(), &cancelled))
                        .unwrap_or_default();
                    if let Some((directory, profile, captures)) = &captures {
                        snapshot
                            .items
                            .extend(capture_items(directory, *profile, captures));
                    }
                    snapshot.watch_error = watch_error.clone();
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    *latest.lock().unwrap_or_else(|p| p.into_inner()) = snapshot;
                    generation.fetch_add(1, Ordering::Release);
                    // Native events are preferred; a bounded metadata-only fallback
                    // also notices recreated roots or a failed OS watcher.
                    match events.recv_timeout(Duration::from_secs(30)) {
                        Ok(()) => {
                            std::thread::sleep(Duration::from_millis(150));
                            while events.try_recv().is_ok() {}
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(_) => break,
                    }
                }
            })
            .expect("media worker");
        this
    }
    pub fn snapshot_since(&self, seen: u64) -> Option<(u64, Snapshot)> {
        let generation = self.generation.load(Ordering::Acquire);
        (generation != seen).then(|| {
            (
                generation,
                self.latest
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone(),
            )
        })
    }
    pub fn refresh(&self) {
        let _ = self.wake.try_send(());
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.wake.try_send(());
    }
}

#[cfg(test)]
mod tests;
