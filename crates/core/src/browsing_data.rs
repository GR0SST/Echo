//! Local metadata counterpart of Chromium's per-profile browsing-data removal.
use crate::{downloads::DownloadTime, Browser, UserProfileId};
use std::time::{Duration, SystemTime};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BrowsingDataRange {
    FifteenMinutes,
    #[default]
    Hour,
    Day,
    Week,
    FourWeeks,
    AllTime,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BrowsingDataPreferences {
    pub range: BrowsingDataRange,
    pub categories: u32,
}
impl Default for BrowsingDataPreferences {
    fn default() -> Self {
        Self {
            range: BrowsingDataRange::Hour,
            categories: 7,
        }
    }
}
impl BrowsingDataRange {
    pub const ALL: [Self; 6] = [
        Self::FifteenMinutes,
        Self::Hour,
        Self::Day,
        Self::Week,
        Self::FourWeeks,
        Self::AllTime,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::FifteenMinutes => "Last 15 min",
            Self::Hour => "Last hour",
            Self::Day => "Last 24 hours",
            Self::Week => "Last 7 days",
            Self::FourWeeks => "Last 4 weeks",
            Self::AllTime => "All time",
        }
    }
    pub fn since(self, now: SystemTime) -> Option<SystemTime> {
        let seconds = match self {
            Self::FifteenMinutes => 900,
            Self::Hour => 3600,
            Self::Day => 86400,
            Self::Week => 604800,
            Self::FourWeeks => 2419200,
            Self::AllTime => return None,
        };
        Some(
            now.checked_sub(Duration::from_secs(seconds))
                .unwrap_or(SystemTime::UNIX_EPOCH),
        )
    }
}
impl Browser {
    pub fn browsing_data_download_count(
        &self,
        profile: UserProfileId,
        since: Option<SystemTime>,
    ) -> usize {
        self.completed_downloads(profile)
            .unwrap_or(&[])
            .iter()
            .filter(|r| download_in_range(r.finished_at(), since))
            .count()
    }
    /// Call only after successful native deletion. Never removes files, tabs,
    /// Favorites, saved passwords or another profile's records.
    pub fn clear_browsing_metadata(
        &mut self,
        profile: UserProfileId,
        categories: u32,
        since: Option<SystemTime>,
    ) {
        if self.is_private() || self.is_blank() {
            return;
        }
        if let Some(p) = self.user_profile_mut(profile) {
            if categories & 1 != 0 {
                p.history
                    .retain(|e| since.is_some_and(|s| e.visited_at < s));
            }
            // Permission usage has no historical timestamp in older sessions.
            // It is not permission authority; native ContentSettings owns that.
            if categories & 32 != 0 && since.is_none() {
                p.site_settings = Default::default();
            }
        }
        if categories & 8 != 0 {
            let ids = self
                .completed_downloads(profile)
                .unwrap_or(&[])
                .iter()
                .filter(|r| download_in_range(r.finished_at(), since))
                .map(|r| r.id())
                .collect::<Vec<_>>();
            for id in ids {
                self.hide_completed_download(id);
            }
        }
    }
}
fn download_in_range(time: DownloadTime, since: Option<SystemTime>) -> bool {
    let Some(since) = since else {
        return true;
    };
    let Some(threshold) = since
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_micros()).ok())
    else {
        return false;
    };
    match time {
        DownloadTime::WindowsEpochMicros(time) => time
            .checked_sub(11_644_473_600_000_000)
            .is_some_and(|t| t >= threshold),
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range_removal_keeps_older_history_other_profiles_and_open_tabs() {
        let mut b = Browser::new();
        let p = b.active_user_profile_id();
        let other = b.create_user_profile("Other").unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100_000);
        b.record_visit(p, "https://old.test", now - Duration::from_secs(7200));
        b.record_visit(p, "https://new.test", now);
        b.record_visit(other, "https://other.test", now);
        let tab = b.new_tab("https://new.test", now);
        b.clear_browsing_metadata(p, 1, BrowsingDataRange::Hour.since(now));
        assert_eq!(
            b.user_profile(p)
                .unwrap()
                .history
                .iter()
                .map(|r| r.url.as_str())
                .collect::<Vec<_>>(),
            ["https://old.test"]
        );
        assert_eq!(b.user_profile(other).unwrap().history.len(), 1);
        assert_eq!(b.active_tab().unwrap().id, tab);
        b.clear_browsing_metadata(p, 1, None);
        assert!(b.user_profile(p).unwrap().history.is_empty());
    }
    #[test]
    fn download_history_clear_never_deletes_the_file_and_preserves_sequence() {
        use crate::downloads::DownloadCompletion;
        let mut b = Browser::new();
        let p = b.active_user_profile_id();
        let path = std::env::temp_dir().join("echo-browsing-data-history-only");
        std::fs::write(&path, b"synthetic").unwrap();
        let completion = |time| DownloadCompletion {
            final_path: path.clone(),
            source_url: "https://fixture.test/a".into(),
            original_url: "https://fixture.test/a".into(),
            mime_type: "text/plain".into(),
            finished_at: DownloadTime::WindowsEpochMicros(time),
        };
        let old = b
            .record_completed_download(p, completion(11_644_473_600_000_001))
            .unwrap();
        let recent = b
            .record_completed_download(p, completion(11_644_473_700_000_000))
            .unwrap();
        b.clear_browsing_metadata(p, 8, Some(SystemTime::UNIX_EPOCH + Duration::from_secs(50)));
        assert_eq!(
            b.completed_downloads(p)
                .unwrap()
                .iter()
                .map(|r| r.id())
                .collect::<Vec<_>>(),
            [old]
        );
        assert!(path.exists());
        b.clear_browsing_metadata(p, 8, None);
        let next = b
            .record_completed_download(p, completion(11_644_473_800_000_000))
            .unwrap();
        assert!(next.sequence() > recent.sequence());
        std::fs::remove_file(path).unwrap();
    }
}
