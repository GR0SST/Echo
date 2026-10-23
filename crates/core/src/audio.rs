//! Tab mute intent survives unloading; internal GPUI pages cannot play audio.
use crate::{omnibox::is_internal_url, Browser, TabId};

impl Browser {
    pub fn set_tab_muted(&mut self, id: TabId, muted: bool) -> bool {
        let Some(tab) = self.tab_mut(id).filter(|tab| !is_internal_url(&tab.url)) else { return false; };
        let changed = tab.muted != muted;
        tab.muted = muted;
        changed
    }

    pub fn set_all_tabs_muted(&mut self, muted: bool) -> usize {
        let mut changed = 0;
        for tab in self.all_tabs_mut() {
            if !is_internal_url(&tab.url) && tab.muted != muted {
                tab.muted = muted;
                changed += 1;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn mute_all_covers_favorites_and_other_spaces_without_touching_archive_or_history_pages() {
        let mut browser = Browser::new();
        let now = SystemTime::UNIX_EPOCH;
        let favorite = browser.new_tab("https://favorite", now);
        browser.favorite_tab(favorite);
        let archived = browser.new_tab("https://archived", now);
        browser.close_tab(archived, now);
        browser.add_space("Work", 20.);
        browser.switch_space(1);
        let local = browser.new_tab("https://local", now);
        let history = browser.new_tab("browser://history", now);
        assert!(!browser.set_tab_muted(history, true));
        assert_eq!(browser.set_all_tabs_muted(true), 2);
        assert_eq!(browser.set_all_tabs_muted(true), 0);
        assert!(browser.favorites[0].muted);
        assert!(!browser.archive[0].tab.muted);
        assert_eq!(browser.active_tab().unwrap().id, history);
        let mut restored = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert!(restored.favorites[0].muted);
        assert!(restored.all_tabs().find(|tab| tab.id == local).unwrap().muted);
        assert_eq!(restored.set_all_tabs_muted(false), 2);
        assert!(!restored.set_tab_muted(archived, true));
    }

    #[test]
    fn closing_and_restoring_a_tab_keeps_its_mute_intent() {
        let mut browser = Browser::new();
        let now = SystemTime::UNIX_EPOCH;
        let id = browser.new_tab("https://example.com", now);
        assert!(browser.set_tab_muted(id, true));
        browser.close_tab(id, now);
        browser.restore_last_archived(now);
        assert!(browser.active_tab().unwrap().muted);
        browser.toggle_pin(id);
        browser.close_tab(id, now);
        assert!(browser.space().pinned[0].muted);
    }
}
