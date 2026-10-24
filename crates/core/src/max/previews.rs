//! A preview answer is display-only text for one captured public-page link.
use crate::{Browser, TabId, UserProfileId};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewIcon {
    Book,
    List,
    Check,
    Warning,
    Globe,
    Dot,
    Question,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewPoint {
    pub icon: PreviewIcon,
    pub label: String,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewContent {
    pub title: String,
    pub points: Vec<PreviewPoint>,
}
impl PreviewContent {
    pub fn valid(&self) -> bool {
        clean(&self.title, 110)
            && (1..=5).contains(&self.points.len())
            && self
                .points
                .iter()
                .all(|p| clean(&p.label, 30) && clean(&p.text, 140))
            && self
                .points
                .iter()
                .map(|p| p.label.chars().count() + p.text.chars().count())
                .sum::<usize>()
                <= 650
    }
}
fn clean(text: &str, max: usize) -> bool {
    !text.trim().is_empty()
        && text == text.trim()
        && text.chars().count() <= max
        && !text
            .chars()
            .any(|c| c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewSnapshot {
    pub tab: TabId,
    pub profile: UserProfileId,
    pub source_url: String,
    pub target_url: String,
}
impl Browser {
    pub fn preview_snapshot(
        &self,
        tab: TabId,
        source: &str,
        target: &str,
    ) -> Option<PreviewSnapshot> {
        if self.is_private()
            || self.is_blank()
            || !self
                .settings
                .max
                .enabled(echo_ai::MaxFeature::FiveSecondPreviews)
        {
            return None;
        }
        let page = self.all_tabs().find(|t| t.id == tab && t.url == source)?;
        let profile = self.user_profile_id_for_tab(page.id)?;
        for address in [source, target] {
            if address.len() > 8192 {
                return None;
            }
            let url = url::Url::parse(address).ok()?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return None;
            }
        }
        Some(PreviewSnapshot {
            tab,
            profile,
            source_url: source.into(),
            target_url: target.into(),
        })
    }
    pub fn preview_is_current(&self, snapshot: &PreviewSnapshot) -> bool {
        self.preview_snapshot(snapshot.tab, &snapshot.source_url, &snapshot.target_url)
            .as_ref()
            == Some(snapshot)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_is_conditional_on_feature_page_and_owner_and_rejects_nonweb_targets() {
        let mut b = Browser::new();
        let tab = b.new_tab(
            "https://example.test/source",
            std::time::SystemTime::UNIX_EPOCH,
        );
        assert!(b
            .preview_snapshot(tab, "https://example.test/source", "https://target.test")
            .is_none());
        b.settings
            .max
            .set_enabled(echo_ai::MaxFeature::FiveSecondPreviews, true);
        let snapshot = b
            .preview_snapshot(tab, "https://example.test/source", "https://target.test")
            .unwrap();
        assert!(b.preview_is_current(&snapshot));
        for target in [
            "javascript:alert(1)",
            "file:///tmp/private",
            "https://user:secret@target.test/",
        ] {
            assert!(b
                .preview_snapshot(tab, &snapshot.source_url, target)
                .is_none());
        }
        b.navigate(tab, "https://example.test/changed");
        assert!(!b.preview_is_current(&snapshot));
    }
    #[test]
    fn preview_output_is_bounded_display_text_and_never_supplies_actions_or_urls() {
        let mut content = PreviewContent {
            title: "Understanding type safety".into(),
            points: vec![PreviewPoint {
                icon: PreviewIcon::Book,
                label: "Definition".into(),
                text: "Prevents type errors in programming.".into(),
            }],
        };
        assert!(content.valid());
        content.points[0].label = "Definition\nOpen file".into();
        assert!(!content.valid());
        assert!(serde_json::from_str::<PreviewContent>(
            r#"{"title":"x","points":[],"open_url":"file:///private"}"#
        )
        .is_err());
    }
}
