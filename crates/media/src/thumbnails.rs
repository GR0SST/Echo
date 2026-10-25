//! Bounded background thumbnails. No decoder, cache or worker reads a file until
//! a discovered stamp is explicitly requested by the visible Media presenter.
use browser_core::media::{MediaKind, MediaStamp};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::File,
    io::BufReader,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
};
const MAX_SOURCE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_EDGE: u32 = 512;
#[derive(Clone, Debug)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}
impl Thumbnail {
    pub fn bytes(&self) -> usize {
        self.rgba.len()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    stamp: MediaStamp,
    edge: u32,
}
/// Injectable to exercise scheduling/cache limits without touching user files.
pub trait Decoder: Send + Sync + 'static {
    fn decode(&self, stamp: &MediaStamp, edge: u32) -> Option<Thumbnail>;
}
pub struct ImageDecoder;
/// Native Quick Look covers HEIC and movies on macOS; portable raster decoding
/// remains available on every platform. Native work shares the same two-worker
/// queue, time limit, stamp validation and output/cache budgets.
pub struct SystemDecoder;
impl Decoder for SystemDecoder {
    fn decode(&self, stamp: &MediaStamp, edge: u32) -> Option<Thumbnail> {
        if let Some(image) = ImageDecoder.decode(stamp, edge) {
            return Some(image);
        }
        if edge == 0 || edge > MAX_EDGE || stamp.bytes > MAX_SOURCE_BYTES {
            return None;
        }
        crate::validate(stamp).ok()?;
        #[cfg(target_os = "macos")]
        {
            let image = crate::macos::thumbnail(&stamp.path, edge)?;
            crate::validate(stamp).ok()?;
            Some(image)
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
}
impl Decoder for ImageDecoder {
    fn decode(&self, stamp: &MediaStamp, edge: u32) -> Option<Thumbnail> {
        if edge == 0
            || edge > MAX_EDGE
            || stamp.bytes > MAX_SOURCE_BYTES
            || MediaKind::from_path(&stamp.path) != Some(MediaKind::Image)
        {
            return None;
        }
        crate::validate(stamp).ok()?;
        let file = File::open(&stamp.path).ok()?;
        let mut reader = image::ImageReader::new(BufReader::new(file))
            .with_guessed_format()
            .ok()?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(64 * 1024 * 1024);
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        reader.limits(limits);
        let image = reader.decode().ok()?.thumbnail(edge, edge).to_rgba8();
        crate::validate(stamp).ok()?;
        Some(Thumbnail {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw().into(),
        })
    }
}
struct Cache {
    entries: HashMap<Key, Option<Arc<Thumbnail>>>,
    order: VecDeque<Key>,
    bytes: usize,
    budget: usize,
    count: usize,
}
impl Cache {
    fn get(&mut self, key: &Key) -> Option<Option<Arc<Thumbnail>>> {
        let value = self.entries.get(key)?.clone();
        self.order.retain(|k| k != key);
        self.order.push_back(key.clone());
        Some(value)
    }
    fn put(&mut self, key: Key, value: Option<Thumbnail>) {
        let value = value.map(Arc::new);
        let size = value.as_ref().map_or(0, |v| v.bytes());
        if size > self.budget {
            return;
        }
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.as_ref().map_or(0, |v| v.bytes());
            self.order.retain(|k| k != &key);
        }
        while self.bytes + size > self.budget || self.entries.len() >= self.count {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.entries.remove(&old) {
                self.bytes -= old.as_ref().map_or(0, |v| v.bytes());
            }
        }
        self.bytes += size;
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
    }
}
pub struct Thumbnails {
    cache: Arc<Mutex<Cache>>,
    pending: Arc<Mutex<HashSet<Key>>>,
    requests: mpsc::SyncSender<Key>,
    cancelled: Arc<AtomicBool>,
}
impl Default for Thumbnails {
    fn default() -> Self {
        Self::new(Arc::new(SystemDecoder), 16 * 1024 * 1024, 96)
    }
}
impl Thumbnails {
    pub fn new(decoder: Arc<dyn Decoder>, bytes: usize, count: usize) -> Self {
        let cache = Arc::new(Mutex::new(Cache {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            budget: bytes,
            count: count.max(1),
        }));
        let pending = Arc::new(Mutex::new(HashSet::new()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (requests, receiver) = mpsc::sync_channel::<Key>(32);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..2 {
            let (cache, pending, cancelled, decoder, receiver) = (
                cache.clone(),
                pending.clone(),
                cancelled.clone(),
                decoder.clone(),
                receiver.clone(),
            );
            std::thread::Builder::new()
                .name(format!("media-thumbnail-{index}"))
                .spawn(move || loop {
                    let Ok(key) = receiver.lock().unwrap_or_else(|p| p.into_inner()).recv() else {
                        break;
                    };
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    let value = decoder.decode(&key.stamp, key.edge);
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    cache
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .put(key.clone(), value);
                    pending
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&key);
                })
                .expect("thumbnail worker");
        }
        Self {
            cache,
            pending,
            requests,
            cancelled,
        }
    }
    /// Nonblocking; queue pressure drops requests so scrolling cannot accumulate work.
    /// Errors are cached too, avoiding repeated decodes of an unsupported/bad file.
    pub fn request(&self, stamp: &MediaStamp, edge: u32) -> Option<Arc<Thumbnail>> {
        if edge == 0 || edge > MAX_EDGE {
            return None;
        }
        let key = Key {
            stamp: stamp.clone(),
            edge,
        };
        if let Some(value) = self
            .cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&key)
        {
            return value;
        }
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        if pending.len() < 34
            && pending.insert(key.clone())
            && self.requests.try_send(key.clone()).is_err()
        {
            pending.remove(&key);
        }
        None
    }
    pub fn memory_bytes(&self) -> usize {
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).bytes
    }
    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap_or_else(|p| p.into_inner()).len()
    }
}
impl Drop for Thumbnails {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn thumbnail_pixels_are_bounded_and_replacements_invalidate_the_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.png");
        image::RgbaImage::from_pixel(1000, 500, image::Rgba([40, 70, 90, 255]))
            .save(&path)
            .unwrap();
        let stamp = crate::stamp(&path).unwrap();
        let image = ImageDecoder.decode(&stamp, 160).unwrap();
        assert_eq!((image.width, image.height), (160, 80));
        assert_eq!(image.bytes(), 160 * 80 * 4);
        std::fs::write(&path, b"replacement").unwrap();
        assert!(ImageDecoder.decode(&stamp, 160).is_none());
    }
    #[test]
    fn cache_evicts_by_bytes_and_count_and_caches_failures() {
        let mut cache = Cache {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            budget: 32,
            count: 2,
        };
        let key = |n| Key {
            stamp: MediaStamp {
                path: format!("/fixture/{n}.png").into(),
                bytes: 0,
                modified: std::time::SystemTime::UNIX_EPOCH,
                identity: None,
            },
            edge: 2,
        };
        let image = || {
            Some(Thumbnail {
                width: 2,
                height: 2,
                rgba: vec![0; 16].into(),
            })
        };
        cache.put(key(1), image());
        cache.put(key(2), image());
        cache.get(&key(1));
        cache.put(key(3), image());
        assert!(cache.get(&key(2)).is_none());
        assert_eq!(cache.bytes, 32);
        cache.put(key(4), None);
        assert!(matches!(cache.get(&key(4)), Some(None)));
        assert_eq!(cache.entries.len(), 2);
        assert!(cache.bytes <= 32);
    }
    #[test]
    fn background_requests_coalesce_and_produce_a_cached_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.png");
        image::RgbaImage::new(16, 16).save(&path).unwrap();
        let stamp = crate::stamp(&path).unwrap();
        let thumbs = Thumbnails::default();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if thumbs.request(&stamp, 8).is_some() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(thumbs.memory_bytes(), 8 * 8 * 4);
        assert!(thumbs.pending_count() <= 1);
    }
}
