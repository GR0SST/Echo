//! A folder is one durable archive item, retaining its local tree and pages.
use super::{entries_mut, Folder, FolderId, PinnedItem};
use crate::{
    splits::SplitView, ArchiveOrigin, ArchiveReason, ArchivedTab, Browser, SpaceId, Tab, TabId,
    UserProfileId,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::SystemTime};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchivedFolder {
    pub root: FolderId,
    pub parent: Option<FolderId>,
    pub order: Vec<PinnedItem>,
    pub pinned_order: Vec<TabId>,
    pub folders: Vec<Folder>,
    /// The archive entry itself holds the first page; these are the others.
    pub tabs: Vec<Tab>,
    pub splits: Vec<SplitView>,
    pub selected: Option<TabId>,
}
impl ArchivedFolder {
    pub fn name(&self) -> &str {
        self.folders
            .iter()
            .find(|f| f.id == self.root)
            .map_or("Folder", |f| f.name.as_str())
    }
    pub(crate) fn all_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.tabs
            .iter()
            .map(|t| t.id.get())
            .chain(self.folders.iter().map(|f| f.id.get()))
            .chain(self.splits.iter().map(|s| s.id.get()))
    }
    fn valid(&self, anchor: TabId) -> bool {
        let tabs: HashSet<_> = std::iter::once(anchor)
            .chain(self.tabs.iter().map(|t| t.id))
            .collect();
        let folders: HashSet<_> = self.folders.iter().map(|f| f.id).collect();
        if self.parent.is_some_and(|id| folders.contains(&id))
            || self
                .order
                .iter()
                .filter(|i| **i == PinnedItem::Folder(self.root))
                .count()
                != 1
            || self.selected.is_some_and(|id| !tabs.contains(&id))
            || std::iter::once(anchor.get())
                .chain(self.all_ids())
                .collect::<HashSet<_>>()
                .len()
                != 1 + self.tabs.len() + self.folders.len() + self.splits.len()
            || tabs.len() != self.tabs.len() + 1
            || folders.len() != self.folders.len()
            || !folders.contains(&self.root)
            || self.folders.iter().any(|f| f.live.is_some())
        {
            return false;
        }
        if tabs
            .iter()
            .any(|id| self.pinned_order.iter().filter(|v| *v == id).count() != 1)
        {
            return false;
        }
        let mut visited = HashSet::new();
        let mut pages = HashSet::new();
        let mut groups = HashSet::new();
        fn visit(
            id: FolderId,
            tree: &ArchivedFolder,
            tabs: &HashSet<TabId>,
            seen: &mut HashSet<FolderId>,
            pages: &mut HashSet<TabId>,
            groups: &mut HashSet<crate::splits::SplitId>,
            depth: usize,
        ) -> bool {
            if !seen.insert(id) || depth >= super::MAX_FOLDER_DEPTH {
                return false;
            }
            let Some(folder) = tree.folders.iter().find(|f| f.id == id) else {
                return false;
            };
            folder.items.iter().all(|item| match *item {
                PinnedItem::Folder(id) => visit(id, tree, tabs, seen, pages, groups, depth + 1),
                PinnedItem::Tab(id) => {
                    tabs.contains(&id)
                        && !tree.splits.iter().any(|s| s.panes.contains(&id))
                        && pages.insert(id)
                }
                PinnedItem::Split(id) => {
                    let Some(split) = tree.splits.iter().find(|s| s.id == id) else {
                        return false;
                    };
                    groups.insert(id)
                        && (2..=crate::splits::MAX_SPLIT_PANES).contains(&split.panes.len())
                        && split.panes.contains(&split.anchor)
                        && split.panes.contains(&split.active)
                        && split
                            .panes
                            .iter()
                            .all(|id| tabs.contains(id) && pages.insert(*id))
                }
            })
        }
        visit(
            self.root,
            self,
            &tabs,
            &mut visited,
            &mut pages,
            &mut groups,
            0,
        ) && pages == tabs
            && visited == folders
            && groups.len() == self.splits.len()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FolderArchiveEdit {
    anchor: TabId,
    root: FolderId,
    space: SpaceId,
    profile: UserProfileId,
    tabs: HashSet<TabId>,
    folders: HashSet<FolderId>,
}
impl FolderArchiveEdit {
    pub(crate) fn anchor(&self) -> TabId {
        self.anchor
    }
    pub(crate) fn apply(&mut self, redo: bool, browser: &mut Browser, now: SystemTime) -> bool {
        if !redo {
            if !browser.archive.iter().any(|a| {
                a.tab.id == self.anchor
                    && a.space == self.space
                    && a.user_profile_id == self.profile
                    && a.folder.as_ref().is_some_and(|f| f.root == self.root)
            }) {
                return false;
            }
            return browser.restore_archived(self.anchor, now).is_some();
        }
        if browser.folder_space(self.root) != Some(self.space)
            || browser
                .spaces
                .iter()
                .find(|s| s.id == self.space)
                .is_none_or(|s| s.user_profile_id != self.profile)
            || browser
                .folder_tabs(self.root)
                .into_iter()
                .collect::<HashSet<_>>()
                != self.tabs
            || browser.folder_tree_ids(self.root) != self.folders
        {
            return false;
        }
        let selection = browser.active_space;
        browser.active_space = browser
            .spaces
            .iter()
            .position(|s| s.id == self.space)
            .unwrap();
        let archived = browser.archive_folder_unrecorded(self.root, now);
        if archived.is_none() {
            browser.active_space = selection;
        }
        archived.is_some()
    }
}

impl Browser {
    fn folder_tree_ids(&self, root: FolderId) -> HashSet<FolderId> {
        let mut pending = vec![root];
        let mut ids = HashSet::new();
        while let Some(id) = pending.pop() {
            if !ids.insert(id) {
                continue;
            }
            if let Some(folder) = self.folder(id) {
                pending.extend(folder.items.iter().filter_map(|i| {
                    if let PinnedItem::Folder(id) = i {
                        Some(*id)
                    } else {
                        None
                    }
                }));
            }
        }
        ids
    }
    pub fn archive_folder(&mut self, root: FolderId, now: SystemTime) -> Option<TabId> {
        let tabs = self.folder_tabs(root).into_iter().collect();
        let folders = self.folder_tree_ids(root);
        let space = self.folder_space(root)?;
        let profile = self.spaces.iter().find(|s| s.id == space)?.user_profile_id;
        let anchor = self.archive_folder_unrecorded(root, now)?;
        self.sidebar_history
            .record_folder_archive(FolderArchiveEdit {
                anchor,
                root,
                space,
                profile,
                tabs,
                folders,
            });
        Some(anchor)
    }
    fn archive_folder_unrecorded(&mut self, root: FolderId, now: SystemTime) -> Option<TabId> {
        if !self.can_edit_persistent_sidebar() || self.folder_space(root) != Some(self.space().id) {
            return None;
        }
        let ids = self.folder_tabs(root);
        let anchor = *ids.first()?;
        let tree = self.folder_tree_ids(root);
        let space = self.space();
        let parent = space
            .folders
            .iter()
            .find(|f| f.items.contains(&PinnedItem::Folder(root)))
            .map(|f| f.id);
        let order = parent
            .and_then(|id| self.folder(id))
            .map_or_else(|| space.pinned_items.clone(), |f| f.items.clone());
        let mut tabs: Vec<_> = space
            .pinned
            .iter()
            .filter(|t| ids.contains(&t.id))
            .cloned()
            .collect();
        let first = tabs.iter().position(|t| t.id == anchor)?;
        let tab = tabs.remove(first);
        let folder = ArchivedFolder {
            root,
            parent,
            order,
            pinned_order: space.pinned.iter().map(|t| t.id).collect(),
            folders: space
                .folders
                .iter()
                .filter(|f| tree.contains(&f.id))
                .cloned()
                .collect(),
            tabs,
            splits: space
                .splits
                .iter()
                .filter(|s| ids.contains(&s.anchor))
                .cloned()
                .collect(),
            selected: space.active_tab.filter(|id| ids.contains(id)),
        };
        if !folder.valid(anchor) {
            return None;
        }
        let entry = ArchivedTab {
            folder: Some(folder),
            origin: ArchiveOrigin::Space,
            tab,
            space: space.id,
            user_profile_id: space.user_profile_id,
            archived_at: now,
            reason: ArchiveReason::Manual,
            split: None,
        };
        let space = self.space_mut();
        space.pinned.retain(|t| !ids.contains(&t.id));
        space.splits.retain(|s| !ids.contains(&s.anchor));
        space.folders.retain(|f| !tree.contains(&f.id));
        for items in std::iter::once(&mut space.pinned_items)
            .chain(space.folders.iter_mut().map(|f| &mut f.items))
        {
            items.retain(|i| *i != PinnedItem::Folder(root));
        }
        if space.active_tab.is_some_and(|id| ids.contains(&id)) {
            space.active_tab = space
                .today
                .first()
                .or_else(|| space.pinned.first())
                .map(|t| t.id);
        }
        self.archive.push(entry);
        self.repair_folders();
        Some(anchor)
    }
    pub(crate) fn can_restore_folder_entry(&self, entry: &ArchivedTab, target: usize) -> bool {
        let Some(saved) = &entry.folder else {
            return true;
        };
        if !self.can_edit_persistent_sidebar()
            || self.spaces[target].user_profile_id != entry.user_profile_id
            || !saved.valid(entry.tab.id)
        {
            return false;
        }
        let ids: HashSet<_> = std::iter::once(entry.tab.id)
            .chain(saved.tabs.iter().map(|t| t.id))
            .collect();
        !self.all_tabs().any(|t| ids.contains(&t.id))
            && !self
                .spaces
                .iter()
                .flat_map(|s| &s.folders)
                .any(|f| saved.folders.iter().any(|g| g.id == f.id))
            && !self
                .spaces
                .iter()
                .flat_map(|s| &s.splits)
                .chain(self.profiles.values().flat_map(|p| &p.favorite_splits))
                .any(|s| saved.splits.iter().any(|g| g.id == s.id))
    }
    pub(crate) fn restore_folder_archive_entry(
        &mut self,
        mut entry: ArchivedTab,
        now: SystemTime,
        target: usize,
    ) -> Option<TabId> {
        let saved = entry.folder.take()?;
        let id = entry.tab.id;
        let mut tabs = vec![entry.tab];
        tabs.extend(saved.tabs);
        for tab in &mut tabs {
            tab.last_active = now;
        }
        let space = &mut self.spaces[target];
        for tab in tabs {
            let at = if space.id == entry.space {
                crate::undo::insertion_index(
                    &space.pinned.iter().map(|t| t.id).collect::<Vec<_>>(),
                    &saved.pinned_order,
                    tab.id,
                )
            } else {
                space.pinned.len()
            };
            space.pinned.insert(at, tab);
        }
        space.splits.extend(saved.splits);
        space.folders.extend(saved.folders);
        let parent = saved
            .parent
            .filter(|id| space.folders.iter().any(|f| f.id == *id));
        let items = entries_mut(space, parent).unwrap();
        let item = PinnedItem::Folder(saved.root);
        let at = if parent == saved.parent {
            crate::undo::insertion_index(items, &saved.order, item)
        } else {
            items.len()
        };
        items.insert(at, item);
        space.active_tab = saved.selected.or(Some(id));
        self.active_space = target;
        self.repair_folders();
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session, splits::Placement};
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;
    fn fixture() -> (Browser, FolderId, FolderId, TabId, TabId) {
        let mut b = Browser::new();
        let root = b.create_folder(b.space().id, None, "Archive tree").unwrap();
        let child = b.create_folder(b.space().id, Some(root), "Nested").unwrap();
        let a = b.new_tab("https://a.test", NOW);
        let c = b.new_tab("https://b.test", NOW);
        b.rename_tab(a, "Saved page");
        b.add_split(a, c, Placement::Right, NOW).unwrap();
        b.move_item_to_folder(a, child);
        b.rename_tab(a, "Saved name");
        b.set_tab_muted(a, true);
        b.clear_sidebar_history();
        (b, root, child, a, c)
    }
    #[test]
    fn folder_archive_round_trips_tree_splits_metadata_and_selection_without_overwriting_new_content(
    ) {
        let (mut b, root, child, a, c) = fixture();
        assert_eq!(b.archive_folder(root, NOW), Some(a));
        assert_eq!(b.archive.len(), 1);
        assert!(b.folder(root).is_none());
        let mut b = session::from_json(&session::to_json(&b)).unwrap();
        let fresh = b.new_tab("https://new.test", NOW);
        assert_eq!(b.restore_archived(a, NOW), Some(a));
        assert!(b.archive.is_empty());
        assert_eq!(b.folder(child).unwrap().name, "Nested");
        assert_eq!(b.folder_tabs(root), vec![a, c]);
        assert_eq!(b.split_for(a).unwrap().panes, vec![a, c]);
        assert!(b.all_tabs().find(|t| t.id == a).unwrap().muted);
        assert_eq!(
            b.all_tabs()
                .find(|t| t.id == a)
                .unwrap()
                .custom_title
                .as_deref(),
            Some("Saved page")
        );
        assert!(b.all_tabs().any(|t| t.id == fresh));
    }
    #[test]
    fn folder_toast_undo_is_exact_and_redo_rejects_structural_changes() {
        let (mut b, root, _, a, _) = fixture();
        b.archive_folder(root, NOW).unwrap();
        let token = b.archive_undo_items().unwrap();
        assert_eq!(token, vec![a]);
        assert!(b.undo_archive_items(&token, NOW));
        assert!(b.redo_sidebar(NOW));
        assert!(b.undo_sidebar(NOW));
        let added = b.new_tab("https://new.test", NOW);
        b.move_item_to_folder(added, root);
        assert!(!b.redo_sidebar(NOW));
        assert!(b.folder(root).is_some());
    }
    #[test]
    fn saved_folder_ids_are_reserved_and_a_deleted_archive_cannot_be_undone() {
        let (mut b, root, _, a, _) = fixture();
        b.archive_folder(root, NOW).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&b)).unwrap();
        json["next_id"] = 0.into();
        let mut loaded = session::from_json(&json.to_string()).unwrap();
        let saved: HashSet<_> = std::iter::once(a.get())
            .chain(loaded.archive[0].folder.as_ref().unwrap().all_ids())
            .collect();
        let new = loaded
            .create_folder(loaded.space().id, None, "New folder")
            .unwrap();
        let page = loaded.new_tab("https://new.test", NOW);
        assert!(!saved.contains(&new.get()));
        assert!(!saved.contains(&page.get()));
        assert_eq!(loaded.restore_archived(a, NOW), Some(a));
        let token = b.archive_undo_items().unwrap();
        b.delete_archived(a);
        assert!(!b.undo_archive_items(&token, NOW));
        assert!(b.folder(root).is_none());
    }
    #[test]
    fn deleted_or_reassigned_destination_never_crosses_profiles_and_corrupt_payload_is_retained() {
        let (mut b, root, _, a, _) = fixture();
        b.archive_folder(root, NOW);
        let profile = b.active_user_profile_id();
        let other = b.create_user_profile("Other").unwrap();
        b.spaces[0].user_profile_id = other;
        assert_eq!(b.restore_archived(a, NOW), None);
        assert_eq!(b.archive.len(), 1);
        b.spaces[0].user_profile_id = profile;
        b.archive[0].folder.as_mut().unwrap().folders.clear();
        assert_eq!(b.restore_archived(a, NOW), None);
        assert_eq!(b.archive.len(), 1);
    }
}
