//! Repair global tab identities without coalescing identical URLs or profiles.
use crate::{Browser, Tab, TabId, folders::PinnedItem, splits::SplitView};
use std::collections::{HashMap, HashSet};

type Mapping = HashMap<TabId, TabId>;
fn pages(tabs: &mut [Tab], used: &mut HashSet<u64>, next: &mut u64, mapping: &mut Mapping) {
    for tab in tabs {
        let old = tab.id;
        if !used.insert(old.get()) {
            tab.id = TabId(*next);
            *next += 1;
            used.insert(tab.id.get());
        }
        // Within an already ambiguous container, references retain its first
        // occurrence. Cross-container collisions map to the new local identity.
        mapping.entry(old).or_insert(tab.id);
    }
}
fn mapped(mapping: &Mapping, id: TabId) -> TabId {
    mapping.get(&id).copied().unwrap_or(id)
}
fn views(splits: &mut [SplitView], mapping: &Mapping) {
    for split in splits {
        split.anchor = mapped(mapping, split.anchor);
        split.active = mapped(mapping, split.active);
        for pane in &mut split.panes {
            *pane = mapped(mapping, *pane);
        }
    }
}
fn items(items: &mut [PinnedItem], mapping: &Mapping) {
    for item in items {
        if let PinnedItem::Tab(id) = item {
            *id = mapped(mapping, *id);
        }
    }
}
impl Browser {
    pub(crate) fn repair_profile_tab_ids(&mut self) {
        let mut used: HashSet<_> = self.spaces.iter().map(|space| space.id.0).collect();
        let mut next = self.next_id;
        for profile in self.profiles.values_mut() {
            let mut mapping = Mapping::new();
            pages(&mut profile.favorites, &mut used, &mut next, &mut mapping);
            views(&mut profile.favorite_splits, &mapping);
            for space in self
                .spaces
                .iter_mut()
                .filter(|space| space.user_profile_id == profile.id)
            {
                space.active_tab = space.active_tab.map(|id| mapped(&mapping, id));
            }
        }
        for space in &mut self.spaces {
            let mut mapping = Mapping::new();
            pages(&mut space.pinned, &mut used, &mut next, &mut mapping);
            pages(&mut space.today, &mut used, &mut next, &mut mapping);
            views(&mut space.splits, &mapping);
            items(&mut space.pinned_items, &mapping);
            for folder in &mut space.folders {
                items(&mut folder.items, &mapping);
            }
            space.active_tab = space.active_tab.map(|id| mapped(&mapping, id));
        }
        for entry in &mut self.archive {
            let mut mapping = Mapping::new();
            pages(
                std::slice::from_mut(&mut entry.tab),
                &mut used,
                &mut next,
                &mut mapping,
            );
            if let Some(saved) = &mut entry.folder {
                pages(&mut saved.tabs, &mut used, &mut next, &mut mapping);
                for folder in &mut saved.folders { items(&mut folder.items, &mapping); }
                views(&mut saved.splits, &mapping);
                for id in &mut saved.pinned_order { *id=mapped(&mapping,*id); }
                saved.selected = saved.selected.map(|id| mapped(&mapping,id));
            }
            if let Some(saved) = &mut entry.split {
                pages(&mut saved.panes, &mut used, &mut next, &mut mapping);
                views(std::slice::from_mut(&mut saved.view), &mapping);
                if !used.insert(saved.view.id.get()) {
                    saved.view.id = crate::splits::SplitId(next);
                    next += 1;
                    used.insert(saved.view.id.get());
                }
            }
        }
        self.next_id = next;
    }
}
