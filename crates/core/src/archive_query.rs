//! Profile-scoped queries shared by Archive presentations.
use crate::{ArchiveReason, ArchivedTab, SpaceId, UserProfileId};
use std::collections::HashSet;

#[derive(Default)]
pub struct ArchiveFilter {
    pub query: String,
    pub spaces: HashSet<SpaceId>,
    pub reason: Option<ArchiveReason>,
}
impl ArchiveFilter {
    pub fn matches(&self, entry: &ArchivedTab, profile: UserProfileId) -> bool {
        if entry.user_profile_id != profile
            || (!self.spaces.is_empty() && !self.spaces.contains(&entry.space))
            || self.reason.is_some_and(|reason| entry.reason != reason)
        {
            return false;
        }
        let query = self.query.trim().to_lowercase();
        let matches = |tab: &crate::Tab| {
            tab.title.to_lowercase().contains(&query) || tab.url.to_lowercase().contains(&query)
        };
        matches(&entry.tab)
            || entry.split.as_ref().is_some_and(|split| {
                split
                    .view
                    .title
                    .as_ref()
                    .is_some_and(|title| title.to_lowercase().contains(&query))
                    || split.panes.iter().any(matches)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Browser;
    use std::time::SystemTime;
    #[test]
    fn archive_query_combines_profile_space_reason_and_case_insensitive_text() {
        let mut browser = Browser::new();
        let profile = browser.active_user_profile_id();
        let tab = browser.new_tab("https://example.org/Library", SystemTime::now());
        browser.set_title(tab, "Archive Fixture");
        browser.close_tab(tab, SystemTime::now());
        let other = browser.create_user_profile("Other").unwrap();
        let entry = &browser.archive[0];
        let mut filter = ArchiveFilter {
            query: "  FIXTURE  ".into(),
            ..Default::default()
        };
        assert!(filter.matches(entry, profile));
        assert!(!filter.matches(entry, other));
        filter.reason = Some(ArchiveReason::Automatic);
        assert!(!filter.matches(entry, profile));
        filter.reason = Some(ArchiveReason::Manual);
        assert!(filter.matches(entry, profile));
        filter.query = "library".into();
        filter.spaces.insert(entry.space);
        assert!(filter.matches(entry, profile));
        filter.query = "not present".into();
        assert!(!filter.matches(entry, profile));
    }
}
