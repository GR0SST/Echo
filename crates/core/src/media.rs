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
    pub fn select(&mut self, stamp: &MediaStamp) -> bool {
        if !self.items.iter().any(|item| &item.stamp == stamp) {
            return false;
        }
        self.selected = Some(stamp.clone());
        true
    }
    pub fn replace(&mut self, mut items: Vec<MediaItem>) {
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
        if self
            .selected
            .as_ref()
            .is_some_and(|stamp| !items.iter().any(|item| &item.stamp == stamp))
        {
            self.selected = None;
        }
        self.items = items;
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
