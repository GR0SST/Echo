//! Turns what the user typed into the command bar into a URL to load.

pub mod calculator;
pub mod search_settings;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchEngine {
    #[default]
    Google,
    DuckDuckGo,
    Bing,
    Yandex,
    Brave,
    Perplexity,
}

/// Isolated profile preference contract for Command Bar search. The Settings
/// surface can expose this without owning the suggestion transport or UI.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandBarSearchPreferences {
    pub engine: SearchEngine,
    pub suggestions: bool,
    pub notice_dismissed: bool,
    #[serde(default = "search_settings::default_providers")]
    pub providers: Vec<search_settings::SearchProvider>,
    pub custom_default: Option<u64>,
    #[serde(default = "search_settings::first_custom_id")]
    pub next_provider_id: u64,
    pub shortcut_mode: search_settings::ShortcutMode,
}
impl Default for CommandBarSearchPreferences {
    fn default() -> Self {
        Self { engine: SearchEngine::Google, suggestions: true, notice_dismissed: false,
            providers: search_settings::default_providers(), custom_default: None,
            next_provider_id: search_settings::first_custom_id(), shortcut_mode: search_settings::ShortcutMode::default() }
    }
}
impl SearchEngine {
    pub const ALL: [Self; 6] = [Self::Google, Self::Bing, Self::DuckDuckGo, Self::Yandex, Self::Brave, Self::Perplexity];
    pub fn label(self) -> &'static str {
        match self { Self::Google => "Google", Self::Bing => "Microsoft Bing", Self::DuckDuckGo => "DuckDuckGo", Self::Yandex => "Yandex", Self::Brave => "Brave", Self::Perplexity => "Perplexity" }
    }
    pub fn supports_suggestions(self) -> bool { matches!(self, Self::Google | Self::DuckDuckGo | Self::Bing) }
    pub fn search(self, query: &str) -> Resolved {
        let endpoint = match self {
            Self::Google => "https://www.google.com/search?q=",
            Self::DuckDuckGo => "https://duckduckgo.com/?q=",
            Self::Bing => "https://www.bing.com/search?q=",
            Self::Yandex => "https://yandex.com/search/?text=",
            Self::Brave => "https://search.brave.com/search?q=",
            Self::Perplexity => "https://www.perplexity.ai/search?q=",
        };
        Resolved::Search { query: query.into(), url: format!("{endpoint}{}", encode_query(query)) }
    }
    pub fn suggest_url(self, query: &str) -> String {
        let endpoint = match self {
            Self::Google => "https://suggestqueries.google.com/complete/search?client=chrome&q=",
            Self::DuckDuckGo => "https://duckduckgo.com/ac/?type=list&q=",
            Self::Bing => "https://api.bing.com/osjson.aspx?query=",
            // No verified completion transport exists for these providers yet.
            Self::Yandex | Self::Brave | Self::Perplexity => return String::new(),
        };
        format!("{endpoint}{}", encode_query(query))
    }
}

pub fn resolve_with_engine(input: &str, engine: SearchEngine) -> Option<Resolved> {
    resolve(input).map(|resolved| match resolved {
        Resolved::Search { query, .. } => engine.search(&query),
        url => url,
    })
}

/// Local catalog of common destinations, using factual names and addresses.
pub const TOP_DOMAINS: &[(&str, &str)] = &[
    ("google.com", "Google"), ("youtube.com", "YouTube"),
    ("linkedin.com", "LinkedIn"), ("wordpress.org", "WordPress"),
    ("apple.com", "Apple"), ("microsoft.com", "Microsoft"),
    ("docs.google.com", "Google Docs"), ("maps.google.com", "Google Maps"),
    ("wikipedia.org", "Wikipedia"), ("cloudflare.com", "Cloudflare"),
    ("github.com", "GitHub"), ("amazon.com", "Amazon"),
    ("reddit.com", "Reddit"), ("facebook.com", "Facebook"),
    ("instagram.com", "Instagram"), ("twitter.com", "Twitter"),
];
pub fn top_domain(prefix: &str) -> Option<(&'static str, &'static str)> {
    let prefix = prefix.to_ascii_lowercase();
    (prefix.len() >= 2 && !prefix.contains(char::is_whitespace))
        .then(|| TOP_DOMAINS.iter().copied().find(|(domain, _)| domain.starts_with(&prefix))).flatten()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteSearch { Wikipedia, ChatGpt, Configured(u64) }
impl SiteSearch {
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        match keyword.trim().to_ascii_lowercase().as_str() {
            "wikipedia" | "wikipedia.org" | "www.wikipedia.org" => Some(Self::Wikipedia),
            "chatgpt" => Some(Self::ChatGpt),
            _ => None,
        }
    }
    pub fn for_profile(keyword: &str, preferences: &CommandBarSearchPreferences) -> Option<Self> {
        preferences.provider_for_shortcut(keyword).map(|p| Self::Configured(p.id)).or_else(|| {
            match Self::from_keyword(keyword) {
                Some(Self::Wikipedia) => preferences.provider(7).filter(|p| p.active && p.shortcut == "wikipedia").map(|p|Self::Configured(p.id)),
                Some(Self::ChatGpt) => Some(Self::ChatGpt),
                _ => None,
            }
        })
    }
    pub fn name(self, preferences: &CommandBarSearchPreferences) -> Option<&str> {
        match self { Self::Wikipedia => Some("Wikipedia"), Self::ChatGpt => Some("ChatGPT"),
            Self::Configured(id) => preferences.provider(id).filter(|p|p.active).map(|p|p.name.as_str()) }
    }
    pub fn search(self, query: &str, preferences: &CommandBarSearchPreferences) -> Option<Resolved> {
        let url = match self {
            Self::Configured(id) => return preferences.provider(id)?.search(query),
            Self::Wikipedia => format!("https://en.wikipedia.org/w/index.php?search={}", encode_query(query)),
            Self::ChatGpt => {
                // Arc's native handoff opens this URL; the website prefills its
                // composer and owns the user's explicit Send action.
                let mut encoded = String::new();
                for byte in query.bytes() {
                    match byte {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'?' => encoded.push(byte as char),
                        _ => encoded.push_str(&format!("%{byte:02X}")),
                    }
                }
                format!("https://chat.openai.com/?q={encoded}")
            }
        };
        Some(Resolved::Search { query: query.into(), url })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// The input is an address.
    Url(String),
    /// The input is a search query; `url` is the search results page.
    Search { query: String, url: String },
}

impl Resolved {
    pub fn url(&self) -> &str {
        match self {
            Resolved::Url(url) | Resolved::Search { url, .. } => url,
        }
    }
}

const SCHEMES: &[&str] = &["http://", "https://", "file://", "about:", "chrome://", "browser://", "data:", "view-source:"];

/// Browser-owned GPUI pages must never be sent to the web engine.
pub fn is_internal_url(url: &str) -> bool {
    url.get(..10).is_some_and(|scheme| scheme.eq_ignore_ascii_case("browser://"))
}

/// Returns `None` for blank input.
pub fn resolve(input: &str) -> Option<Resolved> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    let lower = input.to_ascii_lowercase();
    if SCHEMES.iter().any(|s| lower.starts_with(s)) {
        return Some(Resolved::Url(input.to_string()));
    }
    if !input.contains(char::is_whitespace) && looks_like_host(&lower) {
        let scheme = if is_local(&lower) { "http" } else { "https" };
        return Some(Resolved::Url(format!("{scheme}://{input}")));
    }
    Some(SearchEngine::Google.search(input))
}

fn host_of(input: &str) -> &str {
    let host_port = input.split(['/', '?', '#']).next().unwrap_or(input);
    host_port.rsplit_once(':').map_or(host_port, |(host, port)| {
        if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
            host
        } else {
            host_port
        }
    })
}

fn is_local(input: &str) -> bool {
    let host = host_of(input);
    host == "localhost" || host.parse::<std::net::Ipv4Addr>().is_ok()
}

fn looks_like_host(input: &str) -> bool {
    let host = host_of(input);
    if is_local(input) {
        return true;
    }
    let mut labels = host.split('.');
    let Some(tld) = labels.next_back() else {
        return false;
    };
    let has_dot = host.contains('.');
    let labels_ok = host
        .split('.')
        .all(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-'));
    has_dot && labels_ok && tld.chars().count() >= 2 && tld.chars().all(char::is_alphabetic)
}

fn encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for byte in query.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_engine_routes_search_and_suggest_without_changing_addresses() {
        assert_eq!(resolve_with_engine("rust language", SearchEngine::DuckDuckGo).unwrap().url(), "https://duckduckgo.com/?q=rust+language");
        assert_eq!(resolve_with_engine("example.com", SearchEngine::Bing).unwrap().url(), "https://example.com");
        assert_eq!(SearchEngine::Google.suggest_url("a+b & c"), "https://suggestqueries.google.com/complete/search?client=chrome&q=a%2Bb+%26+c");
        assert_eq!(SiteSearch::from_keyword("Wikipedia "), Some(SiteSearch::Wikipedia));
        assert_eq!(SiteSearch::from_keyword("wikipedia.evil"), None);
        assert_eq!(top_domain("wiki"), Some(("wikipedia.org", "Wikipedia")));
        assert_eq!(top_domain("w"), None);
        assert_eq!(top_domain("wiki text"), None);
    }

    #[test]
    fn profile_search_preferences_have_compatible_defaults_and_persist() {
        let mut browser = crate::Browser::new();
        let mut legacy: serde_json::Value = serde_json::from_str(&crate::session::to_json(&browser)).unwrap();
        for profile in legacy["profiles"].as_object_mut().unwrap().values_mut() {
            profile.as_object_mut().unwrap().remove("settings");
        }
        let loaded = crate::session::from_json(&legacy.to_string()).unwrap();
        assert_eq!(loaded.active_user_profile().settings.search.engine, SearchEngine::Google);
        assert!(loaded.active_user_profile().settings.search.suggestions);
        browser.active_user_profile_mut().settings.search.engine = SearchEngine::Bing;
        browser.active_user_profile_mut().settings.search.suggestions = false;
        let loaded = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert_eq!(loaded.active_user_profile().settings.search.engine, SearchEngine::Bing);
        assert!(!loaded.active_user_profile().settings.search.suggestions);
    }

    fn url(input: &str) -> String {
        resolve(input).unwrap().url().to_string()
    }

    #[test]
    fn blank_input_resolves_to_nothing() {
        assert_eq!(resolve("   "), None);
    }

    #[test]
    fn addresses_get_a_scheme() {
        assert_eq!(url("github.com"), "https://github.com");
        assert_eq!(url("news.ycombinator.com/item?id=1"), "https://news.ycombinator.com/item?id=1");
        assert_eq!(url("localhost:3000"), "http://localhost:3000");
        assert_eq!(url("127.0.0.1:8080/x"), "http://127.0.0.1:8080/x");
        assert_eq!(url("пример.рф"), "https://пример.рф");
    }

    #[test]
    fn explicit_schemes_are_kept() {
        assert_eq!(url("http://example.com"), "http://example.com");
        assert_eq!(url("about:blank"), "about:blank");
        assert_eq!(url("view-source:https://example.com/?a=1#part"), "view-source:https://example.com/?a=1#part");
    }

    #[test]
    fn everything_else_is_a_search() {
        assert_eq!(url("rust gpui"), "https://www.google.com/search?q=rust+gpui");
        assert_eq!(url("arc"), "https://www.google.com/search?q=arc");
        assert_eq!(url("v1.2"), "https://www.google.com/search?q=v1.2");
        assert_eq!(url("c++"), "https://www.google.com/search?q=c%2B%2B");
        assert_eq!(url("привет"), "https://www.google.com/search?q=%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82");
    }
}

#[cfg(test)]
mod chatgpt_tests {
    use super::*;
    #[test]
    fn chatgpt_handoff_matches_arc_and_keeps_the_whole_question_in_one_parameter() {
        assert_eq!(SiteSearch::from_keyword("ChatGPT"), Some(SiteSearch::ChatGpt));
        let preferences=CommandBarSearchPreferences::default();
        assert_eq!(SiteSearch::ChatGpt.search("What is the Rust programming language?",&preferences).unwrap().url(), "https://chat.openai.com/?q=What%20is%20the%20Rust%20programming%20language?");
        let resolved = SiteSearch::ChatGpt.search("a&b=#c+%? 日本語",&preferences).unwrap();
        let url = url::Url::parse(resolved.url()).unwrap();
        assert_eq!(url.query_pairs().collect::<Vec<_>>(), vec![("q".into(), "a&b=#c+%? 日本語".into())]);
        assert!(url.fragment().is_none());
    }
}
