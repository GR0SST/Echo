//! Installed, read-only product documents. Platform layout stays at this seam.
use std::path::{Path, PathBuf};

pub(crate) fn document(name: &str) -> Option<PathBuf> {
    if Path::new(name).components().count() != 1 {
        return None;
    }
    let executable = std::env::current_exe().ok()?;
    let path = directory(&executable)?.join(name);
    path.is_file().then_some(path)
}

#[cfg(target_os = "macos")]
fn directory(executable: &Path) -> Option<PathBuf> {
    Some(executable.parent()?.parent()?.join("Resources"))
}
#[cfg(target_os = "windows")]
fn directory(executable: &Path) -> Option<PathBuf> {
    Some(executable.parent()?.join("resources"))
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn directory(executable: &Path) -> Option<PathBuf> {
    Some(executable.parent()?.parent()?.join("share/echo"))
}
