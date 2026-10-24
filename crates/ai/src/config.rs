use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use url::Url;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Provider {
    #[default]
    OpenAi,
    Anthropic,
    OpenAiCompatible,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Authentication {
    #[default]
    ApiKey,
    ChatGpt,
    Disconnected,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub provider: Provider,
    pub authentication: Authentication,
    /// The compatible API's root, including its version prefix, e.g. /v1.
    pub base_url: String,
    /// An exact provider model ID selected by the user, never a display name.
    pub model: String,
    /// Required for Anthropic keys that are not scoped to one workspace.
    pub anthropic_workspace_id: String,
    /// Opaque references only; secrets never implement Serialize.
    pub api_key_id: String,
    pub chatgpt_account_id: String,
}
impl ProviderConfig {
    pub fn endpoint(&self, suffix: &str) -> Result<Url, &'static str> {
        if self.authentication == Authentication::ChatGpt && self.provider != Provider::OpenAi {
            return Err("ChatGPT credentials can only be used with OpenAI");
        }
        let root = match self.provider {
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::OpenAiCompatible => self.base_url.trim_end_matches('/'),
        };
        let mut url = Url::parse(root).map_err(|_| "Enter a valid provider base URL")?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Provider URLs cannot include credentials, queries, or fragments");
        }
        let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
            return Err("Use HTTPS, or HTTP for a local provider");
        }
        url.set_path(&format!("{}/{}", url.path().trim_end_matches('/'), suffix));
        Ok(url)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MaxFeature {
    TidyTabs,
    InstantLinks,
    FiveSecondPreviews,
    TidyTabTitles,
    TidyDownloads,
    ChatGptCommandBar,
    AskOnPage,
    BrowseForMe,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountReference {
    pub credential_id: String,
    pub label: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaxSettings {
    pub provider: ProviderConfig,
    pub enabled: BTreeSet<MaxFeature>,
    pub automatically_enable_future: bool,
    /// Stable opaque installation identity for public OAuth registration.
    pub host_id: String,
    pub accounts: Vec<AccountReference>,
    pub saved_providers: std::collections::BTreeMap<Provider, ProviderConfig>,
}
impl MaxSettings {
    pub fn enabled(&self, feature: MaxFeature) -> bool {
        self.enabled.contains(&feature)
    }
    pub fn set_enabled(&mut self, feature: MaxFeature, enabled: bool) {
        if enabled {
            self.enabled.insert(feature);
        } else {
            self.enabled.remove(&feature);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_is_scoped_to_the_selected_provider_and_preserves_version_paths() {
        let mut c = ProviderConfig {
            provider: Provider::OpenAiCompatible,
            base_url: "https://model.example/api/v1/".into(),
            ..Default::default()
        };
        assert_eq!(
            c.endpoint("chat/completions").unwrap().as_str(),
            "https://model.example/api/v1/chat/completions"
        );
        for invalid in [
            "http://remote.example/v1",
            "https://key@model.example/v1",
            "https://model.example/v1?key=x",
            "file:///tmp/model",
        ] {
            c.base_url = invalid.into();
            assert!(c.endpoint("chat/completions").is_err());
        }
        c.base_url = "http://127.0.0.1:1234/v1".into();
        assert!(c.endpoint("chat/completions").is_ok());
        c.authentication = Authentication::ChatGpt;
        assert!(c.endpoint("responses").is_err());
        c.provider = Provider::OpenAi;
        assert_eq!(
            c.endpoint("responses").unwrap().as_str(),
            "https://api.openai.com/v1/responses"
        );
    }
    #[test]
    fn configuration_has_no_implicit_data_submission_or_secret_fields() {
        let mut settings: MaxSettings = serde_json::from_str("{}").unwrap();
        assert!(settings.enabled.is_empty());
        settings.set_enabled(MaxFeature::TidyDownloads, true);
        let saved = serde_json::to_string(&settings).unwrap();
        assert!(!saved.contains("access_token") && !saved.contains("api_key\""));
        assert_eq!(
            serde_json::from_str::<MaxSettings>(&saved).unwrap(),
            settings
        );
    }
}

/// Browser context keeps useful web paths but never submits URL credentials,
/// query tokens, fragments, local file paths or inline document bodies.
pub fn context_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return String::new();
    };
    if !matches!(url.scheme(), "http" | "https") {
        return if value == "about:blank" {
            value.into()
        } else {
            format!("{}:", url.scheme())
        };
    }
    let _ = url.set_password(None);
    let _ = url.set_username("");
    url.set_query(None);
    url.set_fragment(None);
    url.into()
}
#[cfg(test)]
mod context_tests {
    #[test]
    fn browser_context_strips_credentials_tokens_and_local_document_bodies() {
        assert_eq!(
            super::context_url("https://alice:private@example.test/doc?token=secret#private"),
            "https://example.test/doc"
        );
        assert_eq!(
            super::context_url("file:///Users/name/private.pdf"),
            "file:"
        );
        assert_eq!(super::context_url("data:text/html,private body"), "data:");
        assert_eq!(super::context_url("about:blank"), "about:blank");
        assert_eq!(super::context_url("not a URL"), "");
    }
}
