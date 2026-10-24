//! Profile-owned history of site permission controls. Effective permissions live
//! in the profile's engine storage; recording a request does not grant access.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Camera,
    Microphone,
    Location,
    Notifications,
    Popups,
    Clipboard,
    AutomaticDownloads,
    Sound,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SiteSettings(BTreeMap<String, BTreeSet<Permission>>);

impl SiteSettings {
    pub fn origin(url: &str) -> Option<String> {
        let url = url::Url::parse(url).ok()?;
        (matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
            .then(|| url.origin().ascii_serialization())
    }
    pub fn record_permission(&mut self, url: &str, permission: Permission) -> bool {
        let Some(origin) = Self::origin(url) else {
            return false;
        };
        self.0.entry(origin).or_default().insert(permission)
    }
    pub fn permissions(&self, url: &str) -> impl Iterator<Item = Permission> + '_ {
        Self::origin(url)
            .and_then(|origin| self.0.get(&origin))
            .into_iter()
            .flatten()
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session, Browser};

    #[test]
    fn permission_usage_is_profile_and_origin_scoped_across_sessions() {
        let mut browser = Browser::new();
        let first = browser.active_user_profile_id();
        let other = browser.create_user_profile("Other").unwrap();
        browser
            .user_profile_mut(first)
            .unwrap()
            .site_settings
            .record_permission("https://CAMERA.example:443/path", Permission::Camera);
        let loaded = session::from_json(&session::to_json(&browser)).unwrap();
        let settings = &loaded.user_profile(first).unwrap().site_settings;
        assert_eq!(
            settings
                .permissions("https://camera.example/next")
                .collect::<Vec<_>>(),
            [Permission::Camera]
        );
        for origin in [
            "http://camera.example",
            "https://camera.example:444",
            "https://other.camera.example",
        ] {
            assert_eq!(settings.permissions(origin).count(), 0);
        }
        assert_eq!(
            loaded
                .user_profile(other)
                .unwrap()
                .site_settings
                .permissions("https://camera.example")
                .count(),
            0
        );
        assert_eq!(
            loaded
                .new_blank_for_user_profile(first)
                .unwrap()
                .active_user_profile()
                .site_settings
                .permissions("https://camera.example")
                .count(),
            1
        );
    }

    #[test]
    fn opaque_origins_are_not_permission_keys_and_old_sessions_remain_readable() {
        let mut settings = SiteSettings::default();
        for origin in [
            "data:text/html,hello",
            "file:///tmp/page",
            "echo://settings",
            "about:blank",
        ] {
            assert!(!settings.record_permission(origin, Permission::Camera));
        }
        let mut old = serde_json::to_value(Browser::new()).unwrap();
        old["format_version"] = 2.into();
        for profile in old["profiles"].as_object_mut().unwrap().values_mut() {
            profile.as_object_mut().unwrap().remove("site_settings");
        }
        assert_eq!(
            session::from_json(&old.to_string())
                .unwrap()
                .active_user_profile()
                .site_settings
                .permissions("https://camera.example")
                .count(),
            0
        );
    }
}
