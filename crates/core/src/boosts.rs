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
    /// The main dot's distance from the pad's center, 0..=100 (Arc's
    /// `primarySaturation`): the center is fully desaturated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saturation_point: Option<u8>,
    /// The accent dot's hue, in degrees from the main dot's (Arc's
    /// `secondaryHueOffset`); it turns with the main dot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_offset: Option<i16>,
    /// The accent dot's distance from the center, 0..=100.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_saturation: Option<u8>,
}

/// Set by the engine on the root of a page whose own background is dark.
pub const DARK_PAGE_ATTRIBUTE: &str = "data-echo-boost-dark";

/// Color pad geometry, in points: the inner pad and the dots on it.
pub const PAD_EXTENT: f32 = 134.;
pub const MAIN_DOT_RADIUS: f32 = 17.;
pub const ACCENT_DOT_RADIUS: f32 = 11.;
/// The dots stay within this circle around the pad's center (a fraction of
/// the pad): full saturation at its edge, none in the middle.
pub const RING_RADIUS: f32 = 0.47;
/// Arc's resting dots: the accent a little clockwise of the main dot.
const DEFAULT_ACCENT_OFFSET: i16 = 47;
const DEFAULT_SATURATION: u8 = 62;

/// The pad point at `angle` degrees (clockwise from the right) and
/// `saturation` (0..=1, the distance out to the circle's edge).
pub fn pad_point(angle: f32, saturation: f32) -> (f32, f32) {
    let angle = angle.to_radians();
    let reach = RING_RADIUS * saturation.clamp(0., 1.);
    (0.5 + reach * angle.cos(), 0.5 + reach * angle.sin())
}

/// Angle (degrees) and saturation (0..=1) of a pad point. The center has
/// no angle of its own.
fn pad_polar(point: (f32, f32)) -> (Option<f32>, f32) {
    let (x, y) = (point.0 - 0.5, point.1 - 0.5);
    let reach = x.hypot(y);
    ((reach > 0.001).then(|| y.atan2(x).to_degrees().rem_euclid(360.)), (reach / RING_RADIUS).min(1.))
}

/// `degrees` folded into -180..=180.
pub fn wrap_degrees(degrees: f32) -> f32 {
    let folded = degrees.rem_euclid(360.);
    if folded > 180. { folded - 360. } else { folded }
}

/// sRGB channels (0..=1) of an HSL color; `hue` in degrees.
fn hsl_rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let chroma = (1. - (2. * lightness - 1.).abs()) * saturation;
    let h = hue.rem_euclid(360.) / 60.;
    let x = chroma * (1. - (h % 2. - 1.).abs());
    let (r, g, b) = match h as u8 {
        0 => (chroma, x, 0.), 1 => (x, chroma, 0.), 2 => (0., chroma, x),
        3 => (0., x, chroma), 4 => (x, 0., chroma), _ => (chroma, 0., x),
    };
    let m = lightness - chroma / 2.;
    [r + m, g + m, b + m]
}

fn css_rgb([r, g, b]: [f32; 3]) -> String {
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    format!("rgb({} {} {})", channel(r), channel(g), channel(b))
}

/// The two blend layers that send the page's black to `dark` and its white
/// to `light`: multiply scales every color, then screen lifts it.
fn color_layers(dark: [f32; 3], light: [f32; 3]) -> (String, String) {
    let scale = std::array::from_fn(|index| 1. - (1. - light[index]) / (1. - dark[index]).max(0.001));
    (css_rgb(scale), css_rgb(dark))
}

impl BoostAppearance {
    /// The main dot's hue and saturation (0..=1), once a color is chosen.
    pub fn main_color(&self) -> Option<(f32, f32)> {
        Some((f32::from(self.tint?), f32::from(self.saturation_point.unwrap_or(DEFAULT_SATURATION)) / 100.))
    }

    pub fn accent_color(&self) -> Option<(f32, f32)> {
        let (hue, saturation) = self.main_color()?;
        Some((hue + f32::from(self.accent_offset.unwrap_or(DEFAULT_ACCENT_OFFSET)),
            self.accent_saturation.map_or(saturation, |value| f32::from(value) / 100.)))
    }

    /// The main dot, 0..=1 per axis, once a color is chosen.
    pub fn main_point(&self) -> Option<(f32, f32)> { self.main_color().map(|(hue, saturation)| pad_point(hue, saturation)) }

    /// The accent dot, which turns around the center with the main dot.
    pub fn accent_point(&self) -> Option<(f32, f32)> { self.accent_color().map(|(hue, saturation)| pad_point(hue, saturation)) }

    /// Moves the main dot toward `point`, held inside the circle; the accent
    /// keeps its hue offset and so turns with it.
    pub fn set_main_point(&mut self, point: (f32, f32)) {
        let (angle, saturation) = pad_polar(point);
        let angle = angle.or(self.tint.map(f32::from)).unwrap_or(0.);
        self.tint = Some(angle.round() as u16 % 360);
        self.saturation_point = Some((saturation * 100.).round() as u8);
    }

    /// Moves the accent dot alone toward `point`, held inside the circle.
    pub fn turn_accent_toward(&mut self, point: (f32, f32)) {
        let Some((main, _)) = self.main_color() else { return; };
        let (angle, saturation) = pad_polar(point);
        if let Some(angle) = angle {
            self.accent_offset = Some(wrap_degrees(angle - main).round() as i16);
        }
        self.accent_saturation = Some((saturation * 100.).round() as u8);
    }

    pub fn clear_color(&mut self) {
        self.tint = None;
        self.saturation_point = None;
        self.accent_offset = None;
        self.accent_saturation = None;
    }

    /// Blend layer colors for light pages and for dark pages. The main dot
    /// tints the page's own background, the accent its text.
    fn color_scheme(&self) -> Option<[(String, String); 2]> {
        let ((hue, saturation), (accent_hue, accent_saturation)) = (self.main_color()?, self.accent_color()?);
        let light_page = color_layers(hsl_rgb(accent_hue, 0.6 * accent_saturation, 0.14), hsl_rgb(hue, 0.75 * saturation, 0.93 - 0.05 * saturation));
        let dark_page = color_layers(hsl_rgb(hue, 0.5 * saturation, 0.08 + 0.1 * saturation), hsl_rgb(accent_hue, 0.6 * accent_saturation, 0.93));
        Some([light_page, dark_page])
    }

    pub fn stylesheet(&self) -> String {
        let mut css = String::new();
        // Like Arc's page color filter, every painted color is remapped: two
        // fixed blend layers over the whole page, chosen by the page's own
        // lightness (the engine marks dark pages on the root).
        if let Some([light, dark]) = self.color_scheme() {
            css.push_str(&format!(concat!(
                "html::before, html::after {{ content: \"\" !important; position: fixed !important; inset: 0 !important; display: block !important; pointer-events: none !important; z-index: 2147483647 !important; }}\n",
                "html::before {{ background: {} !important; mix-blend-mode: multiply !important; }}\n",
                "html::after {{ background: {} !important; mix-blend-mode: screen !important; }}\n",
                "html[{dark}]::before {{ background: {} !important; }}\n",
                "html[{dark}]::after {{ background: {} !important; }}\n"),
                light.0, light.1, dark.0, dark.1, dark = DARK_PAGE_ATTRIBUTE));
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
        boost.appearance=BoostAppearance{tint:Some(210),font:Some("PT Mono".into()),size:2,case:1,inverted:true,contrast:Some(120),brightness:Some(90),saturation:Some(80),saturation_point:Some(80),accent_offset:Some(45),accent_saturation:Some(30)};
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
    fn the_dots_stay_in_the_circle_and_the_accent_turns_with_the_main_dot() {
        let mut look = BoostAppearance::default();
        assert!(look.main_point().is_none() && look.accent_point().is_none() && look.stylesheet().is_empty());
        let reach = |p: (f32, f32)| (p.0 - 0.5).hypot(p.1 - 0.5);
        // Distance from the center is saturation, capped at the circle's edge.
        look.set_main_point((0.5 + RING_RADIUS / 2., 0.5));
        assert_eq!((look.tint, look.saturation_point), (Some(0), Some(50)));
        look.set_main_point((2., 0.5));
        assert_eq!((look.tint, look.saturation_point), (Some(0), Some(100)));
        assert!((reach(look.main_point().unwrap()) - RING_RADIUS).abs() < 0.001);
        // The center is gray and keeps the hue it had.
        look.set_main_point((0.5, 0.5));
        assert_eq!((look.tint, look.saturation_point), (Some(0), Some(0)));
        // The accent keeps its hue offset, so it turns around with the main dot.
        look.set_main_point((0.5, 0.9));
        assert_eq!(look.tint, Some(90));
        assert_eq!(look.accent_color().map(|(hue, _)| hue.round()), Some(137.));
        // Moved alone, it takes its own hue offset and saturation.
        look.turn_accent_toward((0.5, 0.5 - RING_RADIUS / 4.));
        assert_eq!((look.tint, look.accent_offset, look.accent_saturation), (Some(90), Some(180), Some(25)));
        look.set_main_point((0.9, 0.5));
        assert_eq!(look.accent_color().map(|(hue, _)| hue.round()), Some(180.));
        // Light and dark pages get their own pair of blend layers.
        let css = look.stylesheet();
        assert!(css.contains("mix-blend-mode: multiply") && css.contains("mix-blend-mode: screen") && css.contains(DARK_PAGE_ATTRIBUTE));
        look.clear_color();
        assert!(look.main_point().is_none() && look.stylesheet().is_empty());
    }
    #[test]
    fn the_blend_layers_send_black_and_white_to_the_chosen_colors() {
        let dark = [0.1, 0.3, 0.25];
        let light = [0.95, 0.9, 0.92];
        let (scale, lift) = color_layers(dark, light);
        let parse = |css: &str| -> Vec<f32> { css.trim_start_matches("rgb(").trim_end_matches(')').split(' ').map(|v| v.parse::<f32>().unwrap() / 255.).collect() };
        let (scale, lift) = (parse(&scale), parse(&lift));
        for index in 0..3 {
            let white = 1. - (1. - scale[index]) * (1. - lift[index]);
            assert!((white - light[index]).abs() < 0.01 && (lift[index] - dark[index]).abs() < 0.01);
        }
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
