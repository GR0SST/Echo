//! Local, profile-owned site customizations. Sharing is deliberately absent.
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BoostId(u64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boost {
    pub id: BoostId,
    pub name: String,
    /// Canonical ASCII host, without a scheme, port or wildcard.
    domain: String,
    pub enabled: bool,
    pub css: String,
    pub javascript: String,
    pub zap_selectors: Vec<String>,
    #[serde(default)]
    pub appearance: BoostAppearance,
}
impl Boost {
    pub fn domain(&self) -> &str {
        &self.domain
    }
    pub fn matches(&self, url: &str) -> bool {
        self.enabled && domain(url).as_deref() == Some(self.domain.as_str())
    }
    pub fn stylesheet(&self) -> String {
        let mut css = self.appearance.stylesheet();
        css.push_str(&self.css);
        for selector in &self.zap_selectors {
            if !selector.trim().is_empty() {
                css.push_str("\n");
                css.push_str(selector);
                css.push_str(" { display: none !important; }\n");
            }
        }
        css
    }
}

/// Local simple-editor controls; the code editor remains independent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoostAppearance {
    pub tint: Option<u16>,
    pub font: Option<String>,
    pub size: i8,
    pub case: u8,
    pub inverted: bool,
    pub contrast: Option<u16>,
    pub brightness: Option<u16>,
    pub saturation: Option<u16>,
}
impl BoostAppearance {
    pub fn stylesheet(&self) -> String {
        let mut css = String::new();
        if let Some(hue) = self.tint {
            css.push_str(&format!("html, body {{ background-color: hsl({}, 55%, 88%) !important; color: hsl({}, 35%, 16%) !important; }}\n", hue % 360, hue % 360));
        }
        if let Some(font) = &self.font {
            // Font names are CSS string data, never raw source.
            css.push_str(&format!(
                "* {{ font-family: {} !important; }}\n",
                if font == "system-ui" {
                    font.clone()
                } else {
                    serde_json::to_string(font).unwrap()
                }
            ));
        }
        if self.size != 0 {
            css.push_str(&format!(
                "html {{ font-size: {}% !important; }}\n",
                100 + i16::from(self.size.clamp(-5, 10)) * 10
            ));
        }
        let case = match self.case {
            1 => Some("uppercase"),
            2 => Some("lowercase"),
            3 => Some("capitalize"),
            _ => None,
        };
        if let Some(case) = case {
            css.push_str(&format!("* {{ text-transform: {case} !important; }}\n"));
        }
        let mut filters = Vec::new();
        if self.inverted {
            filters.push("invert(1) hue-rotate(180deg)".into());
        }
        for (name, value) in [
            ("contrast", self.contrast),
            ("brightness", self.brightness),
            ("saturate", self.saturation),
        ] {
            if let Some(value) = value {
                filters.push(format!("{name}({}%)", value.min(200)));
            }
        }
        if !filters.is_empty() {
            css.push_str(&format!(
                "html {{ filter: {} !important; }}\n",
                filters.join(" ")
            ));
        }
        css
    }
}

fn domain(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    matches!(url.scheme(), "http" | "https")
        .then(|| url.host_str().map(str::to_owned))
        .flatten()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Boosts {
    #[serde(default)]
    entries: Vec<Boost>,
    #[serde(default)]
    next_id: u64,
}
impl Boosts {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let mut ids = std::collections::HashSet::new();
        for boost in &self.entries {
            if boost.id.0 == 0
                || boost.id.0 > self.next_id
                || !ids.insert(boost.id.0)
                || domain(&format!("https://{}/", boost.domain)).as_deref()
                    != Some(boost.domain.as_str())
            {
                return Err("invalid profile Boost registry");
            }
        }
        Ok(())
    }
    pub fn iter(&self) -> impl Iterator<Item = &Boost> {
        self.entries.iter()
    }
    pub fn get_mut(&mut self, id: BoostId) -> Option<&mut Boost> {
        self.entries.iter_mut().find(|boost| boost.id == id)
    }
    /// Non-web documents cannot acquire an executable Boost.
    pub fn create(&mut self, url: &str) -> Option<BoostId> {
        let domain = domain(url)?;
        let id = BoostId(self.next_id.checked_add(1)?);
        self.next_id = id.0;
        self.entries.push(Boost {
            id,
            name: "My Boost".into(),
            domain,
            enabled: true,
            css: String::new(),
            javascript: String::new(),
            zap_selectors: Vec::new(),
            appearance: BoostAppearance::default(),
        });
        Some(id)
    }
    pub fn remove(&mut self, id: BoostId) -> bool {
        let before = self.entries.len();
        self.entries.retain(|boost| boost.id != id);
        before != self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session, Browser};
    #[test]
    fn site_matching_is_canonical_and_cannot_escape_to_another_origin() {
        let mut boosts = Boosts::default();
        let id = boosts.create("https://EXAMPLE.com:8443/path").unwrap();
        let boost = boosts.get_mut(id).unwrap();
        assert!(boost.matches("http://example.com/other"));
        for url in [
            "https://example.com.evil.test",
            "https://sub.example.com",
            "https://example.com@evil.test",
            "file:///example.com",
            "data:text/html,example.com",
        ] {
            assert!(!boost.matches(url), "{url}");
        }
        boost.enabled = false;
        assert!(!boost.matches("https://example.com"));
        assert!(boosts.create("echo://settings").is_none());
    }
    #[test]
    fn boosts_round_trip_in_their_profile_and_ids_are_not_reused() {
        let mut browser = Browser::new();
        let profile = browser.create_user_profile("Other").unwrap();
        let boosts = &mut browser.active_user_profile_mut().boosts;
        let id = boosts.create("https://example.com").unwrap();
        let boost = boosts.get_mut(id).unwrap();
        boost.css = "body { color: red; }".into();
        boost.javascript = "window.example = true".into();
        boost.zap_selectors.push(".advert".into());
        boost.appearance=BoostAppearance{tint:Some(210),font:Some("PT Mono".into()),size:2,case:1,inverted:true,contrast:Some(120),brightness:Some(90),saturation:Some(80)};
        let appearance=boost.appearance.clone();
        let mut restored = session::from_json(&session::to_json(&browser)).unwrap();
        assert_eq!(
            restored
                .user_profile(profile)
                .unwrap()
                .boosts
                .iter()
                .count(),
            0
        );
        let boosts = &mut restored.active_user_profile_mut().boosts;
        assert_eq!(boosts.iter().next().unwrap().appearance,appearance);
        assert!(boosts
            .iter()
            .next()
            .unwrap()
            .stylesheet()
            .contains(".advert { display: none !important; }"));
        assert_eq!(
            boosts.iter().next().unwrap().javascript,
            "window.example = true"
        );
        assert!(boosts.remove(id));
        assert_ne!(boosts.create("https://example.com").unwrap(), id);
    }
    #[test]
    fn corrupted_boost_ids_and_domains_are_rejected() {
        let mut browser = Browser::new();
        browser
            .active_user_profile_mut()
            .boosts
            .create("https://example.com")
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
        for (field, bad) in [
            ("domain", serde_json::json!("example.com/evil")),
            ("id", serde_json::json!(0)),
        ] {
            let mut changed = value.clone();
            changed["profiles"]["1"]["boosts"]["entries"][0][field] = bad;
            assert!(session::from_json(&changed.to_string()).is_err());
        }
    }
    #[test]
    fn existing_sessions_default_to_no_boosts() {
        let mut value: serde_json::Value =
            serde_json::from_str(&session::to_json(&Browser::new())).unwrap();
        value["profiles"]["1"]
            .as_object_mut()
            .unwrap()
            .remove("boosts");
        assert_eq!(
            session::from_json(&value.to_string())
                .unwrap()
                .active_user_profile()
                .boosts
                .iter()
                .count(),
            0
        );
    }
}
