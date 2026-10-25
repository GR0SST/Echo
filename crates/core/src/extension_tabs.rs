//! Profile-scoped model operations used by the extension API transport.
use crate::{Browser, TabId, UserProfileId};
use std::time::SystemTime;

#[derive(Clone, Debug, Default)]
pub struct ExtensionTabUpdate {
    pub url: Option<String>,
    pub active: bool,
    pub pinned: Option<bool>,
    pub muted: Option<bool>,
}
impl Browser {
    fn extension_space(&self, profile: UserProfileId, tab: Option<TabId>) -> Result<usize, String> {
        if self.user_profile(profile).is_none()
            || tab.is_some_and(|tab| self.user_profile_id_for_tab(tab) != Some(profile))
        {
            return Err("The target tab is absent from this profile".into());
        }
        if let Some(tab) = tab.filter(|tab| !self.is_favorite(*tab)) {
            return self
                .spaces
                .iter()
                .position(|space| {
                    space
                        .pinned
                        .iter()
                        .chain(&space.today)
                        .any(|item| item.id == tab)
                })
                .ok_or_else(|| "The target tab no longer has a Space".into());
        }
        if self.active_user_profile_id() == profile {
            return Ok(self.active_space);
        }
        self.spaces
            .iter()
            .position(|space| space.user_profile_id == profile)
            .ok_or_else(|| "The extension profile has no surviving Space".into())
    }
    pub fn extension_create_tab(
        &mut self,
        profile: UserProfileId,
        url: &str,
        active: bool,
        pinned: bool,
        index: Option<i32>,
        now: SystemTime,
    ) -> Result<TabId, String> {
        let space = self.extension_space(profile, None)?;
        if pinned && !self.can_edit_persistent_sidebar() {
            return Err("Pinned tabs are unavailable in this window".into());
        }
        let mut selection = self.window_selection();
        self.switch_space(space);
        let tab = self.new_tab(url, now);
        if pinned {
            self.toggle_pin(tab);
        }
        if let Some(index) = index {
            // Pinning is structural in Echo. Clamp insertion inside its target
            // section, preserving Favorites, other Spaces and their ownership.
            let favorites = self.active_user_profile().favorites.len();
            let pinned_count = self.space().pinned.len();
            let offset = favorites + if pinned { 0 } else { pinned_count };
            let rows = if pinned {
                &mut self.spaces[space].pinned
            } else {
                &mut self.spaces[space].today
            };
            let from = rows
                .iter()
                .position(|item| item.id == tab)
                .expect("new model tab");
            let item = rows.remove(from);
            let target = if index < 0 {
                rows.len()
            } else {
                (index as usize).saturating_sub(offset).min(rows.len())
            };
            rows.insert(target, item);
        }
        if !active {
            self.edit_in_window(&mut selection, |_| {});
        }
        Ok(tab)
    }
    pub fn extension_update_tab(
        &mut self,
        profile: UserProfileId,
        tab: TabId,
        update: &ExtensionTabUpdate,
        now: SystemTime,
    ) -> Result<(), String> {
        let space = self.extension_space(profile, Some(tab))?;
        let pinned =
            self.is_favorite(tab) || self.spaces[space].pinned.iter().any(|item| item.id == tab);
        if update.pinned.is_some_and(|wanted| wanted != pinned)
            && !self.can_edit_persistent_sidebar()
        {
            return Err("Pinned tabs are unavailable in this window".into());
        }
        let mut selection = self.window_selection();
        self.switch_space(space);
        if let Some(url) = &update.url {
            self.navigate(tab, url);
        }
        if update.pinned.is_some_and(|wanted| wanted != pinned) {
            self.toggle_pin(tab);
        }
        if let Some(muted) = update.muted {
            self.set_tab_muted(tab, muted);
        }
        if update.active {
            self.activate_tab(tab, now);
        } else {
            self.edit_in_window(&mut selection, |_| {});
        }
        Ok(())
    }
    pub fn extension_remove_tab(
        &mut self,
        profile: UserProfileId,
        tab: TabId,
        now: SystemTime,
    ) -> Result<(), String> {
        let space = self.extension_space(profile, Some(tab))?;
        let mut selection = self.window_selection();
        let was_selected = self.active_tab().is_some_and(|active| active.id == tab);
        self.switch_space(space);
        // This API removes the model tab. User Cmd+W's retained-pin/unload
        // operation remains a separate command.
        if self.is_favorite(tab) {
            self.unfavorite_tab(tab);
        } else if self.space().pinned.iter().any(|item| item.id == tab) {
            self.toggle_pin(tab);
        }
        self.close_tab(tab, now)
            .ok_or("The target tab could not be closed")?;
        if !was_selected {
            self.edit_in_window(&mut selection, |_| {});
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;
    #[test]
    fn background_creation_and_updates_preserve_another_profiles_selection_and_data() {
        let mut browser = Browser::new();
        let first = browser.active_user_profile_id();
        let first_tab = browser.new_tab("https://first.example/", NOW);
        let second = browser.create_user_profile("Second").unwrap();
        browser.add_space("Second Space", 30.);
        let second_space = browser.spaces.last().unwrap().id;
        browser
            .assign_space_user_profile(second_space, second)
            .unwrap();
        browser.switch_space(browser.spaces.len() - 1);
        let second_tab = browser.new_tab("https://second.example/", NOW);
        let created = browser
            .extension_create_tab(first, "https://new.example/", false, false, None, NOW)
            .unwrap();
        assert_eq!(browser.active_tab().unwrap().id, second_tab);
        assert_eq!(browser.user_profile_id_for_tab(created), Some(first));
        browser
            .extension_update_tab(
                first,
                created,
                &ExtensionTabUpdate {
                    url: Some("https://updated.example/".into()),
                    muted: Some(true),
                    ..Default::default()
                },
                NOW,
            )
            .unwrap();
        assert_eq!(browser.active_tab().unwrap().id, second_tab);
        assert_eq!(
            browser
                .all_tabs()
                .find(|tab| tab.id == first_tab)
                .unwrap()
                .url,
            "https://first.example/"
        );
        assert!(browser
            .extension_update_tab(
                second,
                created,
                &ExtensionTabUpdate {
                    active: true,
                    ..Default::default()
                },
                NOW
            )
            .is_err());
        browser.extension_remove_tab(first, created, NOW).unwrap();
        assert_eq!(browser.active_tab().unwrap().id, second_tab);
        assert!(!browser.all_tabs().any(|tab| tab.id == created));
        assert_eq!(browser.spaces.len(), 2);
    }
    #[test]
    fn extension_remove_removes_a_pin_without_closing_its_neighbor_or_deleting_a_space() {
        let mut browser = Browser::new();
        let profile = browser.active_user_profile_id();
        let pinned = browser
            .extension_create_tab(profile, "https://pin.example/", true, true, None, NOW)
            .unwrap();
        let neighbor = browser.new_tab("https://neighbor.example/", NOW);
        browser.extension_remove_tab(profile, pinned, NOW).unwrap();
        assert!(!browser.all_tabs().any(|tab| tab.id == pinned));
        assert_eq!(browser.active_tab().unwrap().id, neighbor);
        assert_eq!(browser.spaces.len(), 1);
    }
}
