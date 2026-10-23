//! Folder duplication clones model state only; page engines remain lazy.
use super::*;
use std::time::SystemTime;

impl Browser {
    /// Copies a complete subtree beside its source. All identities are fresh,
    /// while names, icons, Pinned URLs, pane geometry and order are preserved.
    /// The currently selected page and the source's disclosure state stay put.
    pub fn duplicate_folder(&mut self, id: FolderId, now: SystemTime) -> Option<FolderId> {
        let index = self
            .spaces
            .iter()
            .position(|space| space.folders.iter().any(|folder| folder.id == id))?;
        self.repair_folders();
        let descendants = subtree_folders(&self.spaces[index], id);
        let page_ids: HashSet<_> = self.folder_tabs(id).into_iter().collect();
        let mut folders: Vec<_> = self.spaces[index]
            .folders
            .iter()
            .filter(|folder| descendants.contains(&folder.id))
            .cloned()
            .collect();
        let mut pages: Vec<_> = self.spaces[index]
            .pinned
            .iter()
            .filter(|tab| page_ids.contains(&tab.id))
            .cloned()
            .collect();
        let mut splits: Vec<_> = self.spaces[index]
            .splits
            .iter()
            .filter(|split| page_ids.contains(&split.anchor))
            .cloned()
            .collect();
        let mut items = HashMap::new();
        let mut tabs = HashMap::new();
        for page in &mut pages {
            let fresh = TabId(self.next_id());
            tabs.insert(page.id, fresh);
            items.insert(PinnedItem::Tab(page.id), PinnedItem::Tab(fresh));
            page.id = fresh;
            page.last_active = now;
        }
        for split in &mut splits {
            let fresh = SplitId(self.next_id());
            items.insert(PinnedItem::Split(split.id), PinnedItem::Split(fresh));
            split.id = fresh;
            split.anchor = tabs[&split.anchor];
            split.active = tabs[&split.active];
            split.panes = split.panes.iter().map(|id| tabs[id]).collect();
        }
        for folder in &mut folders {
            let fresh = FolderId(self.next_id());
            items.insert(PinnedItem::Folder(folder.id), PinnedItem::Folder(fresh));
            folder.id = fresh;
        }
        for folder in &mut folders {
            folder.items = folder.items.iter().map(|item| items[item]).collect();
        }
        let copy = items[&PinnedItem::Folder(id)];
        self.spaces[index].pinned.extend(pages);
        self.spaces[index].splits.extend(splits);
        self.spaces[index].folders.extend(folders);
        self.replace_pinned_item(PinnedItem::Folder(id), &[PinnedItem::Folder(id), copy]);
        self.repair_folders();
        let PinnedItem::Folder(copy) = copy else {
            unreachable!()
        };
        Some(copy)
    }
}
