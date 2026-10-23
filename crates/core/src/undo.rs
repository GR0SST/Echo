//! Transient sidebar history. Entries retain placement and pin metadata, never
//! snapshots of the browser: background navigation, titles and newly opened
//! pages must survive an Undo. Text editors own independent histories.
use crate::{Browser, Section, SpaceId, Tab, TabId, folders::{FolderId, PinnedItem}};

const HISTORY_LIMIT: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope { Favorites, Space(SpaceId, Section) }

#[derive(Clone, Debug, PartialEq, Eq)]
struct Position {
    id: TabId,
    scope: Scope,
    order: Vec<TabId>,
    pinned_url: Option<String>,
    pinned_title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FolderSlot {
    space: SpaceId,
    folder: Option<FolderId>,
    order: Vec<PinnedItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PinState {
    item: PinnedItem,
    tabs: Vec<Position>,
    folder: Option<FolderSlot>,
    selections: Vec<(SpaceId, Option<TabId>)>,
}

impl PinState {
    pub(crate) fn capture(browser: &Browser, tab: TabId) -> Option<Self> {
        let item = browser.pinned_item(tab);
        let mut tabs = Vec::new();
        for id in browser.related_tabs(tab) {
            let (scope, pool) = locate(browser, id)?;
            let tab = pool.iter().find(|tab| tab.id == id)?;
            tabs.push(Position {
                id, scope, order: pool.iter().map(|tab| tab.id).collect(),
                pinned_url: tab.pinned_url.clone(), pinned_title: tab.pinned_title.clone(),
            });
        }
        let folder = browser.spaces.iter().find_map(|space| {
            if space.pinned_items.contains(&item) {
                return Some(FolderSlot { space: space.id, folder: None, order: space.pinned_items.clone() });
            }
            space.folders.iter().find(|folder| folder.items.contains(&item)).map(|folder|
                FolderSlot { space: space.id, folder: Some(folder.id), order: folder.items.clone() })
        });
        Some(Self { item, tabs, folder, selections: browser.spaces.iter().map(|space| (space.id, space.active_tab)).collect() })
    }

    fn apply(&self, source: &Self, browser: &mut Browser) -> bool {
        // An unrecorded structural operation can invalidate an entry. Do not
        // resurrect closed pages or steal a tab back from another Space.
        if source.tabs.iter().any(|position| locate(browser, position.id).is_none_or(|(scope, pool)| {
                let tab = pool.iter().find(|tab| tab.id == position.id).unwrap();
                scope != position.scope || tab.pinned_url != position.pinned_url
            }))
            || self.tabs.iter().any(|position| match position.scope {
                Scope::Favorites => false,
                Scope::Space(id, _) => !browser.spaces.iter().any(|space| space.id == id),
            })
            || self.tabs.iter().any(|position| browser.pinned_item(position.id) != self.item)
        { return false; }
        let ids: Vec<_> = self.tabs.iter().map(|position| position.id).collect();
        if browser.related_tabs(ids[0]) != ids { return false; }
        let current_folder = Self::capture(browser, ids[0]).and_then(|state| state.folder);
        if current_folder.as_ref().map(|slot| (slot.space, slot.folder))
            != source.folder.as_ref().map(|slot| (slot.space, slot.folder)) { return false; }
        if self.tabs[0].scope == Scope::Favorites && source.tabs[0].scope != Scope::Favorites
            && browser.favorite_count() >= crate::favorites::MAX_FAVORITES { return false; }
        // Retain the current split geometry, focus and customizations.
        let split = browser.split_for(ids[0]).cloned();
        browser.favorite_splits.retain(|split| !ids.contains(&split.anchor));
        for space in &mut browser.spaces { space.splits.retain(|split| !ids.contains(&split.anchor)); }
        for position in &self.tabs {
            let (scope, _) = locate(browser, position.id).unwrap();
            let pool = pool_mut(browser, scope);
            let index = pool.iter().position(|tab| tab.id == position.id).unwrap();
            let mut tab = pool.remove(index);
            tab.pinned_url = position.pinned_url.clone();
            tab.pinned_title = if position.pinned_url.as_ref() == Some(&tab.url) {
                tab.page_title.clone().or_else(|| position.pinned_title.clone())
            } else { position.pinned_title.clone() };
            let pool = pool_mut(browser, position.scope);
            let index = insertion_index(&pool.iter().map(|tab| tab.id).collect::<Vec<_>>(), &position.order, position.id);
            pool.insert(index, tab);
        }
        if let Some(split) = split {
            match self.tabs[0].scope {
                Scope::Favorites => browser.favorite_splits.push(split),
                Scope::Space(id, _) => browser.spaces.iter_mut().find(|space| space.id == id).unwrap().splits.push(split),
            }
        }
        browser.forget_pinned_item(ids[0]);
        if let Some(slot) = &self.folder {
            if let Some(space) = browser.spaces.iter_mut().find(|space| space.id == slot.space) {
                let pool = match slot.folder {
                    Some(id) => space.folders.iter_mut().find(|folder| folder.id == id).map(|folder| &mut folder.items),
                    None => Some(&mut space.pinned_items),
                };
                if let Some(pool) = pool {
                    let index = insertion_index(pool, &slot.order, self.item);
                    pool.insert(index, self.item);
                }
            }
        }
        for &(id, desired) in &self.selections {
            let previous = source.selections.iter().find(|(space, _)| *space == id).map(|(_, active)| *active);
            if previous == Some(desired) { continue; }
            if let Some(space) = browser.spaces.iter_mut().find(|space| space.id == id) {
                // Keep a selection the user changed after the operation.
                if Some(space.active_tab) == previous { space.active_tab = desired; }
            }
        }
        browser.repair_folders();
        true
    }
}

fn locate(browser: &Browser, id: TabId) -> Option<(Scope, &[Tab])> {
    if browser.is_favorite(id) { return Some((Scope::Favorites, &browser.favorites)); }
    browser.spaces.iter().find_map(|space| space.find(id).map(|(section, _)|
        (Scope::Space(space.id, section), match section { Section::Pinned => &space.pinned[..], Section::Today => &space.today[..] })))
}

fn pool_mut(browser: &mut Browser, scope: Scope) -> &mut Vec<Tab> {
    match scope {
        Scope::Favorites => &mut browser.favorites,
        Scope::Space(id, section) => browser.spaces.iter_mut().find(|space| space.id == id).unwrap().section_mut(section),
    }
}

fn insertion_index<T: Copy + Eq>(current: &[T], desired: &[T], item: T) -> usize {
    let index = desired.iter().position(|entry| *entry == item).unwrap();
    // Neighbor identities preserve order when other pages were opened later.
    if let Some(next) = desired[index + 1..].iter().find_map(|id| current.iter().position(|entry| entry == id)) { return next; }
    if let Some(previous) = desired[..index].iter().rev().find_map(|id| current.iter().position(|entry| entry == id)) { return previous + 1; }
    index.min(current.len())
}

#[derive(Clone, Debug)]
struct PinEdit { before: PinState, after: PinState }

#[derive(Clone, Debug, Default)]
pub(crate) struct SidebarHistory { undo: Vec<PinEdit>, redo: Vec<PinEdit> }

impl SidebarHistory {
    pub(crate) fn record_pin(&mut self, before: PinState, after: PinState) {
        if before == after { return; }
        self.redo.clear();
        if self.undo.len() == HISTORY_LIMIT { self.undo.remove(0); }
        self.undo.push(PinEdit { before, after });
    }
}

impl Browser {
    /// Initial profile seeding is not a user action.
    pub fn clear_sidebar_history(&mut self) { self.sidebar_history = SidebarHistory::default(); }

    /// Reverses the last supported sidebar action, independently of text edits.
    /// Invalidated entries are discarded without recreating removed pages.
    pub fn undo_sidebar(&mut self) -> bool {
        while let Some(edit) = self.sidebar_history.undo.pop() {
            if edit.before.apply(&edit.after, self) {
                self.sidebar_history.redo.push(edit);
                return true;
            }
        }
        false
    }

    pub fn redo_sidebar(&mut self) -> bool {
        while let Some(edit) = self.sidebar_history.redo.pop() {
            if edit.after.apply(&edit.before, self) {
                self.sidebar_history.undo.push(edit);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session, splits::Placement};
    use std::time::SystemTime;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;

    fn ids(tabs: &[Tab]) -> Vec<TabId> { tabs.iter().map(|tab| tab.id).collect() }

    #[test]
    fn pin_undo_preserves_navigation_titles_new_pages_and_original_order() {
        let mut browser = Browser::new();
        let bottom = browser.new_tab("https://bottom", NOW);
        let target = browser.new_tab("https://target", NOW);
        let top = browser.new_tab("https://top", NOW);
        browser.toggle_pin(target);
        browser.navigate(target, "https://target/next");
        browser.set_title(target, "Loaded later");
        let newer = browser.new_tab("https://newer", NOW);
        assert!(browser.undo_sidebar());
        assert_eq!(ids(&browser.space().today), vec![newer, top, target, bottom]);
        let tab = browser.all_tabs().find(|tab| tab.id == target).unwrap();
        assert_eq!(tab.url, "https://target/next");
        assert_eq!(tab.title, "Loaded later");
        assert!(tab.pinned_url.is_none());
        assert_eq!(browser.active_tab().unwrap().id, newer);
        assert!(browser.redo_sidebar());
        assert_eq!(browser.space().pinned[0].pinned_url.as_deref(), Some("https://target"));
        assert_eq!(browser.space().pinned[0].url, "https://target/next");
    }

    #[test]
    fn unpin_undo_restores_nested_folder_slot_without_rewinding_folder_edits() {
        let mut browser = Browser::new();
        let target = browser.new_tab("https://target", NOW);
        browser.toggle_pin(target);
        let folder = browser.create_folder(browser.space().id, None, "Original").unwrap();
        browser.move_item_to_folder(target, folder);
        let other = browser.new_tab("https://other", NOW);
        browser.toggle_pin(other);
        browser.move_item_to_folder(other, folder);
        browser.toggle_pin(target);
        browser.rename_folder(folder, "Renamed later");
        assert!(browser.undo_sidebar());
        assert_eq!(browser.folder(folder).unwrap().name, "Renamed later");
        assert_eq!(browser.folder(folder).unwrap().items, vec![PinnedItem::Tab(target), PinnedItem::Tab(other)]);
        assert!(browser.redo_sidebar());
        assert_eq!(browser.folder(folder).unwrap().items, vec![PinnedItem::Tab(other)]);
        assert_eq!(browser.space().today[0].id, target);
    }

    #[test]
    fn split_pin_undo_keeps_group_identity_focus_and_later_geometry() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.add_split(a, b, Placement::Right, NOW).unwrap();
        let split = browser.split_for(a).unwrap().id;
        browser.toggle_pin(b);
        browser.spaces[0].splits[0].weights = vec![0.3, 0.7];
        browser.activate_tab(b, NOW);
        assert!(browser.undo_sidebar());
        assert!(browser.space().pinned.is_empty());
        assert_eq!(browser.split_for(b).unwrap().id, split);
        assert_eq!(browser.split_for(b).unwrap().weights, vec![0.3, 0.7]);
        assert_eq!(browser.active_tab().unwrap().id, b);
        assert!(browser.redo_sidebar());
        assert_eq!(browser.space().pinned.len(), 2);
        assert!(browser.space().today.is_empty());
        assert_eq!(browser.split_for(b).unwrap().panes, vec![a, b]);
    }

    #[test]
    fn favorite_unpin_undo_restores_shared_selections_but_keeps_later_focus() {
        let mut browser = Browser::new();
        let favorite = browser.new_tab("https://favorite", NOW);
        browser.favorite_tab(favorite);
        browser.add_space("Second", 12.);
        browser.switch_space(1);
        let local = browser.new_tab("https://local", NOW);
        browser.activate_tab(favorite, NOW);
        browser.toggle_pin(favorite);
        browser.activate_tab(local, NOW);
        assert!(browser.undo_sidebar());
        assert!(browser.is_favorite(favorite));
        assert_eq!(browser.spaces[0].active_tab, Some(favorite));
        assert_eq!(browser.spaces[1].active_tab, Some(local));
        assert!(browser.redo_sidebar());
        assert!(!browser.is_favorite(favorite));
        assert_eq!(browser.spaces[1].active_tab, Some(local));
    }

    #[test]
    fn new_edits_branch_history_and_sessions_never_store_undo_payloads() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.toggle_pin(a);
        assert!(browser.undo_sidebar());
        browser.toggle_pin(b);
        assert!(!browser.redo_sidebar());
        let json = session::to_json(&browser);
        assert!(!json.contains("sidebar_history"));
        let mut restored = session::from_json(&json).unwrap();
        assert!(!restored.undo_sidebar());
        assert!(!restored.redo_sidebar());
        assert!(browser.undo_sidebar());
        assert!(!browser.undo_sidebar());
    }

    #[test]
    fn a_delayed_title_at_the_pinned_url_does_not_invalidate_undo_or_restore_a_stale_title() {
        let mut browser = Browser::new();
        let tab = browser.new_tab("https://loading", NOW);
        browser.toggle_pin(tab);
        browser.set_title(tab, "Finished loading");
        assert!(browser.undo_sidebar());
        assert!(browser.redo_sidebar());
        assert_eq!(browser.space().pinned[0].pinned_title.as_deref(), Some("Finished loading"));
    }

    #[test]
    fn invalidated_history_does_not_resurrect_pages_or_break_changed_splits() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        browser.toggle_pin(a);
        browser.toggle_pin(a);
        browser.close_tab(a, NOW);
        assert!(!browser.undo_sidebar());
        assert_eq!(browser.archive.len(), 1);
        assert_eq!(browser.all_tabs().count(), 0);
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.toggle_pin(a);
        browser.toggle_pin(b);
        browser.add_split(a, b, Placement::Right, NOW).unwrap();
        assert!(!browser.undo_sidebar());
        assert_eq!(browser.space().pinned.len(), 2);
        assert_eq!(browser.split_for(a).unwrap().panes.len(), 2);
    }

    #[test]
    fn unrecorded_folder_moves_and_a_full_favorite_dock_invalidate_old_entries() {
        let mut browser = Browser::new();
        let target = browser.new_tab("https://target", NOW);
        browser.toggle_pin(target);
        let folder = browser.create_folder(browser.space().id, None, "Moved later").unwrap();
        browser.move_item_to_folder(target, folder);
        assert!(!browser.undo_sidebar());
        assert_eq!(browser.folder_tabs(folder), vec![target]);
        browser.favorite_tab(target);
        browser.toggle_pin(target);
        for index in 0..crate::favorites::MAX_FAVORITES {
            let tab = browser.new_tab(format!("https://favorite-{index}"), NOW);
            assert!(browser.favorite_tab(tab));
        }
        assert!(!browser.undo_sidebar());
        assert_eq!(browser.favorite_count(), crate::favorites::MAX_FAVORITES);
        assert!(browser.space().today.iter().any(|tab| tab.id == target));
    }
}
