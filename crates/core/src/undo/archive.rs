//! Manual whole-item archives are one transaction, including Clear Today and
//! relative archive commands. Payloads remain in the archive until Undo; a
//! cleared or explicitly restored entry cannot be resurrected by stale history.
use super::{insertion_index, SidebarEdit};
use crate::{ArchivedTab, ArchiveReason, Browser, Closed, SpaceId, TabId};
use std::time::SystemTime;

#[derive(Clone, Debug)]
struct ArchivedItem { anchor: TabId, panes: Vec<TabId> }

#[derive(Clone, Debug)]
pub(super) struct ArchiveEdit {
    space: SpaceId,
    user_profile_id: crate::UserProfileId,
    order: Vec<TabId>,
    active: Option<TabId>,
    items: Vec<ArchivedItem>,
}

pub(crate) struct ArchiveCapture {
    space: SpaceId,
    user_profile_id: crate::UserProfileId,
    order: Vec<TabId>,
    active: Option<TabId>,
    archive_start: usize,
}

impl ArchiveCapture {
    pub(crate) fn new(browser: &Browser) -> Self {
        Self { user_profile_id: browser.space().user_profile_id, space: browser.space().id, order: browser.space().today.iter().map(|tab| tab.id).collect(),
            active: browser.space().active_tab, archive_start: browser.archive.len() }
    }

    pub(crate) fn record(self, browser: &mut Browser) {
        if let Some(edit) = self.finish(browser) { browser.sidebar_history.record(SidebarEdit::Archive(edit)); }
    }

    pub(super) fn finish(self, browser: &Browser) -> Option<ArchiveEdit> {
        let items: Vec<_> = browser.archive[self.archive_start..].iter()
            .filter(|entry| entry.space == self.space && entry.reason == ArchiveReason::Manual)
            .map(|entry| ArchivedItem { anchor: entry.tab.id, panes: archived_ids(entry) }).collect();
        if items.is_empty() { return None; }
        Some(ArchiveEdit {
            space: self.space, user_profile_id: self.user_profile_id, order: self.order, active: self.active, items,
        })
    }
}

fn archived_ids(entry: &ArchivedTab) -> Vec<TabId> {
    entry.split.as_ref().map_or_else(|| vec![entry.tab.id], |saved| saved.view.panes.clone())
}

impl ArchiveEdit {
    pub(super) fn apply(&self, redo: bool, browser: &mut Browser, now: SystemTime) -> bool {
        let Some(index) = browser.spaces.iter().position(|space| space.id == self.space) else { return false; };
        if browser.spaces[index].user_profile_id != self.user_profile_id { return false; }
        if redo {
            // Validate the complete transaction before removing anything.
            // A subsequently regrouped, pinned or moved page invalidates it.
            if self.items.iter().any(|item| {
                !item.panes.iter().all(|id| browser.spaces[index].today.iter().any(|tab| tab.id == *id))
                    || browser.related_tabs(item.anchor) != item.panes
            }) { return false; }
            browser.active_space = index;
            for item in &self.items {
                let result = browser.close_tab_unrecorded(item.anchor, now);
                debug_assert_eq!(result, Some(Closed::Archived));
            }
            return true;
        }
        if self.items.iter().any(|item| {
            !browser.archive.iter().any(|entry| entry.tab.id == item.anchor && entry.space == self.space && entry.user_profile_id == self.user_profile_id
                && entry.reason == ArchiveReason::Manual && archived_ids(entry) == item.panes)
                || item.panes.iter().any(|id| browser.all_tabs().any(|tab| tab.id == *id))
                || browser.profiles.values().flat_map(|profile| &profile.favorite_splits).chain(browser.spaces.iter().flat_map(|space| &space.splits))
                    .any(|split| browser.archive.iter().find(|entry| entry.tab.id == item.anchor)
                        .and_then(|entry| entry.split.as_ref()).is_some_and(|saved| saved.view.id == split.id))
        }) { return false; }
        let mut pages = Vec::new();
        let mut splits = Vec::new();
        for item in &self.items {
            let at = browser.archive.iter().position(|entry| entry.tab.id == item.anchor).unwrap();
            let entry = browser.archive.remove(at);
            pages.push(entry.tab);
            if let Some(saved) = entry.split {
                pages.extend(saved.panes);
                splits.push(saved.view);
            }
        }
        pages.sort_by_key(|tab| self.order.iter().position(|id| *id == tab.id).unwrap());
        let space = &mut browser.spaces[index];
        for mut tab in pages {
            tab.last_active = now;
            let at = insertion_index(&space.today.iter().map(|tab| tab.id).collect::<Vec<_>>(), &self.order, tab.id);
            space.today.insert(at, tab);
        }
        space.splits.extend(splits);
        browser.active_space = index;
        if self.active.is_some_and(|active| browser.active_user_profile().favorites.iter().any(|tab| tab.id == active)
            || browser.spaces[index].tabs().any(|tab| tab.id == active)) {
            browser.spaces[index].active_tab = self.active;
        }
        browser.repair_folders();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bulk_archive::Relative, session, splits::Placement};
    use std::time::Duration;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;
    fn later() -> SystemTime { NOW + Duration::from_secs(600) }
    fn ids(browser: &Browser) -> Vec<TabId> { browser.space().today.iter().map(|tab| tab.id).collect() }

    #[test]
    fn undo_restores_original_identity_position_selection_and_metadata_without_rewinding_other_tabs() {
        let mut browser = Browser::new();
        let bottom = browser.new_tab("https://bottom", NOW);
        let middle = browser.new_tab("https://middle", NOW);
        let top = browser.new_tab("https://top", NOW);
        browser.rename_tab(middle, "Custom name");
        browser.activate_tab(middle, NOW);
        assert_eq!(browser.close_tab(middle, NOW), Some(Closed::Archived));
        browser.navigate(top, "https://top/later");
        let newer = browser.new_tab("https://newer", later());
        assert!(browser.undo_sidebar(later()));
        assert_eq!(ids(&browser), vec![newer, top, middle, bottom]);
        assert_eq!(browser.active_tab().unwrap().id, middle);
        assert_eq!(browser.active_tab().unwrap().last_active, later());
        assert_eq!(browser.active_tab().unwrap().custom_title.as_deref(), Some("Custom name"));
        assert_eq!(browser.all_tabs().find(|tab| tab.id == top).unwrap().url, "https://top/later");
        assert!(browser.archive.is_empty());
        browser.navigate(middle, "https://middle/later");
        assert!(browser.redo_sidebar(later()));
        assert_eq!(ids(&browser), vec![newer, top, bottom]);
        assert_eq!(browser.archive[0].tab.url, "https://middle/later");
        assert_eq!(browser.archive[0].archived_at, later());
        assert_eq!(browser.archive[0].reason, ArchiveReason::Manual);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.active_tab().unwrap().url, "https://middle/later");
    }

    #[test]
    fn pin_and_archive_share_one_ordered_history_and_new_archives_clear_redo() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        browser.toggle_pin(a);
        browser.toggle_pin(a);
        browser.close_tab(a, NOW);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(ids(&browser), vec![a]);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.space().pinned[0].id, a);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(ids(&browser), vec![a]);
        assert!(!browser.undo_sidebar(later()));
        for _ in 0..3 { assert!(browser.redo_sidebar(later())); }
        assert!(browser.all_tabs().next().is_none());
        assert!(browser.undo_sidebar(later()));
        let b = browser.new_tab("https://b", NOW);
        browser.close_tab(b, NOW);
        assert!(!browser.redo_sidebar(later()));
        assert!(browser.undo_sidebar(later()));
        assert!(browser.all_tabs().any(|tab| tab.id == a));
    }

    #[test]
    fn clear_today_restores_a_split_and_other_items_as_one_transaction_and_preserves_pins_favorites_and_spaces() {
        let mut browser = Browser::new();
        let favorite = browser.new_tab("https://favorite", NOW);
        browser.favorite_tab(favorite);
        let pin = browser.new_tab("https://pin", NOW);
        browser.toggle_pin(pin);
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.add_split(a, b, Placement::Right, NOW).unwrap();
        let split = browser.split_for(a).unwrap().id;
        browser.spaces[0].splits[0].weights = vec![0.3, 0.7];
        browser.rename_tab(a, "Saved split");
        let single = browser.new_tab("https://single", NOW);
        browser.activate_tab(b, NOW);
        browser.activate_tab(pin, NOW);
        let before = ids(&browser);
        browser.add_space("Other", 10.);
        browser.switch_space(1);
        let elsewhere = browser.new_tab("https://elsewhere", NOW);
        browser.switch_space(0);
        browser.clear_sidebar_history();
        assert_eq!(browser.clear_today(NOW), 2);
        assert_eq!(browser.archive.len(), 2);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(ids(&browser), before);
        assert_eq!(browser.active_tab().unwrap().id, pin);
        assert_eq!(browser.split_for(b).unwrap().id, split);
        assert_eq!(browser.split_for(b).unwrap().active, b);
        assert_eq!(browser.split_for(b).unwrap().title.as_deref(), Some("Saved split"));
        assert_eq!(browser.split_for(b).unwrap().weights, vec![0.3, 0.7]);
        assert!(browser.archive.is_empty());
        assert!(!browser.undo_sidebar(later()));
        assert!(browser.redo_sidebar(later()));
        assert_eq!(browser.archive.len(), 2);
        assert!(ids(&browser).is_empty());
        assert_eq!(browser.active_user_profile().favorites[0].id, favorite);
        assert_eq!(browser.space().pinned[0].id, pin);
        assert_eq!(browser.spaces[1].today[0].id, elsewhere);
        assert!(browser.archive.iter().any(|entry| entry.tab.id == single));
    }

    #[test]
    fn relative_archive_is_atomic_and_retains_the_reference_and_unrelated_archive_entries() {
        let mut browser = Browser::new();
        let old = browser.new_tab("https://old", NOW);
        browser.close_tab(old, NOW);
        let reference = browser.new_tab("https://reference", NOW);
        browser.new_tab("https://first", NOW);
        browser.new_tab("https://second", NOW);
        browser.clear_sidebar_history();
        let before = ids(&browser);
        assert_eq!(browser.archive_relative(reference, Relative::Above, NOW), 2);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(ids(&browser), before);
        assert_eq!(browser.archive.len(), 1);
        assert_eq!(browser.archive[0].tab.id, old);
        assert!(!browser.undo_sidebar(later()));
        assert!(browser.redo_sidebar(later()));
        assert_eq!(ids(&browser), vec![reference]);
    }

    #[test]
    fn explicit_restore_archive_deletion_and_session_reload_do_not_resurrect_or_duplicate_tabs() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.space_mut().active_tab = None;
        browser.clear_today(NOW);
        browser.delete_archived(a);
        assert!(!browser.undo_sidebar(later()));
        assert!(ids(&browser).is_empty());
        assert_eq!(browser.archive[0].tab.id, b);
        browser.restore_last_archived(later());
        browser.close_tab(b, later());
        browser.restore_last_archived(later());
        assert!(!browser.undo_sidebar(later()));
        assert_eq!(ids(&browser), vec![b]);
        browser.close_tab(b, later());
        let mut restored = session::from_json(&session::to_json(&browser)).unwrap();
        assert!(!restored.undo_sidebar(later()));
        assert_eq!(restored.archive.len(), 1);
    }

    #[test]
    fn regrouped_or_moved_pages_invalidate_redo_without_partially_archiving_a_batch() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.space_mut().active_tab = None;
        browser.clear_today(NOW);
        assert!(browser.undo_sidebar(later()));
        browser.add_split(a, b, Placement::Right, later()).unwrap();
        assert!(!browser.redo_sidebar(later()));
        assert_eq!(browser.all_tabs().count(), 2);
        assert!(browser.archive.is_empty());
        browser.close_tab(a, later());
        assert!(browser.undo_sidebar(later()));
        browser.add_space("Moved", 20.);
        browser.move_tab(a, browser.spaces[1].id, crate::Section::Today);
        assert!(!browser.redo_sidebar(later()));
        assert_eq!(browser.spaces[1].today.len(), 2);
        assert!(browser.archive.is_empty());
    }

    #[test]
    fn automatic_archiving_never_becomes_a_manual_undo_entry_even_with_split_views() {
        for grouped in [false, true] {
            let mut browser = Browser::new();
            let a = browser.new_tab("https://a", NOW);
            let b = browser.new_tab("https://b", NOW);
            if grouped { browser.add_split(a, b, Placement::Right, NOW).unwrap(); }
            browser.new_tab("https://active", later());
            browser.active_user_profile_mut().archive_after = Some(Duration::from_secs(100));
            assert!(browser.archive_idle(later()) > 0);
            assert!(!browser.undo_sidebar(later()));
            assert!(browser.archive.iter().all(|entry| entry.reason == ArchiveReason::Automatic));
        }
    }
}
