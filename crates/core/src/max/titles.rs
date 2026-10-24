//! Conditional automatic labels, independent of page titles and user names.
use crate::{Browser, SpaceId, TabId, UserProfileId};
use serde::Serialize;
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TitleSnapshot {
    pub id: TabId,
    pub title: String,
    pub url: String,
    space: SpaceId,
    profile: UserProfileId,
}
impl Browser {
    pub fn title_snapshot(&self, id: TabId) -> Option<TitleSnapshot> {
        if self.is_private()
            || self.is_blank()
            || !self
                .settings
                .max
                .enabled(echo_ai::MaxFeature::TidyTabTitles)
            || self.split_for(id).is_some()
        {
            return None;
        }
        let space = self
            .spaces
            .iter()
            .find(|s| s.pinned.iter().any(|t| t.id == id))?;
        let tab = space.pinned.iter().find(|t| t.id == id)?;
        if tab.page_title.is_none()
            || tab.custom_title.is_some()
            || tab.ai_title.is_some()
            || tab.pinned_url.as_ref() != Some(&tab.url)
            || tab.title.trim().is_empty()
        {
            return None;
        }
        Some(TitleSnapshot {
            id,
            title: tab.title.clone(),
            url: tab.url.clone(),
            space: space.id,
            profile: space.user_profile_id,
        })
    }
    pub fn apply_tidy_title(&mut self, snapshot: &TitleSnapshot, title: &str) -> bool {
        let title = title.trim();
        if title.is_empty()
            || title.chars().count() > 80
            || title.chars().any(char::is_control)
            || self.title_snapshot(snapshot.id).as_ref() != Some(snapshot)
        {
            return false;
        }
        let Some(tab) = self.tab_mut(snapshot.id) else {
            return false;
        };
        tab.ai_title = Some(title.into());
        tab.title = title.into();
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const NOW: std::time::SystemTime = std::time::SystemTime::UNIX_EPOCH;
    fn pinned() -> (Browser, TitleSnapshot) {
        let mut b = Browser::new();
        b.settings
            .max
            .set_enabled(echo_ai::MaxFeature::TidyTabTitles, true);
        let tab = b.new_tab(
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            NOW,
        );
        b.set_title(tab, "Rust (programming language) - Wikipedia");
        b.toggle_pin(tab);
        let snapshot = b.title_snapshot(tab).unwrap();
        (b, snapshot)
    }
    #[test]
    fn ai_title_survives_page_updates_unpin_and_session_but_reset_restores_the_page() {
        let (mut b, s) = pinned();
        assert!(b.apply_tidy_title(&s, "Rust Language"));
        b.set_title(s.id, "Updated page title");
        assert_eq!(b.active_tab().unwrap().title, "Rust Language");
        b.toggle_pin(s.id);
        assert_eq!(b.active_tab().unwrap().title, "Rust Language");
        let mut loaded = crate::session::from_json(&crate::session::to_json(&b)).unwrap();
        assert_eq!(loaded.active_tab().unwrap().title, "Rust Language");
        assert!(loaded.reset_tab_name(s.id));
        assert_eq!(loaded.active_tab().unwrap().title, "Updated page title");
        assert!(loaded.active_tab().unwrap().ai_title.is_none());
    }
    #[test]
    fn ai_title_never_overwrites_a_user_edit_changed_page_or_disabled_feature() {
        let (mut b, s) = pinned();
        b.rename_tab(s.id, "My documentation");
        assert!(!b.apply_tidy_title(&s, "Rust"));
        assert_eq!(b.active_tab().unwrap().title, "My documentation");
        let (mut b, s) = pinned();
        b.navigate(s.id, "https://different.example");
        assert!(!b.apply_tidy_title(&s, "Rust"));
        let (mut b, s) = pinned();
        b.toggle_pin(s.id);
        assert!(!b.apply_tidy_title(&s, "Rust"));
        let (mut b, s) = pinned();
        b.settings
            .max
            .set_enabled(echo_ai::MaxFeature::TidyTabTitles, false);
        assert!(!b.apply_tidy_title(&s, "Rust"));
        let (mut b, s) = pinned();
        assert!(!b.apply_tidy_title(&s, "Bad\nlabel"));
        assert!(!b.apply_tidy_title(&s, &"x".repeat(81)));
    }
}
