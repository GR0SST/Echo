//! Persistent browsing history, separate from open tabs and the archive.
use crate::{Browser, UserProfileId};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visited_at: SystemTime,
}

impl Browser {
    /// One entry per URL, newest visit first. Internal and data pages are excluded.
    pub fn record_visit(&mut self, profile: UserProfileId, url: &str, now: SystemTime) {
        if self.is_private() { return; }
        let web = url.split_once(':').is_some_and(|(scheme, rest)| {
            (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
                && rest.starts_with("//")
        });
        if !web {
            return;
        }
        let Some(data) = self.user_profile_mut(profile) else { return; };
        let title = data.history
            .iter()
            .position(|entry| entry.url == url)
            .map(|index| data.history.remove(index).title)
            .unwrap_or_else(|| url.to_string());
        data.history.insert(
            0,
            HistoryEntry {
                url: url.into(),
                title,
                visited_at: now,
            },
        );
    }

    pub fn update_history_title(&mut self, profile: UserProfileId, url: &str, title: &str) {
        if title.trim().is_empty() {
            return;
        }
        if self.is_private() { return; }
        let Some(data) = self.user_profile_mut(profile) else { return; };
        if let Some(entry) = data.history.iter_mut().find(|entry| entry.url == url) {
            entry.title = title.into();
        }
    }

    pub fn delete_history_entry(&mut self, profile: UserProfileId, url: &str) {
        let Some(data) = self.user_profile_mut(profile) else { return; };
        data.history.retain(|entry| entry.url != url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_visits_stay_out_of_history_but_closed_tabs_can_be_restored() {
        let mut browser = Browser::new_private();
        let now = SystemTime::UNIX_EPOCH;
        let tab = browser.new_tab("https://private.example/secret", now);
        browser.record_visit(browser.active_user_profile_id(), "https://private.example/secret", now);
        browser.update_history_title(browser.active_user_profile_id(), "https://private.example/secret", "Private page");
        assert!(browser.active_user_profile().history.is_empty());
        assert_eq!(browser.active_user_profile().archive_after, None);
        browser.close_tab(tab, now).unwrap();
        assert_eq!(browser.restore_last_archived(now), Some(tab));
        assert_eq!(browser.active_tab().unwrap().url, "https://private.example/secret");
        assert!(browser.active_user_profile().history.is_empty());
    }

    #[test]
    fn visits_are_ordered_deduplicated_and_keep_page_titles() {
        let mut browser = Browser::new();
        let now = SystemTime::UNIX_EPOCH;
        browser.record_visit(browser.active_user_profile_id(), "https://a", now);
        browser.update_history_title(browser.active_user_profile_id(), "https://a", "Page A");
        browser.record_visit(browser.active_user_profile_id(), "https://b", now);
        browser.record_visit(browser.active_user_profile_id(), "https://a", now + std::time::Duration::from_secs(60));
        assert_eq!(browser.active_user_profile().history.len(), 2);
        assert_eq!(browser.active_user_profile().history[0].url, "https://a");
        assert_eq!(browser.active_user_profile().history[0].title, "Page A");
        assert_eq!(browser.active_user_profile().history[1].url, "https://b");
        for url in [
            "data:text/plain,test",
            "browser://history",
            "about:blank",
            "file:///tmp/test",
        ] {
            browser.record_visit(browser.active_user_profile_id(), url, now);
        }
        assert_eq!(browser.active_user_profile().history.len(), 2);
        let restored = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert_eq!(restored.active_user_profile().history[0].title, "Page A");
    }

    #[test]
    fn deleting_history_preserves_open_tabs_and_archive() {
        let mut browser = Browser::new();
        let now = SystemTime::UNIX_EPOCH;
        let tab = browser.new_tab("https://a", now);
        browser.record_visit(browser.active_user_profile_id(), "https://a", now);
        browser.delete_history_entry(browser.active_user_profile_id(), "https://a");
        assert!(browser.active_user_profile().history.is_empty());
        assert_eq!(browser.active_tab().unwrap().id, tab);
        assert!(browser.archive.is_empty());
    }
}
