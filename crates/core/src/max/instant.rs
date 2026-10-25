use crate::{Browser, SpaceId, TabId, UserProfileId};
use echo_ai::{MaxFeature, WebLink};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstantSnapshot {
    pub space: SpaceId,
    pub profile: UserProfileId,
    pub source: Option<(TabId, String)>,
}
impl Browser {
    pub fn instant_snapshot(&self) -> Option<InstantSnapshot> {
        if self.is_private()
            || self.is_blank()
            || !self.settings.max.enabled(MaxFeature::InstantLinks)
        {
            return None;
        }
        Some(InstantSnapshot {
            space: self.space().id,
            profile: self.space().user_profile_id,
            source: self.active_tab().map(|t| (t.id, t.url.clone())),
        })
    }
    /// Publish the complete result block atomically, keeping the previous Today
    /// order. No synthetic loading tab is ever serialized or archived.
    pub fn apply_instant_links(
        &mut self,
        snapshot: &InstantSnapshot,
        links: Vec<WebLink>,
        now: SystemTime,
    ) -> bool {
        if self.instant_snapshot().as_ref() != Some(snapshot)
            || !(1..=10).contains(&links.len())
            || links.iter().any(|l| !l.valid())
        {
            return false;
        }
        let mut urls = std::collections::HashSet::new();
        if links.iter().any(|l| !urls.insert(&l.url)) {
            return false;
        }
        for link in links.into_iter().rev() {
            let id = self.new_tab(link.url, now);
            self.set_title(id, link.title);
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn link(n: usize) -> WebLink {
        WebLink {
            title: format!("Result {n}"),
            url: format!("https://rust-lang.org/{n}"),
        }
    }
    #[test]
    fn instant_results_are_an_atomic_top_block_and_never_replace_the_source_page() {
        let mut b = Browser::new();
        let now = SystemTime::UNIX_EPOCH;
        let source = b.new_tab("https://example.org/source", now);
        assert!(b.instant_snapshot().is_none());
        b.settings.max.set_enabled(MaxFeature::InstantLinks, true);
        let snapshot = b.instant_snapshot().unwrap();
        assert!(!b.apply_instant_links(
            &snapshot,
            vec![
                link(1),
                WebLink {
                    title: "Bad".into(),
                    url: "javascript:1".into()
                }
            ],
            now
        ));
        assert_eq!(b.space().today.len(), 1);
        assert!(b.apply_instant_links(&snapshot, vec![link(1), link(2)], now));
        assert_eq!(
            b.space()
                .today
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Result 1", "Result 2", "https://example.org/source"]
        );
        assert_eq!(b.active_tab().unwrap().title, "Result 1");
        assert_eq!(b.space().today[2].id, source);
        assert!(!b.apply_instant_links(&snapshot, vec![link(3)], now));
    }
    #[test]
    fn instant_snapshot_rejects_navigation_feature_and_profile_changes() {
        let mut b = Browser::new();
        b.settings.max.set_enabled(MaxFeature::InstantLinks, true);
        let now = SystemTime::UNIX_EPOCH;
        let id = b.new_tab("https://example.org", now);
        let snapshot = b.instant_snapshot().unwrap();
        b.navigate(id, "https://example.org/changed");
        assert!(!b.apply_instant_links(&snapshot, vec![link(1)], now));
        let snapshot = b.instant_snapshot().unwrap();
        b.settings.max.set_enabled(MaxFeature::InstantLinks, false);
        assert!(!b.apply_instant_links(&snapshot, vec![link(1)], now));
        b.settings.max.set_enabled(MaxFeature::InstantLinks, true);
        let snapshot = b.instant_snapshot().unwrap();
        let profile = b.create_user_profile("Search test").unwrap();
        b.assign_space_user_profile(b.space().id, profile).unwrap();
        assert!(!b.apply_instant_links(&snapshot, vec![link(1)], now));
    }
}
