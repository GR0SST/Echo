//! Private browser-owned capture files; user Downloads/Documents are never swept.
use std::path::Path;

pub(crate) fn remove_captures(root: &Path, key: &str) -> Result<(), String> {
    let id = key.strip_prefix("user-profile-").and_then(|id| id.parse::<u64>().ok())
        .filter(|id| *id > 1).ok_or("Invalid retired capture scope")?;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let parent = root.join("Captures");
    let path = parent.join(format!("profile-{id}"));
    for directory in [&parent, &path] {
        match std::fs::symlink_metadata(directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink()
                && directory.canonicalize().map_err(|error| error.to_string())? == *directory => (),
            Ok(_) => return Err("Retired captures must be direct private directories".into()),
        }
    }
    std::fs::remove_dir_all(path).map_err(|error| error.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retired_capture_cleanup_preserves_default_other_profiles_user_files_and_aliases() {
        let fixture = tempfile::tempdir().unwrap();
        for name in ["Captures/profile-1", "Captures/profile-2", "Captures/profile-3", "Downloads"] {
            let path = fixture.path().join(name);std::fs::create_dir_all(&path).unwrap();std::fs::write(path.join("fixture.png"), b"fixture").unwrap();
        }
        assert!(remove_captures(fixture.path(), "default").is_err());
        assert!(remove_captures(fixture.path(), "user-profile-1").is_err());
        remove_captures(fixture.path(), "user-profile-3").unwrap();
        assert!(!fixture.path().join("Captures/profile-3").exists());
        for name in ["Captures/profile-1", "Captures/profile-2", "Downloads"] { assert!(fixture.path().join(name).join("fixture.png").exists()); }
        #[cfg(unix)] {
            std::os::unix::fs::symlink(fixture.path().join("Downloads"), fixture.path().join("Captures/profile-4")).unwrap();
            assert!(remove_captures(fixture.path(), "user-profile-4").is_err());
            assert!(fixture.path().join("Downloads/fixture.png").exists());
        }
    }
}
