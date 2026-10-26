//! Only new installations enter local setup; existing sessions deserialize None.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStep {
    /// The animated welcome intro that precedes local setup.
    Welcome,
    Theme,
    Favorites,
    Import,
    /// Retired steps, kept so sessions saved mid-setup still load; resuming
    /// at one finishes setup.
    DefaultBrowser,
    Basics,
}
impl SetupStep {
    /// Arc's order: import, Favorites, then the theme.
    pub fn next(self) -> Option<Self> {
        Some(match self {
            Self::Welcome => Self::Import,
            Self::Import => Self::Favorites,
            Self::Favorites => Self::Theme,
            Self::Theme | Self::DefaultBrowser | Self::Basics => return None,
        })
    }
    /// The step Back returns to; the import step and the intro have none.
    pub fn previous(self) -> Option<Self> {
        Some(match self {
            Self::Favorites => Self::Import,
            Self::Theme => Self::Favorites,
            _ => return None,
        })
    }
}
impl crate::Browser {
    pub fn first_run() -> Self {
        let mut b = Self::new();
        b.onboarding = Some(SetupStep::Welcome);
        b
    }
    pub fn retreat_setup(&mut self, expected: SetupStep) -> bool {
        if self.is_private() || self.is_blank() || self.onboarding != Some(expected) {
            return false;
        }
        let Some(previous) = expected.previous() else { return false };
        self.onboarding = Some(previous);
        true
    }
    pub fn advance_setup(&mut self, expected: SetupStep) -> bool {
        if self.is_private() || self.is_blank() || self.onboarding != Some(expected) {
            return false;
        }
        self.onboarding = expected.next();
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_resumes_and_cannot_be_rewound_by_a_stale_window() {
        let mut b = crate::Browser::first_run();
        assert!(b.all_tabs().next().is_none());
        assert_eq!(b.onboarding, Some(SetupStep::Welcome));
        assert!(!b.advance_setup(SetupStep::Theme));
        assert!(b.advance_setup(SetupStep::Welcome));
        assert_eq!(b.onboarding, Some(SetupStep::Import));
        assert!(!b.retreat_setup(SetupStep::Import));
        assert!(b.advance_setup(SetupStep::Import));
        assert!(!b.advance_setup(SetupStep::Import));
        let mut b = crate::session::from_json(&crate::session::to_json(&b)).unwrap();
        assert_eq!(b.onboarding, Some(SetupStep::Favorites));
        assert!(b.advance_setup(SetupStep::Favorites));
        assert!(b.retreat_setup(SetupStep::Theme));
        assert_eq!(b.onboarding, Some(SetupStep::Favorites));
        for step in [SetupStep::Favorites, SetupStep::Theme] {
            assert!(b.advance_setup(step));
        }
        assert!(b.onboarding.is_none());
        assert!(!b.advance_setup(SetupStep::Theme));
        // A session saved at a retired step finishes setup when resumed.
        b.onboarding = Some(SetupStep::DefaultBrowser);
        assert!(b.advance_setup(SetupStep::DefaultBrowser));
        assert!(b.onboarding.is_none());
        let mut value: serde_json::Value =
            serde_json::from_str(&crate::session::to_json(&b)).unwrap();
        value.as_object_mut().unwrap().remove("onboarding");
        assert!(
            crate::session::from_json(&value.to_string())
                .unwrap()
                .onboarding
                .is_none()
        );
    }
}
