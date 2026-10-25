//! Display metadata and platform file operations for canonical completed records.
use browser_core::downloads::{CompletedDownload, CompletedDownloadId};
use gpui::{AnyElement, App, RenderImage, div, prelude::*, px};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileAction {
    Open,
    Copy,
    Reveal,
    Hide,
    Trash,
}
impl FileAction {
    pub(crate) fn selector(self) -> &'static str {
        match self {
            Self::Open => "download-open",
            Self::Copy => "download-copy",
            Self::Reveal => "download-reveal",
            Self::Hide => "download-hide",
            Self::Trash => "download-trash",
        }
    }
}

pub(crate) fn kind(record: &CompletedDownload) -> &'static str {
    kind_for(record.final_path(),record.mime_type())
}
pub(crate) fn kind_for(path:&Path,mime:&str)->&'static str {
    let ext=path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
    match (mime, ext.as_str()) {
        (_, "dmg") | ("application/x-apple-diskimage", _) => "Disk Image",
        ("application/pdf", _) | (_, "pdf") => "PDF",
        (mime, _) if mime.starts_with("image/") => "Image",
        (_, "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "avif") => "Image",
        (mime, _) if mime.starts_with("video/") => "Movie",
        (_, "mp4" | "mov" | "m4v" | "webm") => "Movie",
        (_, "bin" | "zip" | "gz" | "tar" | "rar" | "7z" | "bz2" | "xz") => "Archive",
        _ => "Download",
    }
}

pub(crate) fn subtitle(record: &CompletedDownload) -> String {
    let kind = kind(record);
    if kind=="Download" {return kind.into();}
    let host = url::Url::parse(record.source_url()).ok().and_then(|url| {
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let host = url.host_str()?.trim_start_matches("www.").to_string();
        if host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        {
            return None;
        }
        Some(host)
    });
    match host {
        Some(host) => format!("{kind} from {host}"),
        None => kind.into(),
    }
}

#[derive(Clone)]
enum Artwork {
    Pending,
    Ready {
        icon: Option<Arc<RenderImage>>,
        thumbnail: Option<PathBuf>,
    },
}
#[cfg(target_os = "macos")]
static ARTWORK_REQUESTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(target_os = "macos")]
struct ArtworkPermit;
#[cfg(target_os = "macos")]
impl Drop for ArtworkPermit {
    fn drop(&mut self) {
        ARTWORK_REQUESTS.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}
#[derive(Default)]
pub(crate) struct FileArtwork {
    records: HashMap<CompletedDownloadId, PathBuf>,
    previews: Arc<Mutex<HashMap<PathBuf, ((u64, Option<std::time::SystemTime>), Artwork)>>>,
}
impl FileArtwork {
    pub(crate) fn update(&mut self, records: Vec<CompletedDownload>) {
        self.records = records
            .into_iter()
            .map(|record| (record.id(), record.final_path().to_owned()))
            .collect();
        self.previews
            .lock()
            .unwrap()
            .retain(|path, _| self.records.values().any(|record| record == path));
    }
    pub(crate) fn render(&mut self, id: CompletedDownloadId, cx: &mut App) -> AnyElement {
        let Some(path) = self.records.get(&id).cloned() else {
            return fallback();
        };
        let Ok(metadata) = path.metadata() else {
            return fallback();
        };
        if !metadata.is_file() {
            return fallback();
        }
        let stamp = (metadata.len(), metadata.modified().ok());
        {
            let mut previews = self.previews.lock().unwrap();
            if previews
                .get(&path)
                .is_some_and(|(known, _)| *known != stamp)
            {
                previews.remove(&path);
            }
        }
        #[cfg(target_os = "macos")]
        {
            if !self.previews.lock().unwrap().contains_key(&path)
                && ARTWORK_REQUESTS
                    .fetch_update(
                        std::sync::atomic::Ordering::AcqRel,
                        std::sync::atomic::Ordering::Acquire,
                        |count| (count < 2).then_some(count + 1),
                    )
                    .is_ok()
            {
                let permit = ArtworkPermit;
                self.previews
                    .lock()
                    .unwrap()
                    .insert(path.clone(), (stamp, Artwork::Pending));
                let previews = self.previews.clone();
                let source = path.clone();
                let completed_path = path.clone();
                let task = cx.background_executor().spawn(async move {
                    let _permit = permit;
                    let icon = native_icon(&source);
                    let previewable = matches!(
                        source
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase()
                            .as_str(),
                        "pdf"
                            | "png"
                            | "jpg"
                            | "jpeg"
                            | "gif"
                            | "webp"
                            | "heic"
                            | "avif"
                            | "mp4"
                            | "mov"
                            | "m4v"
                            | "webm"
                    );
                    let thumbnail = previewable.then(|| thumbnail(&source)).flatten();
                    Artwork::Ready { icon, thumbnail }
                });
                cx.spawn(async move |cx| {
                    let result = task.await;
                    if let Some((known, artwork)) =
                        previews.lock().unwrap().get_mut(&completed_path)
                    {
                        if *known == stamp {
                            *artwork = result;
                        }
                    }
                    let _ = cx.update(|cx| cx.refresh_windows());
                })
                .detach();
            }
            let artwork = self
                .previews
                .lock()
                .unwrap()
                .get(&path)
                .map(|(_, artwork)| artwork.clone());
            if let Some(Artwork::Ready { icon, thumbnail }) = artwork {
                if let Some(thumbnail) = thumbnail {
                    return slot(
                        gpui::img(thumbnail)
                            .size(px(24.))
                            .object_fit(gpui::ObjectFit::Contain)
                            .into_any_element(),
                    );
                }
                if let Some(icon) = icon {
                    return slot(
                        gpui::img(icon)
                            .size(px(24.))
                            .object_fit(gpui::ObjectFit::Contain)
                            .into_any_element(),
                    );
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("png" | "jpg" | "jpeg" | "webp" | "gif")
        ) {
            return slot(
                gpui::img(path)
                    .size(px(24.))
                    .object_fit(gpui::ObjectFit::Contain)
                    .into_any_element(),
            );
        }
        let _ = cx;
        fallback()
    }
}
fn slot(child: AnyElement) -> AnyElement {
    div()
        .size(px(36.))
        .flex()
        .items_center()
        .justify_center()
        .child(child)
        .into_any_element()
}
fn fallback() -> AnyElement {
    slot(crate::icons::ionicon("document", 24., 1.))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn native_icon(_: &Path) -> Option<Arc<RenderImage>> {
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn native_icon(path: &Path) -> Option<Arc<RenderImage>> {
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    use objc2_foundation::NSString;
    // NSWorkspace supplies the OS artwork, never assets from Arc's bundle.
    let bytes = objc2::rc::autoreleasepool(|_| unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let image: *mut AnyObject =
            msg_send![workspace, iconForFile: &*NSString::from_str(path.to_str()?)];
        if image.is_null() {
            return None;
        }
        let tiff: *mut AnyObject = msg_send![image, TIFFRepresentation];
        if tiff.is_null() {
            return None;
        }
        let rep: Retained<AnyObject> =
            Retained::retain(msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff])?;
        let properties: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
        let data: *mut AnyObject =
            msg_send![&*rep, representationUsingType: 4usize, properties: properties];
        if data.is_null() {
            return None;
        }
        let length: usize = msg_send![data, length];
        let bytes: *const std::ffi::c_void = msg_send![data, bytes];
        if bytes.is_null() || length == 0 {
            return None;
        }
        Some(std::slice::from_raw_parts(bytes.cast::<u8>(), length).to_vec())
    })?;
    let mut rgba = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .ok()?
        .thumbnail(48, 48)
        .into_rgba8();
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
}

#[cfg(target_os = "macos")]
fn thumbnail(path: &Path) -> Option<PathBuf> {
    use std::{
        hash::{Hash, Hasher},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hash);
    let metadata = path.metadata().ok()?;
    metadata.len().hash(&mut hash);
    metadata.modified().ok()?.hash(&mut hash);
    let dir = std::env::temp_dir()
        .join("echo-file-thumbnails")
        .join(format!("{:016x}", hash.finish()));
    let image = dir.join(format!("{}.png", path.file_name()?.to_str()?));
    if image.is_file() {
        return Some(image);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let mut child = Command::new("/usr/bin/qlmanage")
        .args(["-t", "-s", "72", "-o"])
        .arg(&dir)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return (status.success() && image.is_file()).then_some(image),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// GPUI's shared clipboard currently has strings/images only. Keep the native
/// file-URL bridge isolated here until its upstream clipboard supports files.
#[cfg(target_os = "macos")]
pub(crate) fn copy_file(path: &Path, _: &App) -> Result<(), String> {
    use objc2::{
        class, msg_send,
        runtime::{AnyObject, Bool},
    };
    use objc2_foundation::NSString;
    objc2::rc::autoreleasepool(|_| unsafe {
        let path = path.to_str().ok_or("Cannot copy a non-UTF-8 file path")?;
        let url: *mut AnyObject =
            msg_send![class!(NSURL), fileURLWithPath: &*NSString::from_str(path)];
        let values: *mut AnyObject = msg_send![class!(NSArray), arrayWithObject: url];
        let board: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        let _: isize = msg_send![board, clearContents];
        let copied: Bool = msg_send![board, writeObjects: values];
        copied
            .as_bool()
            .then_some(())
            .ok_or_else(|| "Cannot copy this file".into())
    })
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn copy_file(path: &Path, cx: &App) -> Result<(), String> {
    // Preserve a usable file URL on platforms whose GPUI clipboard has no file
    // list representation; never substitute the original network URL.
    let url = url::Url::from_file_path(path).map_err(|_| "Cannot copy this file path")?;
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(url.into()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use browser_core::{
        Browser,
        downloads::{DownloadCompletion, DownloadTime},
    };
    fn record(name: &str, mime: &str, source: &str) -> CompletedDownload {
        let mut browser = Browser::new();
        let profile = browser.default_user_profile_id();
        browser
            .record_completed_download(
                profile,
                DownloadCompletion {
                    final_path: std::env::temp_dir().join(name),
                    source_url: source.into(),
                    original_url: source.into(),
                    mime_type: mime.into(),
                    finished_at: DownloadTime::Unknown,
                },
            )
            .unwrap();
        browser.completed_downloads(profile).unwrap()[0].clone()
    }
    #[test]
    fn reference_file_kinds_and_source_hosts_never_expose_paths_or_credentials() {
        for (name, mime, url, expected) in [
            (
                "fixture.bin",
                "application/octet-stream",
                "http://127.0.0.1:1234/file",
                "Archive",
            ),
            (
                "fixture.DMG",
                "application/octet-stream",
                "https://phibrowser.com/download",
                "Disk Image from phibrowser.com",
            ),
            (
                "fixture.pdf",
                "application/pdf",
                "https://user:secret@www.linkedin.com/path?secret=yes",
                "PDF from linkedin.com",
            ),
            (
                "fixture.png",
                "image/png",
                "file:///Users/example/private/file.png",
                "Image",
            ),
            (
                "fixture.mp4",
                "video/mp4",
                "https://google.com/file",
                "Movie from google.com",
            ),
        ] {
            assert_eq!(subtitle(&record(name, mime, url)), expected);
        }
    }
}

/// Use the native filesystem API on macOS; the crate's default Finder backend
/// sends AppleEvents and may require Automation permission.
pub(crate) fn move_to_trash(path: &Path) -> Result<(), String> {
    let context = {
        #[cfg(target_os = "macos")]
        {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            let mut context = trash::TrashContext::default();
            context.set_delete_method(DeleteMethod::NsFileManager);
            context
        }
        #[cfg(not(target_os = "macos"))]
        {
            trash::TrashContext::default()
        }
    };
    context
        .delete(path)
        .map_err(|error| format!("Could not move file to Trash: {error}"))
}
