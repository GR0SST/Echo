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
    fn old_sessions_enable_headers_and_favorites_ordering_and_new_preferences_persist() {
        let browser = Browser::new();
        let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
        json.as_object_mut().unwrap().remove("include_favorites_in_ordering");
        json["spaces"][0].as_object_mut().unwrap().remove("show_header");
        let mut loaded = session::from_json(&json.to_string()).unwrap();
        assert!(loaded.include_favorites_in_ordering && loaded.space().show_header);
        loaded.include_favorites_in_ordering = false;
        loaded.set_space_header(loaded.space().id, false);
        let loaded = session::from_json(&session::to_json(&loaded)).unwrap();
        assert!(!loaded.include_favorites_in_ordering && !loaded.space().show_header);
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
        browser.include_favorites_in_ordering = false;
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
