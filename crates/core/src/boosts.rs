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
    /// The accent dot's place on the ring, in degrees from the main dot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_offset: Option<i16>,
}

/// Color pad geometry, in points: the inner pad and the dots on it.
pub const PAD_EXTENT: f32 = 134.;
pub const MAIN_DOT_RADIUS: f32 = 17.;
pub const ACCENT_DOT_RADIUS: f32 = 11.;
/// Both dots ride this ring around the pad's center (a fraction of the pad):
/// never in the middle, never past the edge.
pub const RING_RADIUS: f32 = 0.47;
/// The accent trails the main dot by this much until moved on its own.
const DEFAULT_ACCENT_OFFSET: i16 = -26;
/// Closest the accent may come to the main dot without overlapping it.
const MIN_ACCENT_GAP: f32 = 25.;

/// Hue (degrees) and strength (0..=1) of a pad point: the angle around the
/// pad's center and the distance from it.
pub fn pad_color(point: (f32, f32)) -> (f32, f32) {
    let (x, y) = (point.0 - 0.5, point.1 - 0.5);
    (y.atan2(x).to_degrees().rem_euclid(360.), ((x * x + y * y).sqrt() / 0.5).min(1.))
}

/// The ring point at `angle` degrees (clockwise from the right).
pub fn ring_point(angle: f32) -> (f32, f32) {
    let angle = angle.to_radians();
    (0.5 + RING_RADIUS * angle.cos(), 0.5 + RING_RADIUS * angle.sin())
}

/// The angle of a pad point around the center; the center itself has none.
fn pad_angle(point: (f32, f32)) -> Option<f32> {
    let (x, y) = (point.0 - 0.5, point.1 - 0.5);
    (x.hypot(y) > f32::EPSILON).then(|| y.atan2(x).to_degrees().rem_euclid(360.))
}

/// `degrees` folded into -180..=180.
pub fn wrap_degrees(degrees: f32) -> f32 {
    let folded = degrees.rem_euclid(360.);
    if folded > 180. { folded - 360. } else { folded }
}

impl BoostAppearance {
    /// The main dot's angle on the ring, once a color is chosen.
    pub fn main_angle(&self) -> Option<f32> { self.tint.map(f32::from) }

    pub fn accent_angle(&self) -> Option<f32> {
        Some(self.main_angle()? + f32::from(self.accent_offset.unwrap_or(DEFAULT_ACCENT_OFFSET)))
    }

    /// The main dot, 0..=1 per axis, once a color is chosen.
    pub fn main_point(&self) -> Option<(f32, f32)> { self.main_angle().map(ring_point) }

    /// The accent dot, on the same ring a little behind the main dot.
    pub fn accent_point(&self) -> Option<(f32, f32)> { self.accent_angle().map(ring_point) }

    /// Slides the main dot along the ring toward `point`; the accent follows
    /// on the side it came from, so it always trails the motion.
    pub fn set_main_point(&mut self, point: (f32, f32)) {
        let Some(angle) = pad_angle(point) else { return; };
        let angle = angle.round() as u16 % 360;
        if let Some(previous) = self.tint {
            let motion = wrap_degrees(f32::from(angle) - f32::from(previous));
            if motion.abs() >= 1. {
                let gap = self.accent_offset.unwrap_or(DEFAULT_ACCENT_OFFSET).abs();
                self.accent_offset = Some(if motion > 0. { -gap } else { gap });
            }
        }
        self.tint = Some(angle);
    }

    /// Slides the accent along the ring toward `point`, short of the main dot.
    pub fn turn_accent_toward(&mut self, point: (f32, f32)) {
        let (Some(main), Some(angle)) = (self.main_angle(), pad_angle(point)) else { return; };
        let offset = wrap_degrees(angle - main);
        let gap = offset.abs().max(MIN_ACCENT_GAP).round() as i16;
        self.accent_offset = Some(if offset < 0. { -gap } else { gap });
    }

    pub fn clear_color(&mut self) {
        self.tint = None;
        self.accent_offset = None;
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
        boost.appearance=BoostAppearance{tint:Some(210),font:Some("PT Mono".into()),size:2,case:1,inverted:true,contrast:Some(120),brightness:Some(90),saturation:Some(80),accent_offset:Some(45)};
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
    fn both_dots_ride_one_ring_and_the_accent_trails_the_main_dot() {
        let mut look = BoostAppearance::default();
        assert!(look.main_point().is_none() && look.accent_point().is_none() && look.stylesheet().is_empty());
        let on_ring = |p: (f32, f32)| ((p.0 - 0.5).hypot(p.1 - 0.5) - RING_RADIUS).abs() < 0.001;
        // A press anywhere, even near the center or outside the pad, lands on the ring.
        look.set_main_point((0.55, 0.5));
        assert_eq!(look.tint, Some(0));
        assert!(on_ring(look.main_point().unwrap()) && on_ring(look.accent_point().unwrap()));
        look.set_main_point((2., 0.5));
        assert_eq!(look.tint, Some(0));
        look.set_main_point((0.5, 0.5));
        assert_eq!(look.tint, Some(0), "the center has no angle");
        // Clockwise motion leaves the accent behind, counterclockwise flips it.
        look.set_main_point((0.5, 0.9));
        assert_eq!((look.tint, look.accent_offset), (Some(90), Some(-26)));
        look.set_main_point((0.9, 0.5));
        assert_eq!((look.tint, look.accent_offset), (Some(0), Some(26)));
        // The accent slides along the ring alone, never onto the main dot.
        look.turn_accent_toward((0.5, 0.1));
        assert_eq!((look.tint, look.accent_offset), (Some(0), Some(-90)));
        look.turn_accent_toward((0.9, 0.51));
        assert_eq!(look.accent_offset, Some(25));
        assert!(on_ring(look.accent_point().unwrap()));
        // The gap it was given survives the next drag of the main dot.
        look.turn_accent_toward((0.5, 0.1));
        look.set_main_point((0.5, 0.9));
        assert_eq!(look.accent_offset, Some(-90));
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
