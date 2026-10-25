//! Profile-owned search engines and site-search shortcuts. These templates are
//! explicit user preferences, never executable scripts or suggestion endpoints.
use super::{encode_query, CommandBarSearchPreferences, Resolved, SearchEngine};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShortcutMode {
    #[default]
    SpaceOrTab,
    Tab,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchProvider {
    pub id: u64,
    pub name: String,
    pub shortcut: String,
    pub template: String,
    pub active: bool,
    #[serde(default)]
    pub show_in_engine_list: bool,
    /// None identifies a site-search entry rather than a prepopulated engine.
    pub builtin: Option<SearchEngine>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchError {
    Name,
    Shortcut,
    DuplicateShortcut,
    Template,
    Missing,
    DefaultProvider,
    Exhausted,
}
pub fn first_custom_id() -> u64 {
    8
}
pub fn default_providers() -> Vec<SearchProvider> {
    let mut providers = SearchEngine::ALL
        .into_iter()
        .enumerate()
        .map(|(index, engine)| {
            let shortcut = match engine {
                SearchEngine::Google => "google.com",
                SearchEngine::Bing => "bing.com",
                SearchEngine::DuckDuckGo => "duckduckgo.com",
                SearchEngine::Yandex => "yandex.ru",
                SearchEngine::Brave => "search.brave.com",
                SearchEngine::Perplexity => "perplexity.ai",
            };
            SearchProvider {
                id: index as u64 + 1,
                name: engine.label().into(),
                shortcut: shortcut.into(),
                template: engine.search("%s").url().replace("%25s", "%s"),
                active: true,
                show_in_engine_list: true,
                builtin: Some(engine),
            }
        })
        .collect::<Vec<_>>();
    providers.push(SearchProvider {
        id: 7,
        name: "Wikipedia".into(),
        shortcut: "wikipedia".into(),
        template: "https://en.wikipedia.org/w/index.php?search=%s".into(),
        active: true,
        show_in_engine_list: false,
        builtin: None,
    });
    providers
}
impl SearchProvider {
    pub fn search(&self, query: &str) -> Option<Resolved> {
        if !self.active || validate_template(&self.template).is_err() {
            return None;
        }
        Some(Resolved::Search {
            query: query.into(),
            url: expand_template(&self.template, query),
        })
    }
}
fn expand_template(template: &str, query: &str) -> String {
    let query_encoded = encode_query(query);
    let path_encoded = query_encoded.replace('+', "%20");
    let mut out = String::new();
    let mut start = 0;
    for (offset, _) in template.match_indices("%s") {
        out.push_str(&template[start..offset]);
        let prefix = &template[..offset];
        out.push_str(if prefix.contains('?') && !prefix.contains('#') {
            &query_encoded
        } else {
            &path_encoded
        });
        start = offset + 2;
    }
    out.push_str(&template[start..]);
    out
}
fn validate_template(template: &str) -> Result<(), SearchError> {
    if template.len() > 8192 || !template.contains("%s") || template.contains(char::is_control) {
        return Err(SearchError::Template);
    }
    let parsed = url::Url::parse(&template.replace("%s", "echo-query"))
        .map_err(|_| SearchError::Template)?;
    if !matches!(parsed.scheme(), "https" | "http")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(SearchError::Template);
    }
    Ok(())
}
impl CommandBarSearchPreferences {
    pub fn provider(&self, id: u64) -> Option<&SearchProvider> {
        self.providers.iter().find(|p| p.id == id)
    }
    pub fn default_provider(&self) -> Option<&SearchProvider> {
        self.custom_default
            .and_then(|id| self.provider(id))
            .filter(|p| p.active && validate_template(&p.template).is_ok())
            .or_else(|| {
                self.providers.iter().find(|p| {
                    p.builtin == Some(self.engine)
                        && p.active
                        && validate_template(&p.template).is_ok()
                })
            })
    }
    pub fn default_name(&self) -> &str {
        self.default_provider()
            .map_or(self.engine.label(), |p| p.name.as_str())
    }
    pub fn search(&self, query: &str) -> Resolved {
        self.default_provider()
            .and_then(|p| p.search(query))
            .unwrap_or_else(|| self.engine.search(query))
    }
    pub fn resolve(&self, input: &str) -> Option<Resolved> {
        super::resolve(input).map(|resolved| match resolved {
            Resolved::Search { query, .. } => self.search(&query),
            url => url,
        })
    }
    /// Only a verified built-in transport receives suggestion queries. Custom
    /// templates never inherit a background request to a previous provider.
    pub fn suggestion_engine(&self) -> Option<SearchEngine> {
        let p = self.default_provider()?;
        p.builtin.filter(|e| {
            e.supports_suggestions() && e.search("%s").url().replace("%25s", "%s") == p.template
        })
    }
    pub fn provider_for_shortcut(&self, shortcut: &str) -> Option<&SearchProvider> {
        let keyword = shortcut.trim();
        self.providers.iter().find(|p| {
            p.active
                && p.shortcut.eq_ignore_ascii_case(keyword)
                && validate_template(&p.template).is_ok()
        })
    }
    fn validate_provider(
        &self,
        except: Option<u64>,
        name: &str,
        shortcut: &str,
        template: &str,
    ) -> Result<(), SearchError> {
        if name.trim().is_empty() || name.len() > 1024 || name.contains(char::is_control) {
            return Err(SearchError::Name);
        }
        if shortcut.trim().is_empty()
            || shortcut.len() > 1024
            || shortcut.contains(char::is_whitespace)
            || shortcut.contains(char::is_control)
        {
            return Err(SearchError::Shortcut);
        }
        if self
            .providers
            .iter()
            .any(|p| Some(p.id) != except && p.shortcut.eq_ignore_ascii_case(shortcut))
        {
            return Err(SearchError::DuplicateShortcut);
        }
        validate_template(template)
    }
    pub fn add_provider(
        &mut self,
        name: &str,
        shortcut: &str,
        template: &str,
    ) -> Result<u64, SearchError> {
        let (name, shortcut, template) = (name.trim(), shortcut.trim(), template.trim());
        self.validate_provider(None, name, shortcut, template)?;
        let id = self.next_provider_id.max(
            self.providers
                .iter()
                .map(|p| p.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(SearchError::Exhausted)?,
        );
        let next = id.checked_add(1).ok_or(SearchError::Exhausted)?;
        self.providers.push(SearchProvider {
            id,
            name: name.into(),
            shortcut: shortcut.into(),
            template: template.into(),
            active: true,
            show_in_engine_list: false,
            builtin: None,
        });
        self.next_provider_id = next;
        Ok(id)
    }
    pub fn edit_provider(
        &mut self,
        id: u64,
        name: &str,
        shortcut: &str,
        template: &str,
    ) -> Result<(), SearchError> {
        let original = self.provider(id).ok_or(SearchError::Missing)?;
        if original.builtin.is_some() && original.template != template.trim() {
            return Err(SearchError::Template);
        }
        let (name, shortcut, template) = (name.trim(), shortcut.trim(), template.trim());
        self.validate_provider(Some(id), name, shortcut, template)?;
        let p = self.providers.iter_mut().find(|p| p.id == id).unwrap();
        p.name = name.into();
        p.shortcut = shortcut.into();
        p.template = template.into();
        Ok(())
    }
    pub fn make_default(&mut self, id: u64) -> Result<(), SearchError> {
        let provider = self
            .provider(id)
            .filter(|p| p.active)
            .ok_or(SearchError::Missing)?;
        validate_template(&provider.template)?;
        match provider.builtin {
            Some(engine) => {
                self.engine = engine;
                self.custom_default = None;
            }
            None => self.custom_default = Some(id),
        }
        self.providers
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .show_in_engine_list = true;
        Ok(())
    }
    pub fn set_provider_active(&mut self, id: u64, active: bool) -> Result<(), SearchError> {
        if !active && self.default_provider().is_some_and(|p| p.id == id) {
            return Err(SearchError::DefaultProvider);
        }
        self.providers
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(SearchError::Missing)?
            .active = active;
        Ok(())
    }
    pub fn delete_provider(&mut self, id: u64) -> Result<(), SearchError> {
        if self.default_provider().is_some_and(|p| p.id == id) {
            return Err(SearchError::DefaultProvider);
        }
        if self.provider(id).is_none() {
            return Err(SearchError::Missing);
        }
        self.providers.retain(|p| p.id != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_default_resolves_queries_without_changing_addresses_or_sending_builtin_suggestions() {
        let mut p = CommandBarSearchPreferences::default();
        let id = p
            .add_provider(
                "Test Search",
                "test",
                "https://search.test/find?q=%s&source=local",
            )
            .unwrap();
        p.make_default(id).unwrap();
        assert_eq!(p.default_name(), "Test Search");
        assert_eq!(
            p.resolve("rust & λ").unwrap().url(),
            "https://search.test/find?q=rust+%26+%CE%BB&source=local"
        );
        assert_eq!(
            p.resolve("https://example.com/a?q=x").unwrap().url(),
            "https://example.com/a?q=x"
        );
        assert_eq!(p.suggestion_engine(), None);
        assert_eq!(p.delete_provider(id), Err(SearchError::DefaultProvider));
        assert_eq!(
            p.set_provider_active(id, false),
            Err(SearchError::DefaultProvider)
        );
        p.make_default(1).unwrap();
        assert_eq!(p.suggestion_engine(), Some(SearchEngine::Google));
        p.set_provider_active(id, false).unwrap();
        assert!(p.provider_for_shortcut("test").is_none());
        p.set_provider_active(id, true).unwrap();
        assert_eq!(p.provider_for_shortcut("TEST").unwrap().id, id);
    }
    #[test]
    fn invalid_templates_and_duplicate_shortcuts_cannot_partially_change_profile_preferences() {
        let mut p = CommandBarSearchPreferences::default();
        let before = serde_json::to_string(&p).unwrap();
        for template in [
            "javascript:alert('%s')",
            "file:///tmp/%s",
            "https://user:password@site.test/?q=%s",
            "https://site.test/?q=missing",
        ] {
            assert_eq!(
                p.add_provider("Test", "test", template),
                Err(SearchError::Template)
            );
        }
        assert_eq!(
            p.add_provider("Test", "GOOGLE.COM", "https://site.test/?q=%s"),
            Err(SearchError::DuplicateShortcut)
        );
        assert_eq!(
            p.add_provider("Test", "two words", "https://site.test/?q=%s"),
            Err(SearchError::Shortcut)
        );
        assert_eq!(serde_json::to_string(&p).unwrap(), before);
        assert_eq!(
            p.edit_provider(
                1,
                "Google",
                "bing.com",
                &p.provider(1).unwrap().template.clone()
            ),
            Err(SearchError::DuplicateShortcut)
        );
        assert_eq!(serde_json::to_string(&p).unwrap(), before);
    }
    #[test]
    fn legacy_preferences_and_custom_catalog_survive_restart_without_reusing_deleted_ids() {
        let legacy: CommandBarSearchPreferences =
            serde_json::from_str(r#"{"engine":"DuckDuckGo","suggestions":false}"#).unwrap();
        assert_eq!(legacy.default_name(), "DuckDuckGo");
        assert_eq!(legacy.providers.len(), 7);
        let mut p = legacy;
        let first = p
            .add_provider("Docs", "docs", "https://docs.test/search/%s")
            .unwrap();
        p.delete_provider(first).unwrap();
        let mut reopened: CommandBarSearchPreferences =
            serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        let second = reopened
            .add_provider("Docs", "docs", "https://docs.test/search/%s")
            .unwrap();
        assert!(second > first);
        reopened.make_default(second).unwrap();
        let restored: CommandBarSearchPreferences =
            serde_json::from_str(&serde_json::to_string(&reopened).unwrap()).unwrap();
        assert_eq!(restored.default_name(), "Docs");
        assert_eq!(
            restored.search("a/b").url(),
            "https://docs.test/search/a%2Fb"
        );
        assert_eq!(
            CommandBarSearchPreferences::default().default_name(),
            "Google"
        );
    }
}
