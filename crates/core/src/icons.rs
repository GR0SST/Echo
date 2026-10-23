//! User-selected symbols are independent of site favicons and page titles.
use crate::{Browser, SpaceId, TabId};
use unicode_segmentation::UnicodeSegmentation;

pub fn valid_icon(icon: &str) -> bool {
    let icon = icon.trim();
    !icon.is_empty() && icon.len() <= 128 && icon.graphemes(true).count() == 1
        && !icon.chars().any(|character| character.is_control() || character.is_whitespace())
}

fn normalized(icon: Option<&str>) -> Option<Option<String>> {
    match icon { Some(icon) if valid_icon(icon) => Some(Some(icon.trim().to_string())), Some(_) => None, None => Some(None) }
}
fn replace(target: &mut Option<String>, icon: Option<String>) -> bool {
    if *target == icon { return false; }
    *target = icon;
    true
}

impl Browser {
    pub fn item_icon(&self, tab: TabId) -> Option<&str> {
        if let Some(split) = self.split_for(tab) { return split.custom_icon.as_deref(); }
        self.all_tabs().find(|entry| entry.id == tab)?.custom_icon.as_deref()
    }

    pub fn set_item_icon(&mut self, tab: TabId, icon: Option<&str>) -> bool {
        let Some(icon) = normalized(icon) else { return false; };
        if let Some(split) = self.profiles.values_mut().flat_map(|profile| &mut profile.favorite_splits).chain(self.spaces.iter_mut().flat_map(|space| &mut space.splits)).find(|split| split.panes.contains(&tab)) {
            return replace(&mut split.custom_icon, icon);
        }
        self.tab_mut(tab).is_some_and(|tab| replace(&mut tab.custom_icon, icon))
    }

    pub fn set_space_icon(&mut self, space: SpaceId, icon: Option<&str>) -> bool {
        let Some(icon) = normalized(icon) else { return false; };
        self.spaces.iter_mut().find(|entry| entry.id == space).is_some_and(|space| replace(&mut space.custom_icon, icon))
    }

    pub fn set_folder_icon(&mut self, folder: crate::folders::FolderId, icon: Option<&str>) -> bool {
        if !self.can_edit_persistent_sidebar() { return false; }
        let Some(icon) = normalized(icon) else { return false; };
        self.spaces.iter_mut().flat_map(|space| &mut space.folders).find(|entry| entry.id == folder)
            .is_some_and(|folder| replace(&mut folder.custom_icon, icon))
    }

    pub fn reset_item_customization(&mut self, tab: TabId) -> bool {
        let name = self.reset_tab_name(tab);
        self.set_item_icon(tab, None) | name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session, splits::Placement};
    use std::time::SystemTime;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;

    #[test]
    fn icons_accept_one_grapheme_including_flags_skin_tones_and_families() {
        for icon in ["A", "★", "🧭", "🇺🇦", "👨‍👩‍👧‍👦", "👩🏽‍💻", "  🔬  "] { assert!(valid_icon(icon), "{icon}"); }
        for icon in ["", " ", "AB", "🧭🔬", "x\ny", "\0"] { assert!(!valid_icon(icon), "{icon:?}"); }
        assert!(!valid_icon(&format!("a{}", "\u{0301}".repeat(128))));
    }

    #[test]
    fn tab_icons_survive_moves_navigation_duplication_pin_reset_and_archive_restore() {
        let mut browser = Browser::new();
        let tab = browser.new_tab("https://source", NOW);
        browser.set_title(tab, "Source page");
        assert!(browser.set_item_icon(tab, Some(" 🧭 ")));
        browser.rename_tab(tab, "Custom name");
        browser.toggle_pin(tab);
        browser.navigate(tab, "https://source/next");
        browser.reset_pinned_tab(tab);
        assert_eq!(browser.item_icon(tab), Some("🧭"));
        let copy = browser.duplicate_item(tab, NOW).unwrap();
        assert_eq!(browser.item_icon(copy), Some("🧭"));
        browser.favorite_tab(tab);
        browser.unfavorite_tab(tab);
        browser.close_tab(tab, NOW);
        browser.restore_last_archived(NOW);
        let mut browser = session::from_json(&session::to_json(&browser)).unwrap();
        assert_eq!(browser.item_icon(tab), Some("🧭"));
        assert!(browser.reset_item_customization(tab));
        assert_eq!(browser.item_icon(tab), None);
        assert_eq!(browser.active_tab().unwrap().title, "Source page");
        assert_eq!(browser.item_icon(copy), Some("🧭"));
    }

    #[test]
    fn group_icons_do_not_replace_page_icons_and_follow_the_whole_group() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        browser.set_item_icon(a, Some("A"));
        browser.add_split(a, b, Placement::Right, NOW).unwrap();
        assert_eq!(browser.item_icon(a), None);
        browser.set_item_icon(b, Some("🔬"));
        browser.rename_tab(a, "Comparison");
        let copy = browser.duplicate_item(a, NOW).unwrap();
        assert_eq!(browser.item_icon(copy), Some("🔬"));
        browser.close_tab(a, NOW);
        browser.restore_last_archived(NOW);
        assert_eq!(browser.item_icon(a), Some("🔬"));
        browser.reset_item_customization(a);
        assert_eq!(browser.item_icon(a), None);
        browser.separate_split(a);
        assert_eq!(browser.item_icon(a), Some("A"));
        assert_eq!(browser.item_icon(copy), Some("🔬"));
    }

    #[test]
    fn space_icons_are_independent_and_invalid_edits_do_not_change_existing_values() {
        let mut browser = Browser::new();
        let space = browser.space().id;
        let other = browser.add_space("Work", 20.);
        assert!(browser.set_space_icon(space, Some("👩🏽‍💻")));
        assert!(!browser.set_space_icon(space, Some("two words")));
        assert_eq!(browser.spaces[0].custom_icon.as_deref(), Some("👩🏽‍💻"));
        assert_eq!(browser.spaces[1].custom_icon, None);
        assert!(!browser.set_space_icon(SpaceId(u64::MAX), Some("A")));
        let mut browser = session::from_json(&session::to_json(&browser)).unwrap();
        assert_eq!(browser.space().custom_icon.as_deref(), Some("👩🏽‍💻"));
        assert!(!browser.set_item_icon(TabId(u64::MAX), Some("A")));
        assert!(browser.set_space_icon(other, Some("W")));
        assert!(browser.set_space_icon(space, None));
    }
}
