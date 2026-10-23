//! Window selection is separate from the profile's shared tab identities.
//! A projection is read-only; edits run against the single profile model.
use crate::{splits::SplitId, Browser, Space, SpaceId, TabId};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ops::Deref};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowSelection {
    active_space: SpaceId,
    space_index: usize,
    tabs: HashMap<SpaceId, TabSelection>,
    panes: HashMap<SplitId, TabId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TabSelection {
    active: Option<TabId>,
    /// Neighbors from this window, used if another window closes its tab.
    fallback: Vec<TabId>,
}

/// A read-only snapshot of the profile with one window's selection applied.
/// It cannot mutate tabs or allocate IDs; session ownership stays with the profile.
#[derive(Clone, Debug)]
pub struct WindowProjection(Browser);

impl WindowSelection {
    /// Selected tabs in all remembered Spaces, e.g. for profile-wide idle
    /// archiving to protect the selections of every open window.
    pub fn selected_tabs(&self) -> impl Iterator<Item = TabId> + '_ {
        self.tabs.values().filter_map(|selection| selection.active)
    }
}

impl Deref for WindowProjection {
    type Target = Browser;
    fn deref(&self) -> &Browser { &self.0 }
}

impl Browser {
    pub fn window_selection(&self) -> WindowSelection {
        WindowSelection {
            active_space: self.space().id,
            space_index: self.active_space,
            tabs: self.spaces.iter().map(|space| {
                (space.id, TabSelection { active: space.active_tab, fallback: fallback(self, space) })
            }).collect(),
            panes: self.favorite_splits.iter().chain(self.spaces.iter().flat_map(|space| &space.splits))
                .map(|split| (split.id, split.active)).collect(),
        }
    }

    /// Arc's New Window keeps the current Space and shared sidebar, while
    /// starting with no selected page in any Space.
    pub fn new_window_selection(&self) -> WindowSelection {
        let mut selection = self.window_selection();
        for tab in selection.tabs.values_mut() {
            tab.active = None;
            tab.fallback.clear();
        }
        selection
    }

    pub fn window_projection(&self, selection: &WindowSelection) -> WindowProjection {
        let mut browser = self.clone();
        browser.apply_window_selection(selection);
        WindowProjection(browser)
    }

    /// Select the originating window, perform an edit on the shared model,
    /// then remember that window's resulting selection. The model retains
    /// the last edited window's selection as its session restoration default.
    pub fn edit_in_window<R>(&mut self, selection: &mut WindowSelection, edit: impl FnOnce(&mut Browser) -> R) -> R {
        self.apply_window_selection(selection);
        let result = edit(self);
        *selection = self.window_selection();
        result
    }

    fn apply_window_selection(&mut self, selection: &WindowSelection) {
        self.active_space = self.spaces.iter().position(|space| space.id == selection.active_space)
            .unwrap_or_else(|| selection.space_index.min(self.spaces.len() - 1));
        let favorites: Vec<_> = self.favorites.iter().map(|tab| tab.id).collect();
        for space in &mut self.spaces {
            let valid = |id| favorites.contains(&id) || space.tabs().any(|tab| tab.id == id);
            let active = selection.tabs.get(&space.id).and_then(|saved| {
                let active = saved.active?;
                if valid(active) { Some(active) }
                else { saved.fallback.iter().copied().find(|id| valid(*id)) }
            });
            space.active_tab = active;
        }
        for split in self.favorite_splits.iter_mut().chain(self.spaces.iter_mut().flat_map(|space| &mut space.splits)) {
            if let Some(active) = selection.panes.get(&split.id).filter(|id| split.panes.contains(id)) {
                split.active = *active;
            }
        }
        // The selected pane takes precedence if a group changed while this
        // window was inactive, including a split that did not exist before.
        let selected: Vec<_> = self.spaces.iter().enumerate().filter(|(index, _)| *index != self.active_space)
            .filter_map(|(_, space)| space.active_tab).chain(self.space().active_tab).collect();
        for tab in selected { self.note_split_focus(tab); }
    }
}

fn fallback(browser: &Browser, space: &Space) -> Vec<TabId> {
    let Some(active) = space.active_tab else { return Vec::new(); };
    let mut candidates = Vec::new();
    if let Some(split) = browser.split_for(active) {
        candidates.extend(neighbors(&split.panes, active));
    }
    let section = if space.today.iter().any(|tab| tab.id == active) { &space.today }
        else if space.pinned.iter().any(|tab| tab.id == active) { &space.pinned }
        else { &browser.favorites };
    candidates.extend(neighbors(&section.iter().map(|tab| tab.id).collect::<Vec<_>>(), active));
    candidates.extend(space.tabs().chain(&browser.favorites).map(|tab| tab.id));
    let mut seen = std::collections::HashSet::from([active]);
    candidates.retain(|id| seen.insert(*id));
    candidates
}

fn neighbors(ids: &[TabId], active: TabId) -> impl Iterator<Item = TabId> + '_ {
    let index = ids.iter().position(|id| *id == active).unwrap_or(ids.len());
    ids.get(index + 1..).unwrap_or_default().iter().chain(ids[..index].iter().rev()).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::splits::{Axis, Placement};
    use std::time::SystemTime;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;

    #[test]
    fn a_new_window_keeps_the_shared_sidebar_but_starts_without_a_page() {
        let mut profile = Browser::new();
        let favorite = profile.new_tab("https://favorite", NOW);
        profile.favorite_tab(favorite);
        let pinned = profile.new_tab("https://pinned", NOW);
        profile.toggle_pin(pinned);
        profile.new_tab("https://today", NOW);
        profile.add_space("Work", 20.);
        profile.switch_space(1);
        profile.new_tab("https://work", NOW);
        let original = crate::session::to_json(&profile);
        let window = profile.window_projection(&profile.new_window_selection());
        assert_eq!(window.space().name, "Work");
        assert!(window.spaces.iter().all(|space| space.active_tab.is_none()));
        assert_eq!(window.all_tabs().count(), 4);
        assert_eq!(window.favorites[0].id, favorite);
        assert_eq!(window.spaces[0].pinned[0].id, pinned);
        assert_eq!(crate::session::to_json(&profile), original);
    }

    #[test]
    fn window_edits_share_tabs_and_navigation_without_changing_other_selections() {
        let mut profile = Browser::new();
        let first = profile.new_tab("https://first", NOW);
        let mut a = profile.window_selection();
        let mut b = profile.new_window_selection();
        let second = profile.edit_in_window(&mut b, |profile| profile.new_tab("https://second", NOW));
        assert_ne!(first, second);
        assert_eq!(profile.window_projection(&a).active_tab().unwrap().id, first);
        assert_eq!(profile.window_projection(&b).active_tab().unwrap().id, second);
        profile.edit_in_window(&mut a, |profile| profile.navigate(first, "https://first/changed"));
        let window_b = profile.window_projection(&b);
        assert_eq!(window_b.active_tab().unwrap().id, second);
        assert_eq!(window_b.all_tabs().find(|tab| tab.id == first).unwrap().url, "https://first/changed");
        assert_eq!(window_b.all_tabs().count(), 2);
        assert_eq!(profile.active_tab().unwrap().id, first);
    }

    #[test]
    fn spaces_are_remembered_by_identity_when_another_window_reorders_them() {
        let mut profile = Browser::new();
        let personal = profile.new_tab("https://personal", NOW);
        let mut a = profile.window_selection();
        let work = profile.add_space("Work", 40.);
        let mut b = profile.new_window_selection();
        profile.edit_in_window(&mut b, |profile| {
            profile.switch_space(1);
            profile.new_tab("https://work", NOW);
        });
        profile.edit_in_window(&mut a, |profile| profile.move_space(work, 0));
        assert_eq!(profile.window_projection(&a).space().name, "Personal");
        assert_eq!(profile.window_projection(&a).active_tab().unwrap().id, personal);
        assert_eq!(profile.window_projection(&b).space().name, "Work");
    }

    #[test]
    fn closing_a_shared_tab_uses_each_windows_own_neighbor_order() {
        let mut profile = Browser::new();
        let oldest = profile.new_tab("https://oldest", NOW);
        let middle = profile.new_tab("https://middle", NOW);
        profile.new_tab("https://newest", NOW);
        profile.activate_tab(middle, NOW);
        let a = profile.window_selection();
        let mut b = profile.window_selection();
        let empty = profile.new_window_selection();
        profile.edit_in_window(&mut b, |profile| {
            profile.activate_tab(oldest, NOW);
            profile.close_tab(middle, NOW);
        });
        assert_eq!(profile.window_projection(&a).active_tab().unwrap().id, oldest);
        assert_eq!(profile.window_projection(&b).active_tab().unwrap().id, oldest);
        assert!(profile.window_projection(&empty).active_tab().is_none());
        assert_eq!(profile.archive.len(), 1);
    }

    #[test]
    fn split_focus_is_window_local_while_orientation_and_pane_identity_are_shared() {
        let mut profile = Browser::new();
        let left = profile.new_tab("https://left", NOW);
        let right = profile.new_tab("https://right", NOW);
        profile.add_split(left, right, Placement::Right, NOW).unwrap();
        profile.activate_tab(left, NOW);
        let a = profile.window_selection();
        let mut b = profile.window_selection();
        profile.edit_in_window(&mut b, |profile| {
            profile.activate_tab(right, NOW);
            profile.spaces[0].splits[0].axis = Axis::Vertical;
        });
        let window_a = profile.window_projection(&a);
        let window_b = profile.window_projection(&b);
        assert_eq!(window_a.active_split().unwrap().active, left);
        assert_eq!(window_b.active_split().unwrap().active, right);
        assert_eq!(window_a.active_split().unwrap().axis, Axis::Vertical);
        assert_eq!(window_a.active_split().unwrap().panes, window_b.active_split().unwrap().panes);
        profile.edit_in_window(&mut b, |profile| { profile.close_split_pane(right, NOW); });
        assert_eq!(profile.window_projection(&a).active_tab().unwrap().id, left);
        assert_eq!(profile.window_projection(&b).active_tab().unwrap().id, left);
    }

    #[test]
    fn closing_a_standalone_tab_returns_to_the_neighbors_remembered_split_pane() {
        let mut profile = Browser::new();
        let left = profile.new_tab("https://left", NOW);
        let right = profile.new_tab("https://right", NOW);
        profile.add_split(left, right, Placement::Right, NOW).unwrap();
        profile.activate_tab(right, NOW);
        let mut window = profile.window_selection();
        let source = profile.edit_in_window(&mut window, |profile| profile.new_tab("view-source:https://right", NOW));
        profile.edit_in_window(&mut window, |profile| { profile.close_tab(source, NOW); });
        let projected = profile.window_projection(&window);
        assert_eq!(projected.active_tab().unwrap().id, right);
        assert_eq!(projected.active_split().unwrap().active, right);
    }

    #[test]
    fn favorites_can_be_selected_independently_in_different_windows_and_spaces() {
        let mut profile = Browser::new();
        let favorite = profile.new_tab("https://favorite", NOW);
        profile.favorite_tab(favorite);
        profile.activate_tab(favorite, NOW);
        profile.add_space("Work", 40.);
        let a = profile.window_selection();
        let mut b = profile.new_window_selection();
        profile.edit_in_window(&mut b, |profile| {
            profile.switch_space(1);
            profile.activate_tab(favorite, NOW);
            profile.navigate(favorite, "https://favorite/changed");
        });
        let window_a = profile.window_projection(&a);
        let window_b = profile.window_projection(&b);
        assert_eq!(window_a.space().name, "Personal");
        assert_eq!(window_b.space().name, "Work");
        assert_eq!(window_a.active_tab().unwrap().url, "https://favorite/changed");
        assert_eq!(window_a.active_tab().unwrap().id, window_b.active_tab().unwrap().id);
        assert!(profile.spaces.iter().all(|space| space.pinned.is_empty()));
        assert!(profile.spaces.iter().all(|space| space.today.iter().all(|tab| tab.id != favorite)));
    }

    #[test]
    fn saved_window_selection_survives_round_trip_and_removed_spaces() {
        let mut profile = Browser::new();
        profile.add_space("Work", 40.);
        profile.switch_space(1);
        profile.new_tab("https://work", NOW);
        let selection = profile.window_selection();
        let encoded = serde_json::to_string(&selection).unwrap();
        let restored = serde_json::from_str(&encoded).unwrap();
        assert_eq!(profile.window_projection(&restored).active_tab().unwrap().url, "https://work");
        profile.spaces.remove(1);
        let projected = profile.window_projection(&restored);
        assert_eq!(projected.space().name, "Personal");
        assert!(projected.active_tab().is_none());
    }

    #[test]
    fn the_visible_spaces_favorite_pane_wins_over_other_spaces_remembered_panes() {
        let mut profile = Browser::new();
        let left = profile.new_tab("https://left", NOW);
        let right = profile.new_tab("https://right", NOW);
        profile.add_split(left, right, Placement::Right, NOW).unwrap();
        profile.favorite_tab(left);
        profile.activate_tab(left, NOW);
        profile.add_space("Work", 40.);
        profile.switch_space(1);
        profile.activate_tab(right, NOW);
        let work = profile.window_selection();
        profile.switch_space(0);
        let personal = profile.window_selection();
        let original = crate::session::to_json(&profile);
        let projected = profile.window_projection(&personal);
        assert_eq!(projected.active_split().unwrap().active, left);
        assert_eq!(projected.spaces[1].active_tab, Some(right));
        assert_eq!(profile.window_projection(&work).active_split().unwrap().active, right);
        assert_eq!(personal.selected_tabs().count(), 2);
        assert_eq!(crate::session::to_json(&profile), original);
    }
}
