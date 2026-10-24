//! Local Space appearance. Stored separately from the legacy accent so older
//! sessions keep their original appearance until the user edits their theme.
use crate::{Browser, Space, SpaceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeAppearance {
    #[default]
    Automatic,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseTexture {
    #[default]
    Grain,
    Sand,
    Tweed,
    Denim,
    None,
}
impl NoiseTexture {
    pub fn next(self) -> Self {
        match self {
            Self::Grain => Self::Sand,
            Self::Sand => Self::Tweed,
            Self::Tweed => Self::Denim,
            Self::Denim => Self::None,
            Self::None => Self::Grain,
        }
    }
}

/// RGB components in the sRGB color space, in 0..=1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThemeColor {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}
impl ThemeColor {
    /// Convert linear-light sRGB catalog components to stored sRGB.
    pub fn from_linear_srgb(rgb: [f32; 3]) -> Self {
        fn encode(value: f32) -> f32 {
            let value = value.clamp(0., 1.);
            if value <= 0.0031308 { value * 12.92 } else { 1.055 * value.powf(1. / 2.4) - 0.055 }
        }
        Self { red: encode(rgb[0]), green: encode(rgb[1]), blue: encode(rgb[2]) }
    }
    /// Convert Display P3 (D65) to sRGB, clipping outside the output gamut.
    pub fn from_display_p3(rgb: [f32; 3]) -> Self {
        let [r, g, b] = rgb.map(|v| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) });
        Self::from_linear_srgb([
            1.224745 * r - 0.224904 * g,
            -0.042058 * r + 1.042081 * g,
            -0.019642 * r - 0.078655 * g + 1.098537 * b,
        ])
    }
    pub fn valid(self) -> bool {
        [self.red, self.green, self.blue]
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
    }
    pub fn from_hsv(hue: f32, saturation: f32, value: f32) -> Self {
        let h = hue.rem_euclid(360.) / 60.;
        let c = value * saturation;
        let x = c * (1. - (h % 2. - 1.).abs());
        let (r, g, b) = match h as u8 {
            0 => (c, x, 0.),
            1 => (x, c, 0.),
            2 => (0., c, x),
            3 => (0., x, c),
            4 => (x, 0., c),
            _ => (c, 0., x),
        };
        let m = value - c;
        Self {
            red: r + m,
            green: g + m,
            blue: b + m,
        }
    }
    pub fn hsv(self) -> (f32, f32, f32) {
        let max = self.red.max(self.green).max(self.blue);
        let min = self.red.min(self.green).min(self.blue);
        let d = max - min;
        let h = if d == 0. {
            0.
        } else if max == self.red {
            60. * ((self.green - self.blue) / d).rem_euclid(6.)
        } else if max == self.green {
            60. * ((self.blue - self.red) / d + 2.)
        } else {
            60. * ((self.red - self.green) / d + 4.)
        };
        (h, if max == 0. { 0. } else { d / max }, max)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpaceTheme {
    pub colors: Vec<ThemeColor>,
    pub intensity: f32,
    pub noise: f32,
    pub texture: NoiseTexture,
    pub appearance: ThemeAppearance,
}
impl SpaceTheme {
    pub fn from_hue(hue: f32) -> Self {
        Self {
            colors: vec![ThemeColor::from_hsv(hue, 0.5, 0.9)],
            intensity: 0.8,
            noise: 0.0,
            texture: NoiseTexture::Grain,
            appearance: ThemeAppearance::Automatic,
        }
    }
    pub fn valid(&self) -> bool {
        (1..=3).contains(&self.colors.len())
            && self.colors.iter().all(|c| c.valid())
            && [self.intensity, self.noise]
                .into_iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
    }
}
impl Space {
    pub fn accent_hue(&self) -> f32 {
        self.theme
            .as_ref()
            .map_or(self.hue, |theme| theme.colors[0].hsv().0)
    }
    pub fn theme(&self) -> SpaceTheme {
        self.theme
            .clone()
            .unwrap_or_else(|| SpaceTheme::from_hue(self.hue))
    }
}
impl Browser {
    /// A committed theme edit owns only appearance; Undo must not rewind tabs,
    /// the Space's name, or unrelated edits in another window.
    pub fn update_space_theme(&mut self, id: SpaceId, theme: SpaceTheme) -> bool {
        if !theme.valid() {
            return false;
        }
        let Some(space) = self.spaces.iter_mut().find(|space| space.id == id) else {
            return false;
        };
        if space.theme.as_ref() == Some(&theme) {
            return false;
        }
        let before = space.theme.replace(theme.clone());
        self.sidebar_history.record_theme(id, before, Some(theme));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session;
    use std::time::SystemTime;
    #[test]
    fn catalog_colors_decode_transfer_functions_and_clip_p3_to_srgb() {
        let cases = [
            (ThemeColor::from_linear_srgb([1., 0.235, 0.097]), [255., 133., 88.]),
            (ThemeColor::from_display_p3([0.950, 0.840, 0.428]), [248., 213., 88.]),
            (ThemeColor::from_display_p3([0.451, 0.897, 0.612]), [52., 232., 149.]),
            (ThemeColor::from_display_p3([0.496, 0.721, 0.838]), [108., 186., 217.]),
        ];
        for (color, expected) in cases {
            assert!(color.valid());
            for (actual, expected) in [color.red, color.green, color.blue].into_iter().zip(expected) {
                assert!((actual * 255. - expected).abs() < 1.);
            }
        }
        let neutral = ThemeColor::from_display_p3([0.5; 3]);
        assert!((neutral.red - 0.5).abs() < 0.001);
        assert!((neutral.green - 0.5).abs() < 0.001);
        assert!((neutral.blue - 0.5).abs() < 0.001);
        let red = ThemeColor::from_display_p3([1., 0., 0.]);
        assert!((red.red - 1.).abs() < 0.000001);
        assert_eq!((red.green, red.blue), (0., 0.));
    }
    #[test]
    fn texture_cycle_keeps_the_untextured_step_before_returning_to_grain() {
        let mut texture = NoiseTexture::Grain;
        for expected in [NoiseTexture::Sand, NoiseTexture::Tweed, NoiseTexture::Denim, NoiseTexture::None, NoiseTexture::Grain] {
            texture = texture.next();
            assert_eq!(texture, expected);
        }
    }
    #[test]
    fn theme_roundtrip_and_legacy_hue_migration() {
        let mut browser = Browser::new();
        let id = browser.space().id;
        let mut theme = SpaceTheme::from_hue(280.);
        theme.colors.push(ThemeColor::from_hsv(100., 0.6, 0.8));
        theme.colors.push(ThemeColor::from_hsv(30., 0.8, 0.9));
        theme.texture = NoiseTexture::Denim;
        theme.appearance = ThemeAppearance::Dark;
        assert!(browser.update_space_theme(id, theme.clone()));
        assert_eq!(
            session::from_json(&session::to_json(&browser))
                .unwrap()
                .space()
                .theme(),
            theme
        );
        let mut json: serde_json::Value =
            serde_json::from_str(&session::to_json(&browser)).unwrap();
        json["spaces"][0].as_object_mut().unwrap().remove("theme");
        let restored = session::from_json(&json.to_string()).unwrap();
        assert!(restored.space().theme.is_none());
        assert_eq!(
            restored.space().theme(),
            SpaceTheme::from_hue(restored.space().hue)
        );
    }
    #[test]
    fn theme_undo_preserves_unrelated_space_and_tab_changes() {
        let mut b = Browser::new();
        let id = b.space().id;
        let theme = SpaceTheme::from_hue(300.);
        assert!(b.update_space_theme(id, theme.clone()));
        let tab = b.new_tab("https://example.com", SystemTime::UNIX_EPOCH);
        b.rename_space(id, "Renamed");
        assert!(b.undo_sidebar(SystemTime::UNIX_EPOCH));
        assert!(b.space().theme.is_none());
        assert_eq!(b.space().name, "Renamed");
        assert_eq!(b.active_tab().unwrap().id, tab);
        assert!(b.redo_sidebar(SystemTime::UNIX_EPOCH));
        assert_eq!(b.space().theme(), theme);
    }
    #[test]
    fn malformed_theme_is_rejected_without_history_or_session_corruption() {
        let mut b = Browser::new();
        let id = b.space().id;
        for colors in [vec![], vec![ThemeColor::from_hsv(10., 0.5, 1.); 4]] {
            let mut theme = SpaceTheme::from_hue(10.);
            theme.colors = colors;
            assert!(!b.update_space_theme(id, theme));
        }
        let mut theme = SpaceTheme::from_hue(10.);
        theme.intensity = f32::NAN;
        assert!(!b.update_space_theme(id, theme));
        assert!(!b.undo_sidebar(SystemTime::UNIX_EPOCH));
        let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&b)).unwrap();
        let mut theme = SpaceTheme::from_hue(10.);
        theme.noise = 2.;
        json["spaces"][0]["theme"] = serde_json::to_value(theme).unwrap();
        assert!(session::from_json(&json.to_string()).is_err());
    }
}
