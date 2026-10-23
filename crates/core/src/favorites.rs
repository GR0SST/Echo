//! Profile-wide Favorites retain one tab identity across all Spaces.
use crate::{neighbor, Browser, Tab, TabId};

pub const MAX_FAVORITES: usize = 12;

impl Browser {
    pub fn is_favorite(&self, id: TabId) -> bool {
        self.favorites.iter().any(|tab| tab.id == id)
    }

    /// Moves an existing tab into the shared dock, preserving its loaded page.
    /// An existing Pinned tab keeps its original URL; Today captures its URL.
    pub fn favorite_tab(&mut self, id: TabId) -> bool {
        if self.split_for(id).is_some() { return self.favorite_split(id); }
        if self.is_favorite(id) || self.favorite_count() >= MAX_FAVORITES { return false; }
        let Some((space, section, index)) = self.spaces.iter().enumerate().find_map(|(i, space)| {
            space.find(id).map(|(section, index)| (i, section, index))
        }) else { return false; };
        self.forget_pinned_item(id);
        let source = &mut self.spaces[space];
        let mut tab = source.section_mut(section).remove(index);
        if source.active_tab == Some(id) { source.active_tab = neighbor(source, section, index); }
        if tab.pinned_url.is_none() {
            tab.pinned_url = Some(tab.url.clone());
            tab.pinned_title = tab.page_title.clone().or_else(|| Some(tab.title.clone()));
        }
        self.favorites.push(tab);
        self.space_mut().active_tab = Some(id);
        self.repair_folders();
        true
    }

    /// Returns a Favorite to Today in the current Space.
    pub fn unfavorite_tab(&mut self, id: TabId) -> bool {
        if self.split_for(id).is_some() { return self.unfavorite_split(id); }
        let Some(mut tab) = self.take_favorite(id) else { return false; };
        tab.pinned_url = None;
        tab.pinned_title = None;
        let space = self.space_mut();
        space.today.insert(0, tab);
        space.active_tab = Some(id);
        true
    }

    pub(crate) fn take_favorite(&mut self, id: TabId) -> Option<Tab> {
        let index = self.favorites.iter().position(|tab| tab.id == id)?;
        let tab = self.favorites.remove(index);
        // A shared tab may be selected in more than one Space. Once it moves
        // to a local section, those Spaces must not retain a dangling selection.
        self.clear_favorite_selection(id);
        Some(tab)
    }

    pub(crate) fn close_favorite(&mut self, id: TabId) {
        if let Some(tab) = self.favorites.iter_mut().find(|tab| tab.id == id) {
            tab.reset_to_pinned_url();
        }
        self.clear_favorite_selection(id);
    }

    fn clear_favorite_selection(&mut self, id: TabId) {
        for space in &mut self.spaces {
            if space.active_tab == Some(id) {
                let next = space.tabs().next().map(|tab| tab.id);
                space.active_tab = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Closed, Section};
    use std::time::{Duration, SystemTime};
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;

    #[test]
    fn favorites_share_identity_and_close_back_to_their_original_page() {
        let mut browser = Browser::new();
        let id = browser.new_tab("https://original", NOW);
        browser.set_title(id, "Original page");
        browser.toggle_pin(id);
        browser.navigate(id, "https://elsewhere");
        browser.rename_tab(id, "My app");
        assert!(browser.favorite_tab(id));
        assert!(browser.space().pinned.is_empty());
        browser.add_space("Work", 20.);
        browser.switch_space(1);
        browser.activate_tab(id, NOW);
        browser.set_url(id, "https://redirect");
        browser.set_title(id, "Redirected page");
        assert_eq!(browser.active_tab().unwrap().title, "My app");
        assert_eq!(browser.all_tabs().count(), 1);
        assert_eq!(browser.close_tab(id, NOW), Some(Closed::Unloaded));
        assert_eq!(browser.favorites[0].url, "https://original");
        assert_eq!(browser.favorites[0].title, "My app");
        assert_eq!(browser.favorites[0].page_title.as_deref(), Some("Original page"));
        assert!(browser.spaces.iter().all(|space| space.active_tab.is_none()));
        assert!(browser.archive.is_empty());
        browser.activate_tab(id, NOW);
        assert!(browser.reset_tab_name(id));
        assert_eq!(browser.active_tab().unwrap().title, "Original page");
    }

    #[test]
    fn unfavoriting_and_moving_clear_other_spaces_shared_selections() {
        let mut browser = Browser::new();
        let id = browser.new_tab("https://app", NOW);
        browser.favorite_tab(id);
        let work = browser.add_space("Work", 20.);
        browser.switch_space(1);
        browser.activate_tab(id, NOW);
        assert!(browser.unfavorite_tab(id));
        assert!(browser.spaces[0].active_tab.is_none());
        assert_eq!(browser.space().today[0].id, id);
        assert!(browser.space().today[0].pinned_url.is_none());
        assert!(browser.favorite_tab(id));
        assert!(browser.move_tab(id, work, Section::Pinned));
        assert_eq!(browser.space().pinned[0].pinned_url.as_deref(), Some("https://app"));
        assert!(browser.favorite_tab(id));
        browser.navigate(id, "https://app/next");
        assert!(browser.replace_pinned_url(id));
        assert_eq!(browser.favorites[0].pinned_url.as_deref(), Some("https://app/next"));
        assert!(browser.move_tab(id, work, Section::Today));
        assert!(browser.space().today[0].pinned_url.is_none());
        assert!(browser.favorites.is_empty());
        assert_eq!(browser.all_tabs().count(), 1);
    }

    #[test]
    fn favorite_limit_and_sessions_preserve_tabs_and_allocate_unique_ids() {
        let mut browser = Browser::new();
        for index in 0..MAX_FAVORITES {
            let id = browser.new_tab(format!("https://app/{index}"), NOW);
            assert!(browser.favorite_tab(id));
        }
        let extra = browser.new_tab("https://extra", NOW);
        assert!(!browser.favorite_tab(extra));
        assert_eq!(browser.space().today[0].id, extra);
        let mut json: serde_json::Value = serde_json::from_str(&crate::session::to_json(&browser)).unwrap();
        json["next_id"] = 1.into();
        let mut restored = crate::session::from_json(&json.to_string()).unwrap();
        assert_eq!(restored.favorites.len(), MAX_FAVORITES);
        assert!(restored.new_tab("https://new", NOW).get() > extra.get());
        json.as_object_mut().unwrap().remove("favorites");
        assert!(crate::session::from_json(&json.to_string()).unwrap().favorites.is_empty());
    }

    #[test]
    fn favorites_precede_local_items_and_survive_clear_reset_and_auto_archive() {
        let mut browser = Browser::new();
        let favorite = browser.new_tab("https://app", NOW);
        browser.favorite_tab(favorite);
        let local = browser.new_tab("https://local", NOW);
        browser.toggle_pin(local);
        browser.activate_item(Some(0), NOW);
        assert_eq!(browser.active_tab().unwrap().id, favorite);
        browser.cycle_tab(1, NOW);
        assert_eq!(browser.active_tab().unwrap().id, local);
        browser.cycle_tab(1, NOW);
        assert_eq!(browser.active_tab().unwrap().id, favorite);
        browser.navigate(favorite, "https://app/next");
        assert_eq!(browser.reset_space(), vec![local]);
        assert_eq!(browser.favorites[0].url, "https://app/next");
        browser.new_tab("https://today", NOW);
        assert_eq!(browser.clear_today(NOW), 1);
        browser.archive_idle(NOW + Duration::from_secs(86400));
        assert_eq!(browser.favorites[0].id, favorite);
        assert_eq!(browser.archive.len(), 1);
    }
}
