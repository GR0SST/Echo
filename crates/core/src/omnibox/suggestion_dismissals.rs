//! Local, provider-scoped suppression; never deletes browsing history.
use super::{CommandBarSearchPreferences, SearchEngine};
use serde::{Deserialize, Serialize};

const LIMIT: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DismissedSuggestion {
    engine: SearchEngine,
    query: String,
}

fn key(query: &str) -> Option<String> {
    let query = query.trim();
    if query.is_empty() || query.len() > 512 || query.contains(char::is_control) {
        return None;
    }
    Some(query.to_lowercase())
}

impl CommandBarSearchPreferences {
    pub fn suggestion_dismissed(&self, engine: SearchEngine, query: &str) -> bool {
        let Some(query) = key(query) else {
            return false;
        };
        self.dismissed_suggestions
            .iter()
            .any(|entry| entry.engine == engine && entry.query == query)
    }

    pub fn dismiss_suggestion(&mut self, engine: SearchEngine, query: &str) -> bool {
        if !engine.supports_suggestions() {
            return false;
        }
        let Some(query) = key(query) else {
            return false;
        };
        let entry = DismissedSuggestion { engine, query };
        self.dismissed_suggestions
            .retain(|previous| previous != &entry);
        self.dismissed_suggestions.push(entry);
        let excess = self.dismissed_suggestions.len().saturating_sub(LIMIT);
        self.dismissed_suggestions.drain(..excess);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_provider_scoped_dismissal_round_trips_without_suppressing_other_terms() {
        let mut preferences = CommandBarSearchPreferences::default();
        assert!(preferences.dismiss_suggestion(SearchEngine::Google, "  RUST book  "));
        assert!(preferences.suggestion_dismissed(SearchEngine::Google, "rust BOOK"));
        assert!(!preferences.suggestion_dismissed(SearchEngine::Bing, "rust book"));
        assert!(!preferences.suggestion_dismissed(SearchEngine::Google, "rust book advanced"));
        let json = serde_json::to_string(&preferences).unwrap();
        let restored: CommandBarSearchPreferences = serde_json::from_str(&json).unwrap();
        assert!(restored.suggestion_dismissed(SearchEngine::Google, "rust book"));
        for index in 0..LIMIT {
            preferences.dismiss_suggestion(SearchEngine::Google, &format!("term {index}"));
        }
        assert_eq!(preferences.dismissed_suggestions.len(), LIMIT);
        assert!(!preferences.suggestion_dismissed(SearchEngine::Google, "rust book"));
        preferences.dismiss_suggestion(SearchEngine::Google, "term 0");
        preferences.dismiss_suggestion(SearchEngine::Google, "new term");
        assert!(preferences.suggestion_dismissed(SearchEngine::Google, "term 0"));
        assert!(!preferences.suggestion_dismissed(SearchEngine::Google, "term 1"));
    }

    #[test]
    fn invalid_payloads_and_unsupported_providers_do_not_change_preferences() {
        let mut preferences = CommandBarSearchPreferences::default();
        for query in ["", "  ", "line\nbreak", &"x".repeat(513)] {
            assert!(!preferences.dismiss_suggestion(SearchEngine::Google, query));
        }
        assert!(!preferences.dismiss_suggestion(SearchEngine::Perplexity, "term"));
        assert!(preferences.dismissed_suggestions.is_empty());
        let legacy: CommandBarSearchPreferences = serde_json::from_str("{}").unwrap();
        assert!(legacy.dismissed_suggestions.is_empty());
    }

    #[test]
    fn session_keeps_dismissal_in_only_its_own_profile() {
        let mut browser = crate::Browser::new();
        let owner = browser.active_user_profile_id();
        browser
            .active_user_profile_mut()
            .settings
            .search
            .dismiss_suggestion(SearchEngine::Google, "quokka book");
        let other = browser.create_user_profile("Synthetic Work").unwrap();
        let restored = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert!(restored
            .user_profile(owner)
            .unwrap()
            .settings
            .search
            .suggestion_dismissed(SearchEngine::Google, "quokka book"));
        assert!(!restored
            .user_profile(other)
            .unwrap()
            .settings
            .search
            .suggestion_dismissed(SearchEngine::Google, "quokka book"));
    }
}
