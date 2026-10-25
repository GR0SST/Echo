//! Space removal/export transactions retain removed content, never whole sessions.
use super::{insertion_index, SidebarEdit};
use crate::{
    folders::{Folder, FolderId, PinnedItem},
    ArchivedTab, Browser, Space, SpaceId, TabId,
};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub(super) enum SpaceEdit {
    Delete {
        source: Space,
        order: Vec<SpaceId>,
        archive: Vec<ArchivedTab>,
    },
    Export {
        source: Space,
        order: Vec<SpaceId>,
        target: SpaceId,
        target_profile: crate::UserProfileId,
        folder: FolderId,
        contents: Vec<PinnedItem>,
    },
}
fn ids(space: &Space) -> Vec<TabId> {
    space
        .pinned
        .iter()
        .chain(&space.today)
        .map(|tab| tab.id)
        .collect()
}
fn same_structure(a: &Space, b: &Space) -> bool {
    ids(a) == ids(b)
        && a.pinned_items == b.pinned_items
        && a.folders
            .iter()
            .map(|folder| (folder.id, &folder.items))
            .eq(b.folders.iter().map(|folder| (folder.id, &folder.items)))
        && a.splits
            .iter()
            .map(|split| (split.id, &split.panes))
            .eq(b.splits.iter().map(|split| (split.id, &split.panes)))
}
fn archived_ids(entry: &ArchivedTab) -> Vec<TabId> {
    entry
        .split
        .as_ref()
        .map_or_else(|| vec![entry.tab.id], |split| split.view.panes.clone())
}
fn insert(browser: &mut Browser, space: Space, order: &[SpaceId]) {
    let id = space.id;
    let current: Vec<_> = browser.spaces.iter().map(|space| space.id).collect();
    let index = insertion_index(&current, order, id);
    browser.spaces.insert(index, space);
    browser.active_space = index;
    browser.repair_splits();
    browser.repair_folders();
}
impl SpaceEdit {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Delete { .. } => "Delete Space",
            Self::Export { .. } => "Turn into Folder",
        }
    }
    pub(super) fn apply(&mut self, redo: bool, browser: &mut Browser, now: SystemTime) -> bool {
        match self {
            Self::Delete {
                source,
                order,
                archive,
            } => {
                if redo {
                    let Some(current) = browser.spaces.iter().find(|space| space.id == source.id)
                    else {
                        return false;
                    };
                    if !same_structure(current, source)
                        || current.user_profile_id != source.user_profile_id
                        || browser.spaces.len() < 2
                    {
                        return false;
                    }
                    *source = current.clone();
                    *order = browser.spaces.iter().map(|space| space.id).collect();
                    let Some(entries) = browser.remove_space_unrecorded(source.id, now) else {
                        return false;
                    };
                    *archive = entries;
                    true
                } else {
                    if browser.spaces.iter().any(|space| space.id == source.id)
                        || browser.user_profile(source.user_profile_id).is_none()
                        || ids(source)
                            .iter()
                            .any(|id| browser.all_tabs().any(|tab| tab.id == *id))
                        || archive.iter().any(|saved| {
                            !browser.archive.iter().any(|entry| {
                                entry.space == source.id
                                    && entry.user_profile_id == source.user_profile_id
                                    && entry.tab.id == saved.tab.id
                                    && archived_ids(entry) == archived_ids(saved)
                            })
                        })
                    {
                        return false;
                    }
                    browser.archive.retain(|entry| {
                        !archive.iter().any(|saved| {
                            entry.tab.id == saved.tab.id
                                && entry.space == source.id
                                && entry.user_profile_id == source.user_profile_id
                        })
                    });
                    insert(browser, source.clone(), order);
                    true
                }
            }
            Self::Export {
                source,
                order,
                target,
                target_profile,
                folder,
                contents,
            } => {
                if redo {
                    let Some(current) = browser.spaces.iter().find(|space| space.id == source.id)
                    else {
                        return false;
                    };
                    if !same_structure(current, source)
                        || current.user_profile_id != source.user_profile_id
                    {
                        return false;
                    }
                    if browser
                        .spaces
                        .iter()
                        .find(|space| space.id == *target)
                        .is_none_or(|space| space.user_profile_id != *target_profile)
                    {
                        return false;
                    }
                    let mut trial = browser.clone();
                    if trial
                        .export_space_unrecorded(source.id, *target, Some(*folder))
                        .is_none()
                    {
                        return false;
                    }
                    *source = current.clone();
                    *order = browser.spaces.iter().map(|space| space.id).collect();
                    if browser
                        .export_space_unrecorded(source.id, *target, Some(*folder))
                        .is_none()
                    {
                        return false;
                    }
                    *contents = browser.folder(*folder).unwrap().items.clone();
                    true
                } else {
                    let moved = ids(source);
                    let Some(destination) = browser.spaces.iter().find(|space| space.id == *target)
                    else {
                        return false;
                    };
                    if destination.user_profile_id != *target_profile
                        || browser.user_profile(source.user_profile_id).is_none()
                        || browser.spaces.iter().any(|space| space.id == source.id)
                        || browser
                            .folder(*folder)
                            .is_none_or(|entry| entry.items != *contents)
                        || moved
                            .iter()
                            .any(|id| !destination.pinned.iter().any(|tab| tab.id == *id))
                        || source.folders.iter().any(|saved| {
                            destination
                                .folders
                                .iter()
                                .find(|entry| entry.id == saved.id)
                                .is_none_or(|entry| entry.items != saved.items)
                        })
                    {
                        return false;
                    }
                    for id in &moved {
                        let expected = source
                            .splits
                            .iter()
                            .find(|split| split.panes.contains(id))
                            .map_or_else(|| vec![*id], |split| split.panes.clone());
                        if browser.related_tabs(*id) != expected {
                            return false;
                        }
                    }
                    let mut restored = source.clone();
                    for tab in restored.pinned.iter_mut().chain(&mut restored.today) {
                        let current = destination
                            .pinned
                            .iter()
                            .find(|entry| entry.id == tab.id)
                            .unwrap();
                        let (url, title) = (tab.pinned_url.clone(), tab.pinned_title.clone());
                        *tab = current.clone();
                        tab.pinned_url = url;
                        tab.pinned_title = title;
                    }
                    for split in &mut restored.splits {
                        *split = destination
                            .splits
                            .iter()
                            .find(|entry| entry.id == split.id)
                            .unwrap()
                            .clone();
                    }
                    for entry in &mut restored.folders {
                        *entry = destination
                            .folders
                            .iter()
                            .find(|folder| folder.id == entry.id)
                            .unwrap()
                            .clone();
                    }
                    let destination = browser
                        .spaces
                        .iter_mut()
                        .find(|space| space.id == *target)
                        .unwrap();
                    destination.pinned.retain(|tab| !moved.contains(&tab.id));
                    destination
                        .pinned_items
                        .retain(|item| *item != PinnedItem::Folder(*folder));
                    destination.folders.retain(|entry| {
                        entry.id != *folder
                            && !source.folders.iter().any(|saved| saved.id == entry.id)
                    });
                    destination
                        .splits
                        .retain(|split| !source.splits.iter().any(|saved| saved.id == split.id));
                    insert(browser, restored, order);
                    true
                }
            }
        }
    }
}
impl Browser {
    fn remove_space_unrecorded(
        &mut self,
        id: SpaceId,
        now: SystemTime,
    ) -> Option<Vec<ArchivedTab>> {
        if !self.can_edit_persistent_sidebar() || self.is_private() || self.spaces.len() < 2 {
            return None;
        }
        let index = self.spaces.iter().position(|space| space.id == id)?;
        let active = self.space().id;
        self.active_space = index;
        let items: Vec<_> = self
            .space()
            .pinned
            .iter()
            .chain(&self.space().today)
            .filter(|tab| !self.is_split_secondary(tab.id))
            .map(|tab| tab.id)
            .collect();
        let start = self.archive.len();
        let history = std::mem::take(&mut self.sidebar_history);
        self.archive_sidebar_items(&items, now);
        self.sidebar_history = history;
        self.spaces.remove(index);
        self.active_space = self
            .spaces
            .iter()
            .position(|space| space.id == active)
            .unwrap_or(index.min(self.spaces.len() - 1));
        Some(self.archive[start..].to_vec())
    }
    /// The last-Space replacement flow is not referenced yet; preserve it.
    pub fn delete_space(&mut self, id: SpaceId, now: SystemTime) -> bool {
        let Some(source) = self.spaces.iter().find(|space| space.id == id).cloned() else {
            return false;
        };
        let order = self.spaces.iter().map(|space| space.id).collect();
        let Some(archive) = self.remove_space_unrecorded(id, now) else {
            return false;
        };
        self.sidebar_history
            .record(SidebarEdit::Space(SpaceEdit::Delete {
                source,
                order,
                archive,
            }));
        true
    }
    fn export_space_unrecorded(
        &mut self,
        source: SpaceId,
        target: SpaceId,
        reuse: Option<FolderId>,
    ) -> Option<FolderId> {
        if source == target
            || !self.can_edit_persistent_sidebar()
            || self.is_private()
            || self.spaces.len() < 2
        {
            return None;
        }
        let snapshot = self.spaces.iter().find(|space| space.id == source)?.clone();
        let target_profile = self
            .spaces
            .iter()
            .find(|space| space.id == target)?
            .user_profile_id;
        if target_profile != snapshot.user_profile_id {
            // Export intentionally adopts the target profile. The adapter
            // retires changed storage before any new page can be opened.
            let _transition = self
                .assign_space_user_profile(source, target_profile)
                .ok()?;
        }
        let folder = if let Some(id) = reuse {
            if self.folder(id).is_some() {
                return None;
            }
            let destination = self.spaces.iter_mut().find(|space| space.id == target)?;
            destination.folders.push(Folder {
                live: None,
                id,
                name: snapshot.name.clone(),
                collapsed: false,
                custom_icon: snapshot.custom_icon.clone(),
                items: vec![],
            });
            destination.pinned_items.push(PinnedItem::Folder(id));
            id
        } else {
            let id = self.create_folder(target, None, &snapshot.name)?;
            self.spaces
                .iter_mut()
                .find(|space| space.id == target)?
                .folders
                .iter_mut()
                .find(|folder| folder.id == id)?
                .custom_icon = snapshot.custom_icon.clone();
            id
        };
        for item in snapshot.pinned_items {
            let moved = match item {
                PinnedItem::Folder(id) => self.move_folder(id, target, Some(folder)),
                _ => self
                    .pinned_item_tab(source, item)
                    .is_some_and(|tab| self.move_item_to_folder(tab, folder)),
            };
            if !moved {
                return None;
            }
        }
        let today: Vec<_> = snapshot
            .today
            .iter()
            .filter(|tab| !self.is_split_secondary(tab.id))
            .map(|tab| tab.id)
            .collect();
        for id in today {
            if !self.move_item_to_folder(id, folder) {
                return None;
            }
        }
        self.spaces.retain(|space| space.id != source);
        self.active_space = self.spaces.iter().position(|space| space.id == target)?;
        self.repair_folders();
        Some(folder)
    }
    /// Local inverse of Turn into Space. Cross-profile tabs are re-created in
    /// the destination's storage by the adapter; native documents never transfer.
    pub fn export_space_into_folder(
        &mut self,
        source: SpaceId,
        target: SpaceId,
    ) -> Option<FolderId> {
        let mut trial = self.clone();
        trial.export_space_unrecorded(source, target, None)?;
        let snapshot = self.spaces.iter().find(|space| space.id == source)?.clone();
        let order = self.spaces.iter().map(|space| space.id).collect();
        let folder = self.export_space_unrecorded(source, target, None)?;
        let contents = self.folder(folder)?.items.clone();
        let target_profile = self
            .spaces
            .iter()
            .find(|space| space.id == target)?
            .user_profile_id;
        self.sidebar_history
            .record(SidebarEdit::Space(SpaceEdit::Export {
                source: snapshot,
                order,
                target,
                target_profile,
                folder,
                contents,
            }));
        Some(folder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;
    #[test]
    fn delete_space_archives_content_and_undo_preserves_unrelated_pages() {
        let mut b = Browser::new();
        let source = b.space().id;
        let favorite = b.new_tab("https://favorite.example", NOW);
        b.favorite_tab(favorite);
        let pin = b.new_tab("https://pin.example", NOW);
        b.toggle_pin(pin);
        let folder = b.create_folder(source, None, "Folder").unwrap();
        b.move_item_to_folder(pin, folder);
        let a = b.new_tab("https://a.example", NOW);
        let c = b.new_tab("https://c.example", NOW);
        b.add_split(a, c, crate::splits::Placement::Right, NOW)
            .unwrap();
        let other = b.add_space("Other", 30.);
        assert!(b.delete_space(source, NOW));
        assert_eq!(b.archive.len(), 2);
        assert!(b.is_favorite(favorite));
        assert_eq!(b.space().id, other);
        let later = b.new_tab("https://later.example", NOW);
        assert!(b.undo_sidebar(NOW));
        assert_eq!(b.space().id, source);
        assert_eq!(b.folder_tabs(folder), vec![pin]);
        assert_eq!(b.related_tabs(a), vec![a, c]);
        assert!(b.all_tabs().any(|tab| tab.id == later));
        assert!(b.redo_sidebar(NOW));
        assert!(!b.spaces.iter().any(|space| space.id == source));
        assert!(b.all_tabs().any(|tab| tab.id == later));
    }
    #[test]
    fn deleted_space_undo_rejects_an_individually_restored_archive() {
        let mut b = Browser::new();
        let source = b.space().id;
        b.new_tab("https://a.example", NOW);
        b.add_space("Other", 30.);
        assert!(b.delete_space(source, NOW));
        b.restore_last_archived(NOW).unwrap();
        let before = crate::session::to_json(&b);
        assert!(!b.undo_sidebar(NOW));
        assert_eq!(crate::session::to_json(&b), before);
    }
    #[test]
    fn last_space_is_preserved_and_export_adopts_the_target_profile() {
        let mut b = Browser::new();
        let source = b.space().id;
        let before = crate::session::to_json(&b);
        assert!(!b.delete_space(source, NOW));
        assert_eq!(crate::session::to_json(&b), before);
        let target = b.add_space("Other", 30.);
        let profile = b.create_user_profile("Other profile").unwrap();
        b.assign_space_user_profile(target, profile).unwrap();
        let original = b
            .spaces
            .iter()
            .find(|space| space.id == source)
            .unwrap()
            .user_profile_id;
        let folder = b.export_space_into_folder(source, target).unwrap();
        assert!(b.folder(folder).is_some());
        assert_eq!(b.space().user_profile_id, profile);
        assert!(b.undo_sidebar(NOW));
        assert_eq!(b.space().id, source);
        assert_eq!(b.space().user_profile_id, original);
    }
    #[test]
    fn exported_space_undo_preserves_current_navigation_and_unrelated_target_tabs() {
        let mut b = Browser::new();
        let source = b.space().id;
        let pin = b.new_tab("https://pin.example", NOW);
        b.toggle_pin(pin);
        let nested = b.create_folder(source, None, "Nested").unwrap();
        b.move_item_to_folder(pin, nested);
        let a = b.new_tab("https://a.example", NOW);
        let c = b.new_tab("https://c.example", NOW);
        b.add_split(a, c, crate::splits::Placement::Right, NOW)
            .unwrap();
        let target = b.add_space("Other", 30.);
        b.switch_space(1);
        let other = b.new_tab("https://other.example", NOW);
        let folder = b.export_space_into_folder(source, target).unwrap();
        assert!(!b.spaces.iter().any(|space| space.id == source));
        assert_eq!(b.folder_tabs(folder).len(), 3);
        assert!(b.archive.is_empty());
        b.spaces[0]
            .pinned
            .iter_mut()
            .find(|tab| tab.id == a)
            .unwrap()
            .url = "https://updated.example".into();
        let later = b.new_tab("https://later.example", NOW);
        assert!(b.undo_sidebar(NOW));
        assert_eq!(b.space().id, source);
        assert_eq!(b.folder_tabs(nested), vec![pin]);
        assert_eq!(
            b.all_tabs().find(|tab| tab.id == a).unwrap().url,
            "https://updated.example"
        );
        assert!(b.all_tabs().any(|tab| tab.id == other));
        assert!(b.all_tabs().any(|tab| tab.id == later));
        assert!(b.redo_sidebar(NOW));
        assert!(b.folder(folder).is_some());
        assert!(b.archive.is_empty());
    }
    #[test]
    fn over_depth_export_leaves_profile_tree_and_history_unchanged() {
        let mut b = Browser::new();
        let source = b.space().id;
        let tab = b.new_tab("https://before.example", NOW);
        b.toggle_pin(tab);
        let mut parent = None;
        for _ in 0..crate::folders::MAX_FOLDER_DEPTH {
            parent = Some(b.create_folder(source, parent, "Nested").unwrap());
        }
        assert!(b.move_item_to_folder(tab, parent.unwrap()));
        let target = b.add_space("Other", 30.);
        let profile = b.create_user_profile("Other profile").unwrap();
        b.assign_space_user_profile(target, profile).unwrap();
        let before = crate::session::to_json(&b);
        assert!(b.export_space_into_folder(source, target).is_none());
        assert_eq!(crate::session::to_json(&b), before);
        assert!(!b.undo_sidebar(NOW));
    }
    #[test]
    fn export_undo_does_not_remove_a_new_child_added_to_its_folder() {
        let mut b = Browser::new();
        let source = b.space().id;
        b.new_tab("https://a.example", NOW);
        let target = b.add_space("Other", 30.);
        let folder = b.export_space_into_folder(source, target).unwrap();
        let new = b.new_tab("https://new.example", NOW);
        b.move_item_to_folder(new, folder);
        let before = crate::session::to_json(&b);
        assert!(!b.undo_sidebar(NOW));
        assert_eq!(crate::session::to_json(&b), before);
    }
}
