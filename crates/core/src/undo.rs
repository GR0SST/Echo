//! Transient sidebar history. Entries retain placement and pin metadata, never
//! snapshots of the browser: background navigation, titles and newly opened
//! pages must survive an Undo. Text editors own independent histories.
use crate::{Browser, Section, SpaceId, Tab, TabId, folders::{FolderId, PinnedItem}};

const HISTORY_LIMIT: usize = 100;
mod archive;
pub(crate) use archive::ArchiveCapture;
mod pane;
pub(crate) use pane::PaneCapture;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope { Favorites(crate::UserProfileId), Space(SpaceId, Section) }

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
    user_profile_id: crate::UserProfileId,
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
        Some(Self { item, user_profile_id: browser.user_profile_id_for_tab(tab)?, tabs, folder, selections: browser.spaces.iter().map(|space| (space.id, space.active_tab)).collect() })
    }

    fn apply(&self, source: &Self, browser: &mut Browser) -> bool {
        if source.tabs.iter().any(|position| browser.user_profile_id_for_tab(position.id) != Some(source.user_profile_id)) { return false; }
        // An unrecorded structural operation can invalidate an entry. Do not
        // resurrect closed pages or steal a tab back from another Space.
        if source.tabs.iter().any(|position| locate(browser, position.id).is_none_or(|(scope, pool)| {
                let tab = pool.iter().find(|tab| tab.id == position.id).unwrap();
                scope != position.scope || tab.pinned_url != position.pinned_url
            }))
            || self.tabs.iter().any(|position| match position.scope {
                Scope::Favorites(id) => browser.user_profile(id).is_none(),
                Scope::Space(id, _) => !browser.spaces.iter().any(|space| space.id == id && space.user_profile_id == self.user_profile_id),
            })
            || self.tabs.iter().any(|position| browser.pinned_item(position.id) != self.item)
        { return false; }
        let ids: Vec<_> = self.tabs.iter().map(|position| position.id).collect();
        if browser.related_tabs(ids[0]) != ids { return false; }
        let current_folder = Self::capture(browser, ids[0]).and_then(|state| state.folder);
        if current_folder.as_ref().map(|slot| (slot.space, slot.folder))
            != source.folder.as_ref().map(|slot| (slot.space, slot.folder)) { return false; }
        if matches!(self.tabs[0].scope, Scope::Favorites(_)) && !matches!(source.tabs[0].scope, Scope::Favorites(_))
            && favorite_count(browser, self.user_profile_id) >= crate::favorites::MAX_FAVORITES { return false; }
        // Retain the current split geometry, focus and customizations.
        let split = browser.split_for(ids[0]).cloned();
        for profile in browser.profiles.values_mut() { profile.favorite_splits.retain(|split| !ids.contains(&split.anchor)); }
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
                Scope::Favorites(id) => browser.profiles.get_mut(&id).unwrap().favorite_splits.push(split),
                Scope::Space(id, _) => browser.spaces.iter_mut().find(|space| space.id == id).unwrap().splits.push(split),
            }
        }
        self.restore_folder(browser);
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

    fn restore_folder(&self, browser: &mut Browser) {
        browser.forget_pinned_item(self.tabs[0].id);
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
    }
}

fn locate(browser: &Browser, id: TabId) -> Option<(Scope, &[Tab])> {
    if let Some(profile) = browser.profiles.values().find(|profile| profile.favorites.iter().any(|tab| tab.id == id)) { return Some((Scope::Favorites(profile.id), &profile.favorites)); }
    browser.spaces.iter().find_map(|space| space.find(id).map(|(section, _)|
        (Scope::Space(space.id, section), match section { Section::Pinned => &space.pinned[..], Section::Today => &space.today[..] })))
}

fn pool_mut(browser: &mut Browser, scope: Scope) -> &mut Vec<Tab> {
    match scope {
        Scope::Favorites(id) => &mut browser.profiles.get_mut(&id).unwrap().favorites,
        Scope::Space(id, section) => browser.spaces.iter_mut().find(|space| space.id == id).unwrap().section_mut(section),
    }
}

fn favorite_count(browser: &Browser, id: crate::UserProfileId) -> usize {
    browser.user_profile(id).map_or(0, |profile| profile.favorites.iter().filter(|tab| !profile.favorite_splits.iter().any(|split| split.anchor != tab.id && split.panes.contains(&tab.id))).count())
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

#[derive(Clone, Debug)]
enum SidebarEdit { Batch(Vec<SidebarEdit>), Tidy(crate::max::TidyEdit), PeekClose(u64), Pin(PinEdit), Archive(archive::ArchiveEdit), Pane(pane::PaneEdit), RemovePin { pin: PinEdit, archive: archive::ArchiveEdit }, Theme { space: SpaceId, before: Option<crate::themes::SpaceTheme>, after: Option<crate::themes::SpaceTheme> } }

impl SidebarEdit {
    fn apply(&mut self, redo: bool, browser: &mut Browser, now: std::time::SystemTime) -> bool {
        match self {
            Self::Batch(edits) => {
                let apply = |edits: &mut Vec<SidebarEdit>, browser: &mut Browser| {
                    if redo { edits.iter_mut().all(|edit| edit.apply(true, browser, now)) }
                    else { edits.iter_mut().rev().all(|edit| edit.apply(false, browser, now)) }
                };
                // Validate every identity before changing the live model. Never
                // restore a snapshot over navigation or unrelated later tabs.
                if !apply(&mut edits.clone(), &mut browser.clone()) { return false; }
                apply(edits, browser)
            }
            Self::Tidy(edit) => edit.apply(redo, browser),
            Self::PeekClose(_) => false,
            Self::Theme { space, before, after } => {
                let (source, target) = if redo { (&*before, &*after) } else { (&*after, &*before) };
                let Some(space) = browser.spaces.iter_mut().find(|entry| entry.id == *space) else { return false; };
                if &space.theme != source { return false; }
                space.theme = target.clone();
                true
            }
            Self::Pin(edit) => if redo { edit.after.apply(&edit.before, browser) } else { edit.before.apply(&edit.after, browser) },
            Self::Archive(edit) => edit.apply(redo, browser, now),
            Self::Pane(edit) => edit.apply(redo, browser, now),
            Self::RemovePin { pin, archive } => {
                if redo {
                    if !pin.after.apply(&pin.before, browser) { return false; }
                    if archive.apply(true, browser, now) { return true; }
                    pin.before.apply(&pin.after, browser);
                } else {
                    if !archive.apply(false, browser, now) { return false; }
                    if pin.before.apply(&pin.after, browser) { return true; }
                    archive.apply(true, browser, now);
                }
                false
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SidebarHistory { undo: Vec<SidebarEdit>, redo: Vec<SidebarEdit> }

impl SidebarHistory {
    pub(crate) fn record_tidy(&mut self, space: SpaceId, profile: crate::UserProfileId, before:Vec<crate::max::TodayGroup>, after:Vec<crate::max::TodayGroup>, before_order:Vec<TabId>, after_order:Vec<TabId>) {
        self.record(SidebarEdit::Tidy(crate::max::TidyEdit {space,profile,before,after,before_order,after_order}));
    }

    pub(crate) fn record_theme(&mut self, space: SpaceId, before: Option<crate::themes::SpaceTheme>, after: Option<crate::themes::SpaceTheme>) {
        if before != after { self.record(SidebarEdit::Theme { space, before, after }); }
    }
    pub(crate) fn record_pin(&mut self, before: PinState, after: PinState) {
        if before == after { return; }
        self.record(SidebarEdit::Pin(PinEdit { before, after }));
    }

    fn record(&mut self, edit: SidebarEdit) {
        self.redo.clear();
        if self.undo.len() == HISTORY_LIMIT { self.undo.remove(0); }
        self.undo.push(edit);
    }
}

impl Browser {
    /// Archive a sidebar selection as one Undo transaction. Split groups are
    /// atomic; stale, foreign-Space and duplicate identities are ignored.
    pub fn archive_sidebar_items(&mut self, items: &[TabId], now: std::time::SystemTime) -> usize {
        let mut ids = Vec::new();
        for id in items.iter().map(|id| self.item_anchor(*id)) {
            if !ids.contains(&id) && self.space().pinned.iter().chain(&self.space().today).any(|tab| tab.id == id) { ids.push(id); }
        }
        let history = std::mem::take(&mut self.sidebar_history);
        let mut count = 0;
        for id in ids {
            let pinned = self.space().pinned.iter().any(|tab| tab.id == id);
            let closed = if pinned { self.remove_pin(id, now) } else { self.close_tab(id, now) };
            if closed == Some(crate::Closed::Archived) { count += 1; }
        }
        let edits = std::mem::take(&mut self.sidebar_history.undo);
        self.sidebar_history = history;
        if !edits.is_empty() { self.sidebar_history.record(SidebarEdit::Batch(edits)); }
        count
    }

    /// Opaque identities for an archive accessory; other edits must never be
    /// undone accidentally by a still-visible archive notification.
    pub fn archive_undo_items(&self) -> Option<Vec<TabId>> {
        fn anchors(edit: &SidebarEdit) -> Option<Vec<TabId>> {
            match edit {
                SidebarEdit::Archive(edit) | SidebarEdit::RemovePin { archive: edit, .. } => Some(edit.anchors()),
                SidebarEdit::Batch(edits) => Some(edits.iter().map(anchors).collect::<Option<Vec<_>>>()?.into_iter().flatten().collect()),
                _ => None,
            }
        }
        anchors(self.sidebar_history.undo.last()?)
    }
    pub fn undo_archive_items(&mut self, expected: &[TabId], now: std::time::SystemTime) -> bool {
        if self.archive_undo_items().as_deref() != Some(expected) { return false; }
        let mut edit = self.sidebar_history.undo.pop().unwrap();
        if edit.apply(false, self, now) { self.sidebar_history.redo.push(edit); true }
        else { self.sidebar_history.undo.push(edit); false }
    }
    pub fn sidebar_undo_action(&self) -> Option<&'static str> {
        match self.sidebar_history.undo.last() { Some(SidebarEdit::Tidy(_)) => Some("Tidy Tabs"), Some(SidebarEdit::Theme { .. }) => Some("Update Theme"), Some(SidebarEdit::PeekClose(_)) => Some("Close Peek"), _ => None }
    }
    pub fn sidebar_redo_action(&self) -> Option<&'static str> {
        match self.sidebar_history.redo.last() { Some(SidebarEdit::Tidy(_)) => Some("Tidy Tabs"), Some(SidebarEdit::Theme { .. }) => Some("Update Theme"), Some(SidebarEdit::PeekClose(_)) => Some("Close Peek"), _ => None }
    }

    /// The UI owns the retained live renderer; the core only orders its opaque
    /// token alongside ordinary sidebar edits. Tokens are never serialized.
    pub fn record_peek_close(&mut self, token: u64) { self.sidebar_history.record(SidebarEdit::PeekClose(token)); }
    pub fn peek_history_token(&self, redo: bool) -> Option<u64> {
        let stack = if redo { &self.sidebar_history.redo } else { &self.sidebar_history.undo };
        match stack.last() { Some(SidebarEdit::PeekClose(token)) => Some(*token), _ => None }
    }
    pub fn take_peek_history(&mut self, redo: bool) -> Option<u64> {
        let token = self.peek_history_token(redo)?;
        let edit = if redo { self.sidebar_history.redo.pop().unwrap() } else { self.sidebar_history.undo.pop().unwrap() };
        if redo { self.sidebar_history.undo.push(edit); } else { self.sidebar_history.redo.push(edit); }
        Some(token)
    }
    pub fn has_peek_history(&self, token: u64) -> bool {
        self.sidebar_history.undo.iter().chain(&self.sidebar_history.redo).any(|edit| matches!(edit, SidebarEdit::PeekClose(id) if *id == token))
    }
    pub fn discard_peek_history(&mut self, token: u64) {
        self.sidebar_history.undo.retain(|edit| !matches!(edit, SidebarEdit::PeekClose(id) if *id == token));
        self.sidebar_history.redo.retain(|edit| !matches!(edit, SidebarEdit::PeekClose(id) if *id == token));
    }

    /// Arc's context-menu Remove Pin archives a pin; its minus button only unloads it.
    /// Retain the original pin and folder placement in one sidebar Undo transaction.
    pub fn remove_pin(&mut self, id: TabId, now: std::time::SystemTime) -> Option<crate::Closed> {
        if !self.can_edit_persistent_sidebar() || self.split_for(id).is_some()
            || !self.space().pinned.iter().any(|tab| tab.id == id) { return None; }
        let before = PinState::capture(self, id)?;
        self.toggle_pin_unrecorded(id);
        let after = PinState::capture(self, id)?;
        let capture = ArchiveCapture::new(self);
        let closed = self.close_tab_unrecorded(id, now)?;
        let archive = capture.finish(self)?;
        self.sidebar_history.record(SidebarEdit::RemovePin { pin: PinEdit { before, after }, archive });
        Some(closed)
    }

    /// Initial profile seeding is not a user action.
    pub fn clear_sidebar_history(&mut self) { self.sidebar_history = SidebarHistory::default(); }

    /// Reverses the last supported sidebar action, independently of text edits.
    /// Invalidated entries are discarded without recreating removed pages.
    pub fn undo_sidebar(&mut self, now: std::time::SystemTime) -> bool {
        while !self.sidebar_history.undo.is_empty() {
            if self.peek_history_token(false).is_some() { return false; }
            let mut edit = self.sidebar_history.undo.pop().unwrap();
            if edit.apply(false, self, now) {
                self.sidebar_history.redo.push(edit);
                return true;
            }
        }
        false
    }

    pub fn redo_sidebar(&mut self, now: std::time::SystemTime) -> bool {
        while !self.sidebar_history.redo.is_empty() {
            if self.peek_history_token(true).is_some() { return false; }
            let mut edit = self.sidebar_history.redo.pop().unwrap();
            if edit.apply(true, self, now) {
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
    fn live_peek_tokens_preserve_sidebar_order_redo_invalidation_and_session_boundaries() {
        let mut browser = Browser::new();
        browser.clear_sidebar_history();
        browser.record_peek_close(10);
        assert_eq!(browser.sidebar_undo_action(), Some("Close Peek"));
        let id = browser.new_tab("https://example.com", NOW);
        browser.toggle_pin(id);
        assert!(browser.undo_sidebar(NOW));
        assert!(!browser.undo_sidebar(NOW));
        assert_eq!(browser.take_peek_history(false), Some(10));
        assert!(!browser.redo_sidebar(NOW));
        assert_eq!(browser.take_peek_history(true), Some(10));
        assert!(browser.redo_sidebar(NOW));
        assert!(browser.undo_sidebar(NOW));
        browser.record_peek_close(11);
        assert!(browser.has_peek_history(10));
        browser.discard_peek_history(10);
        assert!(!browser.has_peek_history(10));
        let restored: Browser = serde_json::from_str(&serde_json::to_string(&browser).unwrap()).unwrap();
        assert!(!restored.has_peek_history(11));
    }

    #[test]
    fn remove_pin_archives_and_one_undo_restores_original_url_and_folder_slot() {
        let mut browser = Browser::new();
        let target = browser.new_tab("https://target", NOW);
        browser.toggle_pin(target);
        let folder = browser.create_folder(browser.space().id, None, "Reading").unwrap();
        browser.move_item_to_folder(target, folder);
        browser.navigate(target, "https://target/next");
        let active = browser.new_tab("https://active", NOW);
        browser.clear_sidebar_history();
        assert_eq!(browser.remove_pin(target, NOW), Some(crate::Closed::Archived));
        assert_eq!(browser.active_tab().unwrap().id, active);
        assert!(!browser.all_tabs().any(|tab| tab.id == target));
        for _ in 0..3 {
            assert!(browser.undo_sidebar(NOW));
            let restored = browser.all_tabs().find(|tab| tab.id == target).unwrap();
            assert_eq!(restored.url, "https://target/next");
            assert_eq!(restored.pinned_url.as_deref(), Some("https://target"));
            assert_eq!(browser.folder_ancestors(target), vec![folder]);
            assert_eq!(browser.active_tab().unwrap().id, active);
            assert!(browser.redo_sidebar(NOW));
            assert!(!browser.all_tabs().any(|tab| tab.id == target));
            assert_eq!(browser.archive.len(), 1);
        }
    }

    #[test]
    fn remove_pin_rejects_today_and_favorites_without_creating_history() {
        let mut browser = Browser::new();
        let today = browser.new_tab("https://today", NOW);
        let favorite = browser.new_tab("https://favorite", NOW);
        browser.favorite_tab(favorite);
        browser.clear_sidebar_history();
        assert_eq!(browser.remove_pin(today, NOW), None);
        assert_eq!(browser.remove_pin(favorite, NOW), None);
        assert_eq!(browser.all_tabs().count(), 2);
        assert!(!browser.undo_sidebar(NOW));
    }

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
        assert!(browser.undo_sidebar(NOW));
        assert_eq!(ids(&browser.space().today), vec![newer, top, target, bottom]);
        let tab = browser.all_tabs().find(|tab| tab.id == target).unwrap();
        assert_eq!(tab.url, "https://target/next");
        assert_eq!(tab.title, "Loaded later");
        assert!(tab.pinned_url.is_none());
        assert_eq!(browser.active_tab().unwrap().id, newer);
        assert!(browser.redo_sidebar(NOW));
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
        assert!(browser.undo_sidebar(NOW));
        assert_eq!(browser.folder(folder).unwrap().name, "Renamed later");
        assert_eq!(browser.folder(folder).unwrap().items, vec![PinnedItem::Tab(target), PinnedItem::Tab(other)]);
        assert!(browser.redo_sidebar(NOW));
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
        assert!(browser.undo_sidebar(NOW));
        assert!(browser.space().pinned.is_empty());
        assert_eq!(browser.split_for(b).unwrap().id, split);
        assert_eq!(browser.split_for(b).unwrap().weights, vec![0.3, 0.7]);
        assert_eq!(browser.active_tab().unwrap().id, b);
        assert!(browser.redo_sidebar(NOW));
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
        assert!(browser.undo_sidebar(NOW));
        assert!(browser.is_favorite(favorite));
        assert_eq!(browser.spaces[0].active_tab, Some(favorite));
        assert_eq!(browser.spaces[1].active_tab, Some(local));
        assert!(browser.redo_sidebar(NOW));
        assert!(!browser.is_favorite(favorite));
        assert_eq!(browser.spaces[1].active_tab, Some(local));
    }

    #[test]
    fn new_edits_branch_history_and_sessions_never_store_undo_payloads() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.toggle_pin(a);
        assert!(browser.undo_sidebar(NOW));
        browser.toggle_pin(b);
        assert!(!browser.redo_sidebar(NOW));
        let json = session::to_json(&browser);
        assert!(!json.contains("sidebar_history"));
        let mut restored = session::from_json(&json).unwrap();
        assert!(!restored.undo_sidebar(NOW));
        assert!(!restored.redo_sidebar(NOW));
        assert!(browser.undo_sidebar(NOW));
        assert!(!browser.undo_sidebar(NOW));
    }

    #[test]
    fn a_delayed_title_at_the_pinned_url_does_not_invalidate_undo_or_restore_a_stale_title() {
        let mut browser = Browser::new();
        let tab = browser.new_tab("https://loading", NOW);
        browser.toggle_pin(tab);
        browser.set_title(tab, "Finished loading");
        assert!(browser.undo_sidebar(NOW));
        assert!(browser.redo_sidebar(NOW));
        assert_eq!(browser.space().pinned[0].pinned_title.as_deref(), Some("Finished loading"));
    }

    #[test]
    fn invalidated_history_does_not_resurrect_pages_or_break_changed_splits() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        browser.toggle_pin(a);
        browser.toggle_pin(a);
        browser.close_tab_unrecorded(a, NOW);
        assert!(!browser.undo_sidebar(NOW));
        assert_eq!(browser.archive.len(), 1);
        assert_eq!(browser.all_tabs().count(), 0);
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.toggle_pin(a);
        browser.toggle_pin(b);
        browser.add_split(a, b, Placement::Right, NOW).unwrap();
        assert!(!browser.undo_sidebar(NOW));
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
        assert!(!browser.undo_sidebar(NOW));
        assert_eq!(browser.folder_tabs(folder), vec![target]);
        browser.favorite_tab(target);
        browser.toggle_pin(target);
        for index in 0..crate::favorites::MAX_FAVORITES {
            let tab = browser.new_tab(format!("https://favorite-{index}"), NOW);
            assert!(browser.favorite_tab(tab));
        }
        assert!(!browser.undo_sidebar(NOW));
        assert_eq!(browser.favorite_count(), crate::favorites::MAX_FAVORITES);
        assert!(browser.space().today.iter().any(|tab| tab.id == target));
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    const NOW: std::time::SystemTime = std::time::SystemTime::UNIX_EPOCH;
    #[test]
    fn selection_archive_undo_is_atomic_for_pins_and_split_groups() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a.example", NOW);
        browser.toggle_pin(a);
        let b = browser.new_tab("https://b.example", NOW);
        let c = browser.new_tab("https://c.example", NOW);
        browser.add_split(b, c, crate::splits::Placement::Right, NOW).unwrap();
        assert_eq!(browser.archive_sidebar_items(&[a,b,c,a], NOW), 2);
        let tokens = browser.archive_undo_items().unwrap();
        let later = browser.new_tab("https://later.example", NOW);
        assert!(browser.undo_archive_items(&tokens, NOW));
        assert_eq!(browser.space().pinned[0].id, a);
        assert_eq!(browser.split_for(b).unwrap().panes, vec![b,c]);
        assert!(browser.all_tabs().any(|tab| tab.id == later));
        assert!(browser.redo_sidebar(NOW));
        assert!(browser.all_tabs().any(|tab| tab.id == later));
        assert!(!browser.all_tabs().any(|tab| tab.id == a || tab.id == b || tab.id == c));
    }
    #[test]
    fn stale_selection_redo_does_not_partially_archive_surviving_items() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a.example", NOW);
        let b = browser.new_tab("https://b.example", NOW);
        assert_eq!(browser.archive_sidebar_items(&[a,b], NOW), 2);
        assert!(browser.undo_sidebar(NOW));
        // A structural edit outside sidebar history invalidates only one item.
        browser.spaces[0].today.iter_mut().find(|tab| tab.id == b).unwrap().pinned_url = Some("https://changed.example".into());
        let pin = browser.spaces[0].today.iter().find(|tab| tab.id == b).unwrap().clone();
        browser.spaces[0].pinned.push(pin);
        browser.spaces[0].today.retain(|tab| tab.id != b);
        assert!(!browser.redo_sidebar(NOW));
        assert!(browser.all_tabs().any(|tab| tab.id == a));
        assert!(browser.all_tabs().any(|tab| tab.id == b));
    }
}
