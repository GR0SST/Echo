//! Saves and restores the browser model between launches.

use std::io;
use std::path::Path;

use crate::Browser;

pub fn to_json(browser: &Browser) -> String {
    serde_json::to_string_pretty(browser).expect("the model always serializes")
}

pub fn from_json(json: &str) -> Result<Browser, serde_json::Error> {
    let mut browser: Browser = serde_json::from_str(json)?;
    browser.repair();
    Ok(browser)
}

/// `Ok(None)` when no session was saved yet.
pub fn load(path: &Path) -> io::Result<Option<Browser>> {
    match std::fs::read_to_string(path) {
        Ok(json) => from_json(&json)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Writes through a temporary file, so a crash never leaves half a session.
pub fn save(path: &Path, json: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, json)?;
    std::fs::rename(&temp, path)
}

#[cfg(test)]
mod empty_favorites_tests {
    use super::*;

    #[test]
    fn empty_favorites_dismissal_is_profile_local_and_backward_compatible() {
        let mut profile = Browser::new();
        assert!(!profile.empty_favorites_dismissed);
        profile.empty_favorites_dismissed = true;
        profile.add_space("Work", 100.);
        profile.switch_space(1);
        let restored = from_json(&to_json(&profile)).unwrap();
        assert!(restored.empty_favorites_dismissed);
        assert_eq!(restored.spaces.len(), 2);
        assert!(!Browser::new().empty_favorites_dismissed);
        let mut legacy: serde_json::Value = serde_json::from_str(&to_json(&profile)).unwrap();
        legacy.as_object_mut().unwrap().remove("empty_favorites_dismissed");
        assert!(!from_json(&legacy.to_string()).unwrap().empty_favorites_dismissed);
    }
}
