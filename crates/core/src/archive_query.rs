//! Scoped and global queries shared by Archive presentations.
use crate::{ArchiveReason, ArchivedTab, SpaceId, UserProfileId};
use std::collections::HashSet;

#[derive(Default)]
pub struct ArchiveFilter {
    pub query: String,
    pub spaces: HashSet<SpaceId>,
    pub reason: Option<ArchiveReason>,
    pub little_arc: bool,
}
impl ArchiveFilter {
    pub fn matches(&self, entry: &ArchivedTab, profile: UserProfileId) -> bool {
        self.matches_scope(entry, Some(profile))
    }
    /// Library spans all local profiles; website actions retain each row’s owner.
    pub fn matches_global(&self, entry: &ArchivedTab) -> bool {
        self.matches_scope(entry, None)
    }
    fn matches_scope(&self, entry: &ArchivedTab, profile: Option<UserProfileId>) -> bool {
        if profile.is_some_and(|profile| entry.user_profile_id != profile)
            || ((!self.spaces.is_empty() || self.little_arc)
                && !((self.little_arc && entry.origin == crate::ArchiveOrigin::LittleArc)
                    || (entry.origin == crate::ArchiveOrigin::Space && self.spaces.contains(&entry.space))))
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
        assert!(filter.matches_global(entry));
        filter.reason = Some(ArchiveReason::Automatic);
        assert!(!filter.matches(entry, profile));
        assert!(!filter.matches_global(entry));
        filter.reason = Some(ArchiveReason::Manual);
        assert!(filter.matches(entry, profile));
        filter.query = "library".into();
        filter.spaces.insert(entry.space);
        assert!(filter.matches(entry, profile));
        filter.query = "not present".into();
        assert!(!filter.matches(entry, profile));
        assert!(!filter.matches_global(entry));
    }
}

impl crate::Browser {
    /// Retain a closed, nonempty Little Arc lookup in the normal workspace.
    /// The independent lookup's TabId must never enter the canonical inventory.
    pub fn archive_lookup_tab(&mut self, mut tab: crate::Tab, profile: UserProfileId,
        source_space: SpaceId, reason: ArchiveReason, now: std::time::SystemTime) -> Option<crate::TabId> {
        if self.is_private() || self.is_blank() || self.user_profile(profile).is_none()
            || !url::Url::parse(&tab.url).ok().is_some_and(|url| matches!(url.scheme(), "http" | "https")) { return None; }
        let space = self.spaces.iter().find(|space| space.id == source_space && space.user_profile_id == profile)
            .or_else(|| self.spaces.iter().find(|space| space.user_profile_id == profile))?.id;
        tab.id = crate::TabId(self.next_id());
        tab.pinned_url = None;
        let id = tab.id;
        self.archive.push(ArchivedTab { tab, space, user_profile_id: profile, archived_at: now,
            reason, origin: crate::ArchiveOrigin::LittleArc, split: None });
        Some(id)
    }
}

#[cfg(test)]
mod lookup_tests {
    use super::*;
    use crate::{ArchiveOrigin, Browser};
    use std::time::SystemTime;
    #[test]
    fn lookup_archive_allocates_canonical_identity_and_combines_origin_with_space_filters() {
        let now = SystemTime::UNIX_EPOCH;
        let mut browser = Browser::new();
        let profile = browser.active_user_profile_id();
        let space = browser.space().id;
        let selected = browser.new_tab("https://selected.example", now);
        let ordinary = browser.new_tab("https://ordinary.example", now);
        browser.close_tab(ordinary, now);
        let mut lookup = Browser::new();
        let local = lookup.new_tab("https://lookup.example", now);
        lookup.set_title(local, "Little archive fixture");
        let id = browser.archive_lookup_tab(lookup.active_tab().unwrap().clone(), profile, space, ArchiveReason::Manual, now).unwrap();
        assert_ne!(id, local);
        assert_ne!(id, selected);
        assert_ne!(id, ordinary);
        assert_eq!(browser.space().active_tab, Some(selected));
        assert_eq!(browser.all_tabs().count(), 1);
        let restored = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        let entry = restored.archive.iter().find(|entry| entry.tab.id == id).unwrap();
        assert_eq!(entry.origin, ArchiveOrigin::LittleArc);
        assert_eq!(entry.tab.title, "Little archive fixture");
        let mut filter = ArchiveFilter { little_arc: true, ..Default::default() };
        assert!(filter.matches_global(entry));
        let normal = restored.archive.iter().find(|entry| entry.tab.id == ordinary).unwrap();
        assert!(!filter.matches_global(normal));
        filter.spaces.insert(space);
        assert!(filter.matches_global(entry));
        assert!(filter.matches_global(normal), "Space and Little Arc choices are a union");
        filter.little_arc = false;
        assert!(!filter.matches_global(entry), "a source Space must not classify a lookup as an ordinary Space tab");
        let mut old: serde_json::Value = serde_json::from_str(&crate::session::to_json(&browser)).unwrap();
        for entry in old["archive"].as_array_mut().unwrap() { entry.as_object_mut().unwrap().remove("origin"); }
        let legacy = crate::session::from_json(&old.to_string()).unwrap();
        assert!(legacy.archive.iter().all(|entry| entry.origin == ArchiveOrigin::Space));
        lookup.set_url(local, "about:blank");
        assert!(browser.archive_lookup_tab(lookup.active_tab().unwrap().clone(), profile, space, ArchiveReason::Manual, now).is_none());
    }
}
