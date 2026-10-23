//! Split View is one sidebar item containing independently focused web pages.
//! Page identities survive grouping, moving, pinning and archive restoration.
use crate::{neighbor, ArchiveReason, ArchivedTab, Browser, Closed, Section, Tab, TabId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

pub const MAX_SPLIT_PANES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SplitId(pub(crate) u64);
impl SplitId { pub fn get(self) -> u64 { self.0 } }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis { #[default] Horizontal, Vertical }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement { Left, Right, Above, Below }
impl Placement {
    fn axis(self) -> Axis { match self { Self::Left | Self::Right => Axis::Horizontal, _ => Axis::Vertical } }
    fn before(self) -> bool { matches!(self, Self::Left | Self::Above) }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SplitView {
    pub id: SplitId,
    /// Owns the sidebar item's position, independently of visual pane order.
    pub anchor: TabId,
    pub panes: Vec<TabId>,
    pub active: TabId,
    pub axis: Axis,
    #[serde(default)]
    pub weights: Vec<f32>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub custom_icon: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchivedSplit {
    pub view: SplitView,
    /// The anchor is stored in ArchivedTab.tab; the remaining pages live here.
    pub panes: Vec<Tab>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitError { UnknownTab, SameItem, AlreadyGrouped, LimitReached }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope { Favorites, Space(usize, Section) }

impl Browser {
    pub fn split_for(&self, tab: TabId) -> Option<&SplitView> {
        self.favorite_splits.iter().chain(self.spaces.iter().flat_map(|space| &space.splits))
            .find(|split| split.panes.contains(&tab))
    }

    pub fn active_split(&self) -> Option<&SplitView> { self.split_for(self.space().active_tab?) }
    pub fn item_anchor(&self, tab: TabId) -> TabId { self.split_for(tab).map_or(tab, |split| split.anchor) }
    pub fn is_split_secondary(&self, tab: TabId) -> bool { self.item_anchor(tab) != tab }
    pub fn related_tabs(&self, tab: TabId) -> Vec<TabId> { self.split_for(tab).map_or_else(|| vec![tab], |split| split.panes.clone()) }
    pub fn favorite_count(&self) -> usize { self.favorites.iter().filter(|tab| !self.is_split_secondary(tab.id)).count() }

    pub fn item_title(&self, tab: TabId) -> String {
        if let Some(split) = self.split_for(tab) {
            return split.title.clone().unwrap_or_else(|| split.panes.iter().filter_map(|id| self.all_tabs().find(|tab| tab.id == *id).map(|tab| tab.title.clone())).collect::<Vec<_>>().join(" | "));
        }
        self.all_tabs().find(|entry| entry.id == tab).map(|tab| tab.title.clone()).unwrap_or_default()
    }

    pub fn item_has_custom_name(&self, tab: TabId) -> bool {
        if let Some(split) = self.split_for(tab) { return split.title.is_some(); }
        self.all_tabs().find(|entry| entry.id == tab).is_some_and(|tab| tab.custom_title.is_some())
    }

    pub fn activate_sidebar_item(&mut self, tab: TabId, now: SystemTime) {
        let tab = self.split_for(tab).map_or(tab, |split| {
            self.space().active_tab.filter(|id| split.panes.contains(id)).unwrap_or(split.active)
        });
        self.activate_tab(tab, now);
    }

    fn split_mut(&mut self, tab: TabId) -> Option<&mut SplitView> {
        self.favorite_splits.iter_mut().chain(self.spaces.iter_mut().flat_map(|space| &mut space.splits))
            .find(|split| split.panes.contains(&tab))
    }

    fn tab_scope(&self, tab: TabId) -> Option<Scope> {
        if self.is_favorite(tab) { return Some(Scope::Favorites); }
        self.spaces.iter().enumerate().find_map(|(index, space)| space.find(tab).map(|(section, _)| Scope::Space(index, section)))
    }

    fn scope_tabs_mut(&mut self, scope: Scope) -> &mut Vec<Tab> {
        match scope { Scope::Favorites => &mut self.favorites, Scope::Space(index, section) => self.spaces[index].section_mut(section) }
    }

    fn take_tab(&mut self, tab: TabId, scope: Scope) -> Option<Tab> {
        match scope {
            Scope::Favorites => self.take_favorite(tab),
            Scope::Space(index, section) => {
                let space = &mut self.spaces[index];
                let position = space.section_mut(section).iter().position(|entry| entry.id == tab)?;
                let removed = space.section_mut(section).remove(position);
                if space.active_tab == Some(tab) { space.active_tab = neighbor(space, section, position); }
                Some(removed)
            }
        }
    }

    fn place_tab(&mut self, scope: Scope, mut tab: Tab) {
        if matches!(scope, Scope::Space(_, Section::Today)) {
            tab.pinned_url = None;
            tab.pinned_title = None;
        } else if tab.pinned_url.is_none() {
            tab.pinned_url = Some(tab.url.clone());
            tab.pinned_title = tab.page_title.clone().or_else(|| Some(tab.title.clone()));
        }
        match scope {
            Scope::Space(index, Section::Today) => self.spaces[index].today.insert(0, tab),
            Scope::Space(index, Section::Pinned) => { self.spaces[index].pinned.push(tab); self.spaces[index].pinned_collapsed = false; }
            Scope::Favorites => self.favorites.push(tab),
        }
    }

    fn take_split(&mut self, tab: TabId) -> Option<(Scope, SplitView)> {
        let scope = self.tab_scope(tab)?;
        let views = match scope { Scope::Favorites => &mut self.favorite_splits, Scope::Space(index, _) => &mut self.spaces[index].splits };
        let index = views.iter().position(|split| split.panes.contains(&tab))?;
        Some((scope, views.remove(index)))
    }

    fn put_split(&mut self, scope: Scope, view: SplitView) {
        match scope { Scope::Favorites => self.favorite_splits.push(view), Scope::Space(index, _) => self.spaces[index].splits.push(view) }
    }

    fn arrange_split_tabs(&mut self, scope: Scope, split: &SplitView) {
        let tabs = self.scope_tabs_mut(scope);
        let start = tabs.iter().position(|tab| split.panes.contains(&tab.id)).unwrap_or(tabs.len());
        let mut grouped = HashMap::new();
        let mut index = 0;
        while index < tabs.len() {
            if split.panes.contains(&tabs[index].id) { let tab = tabs.remove(index); grouped.insert(tab.id, tab); }
            else { index += 1; }
        }
        for (offset, id) in split.panes.iter().enumerate() {
            if let Some(tab) = grouped.remove(id) { tabs.insert((start + offset).min(tabs.len()), tab); }
        }
    }

    /// Adds an existing standalone page beside a page/item, focusing the new pane.
    /// All panes stay in the anchor's section, including Pinned and Favorites.
    pub fn add_split(&mut self, anchor: TabId, other: TabId, placement: Placement, now: SystemTime) -> Result<SplitId, SplitError> {
        let scope = self.tab_scope(anchor).ok_or(SplitError::UnknownTab)?;
        let other_scope = self.tab_scope(other).ok_or(SplitError::UnknownTab)?;
        if anchor == other || self.related_tabs(anchor).contains(&other) { return Err(SplitError::SameItem); }
        if self.split_for(other).is_some() { return Err(SplitError::AlreadyGrouped); }
        if self.split_for(anchor).is_some_and(|split| split.panes.len() >= MAX_SPLIT_PANES) { return Err(SplitError::LimitReached); }
        self.repair_folders();
        self.forget_pinned_item(other);
        let other_tab = self.take_tab(other, other_scope).ok_or(SplitError::UnknownTab)?;
        self.place_tab(scope, other_tab);
        let mut split = self.take_split(anchor).map(|(_, split)| split).unwrap_or_else(|| SplitView {
            id: SplitId(self.next_id()), anchor, panes: vec![anchor], active: anchor,
            axis: placement.axis(), weights: vec![1.], title: None, custom_icon: None,
        });
        let index = split.panes.iter().position(|id| *id == anchor).unwrap_or(0) + usize::from(!placement.before());
        split.panes.insert(index, other);
        split.weights = vec![1. / split.panes.len() as f32; split.panes.len()];
        split.axis = placement.axis();
        split.active = other;
        let id = split.id;
        self.arrange_split_tabs(scope, &split);
        self.put_split(scope, split);
        if let Scope::Space(index, _) = scope { self.active_space = index; }
        self.activate_tab(other, now);
        self.repair_folders();
        Ok(id)
    }

    pub(crate) fn note_split_focus(&mut self, tab: TabId) {
        if let Some(split) = self.split_mut(tab) { split.active = tab; }
    }

    /// Copies one sidebar item beside its source, including every Split View
    /// page. Loaded engines are never shared: the returned page has a fresh id.
    pub fn duplicate_item(&mut self, tab: TabId, now: SystemTime) -> Option<TabId> {
        let scope = self.tab_scope(tab)?;
        if scope == Scope::Favorites && self.favorite_count() >= crate::favorites::MAX_FAVORITES { return None; }
        let source_item = self.pinned_item(tab);
        let source = self.split_for(tab).cloned();
        let ids = self.related_tabs(tab);
        let originals: Vec<_> = ids.iter().map(|id| self.all_tabs().find(|tab| tab.id == *id).cloned()).collect::<Option<_>>()?;
        let mut mapping = HashMap::new();
        let mut copies = Vec::new();
        for mut page in originals {
            let fresh = TabId(self.next_id());
            mapping.insert(page.id, fresh);
            page.id = fresh;
            page.last_active = now;
            copies.push(page);
        }
        let tabs = self.scope_tabs_mut(scope);
        let position = tabs.iter().enumerate().filter(|(_, page)| ids.contains(&page.id)).map(|(index, _)| index + 1).max()?;
        tabs.splice(position..position, copies);
        let active = if let Some(mut split) = source {
            split.id = SplitId(self.next_id());
            split.anchor = mapping[&split.anchor];
            split.active = mapping[&split.active];
            split.panes = split.panes.iter().map(|id| mapping[id]).collect();
            let active = split.active;
            self.put_split(scope, split);
            active
        } else { mapping[&tab] };
        if let Scope::Space(index, section) = scope {
            self.active_space = index;
            if section == Section::Pinned { self.spaces[index].pinned_collapsed = false; }
        }
        self.activate_tab(active, now);
        if matches!(scope, Scope::Space(_, Section::Pinned)) { self.insert_pinned_copy(source_item, self.pinned_item(active)); }
        Some(active)
    }

    /// Copies an item into Today while retaining its original pin or Favorite.
    /// Split panes receive independent identities and preserve their selected pane.
    pub fn copy_item_to_today(&mut self, tab: TabId, now: SystemTime) -> Option<TabId> {
        let source = self.split_for(tab).cloned();
        let originals: Vec<_> = self.related_tabs(tab).iter().map(|id|
            self.all_tabs().find(|page| page.id == *id).cloned()
        ).collect::<Option<_>>()?;
        let mut mapping = HashMap::new();
        let mut copies = Vec::new();
        for mut page in originals {
            let id = TabId(self.next_id());
            mapping.insert(page.id, id);
            page.id = id;
            page.last_active = now;
            page.pinned_url = None;
            page.pinned_title = None;
            copies.push(page);
        }
        self.space_mut().today.splice(0..0, copies);
        if let Some(mut split) = source {
            split.id = SplitId(self.next_id());
            split.anchor = mapping[&split.anchor];
            split.active = mapping[&split.active];
            split.panes = split.panes.iter().map(|id| mapping[id]).collect();
            self.space_mut().splits.push(split);
        }
        let active = mapping[&tab];
        self.activate_tab(active, now);
        Some(active)
    }

    pub fn set_split_axis(&mut self, tab: TabId, axis: Axis) -> bool {
        let Some(split) = self.split_mut(tab) else { return false; };
        split.axis = axis;
        true
    }

    pub fn set_split_weights(&mut self, tab: TabId, weights: &[f32]) -> bool {
        let Some(split) = self.split_mut(tab) else { return false; };
        let total: f32 = weights.iter().sum();
        if weights.len() != split.panes.len() || !total.is_finite() || total <= 0. || weights.iter().any(|weight| !weight.is_finite() || *weight <= 0.) { return false; }
        split.weights = weights.iter().map(|weight| weight / total).collect();
        true
    }

    pub fn move_split_pane(&mut self, tab: TabId, delta: isize) -> bool {
        let Some(scope) = self.tab_scope(tab) else { return false; };
        let Some(split) = self.split_mut(tab) else { return false; };
        let index = split.panes.iter().position(|id| *id == tab).unwrap();
        let next = index as isize + delta;
        if next < 0 || next >= split.panes.len() as isize { return false; }
        split.panes.swap(index, next as usize);
        split.weights.swap(index, next as usize);
        let arranged = split.clone();
        self.arrange_split_tabs(scope, &arranged);
        true
    }

    pub fn focus_split_pane(&mut self, index: usize, now: SystemTime) -> bool {
        let Some(tab) = self.active_split().and_then(|split| split.panes.get(index)).copied() else { return false; };
        self.activate_tab(tab, now);
        true
    }

    pub fn cycle_split_pane(&mut self, delta: isize, now: SystemTime) -> bool {
        let Some(split) = self.active_split() else { return false; };
        let current = self.space().active_tab.unwrap();
        let index = split.panes.iter().position(|id| *id == current).unwrap_or(0);
        let next = (index as isize + delta).rem_euclid(split.panes.len() as isize) as usize;
        self.focus_split_pane(next, now)
    }

    pub(crate) fn rename_split(&mut self, tab: TabId, name: Option<String>) -> bool {
        let Some(split) = self.split_mut(tab) else { return false; };
        let changed = split.title != name;
        let naming = name.is_some();
        split.title = name;
        changed || naming
    }

    pub(crate) fn toggle_split_pin(&mut self, tab: TabId) {
        let Some(scope) = self.tab_scope(tab) else { return; };
        if scope == Scope::Favorites { self.unfavorite_split(tab); return; }
        let Scope::Space(index, section) = scope else { return; };
        let active = self.spaces[index].active_tab;
        self.forget_pinned_item(tab);
        let Some((_, split)) = self.take_split(tab) else { return; };
        let target = Scope::Space(index, if section == Section::Pinned { Section::Today } else { Section::Pinned });
        let pages: Vec<_> = split.panes.iter().filter_map(|id| self.take_tab(*id, scope)).collect();
        for page in pages { self.place_tab(target, page); }
        self.arrange_split_tabs(target, &split);
        self.put_split(target, split);
        self.spaces[index].active_tab = active;
        self.repair_folders();
    }

    pub(crate) fn move_split(&mut self, tab: TabId, destination: crate::SpaceId, section: Section) -> bool {
        let Some(index) = self.spaces.iter().position(|space| space.id == destination) else { return false; };
        let Some(scope) = self.tab_scope(tab) else { return false; };
        let target = Scope::Space(index, section);
        if scope == target { return false; }
        self.forget_pinned_item(tab);
        let Some((_, split)) = self.take_split(tab) else { return false; };
        let pages: Vec<_> = split.panes.iter().filter_map(|id| self.take_tab(*id, scope)).collect();
        for page in pages { self.place_tab(target, page); }
        self.arrange_split_tabs(target, &split);
        self.put_split(target, split);
        self.repair_folders();
        true
    }

    pub(crate) fn favorite_split(&mut self, tab: TabId) -> bool {
        if self.is_favorite(tab) || self.favorite_count() >= crate::favorites::MAX_FAVORITES { return false; }
        self.forget_pinned_item(tab);
        let Some((scope, split)) = self.take_split(tab) else { return false; };
        let pages: Vec<_> = split.panes.iter().filter_map(|id| self.take_tab(*id, scope)).collect();
        for page in pages { self.place_tab(Scope::Favorites, page); }
        self.arrange_split_tabs(Scope::Favorites, &split);
        self.space_mut().active_tab = Some(split.active);
        self.put_split(Scope::Favorites, split);
        self.repair_folders();
        true
    }

    pub(crate) fn unfavorite_split(&mut self, tab: TabId) -> bool {
        if !self.is_favorite(tab) { return false; }
        let Some((scope, split)) = self.take_split(tab) else { return false; };
        let pages: Vec<_> = split.panes.iter().filter_map(|id| self.take_tab(*id, scope)).collect();
        let target = Scope::Space(self.active_space, Section::Today);
        for page in pages { self.place_tab(target, page); }
        self.arrange_split_tabs(target, &split);
        self.space_mut().active_tab = Some(split.active);
        self.put_split(target, split);
        true
    }

    /// Breaking apart a Favorite must not overflow the profile's dock limit.
    pub fn separate_split(&mut self, tab: TabId) -> bool {
        let Some(split) = self.split_for(tab).cloned() else { return false; };
        if self.is_favorite(tab) && self.favorite_count() + split.panes.len() - 1 > crate::favorites::MAX_FAVORITES { return false; }
        self.take_split(tab);
        self.replace_pinned_item(crate::folders::PinnedItem::Split(split.id), &split.panes.iter().copied().map(crate::folders::PinnedItem::Tab).collect::<Vec<_>>());
        self.repair_folders();
        true
    }

    pub fn separate_split_pane(&mut self, tab: TabId) -> bool {
        if self.is_favorite(tab) && self.favorite_count() >= crate::favorites::MAX_FAVORITES { return false; }
        let Some((scope, mut split)) = self.take_split(tab) else { return false; };
        let original = split.clone();
        remove_pane(&mut split, tab);
        let replacement = if split.panes.len() == 1 { original.panes.iter().copied().map(crate::folders::PinnedItem::Tab).collect() }
            else if original.panes.first() == Some(&tab) { vec![crate::folders::PinnedItem::Tab(tab), crate::folders::PinnedItem::Split(split.id)] }
            else { vec![crate::folders::PinnedItem::Split(split.id), crate::folders::PinnedItem::Tab(tab)] };
        if split.panes.len() > 1 { self.put_split(scope, split); }
        self.replace_pinned_item(crate::folders::PinnedItem::Split(original.id), &replacement);
        self.repair_folders();
        true
    }

    pub fn close_split_pane(&mut self, tab: TabId, now: SystemTime) -> bool {
        let Some((scope, mut split)) = self.take_split(tab) else { return false; };
        let selected: Vec<_> = self.spaces.iter().enumerate().filter(|(_, space)| space.active_tab == Some(tab)).map(|(index, _)| index).collect();
        let Some(mut page) = self.take_tab(tab, scope) else { self.put_split(scope, split); return false; };
        remove_pane(&mut split, tab);
        page.pinned_url = None;
        page.pinned_title = None;
        let space = match scope { Scope::Favorites => self.space().id, Scope::Space(index, _) => self.spaces[index].id };
        self.archive.push(ArchivedTab { tab: page, space, archived_at: now, reason: ArchiveReason::Manual, split: None });
        for index in selected { self.spaces[index].active_tab = Some(split.active); }
        if split.panes.len() > 1 { self.put_split(scope, split); }
        else { self.replace_pinned_item(crate::folders::PinnedItem::Split(split.id), &[crate::folders::PinnedItem::Tab(split.panes[0])]); }
        self.repair_folders();
        true
    }

    pub(crate) fn close_split_item(&mut self, tab: TabId, now: SystemTime) -> Option<Closed> {
        let split = self.split_for(tab)?.clone();
        let scope = self.tab_scope(tab)?;
        if !split.panes.iter().all(|id| self.tab_scope(*id) == Some(scope)) { return None; }
        if let Scope::Space(index, Section::Today) = scope {
            self.take_split(tab)?;
            let mut pages: Vec<_> = split.panes.iter().filter_map(|id| self.take_tab(*id, scope)).collect();
            let anchor = pages.iter().position(|page| page.id == split.anchor)?;
            let page = pages.remove(anchor);
            self.archive.push(ArchivedTab { tab: page, space: self.spaces[index].id, archived_at: now,
                reason: ArchiveReason::Manual, split: Some(ArchivedSplit { view: split, panes: pages }) });
            return Some(Closed::Archived);
        }
        for id in &split.panes { if let Some(page) = self.tab_mut(*id) { page.reset_to_pinned_url(); } }
        let selections: Vec<_> = self.spaces.iter().enumerate().filter_map(|(index, space)| {
            if !space.active_tab.is_some_and(|id| split.panes.contains(&id)) { return None; }
            let local: Vec<_> = space.tabs().filter(|tab| !self.is_split_secondary(tab.id)).map(|tab| tab.id).collect();
            let position = local.iter().position(|id| *id == split.anchor);
            let next = position.and_then(|at| local.get(at + 1).or_else(|| at.checked_sub(1).and_then(|at| local.get(at))))
                .copied().or_else(|| local.iter().find(|id| !split.panes.contains(id)).copied());
            let next = next.map(|id| self.split_for(id).map_or(id, |view| view.active));
            Some((index, next))
        }).collect();
        for (index, next) in selections { self.spaces[index].active_tab = next; }
        Some(Closed::Unloaded)
    }

    pub(crate) fn restore_split_archived(&mut self, mut archived: ArchivedTab, now: SystemTime) -> TabId {
        let mut saved = archived.split.take().unwrap();
        let index = self.spaces.iter().position(|space| space.id == archived.space).unwrap_or(self.active_space);
        let target = Scope::Space(index, Section::Today);
        let anchor = archived.tab.id;
        saved.view.anchor = anchor;
        if !saved.view.panes.contains(&anchor) { saved.view.panes.insert(0, anchor); }
        let mut pages = vec![archived.tab];
        pages.extend(saved.panes);
        if !pages.iter().any(|tab| tab.id == saved.view.active) { saved.view.active = anchor; }
        for mut page in pages { page.last_active = now; self.place_tab(target, page); }
        self.arrange_split_tabs(target, &saved.view);
        self.active_space = index;
        self.spaces[index].active_tab = Some(saved.view.active);
        self.put_split(target, saved.view);
        self.repair_splits();
        self.repair_folders();
        anchor
    }

    pub(crate) fn clear_split_today(&mut self, now: SystemTime) -> usize {
        let items: Vec<_> = self.space().today.iter().filter(|tab| !self.is_split_secondary(tab.id)).map(|tab| tab.id).collect();
        let mut count = 0;
        for id in items { if self.close_tab(id, now) == Some(Closed::Archived) { count += 1; } }
        count
    }

    pub(crate) fn archive_split_idle(&mut self, now: SystemTime) -> usize {
        let Some(limit) = self.archive_after else { return 0; };
        let items: Vec<_> = self.spaces.iter().enumerate().flat_map(|(index, space)| {
            space.today.iter().filter(|tab| !self.is_split_secondary(tab.id)).filter(|tab| {
                let panes = self.related_tabs(tab.id);
                !space.active_tab.is_some_and(|id| panes.contains(&id)) && panes.iter().all(|id| {
                    self.all_tabs().find(|tab| tab.id == *id).is_some_and(|tab| now.duration_since(tab.last_active).unwrap_or_default() >= limit)
                })
            }).map(move |tab| (index, tab.id))
        }).collect();
        // This model operation is exclusive and has no callbacks. Reuse the
        // same close policy for each Space without exposing a Space switch.
        let active = self.active_space;
        let mut count = 0;
        for (index, tab) in items {
            self.active_space = index;
            if self.close_tab(tab, now) == Some(Closed::Archived) {
                self.archive.last_mut().unwrap().reason = ArchiveReason::Automatic;
                count += 1;
            }
        }
        self.active_space = active;
        count
    }

    pub(crate) fn split_max_id(&self) -> u64 {
        self.favorite_splits.iter().map(|split| split.id.get())
            .chain(self.spaces.iter().flat_map(|space| space.splits.iter().map(|split| split.id.get())))
            .chain(self.archive.iter().flat_map(|entry| entry.split.iter().flat_map(|saved| {
                std::iter::once(saved.view.id.get()).chain(saved.view.panes.iter().map(|id| id.get())).chain(saved.panes.iter().map(|tab| tab.id.get()))
            }))).max().unwrap_or(0)
    }

    pub(crate) fn repair_splits(&mut self) {
        let mut used_panes = HashSet::new();
        let mut used_ids = HashSet::new();
        let mut next = self.next_id;
        let lanes: HashMap<_, _> = self.favorites.iter().map(|tab| (tab.id, 0)).collect();
        repair_views(&mut self.favorite_splits, &lanes, &mut used_panes, &mut used_ids, &mut next);
        for space in &mut self.spaces {
            let lanes = space.pinned.iter().map(|tab| (tab.id, 1)).chain(space.today.iter().map(|tab| (tab.id, 2))).collect();
            repair_views(&mut space.splits, &lanes, &mut used_panes, &mut used_ids, &mut next);
        }
        self.next_id = next;
    }
}

fn repair_views(views: &mut Vec<SplitView>, lanes: &HashMap<TabId, u8>, used_panes: &mut HashSet<TabId>, used_ids: &mut HashSet<SplitId>, next: &mut u64) {
    views.retain_mut(|view| {
        let unique: HashSet<_> = view.panes.iter().copied().collect();
        let lane = view.panes.first().and_then(|id| lanes.get(id));
        if !(2..=MAX_SPLIT_PANES).contains(&view.panes.len()) || unique.len() != view.panes.len()
            || lane.is_none() || view.panes.iter().any(|id| lanes.get(id) != lane || used_panes.contains(id)) { return false; }
        used_panes.extend(unique);
        if !used_ids.insert(view.id) { view.id = SplitId(*next); *next += 1; used_ids.insert(view.id); }
        if !view.panes.contains(&view.anchor) { view.anchor = view.panes[0]; }
        if !view.panes.contains(&view.active) { view.active = view.anchor; }
        let sum: f32 = view.weights.iter().sum();
        if view.weights.len() != view.panes.len() || !sum.is_finite() || sum <= 0. || view.weights.iter().any(|weight| !weight.is_finite() || *weight <= 0.) {
            view.weights = vec![1. / view.panes.len() as f32; view.panes.len()];
        } else { for weight in &mut view.weights { *weight /= sum; } }
        view.title = view.title.take().map(|title| title.trim().to_string()).filter(|title| !title.is_empty());
        true
    });
}

fn remove_pane(split: &mut SplitView, tab: TabId) {
    let index = split.panes.iter().position(|id| *id == tab).unwrap();
    split.panes.remove(index);
    split.weights.remove(index);
    if split.anchor == tab { split.anchor = split.panes[0]; }
    if split.active == tab { split.active = split.panes[index.min(split.panes.len() - 1)]; }
    let sum: f32 = split.weights.iter().sum();
    for weight in &mut split.weights { *weight /= sum; }
}

#[cfg(test)]
mod tests;
