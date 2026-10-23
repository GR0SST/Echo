//! Local ordering operates on whole sidebar items rather than physical panes.
use super::*;

impl Browser {
    /// Places an existing item before/after a local target, adopting the
    /// target's Space, section and folder parent. Pages are never cloned.
    /// The caller chooses destination focus after a cross-section move.
    pub fn move_tab_relative(&mut self, tab: TabId, target: TabId, after: bool) -> bool {
        let tab = self.item_anchor(tab);
        let target = self.item_anchor(target);
        if tab == target || !self.all_tabs().any(|entry| entry.id == tab) {
            return false;
        }
        let Some((index, section)) = self.spaces.iter().enumerate().find_map(|(index, space)| {
            if space.pinned.iter().any(|entry| entry.id == target) {
                Some((index, Section::Pinned))
            } else if space.today.iter().any(|entry| entry.id == target) {
                Some((index, Section::Today))
            } else {
                None
            }
        }) else {
            return false;
        };
        let space = self.spaces[index].id;
        if section == Section::Pinned {
            let parent = self.folder_ancestors(target).last().copied();
            let same_parent = self.spaces[index]
                .pinned
                .iter()
                .any(|entry| entry.id == tab)
                && self.folder_ancestors(tab).last().copied() == parent;
            if !same_parent {
                let moved = match parent {
                    Some(parent) => self.move_item_to_folder(tab, parent),
                    None => self.move_item_to_pinned_root(tab, space),
                };
                if !moved {
                    return false;
                }
            }
            let source_item = self.pinned_item(tab);
            let target_item = self.pinned_item(target);
            remove_item(&mut self.spaces[index], source_item);
            let entries = entries_mut(&mut self.spaces[index], parent).unwrap();
            let position = entries
                .iter()
                .position(|item| *item == target_item)
                .unwrap();
            entries.insert(position + usize::from(after), source_item);
        } else {
            self.move_tab(tab, space, section);
            let source = self.related_tabs(tab);
            let target = self.related_tabs(target);
            let today = &mut self.spaces[index].today;
            let pages = extract(today, |tab| source.contains(&tab.id));
            let position = if after {
                today
                    .iter()
                    .rposition(|tab| target.contains(&tab.id))
                    .map(|index| index + 1)
            } else {
                today.iter().position(|tab| target.contains(&tab.id))
            }
            .unwrap();
            today.splice(position..position, pages);
        }
        self.repair_folders();
        true
    }
}
