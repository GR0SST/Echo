//! A bounded cache keeps sidebar icons available for unloaded and restored tabs.
use crate::Browser;
use serde::{Deserialize, Serialize};
use url::Url;

pub const MAX_FAVICON_BYTES: usize = 64 * 1024;
const MAX_ICONS: usize = 256;

#[derive(Debug, Serialize, Deserialize)]
pub struct SiteIcon {
    page: String,
    origin: Option<String>,
    png: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(marker: u8) -> Vec<u8> { [b"\x89PNG\r\n\x1a\n".as_slice(), &[marker]].concat() }

    #[test]
    fn page_icons_win_over_site_fallbacks_and_survive_sessions() {
        let mut browser = Browser::new();
        assert!(browser.cache_favicon("https://example.com/a#one", png(1)));
        assert!(browser.cache_favicon("https://example.com/b", png(2)));
        assert_eq!(browser.favicon("https://example.com/a#two"), Some(png(1).as_slice()));
        assert_eq!(browser.favicon("https://example.com/new"), Some(png(2).as_slice()));
        assert!(browser.favicon("http://example.com/a").is_none());
        assert!(browser.favicon("https://example.com:8080/a").is_none());
        assert!(browser.favicon("https://elsewhere.com/a").is_none());
        assert!(browser.cache_favicon("file:///tmp/a.html", png(3)));
        assert!(browser.favicon("file:///tmp/b.html").is_none());
        let restored = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert_eq!(restored.favicon("https://example.com/a"), Some(png(1).as_slice()));
    }

    #[test]
    fn cache_is_bounded_deduplicated_and_rejects_non_images_or_internal_pages() {
        let mut browser = Browser::new();
        assert!(!browser.cache_favicon("https://example.com", Vec::new()));
        let mut oversized = png(1);
        oversized.resize(MAX_FAVICON_BYTES + 1, 0);
        assert!(!browser.cache_favicon("https://example.com", oversized));
        assert!(!browser.cache_favicon("browser://history", png(1)));
        for index in 0..=MAX_ICONS {
            assert!(browser.cache_favicon(&format!("https://site{index}.example"), png(1)));
        }
        assert_eq!(browser.favicon_cache.len(), MAX_ICONS);
        assert!(browser.favicon("https://site0.example").is_none());
        assert!(browser.cache_favicon("https://site1.example", png(2)));
        assert_eq!(browser.favicon_cache.len(), MAX_ICONS);
        assert_eq!(browser.favicon("https://site1.example"), Some(png(2).as_slice()));
    }
}

fn key(page: &str) -> Option<(String, Option<String>)> {
    let mut url = Url::parse(page).ok()?;
    if !matches!(url.scheme(), "http" | "https" | "file") { return None; }
    url.set_fragment(None);
    let origin = (url.scheme() != "file").then(|| url.origin().ascii_serialization());
    Some((url.to_string(), origin))
}

impl Browser {
    /// CEF decodes the site's image and supplies a small canonical PNG.
    pub fn cache_favicon(&mut self, page: &str, png: Vec<u8>) -> bool {
        if png.len() > MAX_FAVICON_BYTES || !png.starts_with(b"\x89PNG\r\n\x1a\n") { return false; }
        let Some((page, origin)) = key(page) else { return false; };
        self.favicon_cache.retain(|entry| entry.page != page);
        self.favicon_cache.insert(0, SiteIcon { page, origin, png });
        self.favicon_cache.truncate(MAX_ICONS);
        true
    }

    /// Prefer a page's icon; use the site's latest icon while a new page loads.
    pub fn favicon(&self, page: &str) -> Option<&[u8]> {
        let (page, origin) = key(page)?;
        self.favicon_cache.iter().find(|entry| entry.page == page)
            .or_else(|| self.favicon_cache.iter().find(|entry| origin.is_some() && entry.origin == origin))
            .map(|entry| entry.png.as_slice())
    }
}
