//! Captured sidebar identities. Favorites are intentionally not selectable.
use super::*;
use crate::{
    undo::{
        folder::{FolderEdit, Slot},
        PinState,
    },
    UserProfileId,
};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SidebarItem {
    Tab(TabId),
    Folder(FolderId),
}

#[derive(Clone, Debug)]
pub struct ItemSelection {
    pub space: SpaceId,
    pub profile: UserProfileId,
    pub items: Vec<SidebarItem>,
}
impl ItemSelection {
    /// Reject stale identities; selecting an ancestor covers its descendants.
    pub fn capture(browser: &Browser, space: SpaceId, items: &[SidebarItem]) -> Option<Self> {
        let source = browser.spaces.iter().find(|s| s.id == space)?;
        let mut roots = Vec::new();
        for item in items {
            let item = match *item {
                SidebarItem::Tab(id) => {
                    let id = browser.item_anchor(id);
                    if !source.tabs().any(|t| t.id == id) {
                        return None;
                    }
                    SidebarItem::Tab(id)
                }
                SidebarItem::Folder(id) => {
                    if browser.folder_space(id) != Some(space) {
                        return None;
                    }
                    SidebarItem::Folder(id)
                }
            };
            if !roots.contains(&item) {
                roots.push(item);
            }
        }
        let selected = roots.clone();
        roots.retain(|item| {
            !selected.iter().any(|parent| {
                let SidebarItem::Folder(parent) = parent else {
                    return false;
                };
                match *item {
                    SidebarItem::Tab(id) => browser.folder_ancestors(id).contains(parent),
                    SidebarItem::Folder(id) => {
                        id != *parent && super::subtree_folders(source, *parent).contains(&id)
                    }
                }
            })
        });
        Some(Self {
            space,
            profile: source.user_profile_id,
            items: roots,
        })
    }
    pub fn is_current(&self, browser: &Browser) -> bool {
        Self::capture(browser, self.space, &self.items)
            .is_some_and(|s| s.profile == self.profile && s.items == self.items)
    }
    pub fn tabs(&self, browser: &Browser) -> Vec<TabId> {
        if !self.is_current(browser) {
            return vec![];
        }
        let mut ids = Vec::new();
        for item in &self.items {
            let pages = match *item {
                SidebarItem::Tab(id) => browser.related_tabs(id),
                SidebarItem::Folder(id) => browser.folder_tabs(id),
            };
            for id in pages {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    }
    fn editable(&self, browser: &Browser) -> bool {
        self.is_current(browser)
            && browser.can_edit_persistent_sidebar()
            && self.items.iter().all(|item| match *item {
                SidebarItem::Tab(_) => true,
                SidebarItem::Folder(id) => browser
                    .spaces
                    .iter()
                    .find(|s| s.id == self.space)
                    .is_some_and(|s| {
                        super::subtree_folders(s, id)
                            .iter()
                            .all(|id| browser.folder(*id).is_some_and(|f| f.live.is_none()))
                    }),
            })
    }
}
impl Browser {
    /// A complete operation is validated on a model clone before publication;
    /// runtime pages use the unchanged identities, not this temporary clone.
    pub fn group_sidebar_selection(
        &mut self,
        selection: &ItemSelection,
        name: &str,
    ) -> Option<FolderId> {
        if selection.items.is_empty() || !selection.editable(self) {
            return None;
        }
        let mut trial = self.clone();
        let id = trial.sidebar_batch(|browser| {
            let parents: Vec<_> = selection
                .items
                .iter()
                .map(|item| match *item {
                    SidebarItem::Tab(id) => browser.folder_ancestors(id).last().copied(),
                    SidebarItem::Folder(id) => Slot::capture(browser, id).and_then(|s| s.parent),
                })
                .collect();
            let parent = parents
                .first()
                .copied()
                .flatten()
                .filter(|p| parents.iter().all(|v| *v == Some(*p)));
            let original = super::entries_mut(
                browser
                    .spaces
                    .iter_mut()
                    .find(|s| s.id == selection.space)?,
                parent,
            )?
            .clone();
            let folder = browser.create_folder(selection.space, parent, name)?;
            // Replace the first selected sibling with the group; Today-only
            // groups append to Pinned, retaining their existing item order.
            let selected: Vec<_> = selection
                .items
                .iter()
                .map(|item| match *item {
                    SidebarItem::Tab(id) => browser.pinned_item(id),
                    SidebarItem::Folder(id) => PinnedItem::Folder(id),
                })
                .collect();
            let pool = super::entries_mut(
                browser
                    .spaces
                    .iter_mut()
                    .find(|s| s.id == selection.space)?,
                parent,
            )?;
            pool.retain(|i| *i != PinnedItem::Folder(folder));
            let index = original
                .iter()
                .position(|i| selected.contains(i))
                .unwrap_or(pool.len())
                .min(pool.len());
            pool.insert(index, PinnedItem::Folder(folder));
            browser.record_folder_edit(FolderEdit::presence(browser, folder, true)?);
            for item in &selection.items {
                if !browser.move_selected_item(*item, selection.space, Some(folder)) {
                    return None;
                }
            }
            Some(folder)
        })?;
        *self = trial;
        Some(id)
    }
    pub fn move_sidebar_selection(
        &mut self,
        selection: &ItemSelection,
        destination: SpaceId,
    ) -> bool {
        if !selection.editable(self)
            || !self
                .spaces
                .iter()
                .any(|s| s.id == destination && s.user_profile_id == selection.profile)
        {
            return false;
        }
        if destination == selection.space {
            return true;
        }
        let mut trial = self.clone();
        let (mut pins, today): (Vec<_>, Vec<_>) =
            selection
                .items
                .iter()
                .copied()
                .partition(|item| match *item {
                    SidebarItem::Folder(_) => true,
                    SidebarItem::Tab(id) => self
                        .spaces
                        .iter()
                        .find(|s| s.id == selection.space)
                        .is_some_and(|s| s.pinned.iter().any(|t| t.id == id)),
                });
        pins.extend(today.into_iter().rev());
        let ok = trial.sidebar_batch(|browser| {
            pins.into_iter()
                .all(|item| browser.move_selected_item(item, destination, None))
        });
        if ok {
            *self = trial;
        }
        ok
    }
    fn move_selected_item(
        &mut self,
        item: SidebarItem,
        space: SpaceId,
        parent: Option<FolderId>,
    ) -> bool {
        match item {
            SidebarItem::Folder(id) => {
                let Some(before) = Slot::capture(self, id) else {
                    return false;
                };
                let profile = self
                    .spaces
                    .iter()
                    .find(|s| s.id == before.space)
                    .unwrap()
                    .user_profile_id;
                if !self.move_folder(id, space, parent) {
                    return false;
                }
                let after = Slot::capture(self, id).unwrap();
                self.record_folder_edit(FolderEdit::Move {
                    id,
                    before,
                    after,
                    profile,
                });
                true
            }
            SidebarItem::Tab(id) => {
                let Some(before) = PinState::capture(self, id) else {
                    return false;
                };
                let active = self
                    .spaces
                    .iter()
                    .find(|s| s.id == space)
                    .and_then(|s| s.active_tab);
                let ok = match parent {
                    Some(parent) => self.move_item_to_folder(id, parent),
                    None => self.move_sidebar_item_to_space(id, space),
                };
                if !ok {
                    return false;
                }
                if parent.is_some() {
                    if let Some(active) = active {
                        if let Some(target) = self.spaces.iter_mut().find(|s| s.id == space) {
                            if target.tabs().any(|t| t.id == active) {
                                target.active_tab = Some(active);
                            }
                        }
                    }
                }
                let after = PinState::capture(self, id).unwrap();
                self.sidebar_history.record_pin(before, after);
                true
            }
        }
    }
    pub fn delete_sidebar_selection(&mut self, selection: &ItemSelection, now: SystemTime) -> bool {
        if selection.space != self.space().id
            || selection.items.is_empty()
            || !selection.editable(self)
        {
            return false;
        }
        let mut trial = self.clone();
        let ok = trial.sidebar_batch(|browser| {
            selection.items.iter().all(|item| match *item {
                SidebarItem::Tab(id) => {
                    if browser.split_for(id).is_some()
                        && browser.space().pinned.iter().any(|t| t.id == id)
                    {
                        browser.toggle_pin(id);
                        browser.close_tab(id, now) == Some(crate::Closed::Archived)
                    } else {
                        browser.archive_sidebar_items(&[id], now) > 0
                    }
                }
                SidebarItem::Folder(id) => {
                    if !browser.folder_tabs(id).is_empty() {
                        return browser.archive_folder(id, now).is_some();
                    }
                    let mut ids = Vec::new();
                    super::all_items(browser.space(), &[PinnedItem::Folder(id)], 0, &mut ids);
                    for id in ids.into_iter().rev().filter_map(|i| match i {
                        PinnedItem::Folder(id) => Some(id),
                        _ => None,
                    }) {
                        let Some(edit) = FolderEdit::presence(browser, id, false) else {
                            return false;
                        };
                        if !browser.delete_empty_folder(id) {
                            return false;
                        }
                        browser.record_folder_edit(edit);
                    }
                    true
                }
            })
        });
        if ok {
            *self = trial;
        }
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;
    #[test]
    fn a_large_selection_is_one_complete_history_entry() {
        let mut b = Browser::new();
        let space = b.space().id;
        let folders: Vec<_> = (0..130)
            .map(|i| {
                b.create_folder(space, None, &format!("Folder {i}"))
                    .unwrap()
            })
            .collect();
        let original = b.space().pinned_items.clone();
        b.clear_sidebar_history();
        let items: Vec<_> = folders.iter().copied().map(SidebarItem::Folder).collect();
        let selection = ItemSelection::capture(&b, space, &items).unwrap();
        let group = b
            .group_sidebar_selection(&selection, "Large group")
            .unwrap();
        assert_eq!(b.folder(group).unwrap().items.len(), 130);
        assert!(b.undo_sidebar(NOW));
        assert!(b.folder(group).is_none());
        assert_eq!(b.space().pinned_items, original);
        assert!(b.redo_sidebar(NOW));
        assert_eq!(b.folder(group).unwrap().items, original);
    }
    #[test]
    fn rename_undo_changes_only_the_name_and_retains_later_folder_contents() {
        let mut b = Browser::new();
        let space = b.space().id;
        let folder = b.create_folder(space, None, "Original").unwrap();
        b.clear_sidebar_history();
        assert!(b.rename_folder_with_undo(folder, "Renamed"));
        assert_eq!(b.sidebar_undo_action(), Some("Rename Item"));
        let child = b.create_folder(space, Some(folder), "Later child").unwrap();
        b.spaces[0]
            .folders
            .iter_mut()
            .find(|f| f.id == folder)
            .unwrap()
            .custom_icon = Some("📂".into());
        assert!(b.undo_sidebar(NOW));
        assert_eq!(b.folder(folder).unwrap().name, "Original");
        assert_eq!(
            b.folder(folder).unwrap().items,
            vec![PinnedItem::Folder(child)]
        );
        assert_eq!(b.folder(folder).unwrap().custom_icon.as_deref(), Some("📂"));
        assert_eq!(b.sidebar_redo_action(), Some("Rename Item"));
        assert!(b.redo_sidebar(NOW));
        assert_eq!(b.folder(folder).unwrap().name, "Renamed");
        assert!(b.folder(child).is_some());
    }
    #[test]
    fn grouping_preserves_pages_and_undo_keeps_later_navigation_and_unrelated_tabs() {
        let mut b = Browser::new();
        let space = b.space().id;
        let a = b.create_folder(space, None, "A").unwrap();
        let nested = b.create_folder(space, Some(a), "Nested").unwrap();
        let pin = b.new_tab("https://pin.example", NOW);
        assert!(b.move_item_to_folder(pin, nested));
        let today = b.new_tab("https://today.example", NOW);
        b.clear_sidebar_history();
        let selection = ItemSelection::capture(
            &b,
            space,
            &[
                SidebarItem::Folder(a),
                SidebarItem::Tab(pin),
                SidebarItem::Tab(today),
            ],
        )
        .unwrap();
        assert_eq!(selection.items.len(), 2);
        let group = b.group_sidebar_selection(&selection, "Group").unwrap();
        assert_eq!(
            b.folder(group).unwrap().items,
            vec![PinnedItem::Folder(a), PinnedItem::Tab(today)]
        );
        assert_eq!(b.space().active_tab, Some(today));
        b.set_title(pin, "Updated after group");
        let unrelated = b.new_tab("https://later.example", NOW);
        assert!(b.undo_sidebar(NOW));
        assert!(b.folder(group).is_none());
        assert!(b.space().today.iter().any(|t| t.id == today));
        assert_eq!(b.folder_ancestors(pin), vec![a, nested]);
        assert!(b
            .all_tabs()
            .any(|t| t.id == pin && t.title == "Updated after group"));
        assert!(b.all_tabs().any(|t| t.id == unrelated));
        assert_eq!(b.space().active_tab, Some(unrelated));
        assert!(b.redo_sidebar(NOW));
        assert!(b.folder(group).is_some());
        assert!(b.all_tabs().any(|t| t.id == unrelated));
    }
    #[test]
    fn mixed_delete_and_empty_tree_are_one_undo_transaction() {
        let mut b = Browser::new();
        let space = b.space().id;
        let a = b.create_folder(space, None, "A").unwrap();
        let nested = b.create_folder(space, Some(a), "Empty child").unwrap();
        let today = b.new_tab("https://today.example", NOW);
        b.clear_sidebar_history();
        let selected = ItemSelection::capture(
            &b,
            space,
            &[
                SidebarItem::Folder(a),
                SidebarItem::Folder(nested),
                SidebarItem::Tab(today),
            ],
        )
        .unwrap();
        assert!(b.delete_sidebar_selection(&selected, NOW));
        assert!(b.folder(a).is_none());
        assert!(!b.all_tabs().any(|t| t.id == today));
        assert!(b.undo_sidebar(NOW));
        assert!(b.folder(a).is_some());
        assert!(b.folder(nested).is_some());
        assert!(b.all_tabs().any(|t| t.id == today));
        assert!(b.redo_sidebar(NOW));
        assert!(b.folder(a).is_none());
        assert!(!b.all_tabs().any(|t| t.id == today));
    }
    #[test]
    fn stale_or_foreign_selection_cannot_partially_group_or_move() {
        let mut b = Browser::new();
        let space = b.space().id;
        let a = b.create_folder(space, None, "A").unwrap();
        let tab = b.new_tab("https://today.example", NOW);
        let selected =
            ItemSelection::capture(&b, space, &[SidebarItem::Folder(a), SidebarItem::Tab(tab)])
                .unwrap();
        let target = b.add_space("Other", 0.);
        assert!(b.move_sidebar_selection(&selected, target));
        let count = b.all_tabs().count();
        let folders = b.spaces.iter().map(|s| s.folders.len()).sum::<usize>();
        assert!(b.group_sidebar_selection(&selected, "Stale").is_none());
        assert_eq!(b.all_tabs().count(), count);
        assert_eq!(
            b.spaces.iter().map(|s| s.folders.len()).sum::<usize>(),
            folders
        );
        assert!(b.undo_sidebar(NOW));
        assert_eq!(b.folder_space(a), Some(space));
        assert!(b.space().tabs().any(|t| t.id == tab));
        let profile = b.create_user_profile("Foreign").unwrap();
        b.spaces
            .iter_mut()
            .find(|s| s.id == target)
            .unwrap()
            .user_profile_id = profile;
        assert!(!b.move_sidebar_selection(&selected, target));
        assert_eq!(b.folder_space(a), Some(space));
    }
    #[test]
    fn mixed_delete_keeps_a_pinned_split_atomic_through_undo_and_redo() {
        let mut b = Browser::new();
        let space = b.space().id;
        let a = b.new_tab("https://a.example", NOW);
        let other = b.new_tab("https://b.example", NOW);
        assert!(b
            .add_split(a, other, crate::splits::Placement::Right, NOW)
            .is_ok());
        b.toggle_pin(a);
        let split = b.split_for(a).unwrap().clone();
        let empty = b.create_folder(space, None, "Empty").unwrap();
        b.clear_sidebar_history();
        let selected = ItemSelection::capture(
            &b,
            space,
            &[SidebarItem::Tab(a), SidebarItem::Folder(empty)],
        )
        .unwrap();
        assert!(b.delete_sidebar_selection(&selected, NOW));
        assert!(b.split_for(a).is_none());
        assert!(b.folder(empty).is_none());
        assert_eq!(b.archive.len(), 1);
        assert_eq!(b.archive[0].split.as_ref().unwrap().view.id, split.id);
        assert!(b.undo_sidebar(NOW));
        let restored = b.split_for(a).unwrap();
        assert_eq!(restored.id, split.id);
        assert_eq!(restored.panes, split.panes);
        assert!(b.space().pinned.iter().any(|t| t.id == other));
        assert!(b.folder(empty).is_some());
        assert!(b.redo_sidebar(NOW));
        assert!(b.split_for(a).is_none());
        assert!(b.folder(empty).is_none());
    }
    #[test]
    fn invalidated_group_undo_cannot_move_only_some_of_the_children() {
        let mut b = Browser::new();
        let space = b.space().id;
        let a = b.create_folder(space, None, "A").unwrap();
        let other = b.create_folder(space, None, "B").unwrap();
        b.clear_sidebar_history();
        let selected = ItemSelection::capture(
            &b,
            space,
            &[SidebarItem::Folder(a), SidebarItem::Folder(other)],
        )
        .unwrap();
        let group = b.group_sidebar_selection(&selected, "Group").unwrap();
        let later = b.create_folder(space, Some(group), "Later child").unwrap();
        let before = b.folder(group).unwrap().items.clone();
        assert!(!b.undo_sidebar(NOW));
        assert_eq!(b.folder(group).unwrap().items, before);
        assert!(b.folder(later).is_some());
        assert_eq!(b.space().pinned_items, vec![PinnedItem::Folder(group)]);
    }
}
