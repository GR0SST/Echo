//! Pane closure is asymmetric in Arc: Undo reconstructs the group while
//! retaining the surviving selection; Redo archives the entire restored item.
use super::{insertion_index, locate, pool_mut, PinState, Scope, SidebarEdit};
use crate::{ArchiveReason, Browser, SpaceId, Tab, TabId, folders::PinnedItem, splits::SplitView};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub(super) struct PaneEdit {
    closed: TabId,
    space: SpaceId,
    before: PinState,
    split: SplitView,
    whole_archived: bool,
}

pub(crate) struct PaneCapture(PaneEdit);
impl PaneCapture {
    pub(crate) fn new(browser: &Browser, closed: TabId) -> Option<Self> {
        let before = PinState::capture(browser, closed)?;
        let split = browser.split_for(closed)?.clone();
        let space = match before.tabs[0].scope { Scope::Favorites => browser.space().id, Scope::Space(id, _) => id };
        Some(Self(PaneEdit { closed, space, before, split, whole_archived: false }))
    }
    pub(crate) fn record(self, browser: &mut Browser) { browser.sidebar_history.record(SidebarEdit::Pane(self.0)); }
}

impl PaneEdit {
    pub(super) fn apply(&mut self, redo: bool, browser: &mut Browser, now: SystemTime) -> bool {
        let Some(space_index) = browser.spaces.iter().position(|space| space.id == self.space) else { return false; };
        let scope = self.before.tabs[0].scope;
        if redo {
            let Some(current) = browser.split_for(self.split.anchor) else { return false; };
            if current.id != self.split.id || current.panes.len() != self.split.panes.len()
                || !current.panes.iter().all(|id| self.split.panes.contains(id) && locate(browser, *id).is_some_and(|(place, _)| place == scope)) {
                return false;
            }
            let split = current.clone();
            let before = PinState::capture(browser, split.anchor).unwrap();
            // Record current state so repeated Undo preserves subsequent page,
            // group customization, ordering and geometry changes.
            browser.active_space = space_index;
            if !browser.archive_split_for_redo(split.anchor, now) { return false; }
            self.before = before;
            self.split = split;
            self.whole_archived = true;
            return true;
        }
        if self.whole_archived { return self.restore_group(browser, space_index, now); }
        let remaining: Vec<_> = self.split.panes.iter().copied().filter(|id| *id != self.closed).collect();
        if remaining.iter().any(|id| locate(browser, *id).is_none_or(|(place, _)| place != scope))
            || browser.all_tabs().any(|tab| tab.id == self.closed) { return false; }
        let current = browser.split_for(remaining[0]);
        if remaining.len() > 1 {
            if current.is_none_or(|split| split.id != self.split.id || split.panes != remaining) { return false; }
        } else if current.is_some() || all_splits(browser).any(|split| split.id == self.split.id) { return false; }
        let Some(at) = browser.archive.iter().position(|entry| entry.tab.id == self.closed && entry.space == self.space
            && entry.split.is_none() && entry.reason == ArchiveReason::Manual) else { return false; };
        let mut restored = current.cloned().unwrap_or_else(|| self.split.clone());
        let survivor = current.map_or(remaining[0], |split| split.active);
        let active = browser.spaces[space_index].active_tab.filter(|id| remaining.contains(id)).unwrap_or(survivor);
        let removed_at = self.split.panes.iter().position(|id| *id == self.closed).unwrap();
        let closed_weight = self.split.weights[removed_at];
        let weights = current.map_or_else(|| vec![1.0], |split| split.weights.clone());
        restored.weights = weights.into_iter().map(|weight| weight * (1.0 - closed_weight)).collect();
        restored.weights.insert(removed_at, closed_weight);
        restored.panes = self.split.panes.clone();
        restored.anchor = self.split.anchor;
        restored.active = active;
        let mut page = browser.archive.remove(at).tab;
        let position = self.before.tabs.iter().find(|position| position.id == self.closed).unwrap();
        page.pinned_url = position.pinned_url.clone();
        page.pinned_title = position.pinned_title.clone();
        insert_page(browser, &self.before, page, now);
        remove_split(browser, self.split.id);
        put_split(browser, scope, restored);
        if remaining.len() == 1 {
            browser.replace_pinned_item(PinnedItem::Tab(remaining[0]), &[self.before.item]);
        }
        browser.active_space = space_index;
        browser.repair_folders();
        true
    }

    fn restore_group(&self, browser: &mut Browser, space_index: usize, now: SystemTime) -> bool {
        if self.before.tabs[0].scope == Scope::Favorites && browser.favorite_count() >= crate::favorites::MAX_FAVORITES { return false; }
        if self.split.panes.iter().any(|id| browser.all_tabs().any(|tab| tab.id == *id))
            || all_splits(browser).any(|split| split.id == self.split.id) { return false; }
        let Some(at) = browser.archive.iter().position(|entry| entry.tab.id == self.split.anchor && entry.space == self.space
            && entry.reason == ArchiveReason::Manual && entry.split.as_ref().is_some_and(|saved|
                saved.view.id == self.split.id && saved.view.panes == self.split.panes)) else { return false; };
        let entry = browser.archive.remove(at);
        let saved = entry.split.unwrap();
        let mut pages = vec![entry.tab];
        pages.extend(saved.panes);
        for page in pages { insert_page(browser, &self.before, page, now); }
        put_split(browser, self.before.tabs[0].scope, saved.view);
        self.before.restore_folder(browser);
        for &(id, active) in &self.before.selections {
            if active.is_some_and(|tab| self.split.panes.contains(&tab)) {
                if let Some(space) = browser.spaces.iter_mut().find(|space| space.id == id) { space.active_tab = active; }
            }
        }
        browser.active_space = space_index;
        browser.repair_folders();
        true
    }
}

fn insert_page(browser: &mut Browser, state: &PinState, mut page: Tab, now: SystemTime) {
    let position = state.tabs.iter().find(|position| position.id == page.id).unwrap();
    page.last_active = now;
    let pool = pool_mut(browser, position.scope);
    let index = insertion_index(&pool.iter().map(|tab| tab.id).collect::<Vec<_>>(), &position.order, page.id);
    pool.insert(index, page);
}

fn all_splits(browser: &Browser) -> impl Iterator<Item = &SplitView> {
    browser.favorite_splits.iter().chain(browser.spaces.iter().flat_map(|space| &space.splits))
}
fn remove_split(browser: &mut Browser, id: crate::splits::SplitId) {
    browser.favorite_splits.retain(|split| split.id != id);
    for space in &mut browser.spaces { space.splits.retain(|split| split.id != id); }
}
fn put_split(browser: &mut Browser, scope: Scope, split: SplitView) {
    match scope {
        Scope::Favorites => browser.favorite_splits.push(split),
        Scope::Space(id, _) => browser.spaces.iter_mut().find(|space| space.id == id).unwrap().splits.push(split),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Section, splits::{Axis, Placement}};
    use std::time::Duration;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;
    fn later() -> SystemTime { NOW + Duration::from_secs(600) }
    fn pair() -> (Browser, TabId, TabId) {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.add_split(a, b, Placement::Right, NOW).unwrap();
        (browser, a, b)
    }

    #[test]
    fn undo_restores_collapsed_pair_without_rewinding_or_selecting_the_surviving_page() {
        let (mut browser, a, b) = pair();
        let split = browser.active_split().unwrap().id;
        browser.rename_tab(a, "Named group");
        browser.set_split_axis(a, Axis::Vertical);
        browser.spaces[0].splits[0].weights = vec![0.3, 0.7];
        browser.clear_sidebar_history();
        assert!(browser.close_split_pane(b, NOW));
        browser.navigate(a, "https://a/later");
        browser.set_title(a, "Loaded later");
        let newer = browser.new_tab("https://newer", later());
        browser.activate_tab(a, later());
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.active_tab().unwrap().id, a);
        let view = browser.active_split().unwrap();
        assert_eq!(view.id, split);
        assert_eq!(view.panes, vec![a, b]);
        assert_eq!(view.active, a);
        assert_eq!(view.axis, Axis::Vertical);
        assert_eq!(view.title.as_deref(), Some("Named group"));
        for (actual, expected) in view.weights.iter().zip([0.3, 0.7]) { assert!((actual - expected).abs() < 0.00001); }
        assert_eq!(browser.active_tab().unwrap().url, "https://a/later");
        assert_eq!(browser.active_tab().unwrap().title, "Loaded later");
        assert!(browser.all_tabs().any(|tab| tab.id == newer));
        assert!(browser.archive.is_empty());
        assert!(!browser.undo_sidebar(later()));
    }

    #[test]
    fn redo_archives_the_whole_reconstructed_item_and_undo_restores_the_complete_group() {
        let (mut browser, a, b) = pair();
        browser.close_split_pane(b, NOW);
        assert!(browser.undo_sidebar(later()));
        browser.navigate(b, "https://b/later");
        browser.rename_tab(a, "Renamed after Undo");
        assert!(browser.redo_sidebar(later()));
        assert_eq!(browser.all_tabs().count(), 0);
        assert_eq!(browser.archive.len(), 1);
        assert_eq!(browser.archive[0].split.as_ref().unwrap().view.panes, vec![a, b]);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.active_tab().unwrap().id, a);
        assert_eq!(browser.active_split().unwrap().title.as_deref(), Some("Renamed after Undo"));
        assert_eq!(browser.all_tabs().find(|tab| tab.id == b).unwrap().url, "https://b/later");
        assert!(browser.redo_sidebar(later()));
        assert_eq!(browser.archive.len(), 1);
    }

    #[test]
    fn undo_of_anchor_closure_restores_the_anchor_and_later_group_metadata_and_survivor_ratios() {
        let (mut browser, a, b) = pair();
        let c = browser.new_tab("https://c", NOW);
        browser.add_split(a, c, Placement::Right, NOW).unwrap();
        let order = browser.split_for(a).unwrap().panes.clone();
        browser.spaces[0].splits[0].weights = vec![0.2, 0.3, 0.5];
        browser.clear_sidebar_history();
        browser.close_split_pane(a, NOW);
        browser.rename_tab(b, "Changed while closed");
        browser.set_split_axis(b, Axis::Vertical);
        browser.spaces[0].splits[0].weights = vec![0.25, 0.75];
        let survivor = browser.active_tab().unwrap().id;
        assert!(browser.undo_sidebar(later()));
        let group = browser.active_split().unwrap();
        assert_eq!(group.anchor, a);
        assert_eq!(group.panes, order);
        assert_eq!(group.active, survivor);
        assert_eq!(group.title.as_deref(), Some("Changed while closed"));
        assert_eq!(group.axis, Axis::Vertical);
        for (actual, expected) in group.weights.iter().zip([0.2, 0.2, 0.6]) { assert!((actual - expected).abs() < 0.00001); }
    }

    #[test]
    fn pinned_folder_pair_restores_pin_urls_folder_position_and_whole_pinned_redo() {
        let (mut browser, a, b) = pair();
        browser.toggle_pin(a);
        let folder = browser.create_folder(browser.space().id, None, "Folder").unwrap();
        browser.move_item_to_folder(a, folder);
        browser.navigate(b, "https://b/deeper");
        let item = browser.pinned_item(a);
        browser.clear_sidebar_history();
        browser.close_split_pane(b, NOW);
        assert_eq!(browser.folder(folder).unwrap().items, vec![PinnedItem::Tab(a)]);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.folder(folder).unwrap().items, vec![item]);
        let page = browser.all_tabs().find(|tab| tab.id == b).unwrap();
        assert_eq!(page.url, "https://b/deeper");
        assert_eq!(page.pinned_url.as_deref(), Some("https://b"));
        assert!(browser.redo_sidebar(later()));
        assert!(browser.space().pinned.is_empty());
        assert!(browser.folder(folder).unwrap().items.is_empty());
        assert_eq!(browser.archive.len(), 1);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.space().pinned.len(), 2);
        assert_eq!(browser.folder(folder).unwrap().items, vec![item]);
        assert_eq!(browser.active_tab().unwrap().id, a);
    }

    #[test]
    fn favorite_group_and_shared_selections_survive_pane_and_whole_group_undo() {
        let (mut browser, a, b) = pair();
        browser.favorite_tab(a);
        browser.add_space("Other", 10.);
        browser.switch_space(1);
        browser.activate_tab(b, NOW);
        browser.clear_sidebar_history();
        browser.close_split_pane(b, NOW);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.favorite_count(), 1);
        assert_eq!(browser.favorites.len(), 2);
        assert_eq!(browser.spaces[0].active_tab, Some(a));
        assert_eq!(browser.spaces[1].active_tab, Some(a));
        assert!(browser.redo_sidebar(later()));
        assert!(browser.favorites.is_empty());
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.favorite_count(), 1);
        assert_eq!(browser.spaces[0].active_tab, Some(a));
        assert_eq!(browser.spaces[1].active_tab, Some(a));
    }

    #[test]
    fn deleted_or_independently_restored_archives_and_regrouped_survivors_invalidate_undo() {
        for scenario in 0..3 {
            let (mut browser, a, b) = pair();
            browser.close_split_pane(b, NOW);
            match scenario {
                0 => { browser.delete_archived(b); }
                1 => { browser.restore_last_archived(later()); }
                _ => {
                    let c = browser.new_tab("https://c", NOW);
                    browser.add_split(a, c, Placement::Right, NOW).unwrap();
                }
            }
            let count = browser.all_tabs().count();
            assert!(!browser.undo_sidebar(later()));
            assert_eq!(browser.all_tabs().count(), count);
        }
    }

    #[test]
    fn moving_the_reconstructed_group_invalidates_redo_without_archiving_any_pages() {
        let (mut browser, a, b) = pair();
        browser.close_split_pane(b, NOW);
        browser.undo_sidebar(later());
        browser.add_space("Moved", 20.);
        browser.move_tab(a, browser.spaces[1].id, Section::Today);
        assert!(!browser.redo_sidebar(later()));
        assert_eq!(browser.spaces[1].today.len(), 2);
        assert!(browser.archive.is_empty());
    }

    #[test]
    fn consecutive_pane_closures_undo_in_order_before_redo_archives_the_complete_group() {
        let (mut browser, a, b) = pair();
        let c = browser.new_tab("https://c", NOW);
        browser.add_split(a, c, Placement::Right, NOW).unwrap();
        let original = browser.active_split().unwrap().panes.clone();
        browser.close_split_pane(b, NOW);
        browser.close_split_pane(c, NOW);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.active_split().unwrap().panes.len(), 2);
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.active_split().unwrap().panes, original);
        assert_eq!(browser.active_tab().unwrap().id, a);
        assert!(browser.redo_sidebar(later()));
        assert_eq!(browser.archive.len(), 1);
        assert_eq!(browser.archive[0].split.as_ref().unwrap().view.panes, original);
        assert!(!browser.redo_sidebar(later()));
        assert!(browser.undo_sidebar(later()));
        assert_eq!(browser.active_split().unwrap().panes, original);
    }

    #[test]
    fn restoring_an_archived_favorite_group_does_not_overfill_a_dock_that_changed_later() {
        let (mut browser, a, b) = pair();
        browser.favorite_tab(a);
        browser.close_split_pane(b, NOW);
        browser.undo_sidebar(later());
        browser.redo_sidebar(later());
        for index in 0..crate::favorites::MAX_FAVORITES {
            let tab = browser.new_tab(format!("https://favorite-{index}"), NOW);
            assert!(browser.favorite_tab(tab));
        }
        assert!(!browser.undo_sidebar(later()));
        assert_eq!(browser.favorite_count(), crate::favorites::MAX_FAVORITES);
        assert_eq!(browser.archive.len(), 1);
    }
}
