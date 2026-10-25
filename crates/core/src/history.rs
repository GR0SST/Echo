//! Persistent browsing history, separate from open tabs and the archive.
use crate::{Browser, UserProfileId};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visited_at: SystemTime,
    #[serde(default = "one_visit", skip_serializing_if = "is_one_visit")]
    pub visit_count: u32,
}
fn one_visit() -> u32 { 1 }
fn is_one_visit(count: &u32) -> bool { *count == 1 }

/// Bound accumulated frequency and decay it with age. Library ordering remains
/// chronological; this aggregate score is for local suggestion ordering.
pub fn frecency_score(visits: u32, visited_at: SystemTime, now: SystemTime) -> u64 {
    let days = now.duration_since(visited_at).unwrap_or_default().as_secs() / 86_400;
    u64::from(visits.clamp(1, 32)) * 1_000_000 / (days + 1)
}

/// Select only the requested number of results without sorting/copying a large
/// history. Equal scores retain recency, then the input's stable order.
pub fn ranked<'a>(entries: impl IntoIterator<Item = &'a HistoryEntry>, now: SystemTime, limit: usize) -> Vec<&'a HistoryEntry> {
    if limit == 0 { return Vec::new(); }
    let mut best: Vec<((u64, SystemTime), &HistoryEntry)> = Vec::new();
    for entry in entries {
        let key = (frecency_score(entry.visit_count, entry.visited_at, now), entry.visited_at);
        let at = best.partition_point(|(rank, _)| *rank >= key);
        if at < limit {
            best.insert(at, (key, entry));
            if best.len() > limit { best.pop(); }
        }
    }
    best.into_iter().map(|(_, entry)| entry).collect()
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
        let (title, visit_count) = data.history
            .iter()
            .position(|entry| entry.url == url)
            .map(|index| {
                let previous = data.history.remove(index);
                (previous.title, previous.visit_count.saturating_add(1))
            })
            .unwrap_or_else(|| (url.to_string(), 1));
        data.history.insert(
            0,
            HistoryEntry {
                url: url.into(),
                title,
                visited_at: now,
                visit_count,
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
    fn frequency_survives_session_round_trip_and_legacy_entries_start_with_one_visit() {
        let mut browser = Browser::new();
        let profile = browser.active_user_profile_id();
        let now = SystemTime::UNIX_EPOCH;
        for _ in 0..6 { browser.record_visit(profile, "https://frequent.example", now); }
        let restored = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert_eq!(restored.active_user_profile().history[0].visit_count, 6);
        let mut old: serde_json::Value = serde_json::from_str(&crate::session::to_json(&browser)).unwrap();
        old["profiles"]["1"]["history"][0].as_object_mut().unwrap().remove("visit_count");
        let restored = crate::session::from_json(&old.to_string()).unwrap();
        assert_eq!(restored.active_user_profile().history[0].visit_count, 1);
        browser.active_user_profile_mut().history[0].visit_count = u32::MAX;
        browser.record_visit(profile, "https://frequent.example", now);
        assert_eq!(browser.active_user_profile().history[0].visit_count, u32::MAX);
    }

    #[test]
    fn ranked_matches_prefer_revisits_but_age_out_old_frequency_without_reordering_library_data() {
        let day = std::time::Duration::from_secs(86_400);
        let now = SystemTime::UNIX_EPOCH + day * 1000;
        let entries = vec![
            HistoryEntry { url: "https://recent".into(), title: "Recent".into(), visited_at: now, visit_count: 1 },
            HistoryEntry { url: "https://frequent".into(), title: "Frequent".into(), visited_at: now - std::time::Duration::from_secs(60), visit_count: 6 },
            HistoryEntry { url: "https://ancient".into(), title: "Ancient".into(), visited_at: now - day * 400, visit_count: u32::MAX },
        ];
        assert_eq!(ranked(&entries, now, 2).iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(), vec!["Frequent", "Recent"]);
        assert!(ranked(&entries, now, 0).is_empty());
        assert_eq!(entries.iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(), vec!["Recent", "Frequent", "Ancient"]);
        assert_eq!(ranked(entries[..1].iter(), now, 8).len(), 1);
    }

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
