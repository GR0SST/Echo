//! Browser model without UI or engine: spaces, pinned and today tabs,
//! auto-archiving. Time is always passed in so behavior is testable.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

pub mod omnibox;
pub mod links;
pub mod history;
pub mod favorites;
pub mod favicons;
pub mod audio;
pub mod splits;
pub mod bulk_archive;
pub mod preferences;
pub mod icons;
pub mod folders;
pub mod undo;
pub mod windows;
pub mod profiles;
pub mod downloads;
pub use profiles::UserProfileId;

fn default_enabled() -> bool { true }
pub mod session;

#[cfg(test)]
mod blank_tests;

/// Arc's default: unpinned tabs are archived after 12 hours of inactivity.
pub const DEFAULT_ARCHIVE_AFTER: Duration = Duration::from_secs(12 * 60 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(u64);

impl TabId {
    /// Stable numeric value, e.g. for UI element ids.
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpaceId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Pinned,
    Today,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tab {
    pub id: TabId,
    pub title: String,
    /// A user label, kept independently of titles reported by the page.
    #[serde(default)]
    pub custom_title: Option<String>,
    #[serde(default)]
    pub custom_icon: Option<String>,
    #[serde(default)]
    pub page_title: Option<String>,
    #[serde(default)]
    pub pinned_title: Option<String>,
    pub url: String,
    pub last_active: SystemTime,
    /// The page a pinned tab returns to when closed, like Arc's.
    #[serde(default)]
    pub pinned_url: Option<String>,
    #[serde(default)]
    pub muted: bool,
}

impl Tab {
    fn reset_to_pinned_url(&mut self) -> bool {
        let Some(url) = self.pinned_url.clone() else { return false; };
        self.url = url;
        self.page_title = self.pinned_title.clone();
        if let Some(title) = self.custom_title.clone().or_else(|| self.pinned_title.clone()) {
            self.title = title;
        }
        true
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Space {
    pub id: SpaceId,
    pub user_profile_id: UserProfileId,
    pub name: String,
    /// Accent hue in degrees, 0..360.
    pub hue: f32,
    #[serde(default = "default_enabled")]
    pub show_header: bool,
    #[serde(default)]
    pub custom_icon: Option<String>,
    pub pinned: Vec<Tab>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_items: Vec<folders::PinnedItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<folders::Folder>,
    #[serde(default)]
    pub pinned_collapsed: bool,
    pub today: Vec<Tab>,
    pub active_tab: Option<TabId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub splits: Vec<splits::SplitView>,
}

impl Space {
    fn tabs(&self) -> impl Iterator<Item = &Tab> {
        self.pinned.iter().chain(&self.today)
    }

    fn find(&self, id: TabId) -> Option<(Section, usize)> {
        if let Some(i) = self.pinned.iter().position(|t| t.id == id) {
            return Some((Section::Pinned, i));
        }
        self.today
            .iter()
            .position(|t| t.id == id)
            .map(|i| (Section::Today, i))
    }

    fn section_mut(&mut self, section: Section) -> &mut Vec<Tab> {
        match section {
            Section::Pinned => &mut self.pinned,
            Section::Today => &mut self.today,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveReason {
    #[default]
    Unknown,
    Manual,
    Automatic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchivedTab {
    pub tab: Tab,
    pub space: SpaceId,
    /// Origin identity only; global Archive visibility is not yet established.
    pub user_profile_id: UserProfileId,
    pub archived_at: SystemTime,
    #[serde(default)]
    pub reason: ArchiveReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<splits::ArchivedSplit>,
}

/// What closing a tab did to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    /// A Today tab went to the archive.
    Archived,
    /// A pinned tab stays in the sidebar, back at its pinned page; its
    /// page should be unloaded.
    Unloaded,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Browser {
    pub spaces: Vec<Space>,
    pub active_space: usize,
    pub archive: Vec<ArchivedTab>,
    /// Profile-owned data; the workspace retains global Space order and IDs.
    profiles: std::collections::BTreeMap<profiles::UserProfileId, profiles::UserProfile>,
    default_user_profile_id: profiles::UserProfileId,
    next_user_profile_id: u64,
    #[serde(skip)]
    blank_source_user_profile_id: Option<profiles::UserProfileId>,
    #[serde(skip)]
    sidebar_history: undo::SidebarHistory,
    #[serde(skip)]
    private: bool,
    /// Runtime window kind only; Blank session persistence is not established.
    #[serde(skip)]
    blank: bool,
    next_id: u64,
}

impl Default for Browser {
    fn default() -> Self {
        Self::new()
    }
}

impl Browser {
    pub fn new() -> Self {
        Self::with_initial_space("Personal", true, false)
    }

    /// A fresh, normal-privacy, window-local Today list with no persistent
    /// sidebar controls. History and auto-archive policy retain normal defaults.
    /// Blank identity is runtime-only: session serialization does not preserve it.
    pub fn new_blank() -> Self {
        Self::with_initial_space("Space 1", false, true)
    }

    fn with_initial_space(name: &str, show_header: bool, blank: bool) -> Self {
        let mut browser = Self {
            spaces: Vec::new(),
            active_space: 0,
            archive: Vec::new(),
            profiles: std::collections::BTreeMap::from([(UserProfileId::DEFAULT, profiles::UserProfile::default_profile())]),
            default_user_profile_id: UserProfileId::DEFAULT,
            next_user_profile_id: 2,
            blank_source_user_profile_id: blank.then_some(UserProfileId::DEFAULT),
            sidebar_history: undo::SidebarHistory::default(),
            private: false,
            blank,
            next_id: 1,
        };
        browser.add_space(name, 250.0);
        browser.spaces[0].show_header = show_header;
        browser
    }

    /// A window-local tab list that never records browsing history.
    pub fn new_private() -> Self {
        let mut browser = Self::new();
        browser.private = true;
        browser.spaces[0].name = "Incognito".into();
        browser.active_user_profile_mut().archive_after = None;
        browser
    }

    pub fn is_private(&self) -> bool { self.private }

    pub fn is_blank(&self) -> bool { self.blank }

    /// Whether Favorites, Pinned tabs and folders may be edited. Independent
    /// of privacy; callers can also use this to route sidebar controls.
    pub fn can_edit_persistent_sidebar(&self) -> bool { !self.is_blank() }

    /// Makes a deserialized model consistent: at least one space, a valid
    /// active space and ids that never repeat.
    fn repair(&mut self) {
        if self.spaces.is_empty() {
            self.add_space("Personal", 250.0);
        }
        self.active_space = self.active_space.min(self.spaces.len() - 1);
        let max_id = self
            .spaces
            .iter()
            .flat_map(|space| std::iter::once(space.id.0).chain(space.tabs().map(|t| t.id.0)))
            .chain(self.archive.iter().map(|a| a.tab.id.0))
            .chain(self.profiles.values().flat_map(|profile| profile.favorites.iter().map(|tab| tab.id.0)))
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(max_id + 1);
        self.next_id = self.next_id.max(self.split_max_id() + 1).max(self.folder_max_id().saturating_add(1));
        self.repair_profile_tab_ids();
        self.repair_splits();
        self.repair_folders();
        for space in &mut self.spaces {
            if space.active_tab.is_some_and(|id| !space.tabs().any(|tab| tab.id == id) && !self.profiles[&space.user_profile_id].favorites.iter().any(|tab| tab.id == id)) {
                space.active_tab = None;
            }
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn space(&self) -> &Space {
        &self.spaces[self.active_space]
    }

    fn space_mut(&mut self) -> &mut Space {
        &mut self.spaces[self.active_space]
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        let space = self.space();
        let id = space.active_tab?;
        self.active_user_profile().favorites.iter().chain(space.tabs()).find(|t| t.id == id)
    }

    pub fn add_space(&mut self, name: impl Into<String>, hue: f32) -> SpaceId {
        let id = SpaceId(self.next_id());
        let user_profile_id = self.spaces.get(self.active_space).map_or(self.default_user_profile_id, |space| space.user_profile_id);
        self.spaces.push(Space {
            id,
            user_profile_id,
            name: name.into(),
            hue,
            show_header: true,
            custom_icon: None,
            pinned: Vec::new(),
            pinned_items: Vec::new(),
            folders: Vec::new(),
            pinned_collapsed: false,
            today: Vec::new(),
            active_tab: None,
            splits: Vec::new(),
        });
        id
    }

    /// Renames a Space without changing its identity, tabs or theme.
    pub fn rename_space(&mut self, id: SpaceId, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() { return false; }
        let Some(space) = self.spaces.iter_mut().find(|space| space.id == id) else { return false; };
        space.name = name.to_string();
        true
    }

    /// Reorders Spaces while keeping the active Space and all tab identities.
    pub fn move_space(&mut self, id: SpaceId, destination: usize) -> bool {
        let Some(source) = self.spaces.iter().position(|space| space.id == id) else { return false; };
        if destination >= self.spaces.len() || source == destination { return false; }
        let active = self.space().id;
        let space = self.spaces.remove(source);
        self.spaces.insert(destination, space);
        self.active_space = self.spaces.iter().position(|space| space.id == active).unwrap();
        true
    }

    pub fn switch_space(&mut self, index: usize) {
        if index < self.spaces.len() {
            self.active_space = index;
        }
    }

    /// Opens a tab at the top of Today in the active space and focuses it.
    pub fn new_tab(&mut self, url: impl Into<String>, now: SystemTime) -> TabId {
        let id = TabId(self.next_id());
        let url = url.into();
        let space = self.space_mut();
        space.today.insert(
            0,
            Tab {
                id,
                title: url.clone(),
                custom_title: None,
                custom_icon: None,
                page_title: None,
                pinned_title: None,
                url,
                last_active: now,
                pinned_url: None,
                muted: false,
            },
        );
        space.active_tab = Some(id);
        id
    }

    pub fn activate_tab(&mut self, id: TabId, now: SystemTime) {
        if let Some(tab) = self.active_user_profile_mut().favorites.iter_mut().find(|tab| tab.id == id) {
            tab.last_active = now;
            self.space_mut().active_tab = Some(id);
            self.note_split_focus(id);
            return;
        }
        let space = self.space_mut();
        let Some((section, i)) = space.find(id) else {
            return;
        };
        space.section_mut(section)[i].last_active = now;
        space.active_tab = Some(id);
        self.note_split_focus(id);
    }

    /// Focuses the tab `delta` steps away in sidebar order (Favorites,
    /// Pinned, then Today), wrapping around. Like Ctrl+Tab / Ctrl+Shift+Tab.
    pub fn cycle_tab(&mut self, delta: isize, now: SystemTime) {
        let space = self.space();
        let ids: Vec<TabId> = self.active_user_profile().favorites.iter().chain(space.tabs()).filter(|tab| !self.is_split_secondary(tab.id)).map(|t| t.id).collect();
        if ids.is_empty() {
            return;
        }
        let current = space
            .active_tab
            .and_then(|id| ids.iter().position(|t| *t == self.item_anchor(id)));
        let next = match current {
            Some(i) => (i as isize + delta).rem_euclid(ids.len() as isize) as usize,
            None if delta < 0 => ids.len() - 1,
            None => 0,
        };
        self.activate_sidebar_item(ids[next], now);
    }

    /// Removes a tab from the active space. Focus moves to the neighbor
    /// below, or above when the removed tab was last, within the section.
    fn remove_tab(&mut self, id: TabId) -> Option<Tab> {
        let space = self.space_mut();
        let (section, i) = space.find(id)?;
        let tab = space.section_mut(section).remove(i);
        if space.active_tab == Some(id) {
            space.active_tab = neighbor(space, section, i);
        }
        Some(tab)
    }

    /// Closes a tab like Arc's ⌘W: Today tabs go to the archive, pinned
    /// tabs stay pinned and return to their pinned page.
    pub fn close_tab(&mut self, id: TabId, now: SystemTime) -> Option<Closed> {
        let capture = undo::ArchiveCapture::new(self);
        let closed = self.close_tab_unrecorded(id, now);
        capture.record(self);
        closed
    }

    pub(crate) fn close_tab_unrecorded(&mut self, id: TabId, now: SystemTime) -> Option<Closed> {
        if self.split_for(id).is_some() { return self.close_split_item(id, now); }
        if self.is_favorite(id) {
            self.close_favorite(id);
            return Some(Closed::Unloaded);
        }
        let space = self.space_mut();
        match space.find(id)? {
            (Section::Pinned, i) => {
                let tab = &mut space.pinned[i];
                tab.reset_to_pinned_url();
                if space.active_tab == Some(id) {
                    // The tab stays in place, so focus moves past it.
                    space.active_tab = space
                        .pinned
                        .get(i + 1)
                        .or_else(|| space.today.first())
                        .or_else(|| i.checked_sub(1).map(|j| &space.pinned[j]))
                        .map(|t| t.id);
                }
                Some(Closed::Unloaded)
            }
            (Section::Today, _) => {
                let space_id = space.id;
                let tab = self.remove_tab(id)?;
                self.archive.push(ArchivedTab { tab, user_profile_id: self.space().user_profile_id, space: space_id, archived_at: now, reason: ArchiveReason::Manual, split: None });
                Some(Closed::Archived)
            }
        }
    }

    /// Archives background Today items of the active space (Arc's "Clear", ⇧⌘K).
    /// The selected tab, including its entire Split View, stays open.
    /// Returns how many were archived.
    pub fn clear_today(&mut self, now: SystemTime) -> usize {
        let capture = undo::ArchiveCapture::new(self);
        let count = self.clear_today_unrecorded(now);
        capture.record(self);
        count
    }

    fn clear_today_unrecorded(&mut self, now: SystemTime) -> usize {
        self.clear_split_today(now)
    }

    /// Brings back the most recently archived tab at the top of Today in
    /// its space (Arc's "Restore Last Closed Tab", ⇧⌘T) and focuses it.
    pub fn restore_last_archived(&mut self, now: SystemTime) -> Option<TabId> {
        self.restore_archived(self.archive.last()?.tab.id, now)
    }

    /// Restores a chosen archived tab to its original Space and focuses it.
    pub fn restore_archived(&mut self, id: TabId, now: SystemTime) -> Option<TabId> {
        let index = self.archive.iter().position(|archived| archived.tab.id == id)?;
        let source = &self.archive[index];
        let target = self.spaces.iter().position(|space| space.id == source.space && space.user_profile_id == source.user_profile_id)
            .or_else(|| self.spaces.iter().position(|space| space.user_profile_id == source.user_profile_id))?;
        let mut archived = self.archive.remove(index);
        archived.space = self.spaces[target].id;
        if archived.split.is_some() { return Some(self.restore_split_archived(archived, now)); }
        let index = self
            .spaces
            .iter()
            .position(|s| s.id == archived.space)
            .unwrap_or(self.active_space);
        self.active_space = index;
        let mut tab = archived.tab;
        tab.last_active = now;
        let id = tab.id;
        let space = self.space_mut();
        space.today.insert(0, tab);
        space.active_tab = Some(id);
        Some(id)
    }

    /// Permanently removes an archive entry without changing open tabs.
    pub fn delete_archived(&mut self, id: TabId) -> bool {
        let before = self.archive.len();
        self.archive.retain(|archived| archived.tab.id != id);
        self.archive.len() != before
    }

    /// Focuses the `n`th sidebar item (0-based, optional Favorites, Pinned, Today), or the
    /// last one for `None`. Like Arc's ⌘1…⌘8 and ⌘9.
    pub fn activate_item(&mut self, n: Option<usize>, now: SystemTime) {
        let ids: Vec<TabId> = self.active_user_profile().favorites.iter().filter(|_| self.active_user_profile().include_favorites_in_ordering).chain(self.space().tabs()).filter(|tab| !self.is_split_secondary(tab.id)).map(|t| t.id).collect();
        let id = match n {
            Some(n) => ids.get(n),
            None => ids.last(),
        };
        if let Some(&id) = id {
            self.activate_sidebar_item(id, now);
        }
    }

    /// Moves a tab between Pinned and Today. Newly pinned tabs go to the
    /// bottom of Pinned; unpinned tabs go to the top of Today.
    pub fn toggle_pin(&mut self, id: TabId) {
        if !self.can_edit_persistent_sidebar() { return; }
        let before = undo::PinState::capture(self, id);
        self.toggle_pin_unrecorded(id);
        if let Some(before) = before {
            if let Some(after) = undo::PinState::capture(self, id) {
                self.sidebar_history.record_pin(before, after);
            }
        }
    }

    fn toggle_pin_unrecorded(&mut self, id: TabId) {
        if self.split_for(id).is_some() { self.toggle_split_pin(id); return; }
        if self.is_favorite(id) {
            self.unfavorite_tab(id);
            return;
        }
        self.forget_pinned_item(id);
        let space = self.space_mut();
        match space.find(id) {
            Some((Section::Pinned, i)) => {
                let mut tab = space.pinned.remove(i);
                tab.pinned_url = None;
                tab.pinned_title = None;
                space.today.insert(0, tab);
            }
            Some((Section::Today, i)) => {
                let mut tab = space.today.remove(i);
                tab.pinned_url = Some(tab.url.clone());
                tab.pinned_title = tab.page_title.clone().or_else(|| Some(tab.title.clone()));
                space.pinned.push(tab);
            }
            None => {}
        }
        self.repair_folders();
    }

    /// Moves an existing tab without cloning it. Source focus moves to a neighbor;
    /// the caller chooses whether to focus the destination Space and tab.
    pub fn move_tab(&mut self, id: TabId, destination: SpaceId, section: Section) -> bool {
        if section == Section::Pinned && !self.can_edit_persistent_sidebar() { return false; }
        let Some(target_profile) = self.spaces.iter().find(|space| space.id == destination).map(|space| space.user_profile_id) else { return false; };
        if self.user_profile_id_for_tab(id) != Some(target_profile) { return false; }
        if self.split_for(id).is_some() { return self.move_split(id, destination, section); }
        let Some(target) = self.spaces.iter().position(|space| space.id == destination) else { return false; };
        if let Some(mut tab) = self.take_favorite(id) {
            match section {
                Section::Pinned => {
                    self.spaces[target].pinned.push(tab);
                    self.spaces[target].pinned_collapsed = false;
                }
                Section::Today => {
                    tab.pinned_url = None;
                    tab.pinned_title = None;
                    self.spaces[target].today.insert(0, tab);
                }
            }
            self.repair_folders();
            return true;
        }
        let Some((source, old_section, index)) = self.spaces.iter().enumerate().find_map(|(index, space)| space.find(id).map(|(section, tab)| (index, section, tab))) else { return false; };
        if source == target && old_section == section { return false; }
        self.forget_pinned_item(id);
        let source_space = &mut self.spaces[source];
        let mut tab = source_space.section_mut(old_section).remove(index);
        if source_space.active_tab == Some(id) { source_space.active_tab = neighbor(source_space, old_section, index); }
        match section {
            Section::Pinned => {
                if old_section != Section::Pinned {
                    tab.pinned_url = Some(tab.url.clone());
                    tab.pinned_title = tab.page_title.clone().or_else(|| Some(tab.title.clone()));
                }
                self.spaces[target].pinned.push(tab);
                self.spaces[target].pinned_collapsed = false;
            }
            Section::Today => {
                tab.pinned_url = None;
                tab.pinned_title = None;
                self.spaces[target].today.insert(0, tab);
            }
        }
        self.repair_folders();
        true
    }

    /// Returns a pinned tab to the URL it was pinned at, keeping its identity.
    pub fn reset_pinned_tab(&mut self, id: TabId) -> bool {
        let ids = self.related_tabs(id);
        let mut changed = false;
        for id in ids { changed |= self.tab_mut(id).is_some_and(Tab::reset_to_pinned_url); }
        changed
    }

    /// Resets every pinned page in the current Space, leaving Today untouched.
    /// The returned pages should be unloaded to reset their navigation state too.
    pub fn reset_space(&mut self) -> Vec<TabId> {
        self.space_mut().pinned.iter_mut().filter_map(|tab| {
            tab.reset_to_pinned_url().then_some(tab.id)
        }).collect()
    }

    /// Changes the original pinned URL without navigating the current page.
    pub fn replace_pinned_url(&mut self, id: TabId) -> bool {
        let ids = self.related_tabs(id);
        let mut changed = false;
        for id in ids {
            if let Some(tab) = self.tab_mut(id).filter(|tab| tab.pinned_url.is_some()) {
                tab.pinned_url = Some(tab.url.clone());
                tab.pinned_title = tab.page_title.clone().or_else(|| Some(tab.title.clone()));
                changed = true;
            }
        }
        changed
    }

    /// Loads `url` in an existing tab of any space.
    pub fn navigate(&mut self, id: TabId, url: impl Into<String>) {
        let url = url.into();
        if let Some(tab) = self.tab_mut(id) {
            tab.page_title = None;
            tab.title = tab.custom_title.clone().unwrap_or_else(|| url.clone());
            tab.url = url;
        }
    }

    /// Records the URL a tab's page actually shows (after redirects or
    /// in-page navigation) without touching its title.
    pub fn set_url(&mut self, id: TabId, url: impl Into<String>) {
        if let Some(tab) = self.tab_mut(id) { tab.url = url.into(); }
    }

    /// Every open tab, with shared Favorites included once.
    pub fn all_tabs(&self) -> impl Iterator<Item = &Tab> {
        self.profiles.values().flat_map(|profile| &profile.favorites).chain(self.spaces.iter().flat_map(|space| space.tabs()))
    }

    fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.all_tabs_mut().find(|tab| tab.id == id)
    }

    fn all_tabs_mut(&mut self) -> impl Iterator<Item = &mut Tab> {
        self.profiles.values_mut().flat_map(|profile| &mut profile.favorites).chain(self.spaces.iter_mut().flat_map(|space| {
            space.pinned.iter_mut().chain(&mut space.today)
        }))
    }

    pub fn set_title(&mut self, id: TabId, title: impl Into<String>) {
        let title = title.into();
        if let Some(tab) = self.tab_mut(id) {
            tab.page_title = Some(title.clone());
            if tab.pinned_url.as_ref() == Some(&tab.url) { tab.pinned_title = Some(title.clone()); }
            tab.title = tab.custom_title.clone().unwrap_or(title);
        }
    }

    pub fn rename_tab(&mut self, id: TabId, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() { return false; }
        if self.split_for(id).is_some() { return self.rename_split(id, Some(name.to_string())); }
        let Some(tab) = self.tab_mut(id) else { return false; };
        // Preserve the page label from sessions saved before page_title existed.
        if tab.custom_title.is_none() && tab.page_title.is_none() { tab.page_title = Some(tab.title.clone()); }
        tab.custom_title = Some(name.to_string());
        tab.title = name.to_string();
        true
    }

    pub fn reset_tab_name(&mut self, id: TabId) -> bool {
        if self.split_for(id).is_some() { return self.rename_split(id, None); }
        let Some(tab) = self.tab_mut(id) else { return false; };
        if tab.custom_title.take().is_none() { return false; }
        tab.title = tab.page_title.clone().unwrap_or_else(|| tab.url.clone());
        true
    }

    /// Archives Today tabs idle longer than `archive_after` in every space.
    /// Favorites, Pinned tabs and each Space's focused tab are never archived.
    /// Returns the number of archived tabs.
    pub fn archive_idle(&mut self, now: SystemTime) -> usize {
        self.archive_idle_except(now, &[])
    }

    /// The profile owner also protects selections from its other windows.
    pub fn archive_idle_except(&mut self, now: SystemTime, protected: &[TabId]) -> usize {
        self.archive_split_idle(now, protected)
    }
}

/// The tab to focus after the one at `i` in `section` is removed.
fn neighbor(space: &Space, section: Section, i: usize) -> Option<TabId> {
    let tabs = match section {
        Section::Pinned => &space.pinned,
        Section::Today => &space.today,
    };
    tabs.get(i)
        .or_else(|| i.checked_sub(1).and_then(|j| tabs.get(j)))
        .map(|t| t.id)
        .or_else(|| space.tabs().next().map(|t| t.id))
        .map(|id| space.splits.iter().find(|split| split.panes.contains(&id)).map_or(id, |split| split.active))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(3600);

    fn t0() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    fn ids(tabs: &[Tab]) -> Vec<TabId> {
        tabs.iter().map(|t| t.id).collect()
    }

    #[test]
    fn new_tabs_open_at_top_of_today_and_take_focus() {
        let mut b = Browser::new();
        let a = b.new_tab("a", t0());
        let c = b.new_tab("c", t0());
        assert_eq!(ids(&b.space().today), vec![c, a]);
        assert_eq!(b.active_tab().unwrap().id, c);
    }

    #[test]
    fn closing_focused_tab_focuses_neighbor() {
        let mut b = Browser::new();
        let a = b.new_tab("a", t0());
        let c = b.new_tab("c", t0());
        let d = b.new_tab("d", t0()); // order: d, c, a
        b.activate_tab(c, t0());
        assert_eq!(b.close_tab(c, t0()), Some(Closed::Archived));
        assert_eq!(b.active_tab().unwrap().id, a);
        b.close_tab(a, t0());
        assert_eq!(b.active_tab().unwrap().id, d);
        b.close_tab(d, t0());
        assert!(b.active_tab().is_none());
        assert_eq!(b.archive.iter().map(|a| a.tab.id).collect::<Vec<_>>(), vec![c, a, d]);
    }

    #[test]
    fn closing_a_pinned_tab_keeps_it_at_its_pinned_page() {
        let mut b = Browser::new();
        let p = b.new_tab("https://pinned", t0());
        b.toggle_pin(p);
        let a = b.new_tab("a", t0());
        b.activate_tab(p, t0());
        b.navigate(p, "https://pinned/elsewhere");

        assert_eq!(b.close_tab(p, t0()), Some(Closed::Unloaded));

        assert_eq!(ids(&b.space().pinned), vec![p]);
        assert_eq!(b.space().pinned[0].url, "https://pinned");
        assert_eq!(b.active_tab().unwrap().id, a);
        assert!(b.archive.is_empty());
    }

    #[test]
    fn closing_the_only_pinned_tab_leaves_nothing_focused() {
        let mut b = Browser::new();
        let p = b.new_tab("p", t0());
        b.toggle_pin(p);
        b.close_tab(p, t0());
        assert!(b.active_tab().is_none());
    }

    #[test]
    fn restore_brings_back_the_last_archived_tab_in_its_space() {
        let mut b = Browser::new();
        let a = b.new_tab("a", t0());
        b.close_tab(a, t0());
        b.add_space("Work", 200.0);
        b.switch_space(1);

        assert_eq!(b.restore_last_archived(t0() + HOUR), Some(a));

        assert_eq!(b.active_space, 0);
        assert_eq!(b.active_tab().unwrap().id, a);
        assert_eq!(b.active_tab().unwrap().last_active, t0() + HOUR);
        assert!(b.archive.is_empty());
        assert_eq!(b.restore_last_archived(t0()), None);
    }

    #[test]
    fn clear_archives_background_today_and_keeps_the_selected_page() {
        let mut b = Browser::new();
        let p = b.new_tab("p", t0());
        b.toggle_pin(p);
        b.new_tab("a", t0());
        let active = b.new_tab("c", t0());
        assert_eq!(b.clear_today(t0()), 1);
        assert_eq!(b.space().today.len(), 1);
        assert_eq!(b.active_tab().unwrap().id, active);
        assert_eq!(b.archive.len(), 1);
        assert_eq!(b.clear_today(t0()), 0);
    }

    #[test]
    fn activate_item_counts_pinned_then_today() {
        let mut b = Browser::new();
        let p = b.new_tab("p", t0());
        b.toggle_pin(p);
        let a = b.new_tab("a", t0());
        let c = b.new_tab("c", t0()); // sidebar: p | c, a
        b.activate_item(Some(0), t0());
        assert_eq!(b.active_tab().unwrap().id, p);
        b.activate_item(Some(1), t0());
        assert_eq!(b.active_tab().unwrap().id, c);
        b.activate_item(None, t0());
        assert_eq!(b.active_tab().unwrap().id, a);
        b.activate_item(Some(7), t0());
        assert_eq!(b.active_tab().unwrap().id, a);
    }

    #[test]
    fn session_round_trips_and_keeps_ids_unique() {
        let mut b = Browser::new();
        let p = b.new_tab("https://p", t0());
        b.toggle_pin(p);
        let a = b.new_tab("https://a", t0());
        b.close_tab(a, t0());
        b.add_space("Work", 200.0);
        b.switch_space(1);

        let mut restored = session::from_json(&session::to_json(&b)).unwrap();

        assert_eq!(restored.active_space, 1);
        assert_eq!(restored.spaces[0].pinned[0].pinned_url.as_deref(), Some("https://p"));
        assert_eq!(restored.archive[0].tab.id, a);
        let new = restored.new_tab("n", t0());
        assert!(new != p && new != a);
    }

    #[test]
    fn cycle_tab_walks_pinned_then_today_and_wraps() {
        let mut b = Browser::new();
        let p = b.new_tab("p", t0());
        b.toggle_pin(p);
        let a = b.new_tab("a", t0());
        let c = b.new_tab("c", t0()); // sidebar: p | c, a
        assert_eq!(b.active_tab().unwrap().id, c);
        b.cycle_tab(1, t0());
        assert_eq!(b.active_tab().unwrap().id, a);
        b.cycle_tab(1, t0());
        assert_eq!(b.active_tab().unwrap().id, p);
        b.cycle_tab(-1, t0());
        assert_eq!(b.active_tab().unwrap().id, a);
    }

    #[test]
    fn toggle_pin_moves_between_sections() {
        let mut b = Browser::new();
        let a = b.new_tab("a", t0());
        b.toggle_pin(a);
        assert_eq!(ids(&b.space().pinned), vec![a]);
        assert_eq!(b.space().pinned[0].pinned_url.as_deref(), Some("a"));
        assert!(b.space().today.is_empty());
        b.toggle_pin(a);
        assert_eq!(ids(&b.space().today), vec![a]);
        assert_eq!(b.space().today[0].pinned_url, None);
    }

    #[test]
    fn archives_only_idle_unpinned_unfocused_tabs() {
        let mut b = Browser::new();
        let old = b.new_tab("old", t0());
        let pinned = b.new_tab("pinned", t0());
        b.toggle_pin(pinned);
        let fresh = b.new_tab("fresh", t0() + 11 * HOUR);
        let focused = b.new_tab("focused", t0()); // focused but stale

        let archived = b.archive_idle(t0() + 12 * HOUR);

        assert_eq!(archived, 1);
        assert_eq!(b.archive[0].tab.id, old);
        assert_eq!(b.archive[0].reason, ArchiveReason::Automatic);
        assert_eq!(ids(&b.space().today), vec![focused, fresh]);
        assert_eq!(ids(&b.space().pinned), vec![pinned]);
    }

    #[test]
    fn archiving_can_be_disabled() {
        let mut b = Browser::new();
        b.new_tab("a", t0());
        b.new_tab("b", t0());
        b.active_user_profile_mut().archive_after = None;
        assert_eq!(b.archive_idle(t0() + 1000 * HOUR), 0);
    }

    #[test]
    fn spaces_keep_separate_tabs_and_focus() {
        let mut b = Browser::new();
        let a = b.new_tab("a", t0());
        b.add_space("Work", 200.0);
        b.switch_space(1);
        assert!(b.active_tab().is_none());
        let w = b.new_tab("w", t0());
        assert_eq!(b.active_tab().unwrap().id, w);
        b.switch_space(0);
        assert_eq!(b.active_tab().unwrap().id, a);
    }

    #[test]
    fn restoring_a_chosen_entry_preserves_the_rest_of_the_archive() {
        let mut browser = Browser::new();
        let a = browser.new_tab("a", t0());
        let b = browser.new_tab("b", t0());
        browser.close_tab(a, t0());
        browser.close_tab(b, t0());
        browser.add_space("Work", 200.0);
        browser.switch_space(1);
        assert_eq!(browser.restore_archived(a, t0() + HOUR), Some(a));
        assert_eq!(browser.active_space, 0);
        assert_eq!(browser.active_tab().unwrap().id, a);
        assert_eq!(browser.archive.len(), 1);
        assert_eq!(browser.archive[0].tab.id, b);
        assert!(browser.delete_archived(b));
        assert!(!browser.delete_archived(b));
        assert_eq!(browser.restore_archived(a, t0()), None);
        assert_eq!(browser.active_tab().unwrap().id, a);
    }

    #[test]
    fn archive_reasons_survive_sessions_and_old_entries_are_unknown() {
        let mut browser = Browser::new();
        let tab = browser.new_tab("manual", t0());
        browser.close_tab(tab, t0());
        assert_eq!(browser.archive[0].reason, ArchiveReason::Manual);
        let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
        json["archive"][0].as_object_mut().unwrap().remove("reason");
        let restored = session::from_json(&json.to_string()).unwrap();
        assert_eq!(restored.archive[0].reason, ArchiveReason::Unknown);
    }


    #[test]
    fn pinned_url_can_be_reset_or_replaced_without_unpinning() {
        let mut browser = Browser::new();
        let tab = browser.new_tab("https://original", t0());
        browser.toggle_pin(tab);
        browser.navigate(tab, "https://original/elsewhere");
        assert!(browser.reset_pinned_tab(tab));
        assert_eq!(browser.active_tab().unwrap().url, "https://original");
        browser.navigate(tab, "https://replacement");
        assert!(browser.replace_pinned_url(tab));
        browser.navigate(tab, "https://replacement/elsewhere");
        assert!(browser.reset_pinned_tab(tab));
        assert_eq!(browser.active_tab().unwrap().url, "https://replacement");
        assert_eq!(browser.space().pinned.len(), 1);
        assert_eq!(browser.space().pinned[0].id, tab);
        let unpinned = browser.new_tab("https://today", t0());
        assert!(!browser.reset_pinned_tab(unpinned));
        assert!(!browser.replace_pinned_url(unpinned));
    }


    #[test]
    fn resetting_a_space_preserves_today_focus_and_other_spaces() {
        let mut browser = Browser::new();
        let pinned = browser.new_tab("https://pinned", t0());
        browser.toggle_pin(pinned);
        browser.navigate(pinned, "https://pinned/elsewhere");
        let today = browser.new_tab("https://today", t0());
        browser.add_space("Work", 200.0);
        browser.switch_space(1);
        let other = browser.new_tab("https://other", t0());
        browser.toggle_pin(other);
        browser.navigate(other, "https://other/elsewhere");
        browser.switch_space(0);
        assert_eq!(browser.reset_space(), vec![pinned]);
        assert_eq!(browser.space().pinned[0].url, "https://pinned");
        assert_eq!(browser.active_tab().unwrap().id, today);
        assert_eq!(browser.active_tab().unwrap().url, "https://today");
        assert_eq!(browser.spaces[1].pinned[0].url, "https://other/elsewhere");
        assert!(browser.archive.is_empty());
    }


    #[test]
    fn custom_tab_names_survive_navigation_and_pinned_resets() {
        let mut browser = Browser::new();
        let tab = browser.new_tab("https://original", t0());
        browser.set_title(tab, "Original page");
        browser.toggle_pin(tab);
        assert!(browser.rename_tab(tab, "My page"));
        assert!(!browser.rename_tab(tab, "   "));
        browser.navigate(tab, "https://elsewhere");
        browser.set_title(tab, "Elsewhere page");
        assert_eq!(browser.active_tab().unwrap().title, "My page");
        assert!(browser.reset_tab_name(tab));
        assert_eq!(browser.active_tab().unwrap().title, "Elsewhere page");
        browser.rename_tab(tab, "My page");
        browser.reset_space();
        assert_eq!(browser.active_tab().unwrap().title, "My page");
        assert_eq!(browser.active_tab().unwrap().url, "https://original");
        browser.reset_tab_name(tab);
        assert_eq!(browser.active_tab().unwrap().title, "Original page");
    }

    #[test]
    fn renamed_tabs_survive_archive_restore_and_session_reload() {
        let mut browser = Browser::new();
        let tab = browser.new_tab("https://original", t0());
        browser.set_title(tab, "Original page");
        browser.rename_tab(tab, "My page");
        browser.close_tab(tab, t0());
        let mut restored = session::from_json(&session::to_json(&browser)).unwrap();
        restored.restore_archived(tab, t0() + HOUR);
        restored.set_title(tab, "Updated page");
        assert_eq!(restored.active_tab().unwrap().title, "My page");
        restored.reset_tab_name(tab);
        assert_eq!(restored.active_tab().unwrap().title, "Updated page");
    }


    #[test]
    fn legacy_sessions_can_rename_and_restore_their_existing_tab_labels() {
        let mut browser = Browser::new();
        let id = browser.new_tab("https://legacy", t0());
        browser.set_title(id, "Legacy page title");
        let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
        json["profiles"]["1"].as_object_mut().unwrap().remove("history");
        let tab = json["spaces"][0]["today"][0].as_object_mut().unwrap();
        for field in ["custom_title", "page_title", "pinned_title"] { tab.remove(field); }
        let mut restored = session::from_json(&json.to_string()).unwrap();
        assert!(restored.rename_tab(id, "Custom label"));
        assert!(restored.reset_tab_name(id));
        assert_eq!(restored.active_tab().unwrap().title, "Legacy page title");
    }


    #[test]
    fn moving_tabs_preserves_identity_labels_and_original_pin_urls() {
        let mut browser = Browser::new();
        let tab = browser.new_tab("https://original", t0());
        browser.set_title(tab, "Original title");
        browser.rename_tab(tab, "Custom title");
        browser.toggle_pin(tab);
        browser.navigate(tab, "https://elsewhere");
        let today = browser.new_tab("https://today", t0());
        let work = browser.add_space("Work", 200.0);
        assert!(browser.move_tab(tab, work, Section::Pinned));
        assert_eq!(browser.active_tab().unwrap().id, today);
        assert_eq!(browser.spaces[1].pinned[0].id, tab);
        assert_eq!(browser.spaces[1].pinned[0].url, "https://elsewhere");
        assert_eq!(browser.spaces[1].pinned[0].pinned_url.as_deref(), Some("https://original"));
        assert_eq!(browser.spaces[1].pinned[0].title, "Custom title");
        assert!(!browser.move_tab(tab, work, Section::Pinned));
        assert!(browser.move_tab(tab, work, Section::Today));
        assert_eq!(browser.spaces[1].today[0].id, tab);
        assert_eq!(browser.spaces[1].today[0].pinned_url, None);
        assert_eq!(browser.spaces[1].today[0].title, "Custom title");
        assert!(browser.archive.is_empty());
    }

}

pub mod space_creation;
