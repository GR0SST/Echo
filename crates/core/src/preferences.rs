//! Preferences that affect the sidebar without changing tab identity or focus.
use crate::{Browser, SpaceId};

impl Browser {
    pub fn set_space_header(&mut self, space: SpaceId, visible: bool) -> bool {
        let Some(space) = self.spaces.iter_mut().find(|entry| entry.id == space) else { return false; };
        let changed = space.show_header != visible;
        space.show_header = visible;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session, splits::Placement};
    use std::time::SystemTime;

    #[test]
    fn every_space_move_preserves_active_identity_and_each_spaces_complete_model() {
        let mut browser = Browser::new();
        browser.add_space("Work", 100.);
        browser.add_space("Research", 200.);
        for index in 0..3 {
            browser.switch_space(index);
            let a = browser.new_tab(format!("https://{index}-a.example"), SystemTime::UNIX_EPOCH);
            let b = browser.new_tab(format!("https://{index}-b.example"), SystemTime::UNIX_EPOCH);
            browser.add_split(a, b, Placement::Right, SystemTime::UNIX_EPOCH).unwrap();
            let folder = browser.create_folder(browser.space().id, None, "Folder").unwrap();
            browser.move_item_to_folder(a, folder);
        }
        let json = session::to_json(&browser);
        for active in 0..3 {
            for source in 0..3 {
                for destination in 0..3 {
                    let mut browser = session::from_json(&json).unwrap();
                    browser.switch_space(active);
                    let active_id = browser.space().id;
                    let active_tab = browser.space().active_tab;
                    let snapshots: Vec<_> = browser.spaces.iter().map(|space| (space.id, serde_json::to_value(space).unwrap())).collect();
                    let mut expected: Vec<_> = browser.spaces.iter().map(|space| space.id).collect();
                    let moved = expected.remove(source);
                    expected.insert(destination, moved);
                    assert_eq!(browser.move_space(moved, destination), source != destination);
                    assert_eq!(browser.spaces.iter().map(|space| space.id).collect::<Vec<_>>(), expected);
                    assert_eq!(browser.space().id, active_id);
                    assert_eq!(browser.space().active_tab, active_tab);
                    for space in &browser.spaces {
                        assert_eq!(serde_json::to_value(space).unwrap(), snapshots.iter().find(|(id, _)| *id == space.id).unwrap().1);
                    }
                    let loaded = session::from_json(&session::to_json(&browser)).unwrap();
                    assert_eq!(loaded.space().id, active_id);
                    assert_eq!(loaded.spaces.iter().map(|space| space.id).collect::<Vec<_>>(), expected);
                }
            }
        }
    }

    #[test]
    fn invalid_space_moves_are_no_ops() {
        let mut browser = Browser::new();
        let before = session::to_json(&browser);
        assert!(!browser.move_space(browser.space().id, 0));
        assert!(!browser.move_space(browser.space().id, 1));
        assert!(!browser.move_space(SpaceId(u64::MAX), 0));
        assert_eq!(session::to_json(&browser), before);
    }

    #[test]
    fn old_sessions_enable_headers_and_favorites_ordering_and_new_preferences_persist() {
        let browser = Browser::new();
        let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
        json["profiles"]["1"].as_object_mut().unwrap().remove("include_favorites_in_ordering");
        json["spaces"][0].as_object_mut().unwrap().remove("show_header");
        let mut loaded = session::from_json(&json.to_string()).unwrap();
        assert!(loaded.active_user_profile().include_favorites_in_ordering && loaded.space().show_header);
        loaded.active_user_profile_mut().include_favorites_in_ordering = false;
        loaded.set_space_header(loaded.space().id, false);
        let loaded = session::from_json(&session::to_json(&loaded)).unwrap();
        assert!(!loaded.active_user_profile().include_favorites_in_ordering && !loaded.space().show_header);
    }

    #[test]
    fn excluding_favorites_changes_numbered_items_but_not_tab_cycling_or_split_pane_focus() {
        let mut browser = Browser::new();
        let now = SystemTime::UNIX_EPOCH;
        let a = browser.new_tab("https://a", now);
        let b = browser.new_tab("https://b", now);
        browser.add_split(a, b, Placement::Right, now).unwrap();
        browser.favorite_tab(a);
        let local = browser.new_tab("https://local", now);
        browser.activate_item(Some(0), now);
        assert_eq!(browser.active_tab().unwrap().id, b);
        browser.active_user_profile_mut().include_favorites_in_ordering = false;
        browser.activate_item(Some(0), now);
        assert_eq!(browser.active_tab().unwrap().id, local);
        browser.activate_item(None, now);
        assert_eq!(browser.active_tab().unwrap().id, local);
        browser.cycle_tab(1, now);
        assert_eq!(browser.active_tab().unwrap().id, b);
        browser.cycle_tab(-1, now);
        assert_eq!(browser.active_tab().unwrap().id, local);
    }

    #[test]
    fn header_visibility_is_per_space_and_unknown_ids_do_not_change_preferences() {
        let mut browser = Browser::new();
        let other = browser.add_space("Work", 100.);
        assert!(browser.set_space_header(other, false));
        assert!(browser.space().show_header);
        assert!(!browser.spaces[1].show_header);
        assert!(!browser.set_space_header(SpaceId(u64::MAX), false));
        assert!(!browser.set_space_header(other, false));
    }
}
