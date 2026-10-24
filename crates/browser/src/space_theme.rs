//! Space backgrounds use one continuous field across sidebar and frame. The
//! procedural overlays are original textures; no reference-app assets ship.
use crate::window::BrowserWindow;
use browser_core::themes::{NoiseTexture, SpaceTheme, ThemeAppearance, ThemeColor};
use gpui::{AnyElement, Bounds, RenderImage, canvas, prelude::*, px};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Surface {
    pub theme: SpaceTheme,
    pub dark: bool,
}
impl Surface {
    pub(crate) fn sample(&self, x: f32, y: f32) -> ThemeColor {
        let colors = &self.theme.colors;
        let t = (y * 0.82 + x * 0.18).clamp(0., 1.);
        let position = t * (colors.len() - 1) as f32;
        let a = position.floor() as usize;
        let b = position.ceil() as usize;
        let color = mix(colors[a], colors[b], position.fract());
        // Keep chroma in the pigment, with intensity controlling its mix into
        // the appearance's neutral ground, independently of texture strength.
        let (h, s, v) = color.hsv();
        let color = ThemeColor::from_hsv(
            h,
            if self.dark { s } else { s * 0.22 },
            if self.dark { v * 0.34 } else { v * 0.65 + 0.3 },
        );
        let neutral = if self.dark { 0.035 } else { 0.91 };
        mix(
            ThemeColor {
                red: neutral,
                green: neutral,
                blue: neutral,
            },
            color,
            self.theme.intensity,
        )
    }
}
fn mix(a: ThemeColor, b: ThemeColor, t: f32) -> ThemeColor {
    ThemeColor {
        red: a.red + (b.red - a.red) * t,
        green: a.green + (b.green - a.green) * t,
        blue: a.blue + (b.blue - a.blue) * t,
    }
}
fn hash(x: u32, y: u32) -> f32 {
    let mut v = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263));
    v = (v ^ (v >> 13)).wrapping_mul(1274126177);
    ((v ^ (v >> 16)) & 65535) as f32 / 65535. - 0.5
}
fn noise(texture: NoiseTexture, x: u32, y: u32) -> f32 {
    match texture {
        NoiseTexture::Grain => hash(x, y),
        NoiseTexture::Sand => hash(x / 2, y / 2) * 0.65 + hash(x, y) * 0.35,
        NoiseTexture::Tweed => {
            hash(x / 3, y / 3) * 0.5 + hash(x % 4, y) * 0.25 + hash(x, y % 4) * 0.25
        }
        NoiseTexture::Denim => hash(x / 5, y) * 0.4 + hash(x, y / 5) * 0.4 + hash(x, y) * 0.2,
    }
}

#[derive(Clone, PartialEq)]
struct PaintKey {
    from: Surface,
    to: Surface,
    fraction: f32,
    translucent: bool,
}
struct PaintData {
    key: PaintKey,
    gradient: Arc<RenderImage>,
    texture: Arc<RenderImage>,
}
#[derive(Clone, Default)]
pub(crate) struct ThemePaintCache(Rc<RefCell<Option<PaintData>>>);
impl ThemePaintCache {
    fn images(&self, key: PaintKey) -> (Arc<RenderImage>, Arc<RenderImage>) {
        let mut cache = self.0.borrow_mut();
        if let Some(data) = cache.as_ref().filter(|data| data.key == key) {
            return (data.gradient.clone(), data.texture.clone());
        }
        let gradient = image::RgbaImage::from_fn(64, 64, |x, y| {
            let c = mix(
                key.from.sample(x as f32 / 63., y as f32 / 63.),
                key.to.sample(x as f32 / 63., y as f32 / 63.),
                key.fraction,
            );
            image::Rgba([
                (c.blue * 255.) as u8,
                (c.green * 255.) as u8,
                (c.red * 255.) as u8,
                if key.translucent { 224 } else { 255 },
            ])
        });
        let texture = image::RgbaImage::from_fn(128, 128, |x, y| {
            let amount = |s: &Surface| noise(s.theme.texture, x, y) * s.theme.noise;
            let n = amount(&key.from) * (1. - key.fraction) + amount(&key.to) * key.fraction;
            let value = if n < 0. { 0 } else { 255 };
            image::Rgba([value, value, value, (n.abs() * 60.) as u8])
        });
        let gradient = Arc::new(RenderImage::new(vec![image::Frame::new(gradient)]));
        let texture = Arc::new(RenderImage::new(vec![image::Frame::new(texture)]));
        *cache = Some(PaintData {
            key,
            gradient: gradient.clone(),
            texture: texture.clone(),
        });
        (gradient, texture)
    }
}
/// Shared chrome semantics for hosted Library/Media/Downloads surfaces.
/// Surface fills stay with their owners; these values never alter layout.
#[derive(Clone, Copy)]
pub(crate) struct ChromeColors {
    pub foreground: gpui::Hsla,
    pub secondary: gpui::Hsla,
    pub muted: gpui::Hsla,
    pub control: gpui::Hsla,
    pub hover: gpui::Hsla,
    pub selected: gpui::Hsla,
    pub border: gpui::Hsla,
}

impl BrowserWindow {
    pub(crate) fn theme_colors(&self) -> ChromeColors {
        let dark = self.theme_is_dark();
        let ink = if dark { 1. } else { 0. };
        let foreground = gpui::hsla(
            self.accent(),
            if dark { 0.12 } else { 0. },
            if dark { 0.92 } else { 0.1 },
            1.,
        );
        ChromeColors {
            foreground,
            secondary: foreground.opacity(0.65),
            muted: if dark {
                foreground.opacity(0.55)
            } else {
                gpui::hsla(0., 0., 0.4, 1.)
            },
            control: gpui::hsla(0., 0., ink, 0.08),
            hover: gpui::hsla(0., 0., ink, if dark { 0.10 } else { 0.06 }),
            selected: gpui::hsla(0., 0., ink, if dark { 0.16 } else { 0.08 }),
            border: gpui::hsla(0., 0., ink, 0.12),
        }
    }
    pub(crate) fn theme_is_dark(&self) -> bool {
        self.browser.is_private()
            || match self
                .theme_editor
                .as_ref()
                .filter(|e| e.space == self.browser.space().id)
                .map_or_else(|| self.browser.space().theme(), |e| e.draft.clone())
                .appearance
            {
                ThemeAppearance::Light => false,
                ThemeAppearance::Dark => true,
                ThemeAppearance::Automatic => self.theme_system_dark,
            }
    }
    pub(crate) fn theme_surface(&self, translucent: bool) -> AnyElement {
        use crate::sidebar_pages::SidebarPage;
        let surface = |space: &browser_core::Space| {
            let mut theme = self
                .theme_editor
                .as_ref()
                .filter(|e| e.space == space.id)
                .map_or_else(|| space.theme(), |e| e.draft.clone());
            if space.theme.is_none()
                && !self
                    .theme_editor
                    .as_ref()
                    .is_some_and(|e| e.space == space.id && e.picked)
            {
                theme.intensity = 0.2;
                theme.noise = 0.12;
            }
            let dark = match theme.appearance {
                ThemeAppearance::Automatic => self.theme_system_dark,
                ThemeAppearance::Light => false,
                ThemeAppearance::Dark => true,
            };
            Surface { theme, dark }
        };
        let fallback = surface(self.browser.space());
        let surfaces: Vec<_> = if self.browser.is_blank() || self.browser.is_private() {
            self.browser.spaces.iter().map(surface).collect()
        } else {
            self.sidebar_navigation
                .controller
                .pages(&self.browser)
                .into_iter()
                .map(|page| match page {
                    SidebarPage::Space(id) => self
                        .browser
                        .spaces
                        .iter()
                        .find(|s| s.id == id)
                        .map(&surface)
                        .unwrap_or_else(|| fallback.clone()),
                    SidebarPage::CreateSpace => self
                        .sidebar_navigation
                        .controller
                        .draft()
                        .map(|draft| Surface {
                            theme: SpaceTheme::from_hue(draft.hue),
                            dark: false,
                        })
                        .unwrap_or_else(|| fallback.clone()),
                    SidebarPage::Library => fallback.clone(),
                })
                .collect()
        };
        let position = self
            .pager
            .position
            .clamp(0., surfaces.len().saturating_sub(1) as f32);
        let from = surfaces
            .get(position.floor() as usize)
            .unwrap_or(&fallback)
            .clone();
        let to = surfaces
            .get(position.ceil() as usize)
            .unwrap_or(&fallback)
            .clone();
        let (gradient, texture) = self.theme_paint_cache.images(PaintKey {
            from,
            to,
            fraction: position.fract(),
            translucent,
        });
        let private = self.browser.is_private();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                if private {
                    window.paint_quad(gpui::fill(bounds, gpui::hsla(210. / 360., 0.06, 0.05, 1.)));
                    return;
                }
                let _ = window.paint_image(bounds, Default::default(), gradient, 0, false);
                window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                    let cols = (f32::from(bounds.size.width) / 128.).ceil() as u32;
                    let rows = (f32::from(bounds.size.height) / 128.).ceil() as u32;
                    for row in 0..rows {
                        for col in 0..cols {
                            let tile = Bounds::new(
                                bounds.origin
                                    + gpui::point(px(col as f32 * 128.), px(row as f32 * 128.)),
                                gpui::size(px(128.), px(128.)),
                            );
                            let _ = window.paint_image(
                                tile,
                                Default::default(),
                                texture.clone(),
                                0,
                                false,
                            );
                        }
                    }
                });
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appearance_intensity_and_third_color_affect_the_shared_background() {
        let mut s = Surface {
            theme: SpaceTheme::from_hue(0.),
            dark: false,
        };
        s.theme.colors.push(ThemeColor::from_hsv(120., 0.8, 1.));
        s.theme.colors.push(ThemeColor::from_hsv(240., 0.8, 1.));
        assert_ne!(s.sample(0., 0.), s.sample(1., 1.));
        let light = s.sample(0., 0.);
        s.dark = true;
        assert!(s.sample(0., 0.).red < light.red);
        s.theme.intensity = 0.;
        assert_eq!(s.sample(0., 0.), s.sample(1., 1.));
    }
    #[test]
    fn textures_are_deterministic_and_distinct() {
        let values: Vec<_> = [
            NoiseTexture::Grain,
            NoiseTexture::Sand,
            NoiseTexture::Tweed,
            NoiseTexture::Denim,
        ]
        .map(|t| (0..64).map(|i| noise(t, i, i * 3)).collect::<Vec<_>>())
        .into();
        for a in 0..4 {
            for b in a + 1..4 {
                assert_ne!(values[a], values[b]);
            }
        }
        assert_eq!(
            noise(NoiseTexture::Grain, 7, 9),
            noise(NoiseTexture::Grain, 7, 9)
        );
    }
}
