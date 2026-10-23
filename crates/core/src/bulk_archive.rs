//! Relative archive actions only remove Today items from the current Space.
//! Favorites and Pinned items can be reference points, but are never archived.
use crate::{Browser, Closed, TabId};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relative { Other, Above, Below }

impl Browser {
    pub fn tabs_to_archive(&self, reference: TabId, relative: Relative) -> Vec<TabId> {
        let anchor = self.item_anchor(reference);
        let order: Vec<_> = self.active_user_profile().favorites.iter().chain(self.space().tabs())
            .filter(|tab| !self.is_split_secondary(tab.id)).map(|tab| tab.id).collect();
        let Some(position) = order.iter().position(|id| *id == anchor) else { return Vec::new(); };
        self.space().today.iter().filter(|tab| !self.is_split_secondary(tab.id))
            .filter(|tab| tab.id != anchor)
            .filter(|tab| {
                let index = order.iter().position(|id| *id == tab.id).unwrap();
                match relative { Relative::Other => true, Relative::Above => index < position, Relative::Below => index > position }
            }).map(|tab| tab.id).collect()
    }

    /// Split Views are archived as one item. If the focused item is removed,
    /// focus returns to the reference item; otherwise the current page stays.
    pub fn archive_relative(&mut self, reference: TabId, relative: Relative, now: SystemTime) -> usize {
        let capture = crate::undo::ArchiveCapture::new(self);
        let tabs = self.tabs_to_archive(reference, relative);
        let active_removed = self.space().active_tab.is_some_and(|active| tabs.contains(&self.item_anchor(active)));
        let mut count = 0;
        for id in tabs { if self.close_tab_unrecorded(id, now) == Some(Closed::Archived) { count += 1; } }
        if active_removed { self.activate_sidebar_item(reference, now); }
        capture.record(self);
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session, splits::Placement};
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;

    #[test]
    fn above_and_below_follow_sidebar_order_and_keep_the_reference() {
        for (relative, expected) in [(Relative::Above, vec![4]), (Relative::Below, vec![2]), (Relative::Other, vec![4, 2])] {
            let mut browser = Browser::new();
            let a = browser.new_tab("https://a", NOW);
            let b = browser.new_tab("https://b", NOW);
            browser.new_tab("https://c", NOW);
            browser.activate_tab(b, NOW);
            assert_eq!(browser.tabs_to_archive(b, relative).iter().map(|id| id.get()).collect::<Vec<_>>(), expected);
            assert_eq!(browser.archive_relative(b, relative, NOW), expected.len());
            assert_eq!(browser.active_tab().unwrap().id, b);
            assert!(browser.all_tabs().any(|tab| tab.id == b));
            if relative == Relative::Above { assert!(browser.all_tabs().any(|tab| tab.id == a)); }
        }
    }

    #[test]
    fn other_tabs_preserve_favorites_pins_other_spaces_and_focus_an_inactive_reference_if_needed() {
        let mut browser = Browser::new();
        let favorite = browser.new_tab("https://favorite", NOW);
        browser.favorite_tab(favorite);
        let pin = browser.new_tab("https://pin", NOW);
        browser.toggle_pin(pin);
        let reference = browser.new_tab("https://reference", NOW);
        browser.new_tab("https://active", NOW);
        browser.add_space("Work", 10.);
        browser.switch_space(1);
        let elsewhere = browser.new_tab("https://elsewhere", NOW);
        browser.switch_space(0);
        assert_eq!(browser.archive_relative(reference, Relative::Other, NOW), 1);
        assert_eq!(browser.active_tab().unwrap().id, reference);
        assert!(browser.is_favorite(favorite));
        assert_eq!(browser.space().pinned[0].id, pin);
        assert_eq!(browser.spaces[1].today[0].id, elsewhere);
        assert!(browser.tabs_to_archive(elsewhere, Relative::Other).is_empty());
        assert_eq!(browser.archive_relative(favorite, Relative::Above, NOW), 0);
        assert_eq!(browser.archive_relative(pin, Relative::Below, NOW), 1);
        assert_eq!(browser.active_tab().unwrap().id, pin);
    }

    #[test]
    fn split_groups_are_atomic_on_both_sides_and_restore_with_all_their_pages() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.add_split(a, b, Placement::Right, NOW).unwrap();
        let target = browser.new_tab("https://target", NOW);
        let c = browser.new_tab("https://c", NOW);
        let d = browser.new_tab("https://d", NOW);
        browser.add_split(c, d, Placement::Right, NOW).unwrap();
        browser.activate_tab(b, NOW);
        assert_eq!(browser.archive_relative(target, Relative::Other, NOW), 2);
        assert_eq!(browser.active_tab().unwrap().id, target);
        assert_eq!(browser.archive.len(), 2);
        assert!(browser.archive.iter().all(|entry| entry.split.is_some()));
        let mut browser = session::from_json(&session::to_json(&browser)).unwrap();
        browser.restore_last_archived(NOW).unwrap();
        assert_eq!(browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(browser.all_tabs().count(), 3);
        assert_eq!(browser.tabs_to_archive(browser.active_tab().unwrap().id, Relative::Other), vec![target]);
    }
}
