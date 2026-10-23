//! A folder becomes a Space by moving its contents, never copying pages.
use super::*;
use std::time::SystemTime;

impl Browser {
    /// Promotes the folder's children into the Pinned root of a new Space.
    /// Nested folders retain their identities, order and disclosure state.
    pub fn folder_into_space(&mut self, id: FolderId, now: SystemTime) -> Option<SpaceId> {
        if !self.can_edit_persistent_sidebar() { return None; }
        let source = self
            .spaces
            .iter()
            .position(|space| space.folders.iter().any(|folder| folder.id == id))?;
        self.repair_folders();
        let folder = self.folder(id)?.clone();
        let pages = self.folder_tabs(id);
        let selected = self.spaces[source]
            .active_tab
            .filter(|tab| pages.contains(tab))
            .or_else(|| {
                pages
                    .first()
                    .map(|tab| self.split_for(*tab).map_or(*tab, |split| split.active))
            });
        let space = self.add_space(folder.name, self.spaces[source].hue);
        let source_profile = self.spaces[source].user_profile_id;
        self.spaces.last_mut().unwrap().user_profile_id = source_profile;
        // The new empty Space always has room for the original subtree.
        assert!(self.move_folder(id, space, None));
        self.replace_pinned_item(PinnedItem::Folder(id), &folder.items);
        let index = self
            .spaces
            .iter()
            .position(|entry| entry.id == space)
            .unwrap();
        self.spaces[index].folders.retain(|folder| folder.id != id);
        self.spaces[index].custom_icon = folder.custom_icon;
        self.spaces[index].active_tab = selected;
        self.repair_folders();
        self.active_space = index;
        if let Some(selected) = selected {
            self.activate_tab(selected, now);
        }
        Some(space)
    }
}
