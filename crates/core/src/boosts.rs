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
    /// The main dot on the color pad, 0..=1000 per axis. Older Boosts only
    /// stored `tint`; their dot is derived from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_point: Option<[u16; 2]>,
    /// The accent dot's angle around the main dot, in degrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_angle: Option<u16>,
}

/// Color pad geometry, in points: the inner pad and the dots on it.
pub const PAD_EXTENT: f32 = 134.;
pub const MAIN_DOT_RADIUS: f32 = 17.;
pub const ACCENT_DOT_RADIUS: f32 = 11.;
/// The accent dot keeps this distance from the main dot's center.
pub const ACCENT_DISTANCE: f32 = 31.;
const DEFAULT_ACCENT_ANGLE: f32 = 112.;

/// Hue (degrees) and strength (0..=1) of a pad point: the angle around the
/// pad's center and the distance from it.
pub fn pad_color(point: (f32, f32)) -> (f32, f32) {
    let (x, y) = (point.0 - 0.5, point.1 - 0.5);
    (y.atan2(x).to_degrees().rem_euclid(360.), ((x * x + y * y).sqrt() / 0.5).min(1.))
}

impl BoostAppearance {
    /// The main dot, 0..=1 per axis, once a color is chosen.
    pub fn main_point(&self) -> Option<(f32, f32)> {
        if let Some([x, y]) = self.color_point { return Some((f32::from(x) / 1000., f32::from(y) / 1000.)); }
        let angle = f32::from(self.tint?).to_radians();
        Some((0.5 + 0.35 * angle.cos(), 0.5 + 0.35 * angle.sin()))
    }

    /// The accent dot, which travels with the main dot at a fixed distance.
    pub fn accent_point(&self) -> Option<(f32, f32)> {
        let (x, y) = self.main_point()?;
        let angle = self.accent_angle.map_or(DEFAULT_ACCENT_ANGLE, f32::from).to_radians();
        let reach = ACCENT_DISTANCE / PAD_EXTENT;
        Some(((x + reach * angle.cos()).clamp(0., 1.), (y + reach * angle.sin()).clamp(0., 1.)))
    }

    /// Moves the main dot (the accent follows), keeping it inside the pad.
    pub fn set_main_point(&mut self, point: (f32, f32)) {
        let margin = MAIN_DOT_RADIUS / PAD_EXTENT;
        let (x, y) = (point.0.clamp(margin, 1. - margin), point.1.clamp(margin, 1. - margin));
        self.color_point = Some([(x * 1000.).round() as u16, (y * 1000.).round() as u16]);
        self.tint = Some(pad_color((x, y)).0.round() as u16 % 360);
    }

    /// Turns the accent dot around the main dot toward `point`.
    pub fn turn_accent_toward(&mut self, point: (f32, f32)) {
        let Some((x, y)) = self.main_point() else { return; };
        if (point.0 - x).abs() < f32::EPSILON && (point.1 - y).abs() < f32::EPSILON { return; }
        self.accent_angle = Some((point.1 - y).atan2(point.0 - x).to_degrees().rem_euclid(360.).round() as u16 % 360);
    }

    pub fn clear_color(&mut self) {
        self.tint = None;
        self.color_point = None;
        self.accent_angle = None;
    }

    pub fn stylesheet(&self) -> String {
        let mut css = String::new();
        // The main dot colors the page, the accent dot its text.
        if let (Some(main), Some(accent)) = (self.main_point(), self.accent_point()) {
            let (hue, strength) = pad_color(main);
            let (text_hue, text_strength) = pad_color(accent);
            css.push_str(&format!("html, body {{ background-color: hsl({:.0}, {:.0}%, 88%) !important; color: hsl({:.0}, {:.0}%, 18%) !important; }}\n",
                hue, 20. + 50. * strength, text_hue, 30. + 40. * text_strength));
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
        // Text size is host page zoom in the browser (Arc's Boost text zoom), not CSS.
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
    /// Names are presentation data; changing one never changes a Boost's rules
    /// or executable identity. The caller owns any dialog validation.
    pub fn rename(&mut self, id: BoostId, name: String) -> bool {
        let Some(boost) = self.get_mut(id) else { return false; };
        if boost.name == name { return false; }
        boost.name = name;
        true
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
        boost.appearance=BoostAppearance{tint:Some(210),font:Some("PT Mono".into()),size:2,case:1,inverted:true,contrast:Some(120),brightness:Some(90),saturation:Some(80),color_point:Some([200,700]),accent_angle:Some(45)};
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
    fn the_accent_dot_follows_the_main_dot_and_turns_on_its_own() {
        let mut look = BoostAppearance::default();
        assert!(look.main_point().is_none() && look.accent_point().is_none() && look.stylesheet().is_empty());
        look.set_main_point((0.3, 0.4));
        let reach = ACCENT_DISTANCE / PAD_EXTENT;
        let (main, accent) = (look.main_point().unwrap(), look.accent_point().unwrap());
        assert!((((accent.0 - main.0).powi(2) + (accent.1 - main.1).powi(2)).sqrt() - reach).abs() < 0.01);
        // Moving the main dot carries the accent by the same offset.
        look.set_main_point((0.6, 0.5));
        let (moved, carried) = (look.main_point().unwrap(), look.accent_point().unwrap());
        assert!(((carried.0 - moved.0) - (accent.0 - main.0)).abs() < 0.01 && ((carried.1 - moved.1) - (accent.1 - main.1)).abs() < 0.01);
        // Turning the accent leaves the main dot and the page tint alone.
        let tint = look.tint;
        look.turn_accent_toward((0.6, 0.1));
        assert_eq!(look.main_point(), Some(moved));
        assert_eq!(look.tint, tint);
        assert_eq!(look.accent_angle, Some(270));
        let turned = look.accent_point().unwrap();
        assert!((turned.0 - moved.0).abs() < 0.01 && turned.1 < moved.1);
        // The main dot stays inside the pad; reset clears both dots.
        look.set_main_point((-1., 2.));
        let margin = MAIN_DOT_RADIUS / PAD_EXTENT;
        assert_eq!(look.main_point().map(|(x, y)| ((x * 1000.).round(), (y * 1000.).round())), Some(((margin * 1000.).round(), ((1. - margin) * 1000.).round())));
        look.clear_color();
        assert!(look.main_point().is_none() && look.stylesheet().is_empty());
    }
    #[test]
    fn rename_and_delete_target_only_the_captured_profile_and_survive_sessions() {
        let mut browser = Browser::new();
        let owner = browser.active_user_profile_id();
        let other = browser.create_user_profile("Other").unwrap();
        let id = browser.user_profile_mut(owner).unwrap().boosts.create("https://example.com").unwrap();
        let peer = browser.user_profile_mut(other).unwrap().boosts.create("https://example.com").unwrap();
        assert_eq!(id, peer, "Boost IDs are profile-local");
        let boosts = &mut browser.user_profile_mut(owner).unwrap().boosts;
        let boost = boosts.get_mut(id).unwrap();
        boost.enabled = false;
        boost.css = "body { color: red; }".into();
        boost.javascript = "window.custom = true".into();
        boost.zap_selectors.push(".ad".into());
        boost.appearance.tint = Some(220);
        let mut expected = boost.clone();
        expected.name = "Reading 🦉 — Текст".into();
        assert!(boosts.rename(id, expected.name.clone()));
        assert!(!boosts.rename(id, expected.name.clone()));
        assert!(!boosts.rename(BoostId(999), "Missing".into()));
        assert_eq!(boosts.iter().next(), Some(&expected));
        let mut restored = session::from_json(&session::to_json(&browser)).unwrap();
        assert_eq!(restored.user_profile(owner).unwrap().boosts.iter().next(), Some(&expected));
        assert_eq!(restored.user_profile(other).unwrap().boosts.iter().next().unwrap().name, "My Boost");
        assert!(restored.user_profile_mut(owner).unwrap().boosts.remove(id));
        assert!(!restored.user_profile_mut(owner).unwrap().boosts.remove(id));
        let restored = session::from_json(&session::to_json(&restored)).unwrap();
        assert_eq!(restored.user_profile(owner).unwrap().boosts.iter().count(), 0);
        assert_eq!(restored.user_profile(other).unwrap().boosts.iter().next().unwrap().id, peer);
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
