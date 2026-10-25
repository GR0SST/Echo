//! Arc's profile-owned New Document actions. Destinations are the creation URLs
//! shipped in Arc 1.170; no Echo service or browser account participates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentApp { Confluence, Google, Notion, Word }
impl DocumentApp {
    pub fn from_preference(value: &str) -> Option<Self> {
        match value {
            "—" | "Google Doc" => Some(Self::Google),
            "Confluence Page" => Some(Self::Confluence),
            "Notion Page" => Some(Self::Notion),
            "Word Doc" => Some(Self::Word),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Confluence => "New Confluence Page", Self::Google => "New Google Doc",
            Self::Notion => "New Notion Page", Self::Word => "New Word Doc",
        }
    }
    pub fn url(self) -> &'static str {
        match self {
            Self::Confluence => "https://confluence.new", Self::Google => "https://doc.new",
            Self::Notion => "https://app.notion.com/new", Self::Word => "https://word.new",
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_creation_preference_is_profile_local_persistent_and_bounded_to_known_apps() {
        let mut browser = crate::Browser::new();
        let original = browser.active_user_profile_id();
        let other = browser.create_user_profile("Document fixture").unwrap();
        browser.user_profile_mut(other).unwrap().settings.document_app = "Notion Page".into();
        let loaded = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert_eq!(DocumentApp::from_preference(&loaded.user_profile(original).unwrap().settings.document_app), Some(DocumentApp::Google));
        assert_eq!(DocumentApp::from_preference(&loaded.user_profile(other).unwrap().settings.document_app), Some(DocumentApp::Notion));
        assert_eq!(DocumentApp::from_preference("https://untrusted.invalid"), None);
    }
}
