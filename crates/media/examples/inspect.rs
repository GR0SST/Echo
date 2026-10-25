//! Explicit fixture-only native check. No default folders, writes, or scanning.
use echo_media::{
    stamp,
    thumbnails::{Decoder, SystemDecoder},
};
fn main() {
    for path in std::env::args_os().skip(1) {
        let stamp = stamp(std::path::Path::new(&path)).expect("fixture file");
        let image = SystemDecoder
            .decode(&stamp, 160)
            .expect("native fixture thumbnail");
        println!(
            "{}: {}x{}, {} bytes",
            stamp.path.file_name().unwrap().to_string_lossy(),
            image.width,
            image.height,
            image.bytes()
        );
    }
}
