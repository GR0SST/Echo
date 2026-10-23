//! Ordered Pinned folders refer to existing pages and Split View items.
//! The physical page pool stays flat, so grouping never reloads or clones pages.
use crate::{splits::SplitId, Browser, Section, Space, SpaceId, TabId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_FOLDER_DEPTH: usize = 32;

mod copy;
mod convert;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FolderId(u64);
impl FolderId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum PinnedItem {
    Tab(TabId),
    Split(SplitId),
    Folder(FolderId),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub custom_icon: Option<String>,
    #[serde(default)]
    pub items: Vec<PinnedItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinnedRow {
    pub item: PinnedItem,
    pub depth: usize,
}

impl Browser {
    pub fn folder(&self, id: FolderId) -> Option<&Folder> {
        self.spaces
            .iter()
            .flat_map(|space| &space.folders)
            .find(|folder| folder.id == id)
    }

    pub fn folder_space(&self, id: FolderId) -> Option<SpaceId> {
        self.spaces
            .iter()
            .find(|space| space.folders.iter().any(|folder| folder.id == id))
            .map(|space| space.id)
    }

    pub fn pinned_item(&self, tab: TabId) -> PinnedItem {
        self.split_for(tab)
            .map_or(PinnedItem::Tab(tab), |split| PinnedItem::Split(split.id))
    }

    pub fn pinned_item_tab(&self, space: SpaceId, item: PinnedItem) -> Option<TabId> {
        let space = self.spaces.iter().find(|entry| entry.id == space)?;
        item_tab(space, item)
    }

    /// Folder headers are rows; a focused page may peek under a closed folder.
    pub fn pinned_rows(&self, space: SpaceId, include_hidden: bool) -> Vec<PinnedRow> {
        let Some(space) = self.spaces.iter().find(|entry| entry.id == space) else {
            return Vec::new();
        };
        let active = space.active_tab.map(|tab| self.pinned_item(tab));
        let mut rows = Vec::new();
        rows_for(
            space,
            &space.pinned_items,
            0,
            include_hidden,
            active,
            &mut rows,
        );
        rows
    }

    pub fn folder_ancestors(&self, tab: TabId) -> Vec<FolderId> {
        let item = self.pinned_item(tab);
        for space in &self.spaces {
            if let Some(path) = find_path(space, &space.pinned_items, item, &mut Vec::new(), 0) {
                return path;
            }
        }
        Vec::new()
    }

    pub fn folder_tabs(&self, id: FolderId) -> Vec<TabId> {
        let Some(space) = self.spaces.iter().find(|space| space.folders.iter().any(|folder| folder.id == id)) else { return Vec::new(); };
        let mut items = Vec::new();
        all_items(space, &[PinnedItem::Folder(id)], 0, &mut items);
        items.into_iter().flat_map(|item| match item {
            PinnedItem::Tab(id) => vec![id],
            PinnedItem::Split(id) => space.splits.iter().find(|split| split.id == id).map(|split| split.panes.clone()).unwrap_or_default(),
            PinnedItem::Folder(_) => Vec::new(),
        }).collect()
    }

    pub fn folder_path(&self, id: FolderId) -> String {
        let Some(space) = self
            .spaces
            .iter()
            .find(|space| space.folders.iter().any(|folder| folder.id == id))
        else {
            return String::new();
        };
        let mut path = find_path(
            space,
            &space.pinned_items,
            PinnedItem::Folder(id),
            &mut Vec::new(),
            0,
        )
        .unwrap_or_default();
        path.push(id);
        path.iter()
            .filter_map(|id| space.folders.iter().find(|folder| folder.id == *id))
            .map(|folder| folder.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    }

    pub fn create_folder(
        &mut self,
        space: SpaceId,
        parent: Option<FolderId>,
        name: &str,
    ) -> Option<FolderId> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let index = self.spaces.iter().position(|entry| entry.id == space)?;
        if let Some(parent) = parent {
            if self.folder_space(parent) != Some(space) {
                return None;
            }
            let depth = find_path(
                &self.spaces[index],
                &self.spaces[index].pinned_items,
                PinnedItem::Folder(parent),
                &mut Vec::new(),
                0,
            )?
            .len()
                + 1;
            if depth >= MAX_FOLDER_DEPTH {
                return None;
            }
        }
        self.repair_folders();
        let id = FolderId(self.next_id());
        self.spaces[index].folders.push(Folder {
            id,
            name: name.to_string(),
            collapsed: false,
            custom_icon: None,
            items: Vec::new(),
        });
        entries_mut(&mut self.spaces[index], parent)?.push(PinnedItem::Folder(id));
        self.spaces[index].pinned_collapsed = false;
        Some(id)
    }

    pub fn rename_folder(&mut self, id: FolderId, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(folder) = self
            .spaces
            .iter_mut()
            .flat_map(|space| &mut space.folders)
            .find(|folder| folder.id == id)
        else {
            return false;
        };
        folder.name = name.to_string();
        true
    }

    pub fn set_folder_collapsed(&mut self, id: FolderId, collapsed: bool) -> bool {
        let Some(folder) = self
            .spaces
            .iter_mut()
            .flat_map(|space| &mut space.folders)
            .find(|folder| folder.id == id)
        else {
            return false;
        };
        let changed = folder.collapsed != collapsed;
        folder.collapsed = collapsed;
        changed
    }

    pub fn set_all_folders_collapsed(&mut self, space: SpaceId, collapsed: bool) -> usize {
        let Some(space) = self.spaces.iter_mut().find(|entry| entry.id == space) else {
            return 0;
        };
        let mut changed = 0;
        for folder in &mut space.folders {
            if folder.collapsed != collapsed {
                folder.collapsed = collapsed;
                changed += 1;
            }
        }
        changed
    }

    pub fn delete_empty_folder(&mut self, id: FolderId) -> bool {
        let Some(index) = self.spaces.iter().position(|space| {
            space
                .folders
                .iter()
                .any(|folder| folder.id == id && folder.items.is_empty())
        }) else {
            return false;
        };
        remove_item(&mut self.spaces[index], PinnedItem::Folder(id));
        self.spaces[index].folders.retain(|folder| folder.id != id);
        true
    }

    /// Pins the whole item in the destination Space, without expanding a closed
    /// destination folder. The caller chooses whether to focus its page.
    pub fn move_item_to_folder(&mut self, tab: TabId, folder: FolderId) -> bool {
        let Some(space) = self.folder_space(folder) else {
            return false;
        };
        if !self.all_tabs().any(|entry| entry.id == tab) {
            return false;
        }
        self.repair_folders();
        let item = self.pinned_item(tab);
        if self.folder_ancestors(tab).last() == Some(&folder) {
            return false;
        }
        self.move_tab(tab, space, Section::Pinned);
        let index = self
            .spaces
            .iter()
            .position(|entry| entry.id == space)
            .unwrap();
        if !self
            .related_tabs(tab)
            .iter()
            .all(|id| self.spaces[index].pinned.iter().any(|page| page.id == *id))
        {
            return false;
        }
        for space in &mut self.spaces {
            remove_item(space, item);
        }
        self.spaces[index]
            .folders
            .iter_mut()
            .find(|entry| entry.id == folder)
            .unwrap()
            .items
            .push(item);
        self.spaces[index].pinned_collapsed = false;
        self.repair_folders();
        true
    }

    pub fn move_item_to_pinned_root(&mut self, tab: TabId, space: SpaceId) -> bool {
        if !self.spaces.iter().any(|entry| entry.id == space)
            || !self.all_tabs().any(|entry| entry.id == tab)
        {
            return false;
        }
        self.repair_folders();
        let item = self.pinned_item(tab);
        self.move_tab(tab, space, Section::Pinned);
        for space in &mut self.spaces {
            remove_item(space, item);
        }
        let space = self
            .spaces
            .iter_mut()
            .find(|entry| entry.id == space)
            .unwrap();
        space.pinned_items.push(item);
        space.pinned_collapsed = false;
        self.repair_folders();
        true
    }

    /// Moves a complete subtree. Pages and Split Views keep their identities,
    /// original URLs and loaded engine views, including across Spaces.
    pub fn can_move_folder(&self, id: FolderId, destination: SpaceId, parent: Option<FolderId>) -> bool {
        let Some(source) = self
            .spaces
            .iter()
            .position(|space| space.folders.iter().any(|folder| folder.id == id))
        else {
            return false;
        };
        let Some(target) = self.spaces.iter().position(|space| space.id == destination) else {
            return false;
        };
        if parent.is_some_and(|parent| self.folder_space(parent) != Some(destination)) {
            return false;
        }
        let descendants = subtree_folders(&self.spaces[source], id);
        if parent.is_some_and(|parent| descendants.contains(&parent)) {
            return false;
        }
        let parent_depth = parent.map_or(0, |parent| {
            find_path(
                &self.spaces[target],
                &self.spaces[target].pinned_items,
                PinnedItem::Folder(parent),
                &mut Vec::new(),
                0,
            )
            .map_or(MAX_FOLDER_DEPTH, |path| path.len() + 1)
        });
        let subtree_depth = folder_depth(&self.spaces[source], id, 0);
        if parent_depth + subtree_depth > MAX_FOLDER_DEPTH {
            return false;
        }
        true
    }

    pub fn move_folder(
        &mut self,
        id: FolderId,
        destination: SpaceId,
        parent: Option<FolderId>,
    ) -> bool {
        if !self.can_move_folder(id, destination, parent) { return false; }
        let source = self.spaces.iter().position(|space| space.folders.iter().any(|folder| folder.id == id)).unwrap();
        let target = self.spaces.iter().position(|space| space.id == destination).unwrap();
        let descendants = subtree_folders(&self.spaces[source], id);
        self.repair_folders();
        remove_item(&mut self.spaces[source], PinnedItem::Folder(id));
        if source != target {
            let mut items = Vec::new();
            all_items(
                &self.spaces[source],
                &[PinnedItem::Folder(id)],
                0,
                &mut items,
            );
            let pages: HashSet<_> = items
                .iter()
                .flat_map(|item| match *item {
                    PinnedItem::Tab(id) => vec![id],
                    PinnedItem::Split(id) => self.spaces[source]
                        .splits
                        .iter()
                        .find(|split| split.id == id)
                        .map(|split| split.panes.clone())
                        .unwrap_or_default(),
                    PinnedItem::Folder(_) => Vec::new(),
                })
                .collect();
            let moved_pages = extract(&mut self.spaces[source].pinned, |tab| {
                pages.contains(&tab.id)
            });
            let moved_splits = extract(&mut self.spaces[source].splits, |split| {
                split.panes.iter().all(|id| pages.contains(id))
            });
            let moved_folders = extract(&mut self.spaces[source].folders, |folder| {
                descendants.contains(&folder.id)
            });
            let selected = self.spaces[source]
                .active_tab
                .is_some_and(|id| pages.contains(&id));
            if selected {
                let next = self.spaces[source].tabs().next().map(|tab| tab.id);
                self.spaces[source].active_tab = next;
            }
            self.spaces[target].pinned.extend(moved_pages);
            self.spaces[target].splits.extend(moved_splits);
            self.spaces[target].folders.extend(moved_folders);
        }
        entries_mut(&mut self.spaces[target], parent)
            .unwrap()
            .push(PinnedItem::Folder(id));
        self.spaces[target].pinned_collapsed = false;
        self.repair_folders();
        true
    }

    /// Removes stale placements before an item leaves Pinned, so pinning the
    /// same page again puts it at the root instead of reviving old membership.
    pub(crate) fn forget_pinned_item(&mut self, tab: TabId) {
        let item = self.pinned_item(tab);
        for space in &mut self.spaces {
            remove_item(space, item);
        }
    }

    pub(crate) fn replace_pinned_item(&mut self, item: PinnedItem, replacement: &[PinnedItem]) {
        for space in &mut self.spaces {
            replace_in(&mut space.pinned_items, item, replacement);
            for folder in &mut space.folders {
                replace_in(&mut folder.items, item, replacement);
            }
        }
    }

    pub(crate) fn insert_pinned_copy(&mut self, source: PinnedItem, copy: PinnedItem) {
        self.repair_folders();
        for space in &mut self.spaces {
            remove_item(space, copy);
        }
        self.replace_pinned_item(source, &[source, copy]);
        self.repair_folders();
    }

    pub(crate) fn folder_max_id(&self) -> u64 {
        self.spaces
            .iter()
            .flat_map(|space| &space.folders)
            .map(|folder| folder.id.0)
            .max()
            .unwrap_or(0)
    }

    /// Repairs references, cycles, duplicate IDs and orphan folders without
    /// removing a physical page. Unlisted Pinned items are appended at the root.
    pub(crate) fn repair_folders(&mut self) {
        let mut used: HashSet<u64> = self
            .spaces
            .iter()
            .map(|space| space.id.0)
            .chain(self.all_tabs().map(|tab| tab.id.0))
            .chain(self.archive.iter().map(|entry| entry.tab.id.0))
            .chain(
                self.spaces
                    .iter()
                    .flat_map(|space| &space.splits)
                    .chain(&self.favorite_splits)
                    .map(|split| split.id.get()),
            )
            .collect();
        for entry in &self.archive {
            if let Some(split) = &entry.split {
                used.insert(split.view.id.get());
                used.extend(split.panes.iter().map(|tab| tab.id.0));
            }
        }
        let mut next = self.next_id.max(self.folder_max_id().saturating_add(1));
        for space in &mut self.spaces {
            let mut remap = HashMap::new();
            for folder in &mut space.folders {
                let original = folder.id;
                if !used.insert(folder.id.0) {
                    folder.id = FolderId(next);
                    next += 1;
                    used.insert(folder.id.0);
                }
                remap.entry(original).or_insert(folder.id);
                if folder.name.trim().is_empty() {
                    folder.name = "Folder".into();
                }
            }
            for item in space.pinned_items.iter_mut().chain(
                space
                    .folders
                    .iter_mut()
                    .flat_map(|folder| &mut folder.items),
            ) {
                if let PinnedItem::Folder(id) = item {
                    if let Some(repaired) = remap.get(id) {
                        *id = *repaired;
                    }
                }
            }
            normalize_space(space);
        }
        self.next_id = next;
    }
}

fn entries_mut(space: &mut Space, parent: Option<FolderId>) -> Option<&mut Vec<PinnedItem>> {
    match parent {
        None => Some(&mut space.pinned_items),
        Some(parent) => space
            .folders
            .iter_mut()
            .find(|folder| folder.id == parent)
            .map(|folder| &mut folder.items),
    }
}
fn remove_item(space: &mut Space, item: PinnedItem) {
    space.pinned_items.retain(|entry| *entry != item);
    for folder in &mut space.folders {
        folder.items.retain(|entry| *entry != item);
    }
}
fn replace_in(items: &mut Vec<PinnedItem>, item: PinnedItem, replacement: &[PinnedItem]) {
    if let Some(index) = items.iter().position(|entry| *entry == item) {
        items.splice(index..index + 1, replacement.iter().copied());
    }
}
fn extract<T>(items: &mut Vec<T>, mut predicate: impl FnMut(&T) -> bool) -> Vec<T> {
    let mut extracted = Vec::new();
    let mut kept = Vec::new();
    for item in items.drain(..) {
        if predicate(&item) {
            extracted.push(item);
        } else {
            kept.push(item);
        }
    }
    *items = kept;
    extracted
}
fn item_tab(space: &Space, item: PinnedItem) -> Option<TabId> {
    match item {
        PinnedItem::Tab(id) => space.pinned.iter().any(|tab| tab.id == id).then_some(id),
        PinnedItem::Split(id) => space
            .splits
            .iter()
            .find(|split| split.id == id && space.pinned.iter().any(|tab| tab.id == split.anchor))
            .map(|split| split.anchor),
        PinnedItem::Folder(_) => None,
    }
}
fn rows_for(
    space: &Space,
    items: &[PinnedItem],
    depth: usize,
    hidden: bool,
    active: Option<PinnedItem>,
    rows: &mut Vec<PinnedRow>,
) {
    if depth > MAX_FOLDER_DEPTH {
        return;
    }
    for item in items {
        rows.push(PinnedRow { item: *item, depth });
        if let PinnedItem::Folder(id) = item {
            if let Some(folder) = space.folders.iter().find(|folder| folder.id == *id) {
                if hidden || !folder.collapsed {
                    rows_for(space, &folder.items, depth + 1, hidden, active, rows);
                } else if let Some(active) = active {
                    let mut descendants = Vec::new();
                    all_items(space, &folder.items, depth + 1, &mut descendants);
                    if descendants.contains(&active) {
                        rows.push(PinnedRow {
                            item: active,
                            depth: depth + 1,
                        });
                    }
                }
            }
        }
    }
}
fn all_items(space: &Space, items: &[PinnedItem], depth: usize, output: &mut Vec<PinnedItem>) {
    if depth > MAX_FOLDER_DEPTH {
        return;
    }
    for item in items {
        output.push(*item);
        if let PinnedItem::Folder(id) = item {
            if let Some(folder) = space.folders.iter().find(|folder| folder.id == *id) {
                all_items(space, &folder.items, depth + 1, output);
            }
        }
    }
}
fn find_path(
    space: &Space,
    items: &[PinnedItem],
    target: PinnedItem,
    path: &mut Vec<FolderId>,
    depth: usize,
) -> Option<Vec<FolderId>> {
    if depth > MAX_FOLDER_DEPTH {
        return None;
    }
    for item in items {
        if *item == target {
            return Some(path.clone());
        }
        if let PinnedItem::Folder(id) = item {
            if let Some(folder) = space.folders.iter().find(|folder| folder.id == *id) {
                path.push(*id);
                if let Some(found) = find_path(space, &folder.items, target, path, depth + 1) {
                    return Some(found);
                }
                path.pop();
            }
        }
    }
    None
}
fn subtree_folders(space: &Space, id: FolderId) -> HashSet<FolderId> {
    let mut items = Vec::new();
    all_items(space, &[PinnedItem::Folder(id)], 0, &mut items);
    items
        .into_iter()
        .filter_map(|item| {
            if let PinnedItem::Folder(id) = item {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}
fn folder_depth(space: &Space, id: FolderId, depth: usize) -> usize {
    if depth >= MAX_FOLDER_DEPTH {
        return MAX_FOLDER_DEPTH;
    }
    let Some(folder) = space.folders.iter().find(|folder| folder.id == id) else {
        return 0;
    };
    1 + folder
        .items
        .iter()
        .filter_map(|item| {
            if let PinnedItem::Folder(id) = item {
                Some(folder_depth(space, *id, depth + 1))
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0)
}

fn normalize_space(space: &mut Space) {
    let mut canonical = HashMap::new();
    let mut expected = Vec::new();
    let mut expected_seen = HashSet::new();
    for tab in &space.pinned {
        let item = if let Some(split) = space
            .splits
            .iter()
            .find(|split| split.panes.contains(&tab.id))
        {
            if split.anchor != tab.id {
                continue;
            }
            PinnedItem::Split(split.id)
        } else {
            PinnedItem::Tab(tab.id)
        };
        canonical.insert(tab.id, item);
        if expected_seen.insert(item) {
            expected.push(item);
        }
    }
    let children: HashMap<_, _> = space
        .folders
        .iter()
        .map(|folder| (folder.id, folder.items.clone()))
        .collect();
    let mut used = HashSet::new();
    let mut repaired = HashMap::new();
    let mut roots = normalize_items(
        &space.pinned_items,
        0,
        &canonical,
        &expected_seen,
        &children,
        &mut used,
        &mut repaired,
    );
    for folder in &space.folders {
        if !used.contains(&PinnedItem::Folder(folder.id)) {
            roots.extend(normalize_items(
                &[PinnedItem::Folder(folder.id)],
                0,
                &canonical,
                &expected_seen,
                &children,
                &mut used,
                &mut repaired,
            ));
        }
    }
    for item in expected {
        if used.insert(item) {
            roots.push(item);
        }
    }
    space.pinned_items = roots;
    for folder in &mut space.folders {
        folder.items = repaired.remove(&folder.id).unwrap_or_default();
    }
    let mut items = Vec::new();
    all_items(space, &space.pinned_items, 0, &mut items);
    let mut ordered = Vec::new();
    for item in items {
        match item {
            PinnedItem::Tab(id) => ordered.push(id),
            PinnedItem::Split(id) => {
                if let Some(split) = space.splits.iter().find(|split| split.id == id) {
                    ordered.extend(&split.panes);
                }
            }
            PinnedItem::Folder(_) => {}
        }
    }
    let ranks: HashMap<_, _> = ordered
        .into_iter()
        .enumerate()
        .map(|(index, id)| (id, index))
        .collect();
    space
        .pinned
        .sort_by_key(|tab| ranks.get(&tab.id).copied().unwrap_or(usize::MAX));
}
fn normalize_items(
    items: &[PinnedItem],
    depth: usize,
    canonical: &HashMap<TabId, PinnedItem>,
    allowed: &HashSet<PinnedItem>,
    children: &HashMap<FolderId, Vec<PinnedItem>>,
    used: &mut HashSet<PinnedItem>,
    repaired: &mut HashMap<FolderId, Vec<PinnedItem>>,
) -> Vec<PinnedItem> {
    if depth > MAX_FOLDER_DEPTH {
        return Vec::new();
    }
    let mut output = Vec::new();
    for item in items {
        let item = match *item {
            PinnedItem::Tab(id) => {
                let Some(item) = canonical.get(&id) else {
                    continue;
                };
                *item
            }
            item => item,
        };
        if !matches!(item, PinnedItem::Folder(_)) && !allowed.contains(&item) {
            continue;
        }
        if let PinnedItem::Folder(id) = item {
            if depth >= MAX_FOLDER_DEPTH || !children.contains_key(&id) {
                continue;
            }
        }
        if !used.insert(item) {
            continue;
        }
        output.push(item);
        if let PinnedItem::Folder(id) = item {
            let normalized = normalize_items(
                &children[&id],
                depth + 1,
                canonical,
                allowed,
                children,
                used,
                repaired,
            );
            repaired.insert(id, normalized);
        }
    }
    output
}

#[cfg(test)]
mod tests;
