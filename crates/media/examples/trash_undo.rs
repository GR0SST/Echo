//! Native validation for a dedicated, explicitly named test fixture only.
use echo_media::file_actions::{SystemTrash, TrashPlatform};
fn main() {
    let path = std::path::PathBuf::from(std::env::args_os().nth(1).expect("fixture path"));
    assert!(path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("echo-media-fixture-"));
    let before = std::fs::read(&path).expect("fixture bytes");
    let stamp = echo_media::stamp(&path).expect("fixture stamp");
    let receipt = SystemTrash.move_to_trash(&stamp).expect("native trash");
    assert!(!path.exists());
    SystemTrash.restore(&receipt).expect("native undo");
    assert_eq!(std::fs::read(path).unwrap(), before);
    println!("native Trash and Undo restored the exact fixture bytes");
}
