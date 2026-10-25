//! Local Library media metadata. Filesystem access belongs to the platform layer.
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
}
impl MediaKind {
    /// Portable fallback for the image/movie types supported by the local viewer.
    /// Documents (including PDF), executables and web pages are not media.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "avif" | "bmp" | "tif"
            | "tiff" => Some(Self::Image),
            "mp4" | "mov" | "m4v" | "webm" | "avi" | "mkv" | "mpg" | "mpeg" | "3gp" => {
                Some(Self::Video)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MediaStamp {
    pub path: PathBuf,
    pub bytes: u64,
    pub modified: SystemTime,
    /// Platform file identity, when available; replacement invalidates actions/cache.
    pub identity: Option<(u64, u64)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaSource {
    Folder,
    Capture {
        profile: crate::UserProfileId,
        id: crate::captures::CaptureId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaItem {
    pub stamp: MediaStamp,
    pub kind: MediaKind,
    pub source: MediaSource,
    pub name: String,
}
impl MediaItem {
    pub fn matches(&self, query: &str) -> bool {
        self.name
            .to_lowercase()
            .contains(&query.trim().to_lowercase())
    }
}

/// Reconcile one complete generation. Selection follows file identity, never a
/// list index, so a filesystem update cannot redirect an action to another file.
#[derive(Default)]
pub struct MediaInventory {
    items: Vec<MediaItem>,
    selected: Option<MediaStamp>,
}
impl MediaInventory {
    pub fn items(&self) -> &[MediaItem] {
        &self.items
    }
    pub fn selected(&self) -> Option<&MediaItem> {
        self.selected
            .as_ref()
            .and_then(|stamp| self.items.iter().find(|item| &item.stamp == stamp))
    }
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }
    pub fn select(&mut self, stamp: &MediaStamp) -> bool {
        if !self.items.iter().any(|item| &item.stamp == stamp) {
            return false;
        }
        self.selected = Some(stamp.clone());
        true
    }
    pub fn replace(&mut self, mut items: Vec<MediaItem>) -> bool {
        // Capture metadata wins over duplicate filesystem discovery.
        items.sort_by_key(|item| !matches!(item.source, MediaSource::Capture { .. }));
        let mut seen = std::collections::HashSet::new();
        items.retain(|item| seen.insert(item.stamp.path.clone()));
        items.sort_by(|a, b| {
            b.stamp
                .modified
                .cmp(&a.stamp.modified)
                .then_with(|| a.stamp.path.cmp(&b.stamp.path))
        });
        if self.items == items {
            return false;
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|stamp| !items.iter().any(|item| &item.stamp == stamp))
        {
            self.selected = None;
        }
        self.items = items;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(path: &str, seconds: u64) -> MediaItem {
        MediaItem {
            name: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into(),
            kind: MediaKind::Image,
            source: MediaSource::Folder,
            stamp: MediaStamp {
                path: path.into(),
                bytes: 10,
                modified: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds),
                identity: Some((1, seconds)),
            },
        }
    }
    #[test]
    fn portable_types_exclude_documents_and_search_preserves_literal_names() {
        for path in [
            "Photo.JPEG",
            "photo.heic",
            "screen.png",
            "clip.MOV",
            "clip.webm",
        ] {
            assert!(MediaKind::from_path(Path::new(path)).is_some());
        }
        for path in [
            "report.pdf",
            "page.html",
            "photo.png.exe",
            "README",
            "vector.svg",
        ] {
            assert!(MediaKind::from_path(Path::new(path)).is_none());
        }
        assert!(item("/fixtures/Crème 🌿.png", 1).matches(" CRÈME "));
        assert!(!item("/fixtures/Crème 🌿.png", 1).matches("mp4"));
    }
    #[test]
    fn refresh_deduplicates_and_replacement_cannot_retarget_selection() {
        let a = item("/fixtures/a.png", 1);
        let b = item("/fixtures/b.png", 2);
        let mut model = MediaInventory::default();
        model.replace(vec![a.clone(), b.clone(), a.clone()]);
        assert_eq!(model.items(), &[b.clone(), a.clone()]);
        assert!(model.select(&a.stamp));
        model.replace(vec![b.clone(), a.clone()]);
        assert_eq!(model.selected(), Some(&a));
        let mut replacement = a.clone();
        replacement.stamp.identity = Some((1, 99));
        model.replace(vec![b, replacement]);
        assert!(model.selected().is_none());
        assert!(!model.select(&a.stamp));
    }
}

/// Folder access is local to a browser profile. Cancelling or revoking a grant
/// stays denied until the user explicitly enables it again in Settings.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum MediaFolder {
    Documents,
    Desktop,
    Downloads,
}
impl MediaFolder {
    pub const ALL: [Self; 3] = [Self::Documents, Self::Desktop, Self::Downloads];
    pub fn label(self) -> &'static str {
        match self {
            Self::Documents => "Documents",
            Self::Desktop => "Desktop",
            Self::Downloads => "Downloads",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FolderGrant {
    #[default]
    Unknown,
    Denied,
    Granted(PathBuf),
}
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MediaAccess {
    pub folders: std::collections::BTreeMap<MediaFolder, FolderGrant>,
    pub revision: u64,
}
impl MediaAccess {
    pub fn grant(&self, folder: MediaFolder) -> FolderGrant {
        self.folders.get(&folder).cloned().unwrap_or_default()
    }
    pub fn paths(&self) -> Vec<PathBuf> {
        self.folders
            .values()
            .filter_map(|g| {
                if let FolderGrant::Granted(p) = g {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    /// Reserve before opening the OS panel; reopening Media cannot stack prompts.
    pub fn begin(&mut self, folder: MediaFolder, explicit: bool) -> Option<u64> {
        if !explicit && self.grant(folder) != FolderGrant::Unknown {
            return None;
        }
        self.revision = self.revision.wrapping_add(1);
        self.folders.insert(folder, FolderGrant::Denied);
        Some(self.revision)
    }
    pub fn complete(&mut self, folder: MediaFolder, revision: u64, path: Option<PathBuf>) -> bool {
        if self.revision != revision || self.grant(folder) != FolderGrant::Denied {
            return false;
        }
        if let Some(path) = path.filter(|p| p.is_absolute()) {
            self.folders.insert(folder, FolderGrant::Granted(path));
        }
        true
    }
    pub fn revoke(&mut self, folder: MediaFolder) {
        self.revision = self.revision.wrapping_add(1);
        self.folders.insert(folder, FolderGrant::Denied);
    }
}
#[cfg(test)]
mod access_tests {
    use super::*;
    #[test]
    fn grants_are_profile_local_persisted_and_revocation_rejects_late_panels() {
        let mut browser = crate::Browser::new();
        let first = browser.active_user_profile_id();
        let second = browser.create_user_profile("Other").unwrap();
        let access = &mut browser.user_profile_mut(first).unwrap().settings.media;
        let token = access.begin(MediaFolder::Downloads, false).unwrap();
        assert!(access.begin(MediaFolder::Downloads, false).is_none());
        access.revoke(MediaFolder::Downloads);
        assert!(!access.complete(MediaFolder::Downloads, token, Some(std::env::temp_dir())));
        let token = access.begin(MediaFolder::Downloads, true).unwrap();
        assert!(access.complete(MediaFolder::Downloads, token, Some(std::env::temp_dir())));
        let loaded = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert_eq!(
            loaded.user_profile(first).unwrap().settings.media.paths(),
            vec![std::env::temp_dir()]
        );
        assert!(loaded
            .user_profile(second)
            .unwrap()
            .settings
            .media
            .paths()
            .is_empty());
        let mut legacy: serde_json::Value = serde_json::from_str(&crate::session::to_json(&browser)).unwrap();
        legacy["profiles"]["1"]["settings"]
            .as_object_mut()
            .unwrap()
            .remove("media");
        let loaded = crate::session::from_json(&legacy.to_string()).unwrap();
        assert_eq!(
            loaded.active_user_profile().settings.media,
            MediaAccess::default()
        );
    }
}
