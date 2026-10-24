//! Only new installations enter local setup; existing sessions deserialize None.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStep {
    Theme,
    Favorites,
    Import,
    DefaultBrowser,
    Basics,
}
impl SetupStep {
    pub fn next(self) -> Option<Self> {
        Some(match self {
            Self::Theme => Self::Favorites,
            Self::Favorites => Self::Import,
            Self::Import => Self::DefaultBrowser,
            Self::DefaultBrowser => Self::Basics,
            Self::Basics => return None,
        })
    }
}
impl crate::Browser {
    pub fn first_run() -> Self {
        let mut b = Self::new();
        b.onboarding = Some(SetupStep::Theme);
        b
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
        assert!(b.advance_setup(SetupStep::Theme));
        assert!(!b.advance_setup(SetupStep::Theme));
        let mut b = crate::session::from_json(&crate::session::to_json(&b)).unwrap();
        assert_eq!(b.onboarding, Some(SetupStep::Favorites));
        for step in [
            SetupStep::Favorites,
            SetupStep::Import,
            SetupStep::DefaultBrowser,
            SetupStep::Basics,
        ] {
            assert!(b.advance_setup(step));
        }
        assert!(b.onboarding.is_none());
        assert!(!b.advance_setup(SetupStep::Theme));
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
