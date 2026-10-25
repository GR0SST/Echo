//! Only actual provider search sources may become Instant Links. Model prose
//! and fabricated JSON URLs are never interpreted as navigation commands.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebLink {
    pub title: String,
    pub url: String,
}
impl WebLink {
    pub fn valid(&self) -> bool {
        let Ok(url) = url::Url::parse(&self.url) else {
            return false;
        };
        let Some(host) = url.host_str() else {
            return false;
        };
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none_or(|p| p == 80 || p == 443)
            && !host.eq_ignore_ascii_case("localhost")
            && !host.ends_with(".localhost")
            && !host.ends_with(".local")
            && host.contains('.')
            && matches!(url.host(), Some(url::Host::Domain(_)))
            && self.url.len() <= 8192
            && !self.title.trim().is_empty()
            && self.title.chars().count() <= 300
            && !self.title.chars().any(|c| {
                c.is_control() || matches!(c, '\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
            })
    }
}
