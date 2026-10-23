//! Saves and restores the browser model between launches.

use std::io;
use std::path::Path;

use crate::Browser;

/// Version 2 is one workspace with an explicit profile registry. Versioned
/// corruption is an error; it must never authenticate through legacy Default.
pub fn to_json(browser: &Browser) -> String {
    let mut value = serde_json::to_value(browser).expect("the model always serializes");
    value["format_version"] = 2.into();
    serde_json::to_string_pretty(&value).expect("the model always serializes")
}

pub fn from_json(json: &str) -> Result<Browser, serde_json::Error> {
    use serde::de::Error;
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    let new_format = ["format_version", "profiles", "default_user_profile_id", "next_user_profile_id"]
        .iter().any(|key| value.get(key).is_some());
    if new_format {
        if value.get("format_version").and_then(|v| v.as_u64()) != Some(2) {
            return Err(serde_json::Error::custom("invalid or unsupported workspace format"));
        }
    } else {
        migrate_legacy(&mut value)?;
    }
    let mut browser: Browser = serde_json::from_value(value)?;
    if browser.default_user_profile_id != crate::UserProfileId::DEFAULT
        || !browser.profiles.contains_key(&browser.default_user_profile_id)
        || browser.profiles.iter().any(|(id, profile)| id.0 == 0 || *id != profile.id || profile.name.trim().is_empty())
        || browser.spaces.iter().any(|space| !browser.profiles.contains_key(&space.user_profile_id))
        || browser.archive.iter().any(|entry| entry.user_profile_id.0 == 0)
    { return Err(serde_json::Error::custom("invalid user profile registry or ownership")); }
    let max_profile = browser.profiles.keys().map(|id| id.0)
        .chain(browser.archive.iter().map(|entry| entry.user_profile_id.0)).max().unwrap_or(1);
    if browser.next_user_profile_id <= max_profile {
        // A persisted watermark also covers deleted identities. Never repair it
        // downward or infer a fresh Default on malformed versioned input.
        return Err(serde_json::Error::custom("invalid user profile identity watermark"));
    }
    browser.repair();
    Ok(browser)
}

fn migrate_legacy(value: &mut serde_json::Value) -> Result<(), serde_json::Error> {
    use serde::de::Error;
    use crate::profiles::UserProfile;
    let object = value.as_object_mut().ok_or_else(|| serde_json::Error::custom("expected workspace object"))?;
    // Reject partial new ownership without its registry rather than reassigning
    // potentially authenticated pages to Default.
    if ["spaces", "archive"].iter().any(|key| object.get(*key).and_then(|v| v.as_array()).is_some_and(|entries| entries.iter().any(|entry| entry.get("user_profile_id").is_some()))) {
        return Err(serde_json::Error::custom("profile ownership requires a versioned registry"));
    }
    let mut profile = serde_json::to_value(UserProfile::default_profile())?;
    for field in ["favorites", "favorite_splits", "history", "archive_after", "empty_favorites_dismissed", "favicon_cache", "include_favorites_in_ordering"] {
        if let Some(data) = object.remove(field) { profile[field] = data; }
    }
    object.insert("profiles".into(), serde_json::json!({"1": profile}));
    object.insert("default_user_profile_id".into(), 1.into());
    object.insert("next_user_profile_id".into(), 2.into());
    if let Some(spaces) = object.get_mut("spaces").and_then(|v| v.as_array_mut()) {
        for space in spaces {
            space.as_object_mut().ok_or_else(|| serde_json::Error::custom("invalid legacy Space"))?.insert("user_profile_id".into(), 1.into());
        }
    }
    if let Some(archive) = object.get_mut("archive").and_then(|v| v.as_array_mut()) {
        for entry in archive {
            entry.as_object_mut().ok_or_else(|| serde_json::Error::custom("invalid legacy archive entry"))?.insert("user_profile_id".into(), 1.into());
        }
    }
    Ok(())
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
        assert!(!profile.active_user_profile().empty_favorites_dismissed);
        profile.active_user_profile_mut().empty_favorites_dismissed = true;
        profile.add_space("Work", 100.);
        profile.switch_space(1);
        let restored = from_json(&to_json(&profile)).unwrap();
        assert!(restored.active_user_profile().empty_favorites_dismissed);
        assert_eq!(restored.spaces.len(), 2);
        assert!(!Browser::new().active_user_profile().empty_favorites_dismissed);
        let mut legacy: serde_json::Value = serde_json::from_str(&to_json(&profile)).unwrap();
        legacy["profiles"]["1"].as_object_mut().unwrap().remove("empty_favorites_dismissed");
        assert!(!from_json(&legacy.to_string()).unwrap().active_user_profile().empty_favorites_dismissed);
    }
}
