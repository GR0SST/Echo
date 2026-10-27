//! Space backgrounds use one continuous field across sidebar and frame. The
//! procedural overlays are original textures; no reference-app assets ship.
use crate::window::BrowserWindow;
#[path = "space_theme_default.rs"]
mod default_field;
use browser_core::themes::{NoiseTexture, SpaceTheme, ThemeAppearance, ThemeColor};
use gpui::{AnyElement, Bounds, RenderImage, canvas, prelude::*, px};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Surface {
    pub theme: SpaceTheme,
    pub dark: bool,
    pub picked: bool,
    pub default_style: bool,
}
impl Surface {
    pub(crate) fn sample(&self, x: f32, y: f32) -> ThemeColor {
        if self.default_style {
            return default_field::sample(self.dark, x, y);
        }
        let colors = &self.theme.colors;
        let t = (y * 0.82 + x * 0.18).clamp(0., 1.);
        let position = t * (colors.len() - 1) as f32;
        let a = position.floor() as usize;
        let b = position.ceil() as usize;
        let color = mix(colors[a], colors[b], position.fract());
        if self.picked {
            if self.dark {
                // Dark appearance keeps hue and saturation; intensity only
                // sets the tint opacity. Light chrome never appears in Dark.
                let (h, s, v) = color.hsv();
                // Saturated pigments stay vivid as in the reference; only
                // near-neutral picks (white, grays) fall to charcoal.
                return ThemeColor::from_hsv(h, s, 0.06 + v * (0.18 + 0.72 * s.min(1.)));
            }
            // Light appearance fades the pigment toward a pale neutral.
            return mix(
                ThemeColor {
                    red: 0.84,
                    green: 0.84,
                    blue: 0.84,
                },
                color,
                0.12 + self.theme.intensity.powf(1.4) * 0.75,
            );
        }
        // Keep chroma in the pigment, with intensity controlling its mix into
        // the appearance's neutral ground, independently of texture strength.
        let (h, s, v) = color.hsv();
        let color = ThemeColor::from_hsv(
            h,
            if self.dark { s } else { s * 0.22 },
            if self.dark { v * 0.34 } else { v * 0.65 + 0.3 },
        );
        let neutral = if self.dark { 0.13 } else { 0.91 };
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
        NoiseTexture::None => 0.,
        NoiseTexture::Grain => hash(x, y),
        NoiseTexture::Sand => {
            hash(x, y) * 0.75 + hash((x + 99) % 100, y) * 0.125 + hash(x, (y + 99) % 100) * 0.125
        }
        NoiseTexture::Tweed => {
            let angle = std::f32::consts::TAU;
            (angle * (0.4 * x as f32 - 0.44 * y as f32)).cos() * 0.7
                + (angle * 0.4 * x as f32).cos() * 0.32
                + hash(x, y) * 0.9
        }
        NoiseTexture::Denim => {
            let angle = std::f32::consts::TAU;
            (angle * 0.375 * x as f32).cos() * (angle * 0.25 * y as f32).cos() * 0.12
                + (hash(x, y) * 0.85 + hash(x, (y + 99) % 100) * 0.15) * 0.85
        }
    }
}

// Grain tiles repeat at 100 logical pixels in both axes.
const NOISE_TILE: u32 = 100;

const GRAIN_OPACITY: f32 = 0.25;
/// Light and dark speckles scaled by the noise dial, after Zen Browser's
/// grain layer but subtler (about 5% mean opacity). The pattern is original.
fn speckle(texture: NoiseTexture, x: u32, y: u32, strength: f32) -> (ThemeColor, f32) {
    if texture == NoiseTexture::None {
        return (ThemeColor { red: 0., green: 0., blue: 0. }, 0.);
    }
    let gray = (0.5 + noise(texture, x, y) * 0.7).clamp(0., 1.);
    let alpha = (0.05 + hash(x.wrapping_add(7), y.wrapping_add(3)) * 0.05).clamp(0., GRAIN_OPACITY);
    (ThemeColor { red: gray, green: gray, blue: gray }, alpha * strength.clamp(0., 1.))
}
fn quantize(value: f32, round: bool) -> u8 {
    let value = value.clamp(0., 1.) * 255.;
    (if round { value.round() } else { value }) as u8
}
fn gradient_alpha(key: &PaintKey) -> u8 {
    let alpha = |s: &Surface| {
        if !key.translucent || s.default_style {
            255.
        } else {
            picked_alpha(s.theme.intensity, s.dark) * 255.
        }
    };
    (alpha(&key.from) * (1. - key.fraction) + alpha(&key.to) * key.fraction).round() as u8
}
/// Tint opacity of a picked theme over the desktop blur. The lowest
/// intensity leaves the window almost see-through; the highest stays
/// slightly translucent, as in the reference.
fn picked_alpha(intensity: f32, _dark: bool) -> f32 {
    // Window tint opacity spans 0.30..0.80 over the HUD blur, the range
    // Zen Browser (MPL-2.0) uses for the same Arc-style control on macOS.
    0.3 + intensity.clamp(0., 1.) * 0.5
}
/// Over the blur, low opacities lift the pigment toward white so the tint
/// stays visible; full opacity keeps the pure pigment (Zen's macOS blend).
fn veil_pigment(color: ThemeColor, alpha: f32) -> ThemeColor {
    let kept = (alpha + 0.3 + 0.6 * (1. - (alpha + 0.3))).min(1.);
    mix(ThemeColor { red: 1., green: 1., blue: 1. }, color, kept)
}
fn default_grain(surface: &Surface, x: u32, y: u32) -> (ThemeColor, f32) {
    let reference = surface.sample(0.1, 0.5);
    let luminance = reference.red * 0.2126 + reference.green * 0.7152 + reference.blue * 0.0722;
    let value = hash(x, y);
    let amplitude = if surface.dark { 1.50 } else { 1.80 } / 255.;
    if value >= 0. {
        (
            ThemeColor {
                red: 1.,
                green: 1.,
                blue: 1.,
            },
            value * amplitude / (1. - luminance).max(0.01),
        )
    } else {
        let color = if surface.dark {
            ThemeColor {
                red: (reference.red - luminance).max(0.),
                green: (reference.green - luminance).max(0.),
                blue: (reference.blue - luminance).max(0.),
            }
        } else {
            ThemeColor {
                red: 0.,
                green: 0.,
                blue: 0.,
            }
        };
        (color, -value * amplitude / luminance.max(0.01))
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
    fn images(&self, mut key: PaintKey) -> (Arc<RenderImage>, Arc<RenderImage>) {
        // The default is a painted field in the reference: desktop crops stay
        // identical over controlled red/green windows in both appearances.
        if key.from.default_style && key.to.default_style {
            key.translucent = false;
        }
        let mut cache = self.0.borrow_mut();
        if let Some(data) = cache.as_ref().filter(|data| data.key == key) {
            return (data.gradient.clone(), data.texture.clone());
        }
        let gradient = image::RgbaImage::from_fn(64, 64, |x, y| {
            let sample = |surface: &Surface| {
                let c = surface.sample(x as f32 / 63., y as f32 / 63.);
                if key.translucent && !surface.default_style {
                    veil_pigment(c, picked_alpha(surface.theme.intensity, surface.dark))
                } else {
                    c
                }
            };
            let c = mix(sample(&key.from), sample(&key.to), key.fraction);
            image::Rgba([
                quantize(c.blue, key.from.default_style || key.to.default_style),
                quantize(c.green, key.from.default_style || key.to.default_style),
                quantize(c.red, key.from.default_style || key.to.default_style),
                gradient_alpha(&key),
            ])
        });
        let texture = image::RgbaImage::from_fn(NOISE_TILE, NOISE_TILE, |x, y| {
            let overlay = |surface: &Surface| {
                if surface.default_style {
                    return default_grain(surface, x, y);
                }
                speckle(surface.theme.texture, x, y, surface.theme.noise)
            };
            let (from, a) = overlay(&key.from);
            let (to, b) = overlay(&key.to);
            let (a, b) = (a * (1. - key.fraction), b * key.fraction);
            let alpha = a + b;
            let color = if alpha > 0. {
                mix(from, to, b / alpha)
            } else {
                from
            };
            image::Rgba([
                quantize(color.blue, true),
                quantize(color.green, true),
                quantize(color.red, true),
                (alpha / GRAIN_OPACITY * 255.).round().clamp(0., 255.) as u8,
            ])
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
/// One image for a rounded panel: field and grain composed at the panel's
/// own size, so a single rounded paint clips both (GPUI clips rectangles).
fn panel_image(key: &PaintKey, width: u32, height: u32) -> Arc<RenderImage> {
    thread_local! {
        static CACHE: RefCell<Option<(PaintKey, u32, u32, Arc<RenderImage>)>> = const { RefCell::new(None) };
    }
    if let Some(image) = CACHE.with(|cache| cache.borrow().as_ref()
        .filter(|(k, w, h, _)| k == key && *w == width && *h == height).map(|entry| entry.3.clone())) {
        return image;
    }
    let (width, height) = (width.max(2), height.max(2));
    let fill_alpha = gradient_alpha(key) as f32 / 255.;
    let image = image::RgbaImage::from_fn(width, height, |x, y| {
        let (u, v) = (x as f32 / (width - 1) as f32, y as f32 / (height - 1) as f32);
        let sample = |surface: &Surface| {
            let c = surface.sample(u, v);
            if key.translucent && !surface.default_style {
                veil_pigment(c, picked_alpha(surface.theme.intensity, surface.dark))
            } else {
                c
            }
        };
        let base = mix(sample(&key.from), sample(&key.to), key.fraction);
        let (tx, ty) = (x % NOISE_TILE, y % NOISE_TILE);
        let grain = |surface: &Surface| if surface.default_style {
            default_grain(surface, tx, ty)
        } else {
            speckle(surface.theme.texture, tx, ty, surface.theme.noise)
        };
        let ((from, a), (to, b)) = (grain(&key.from), grain(&key.to));
        let (a, b) = (a * (1. - key.fraction), b * key.fraction);
        let grain_alpha = (a + b).min(1.);
        let grain_color = if a + b > 0. { mix(from, to, b / (a + b)) } else { from };
        let alpha = grain_alpha + fill_alpha * (1. - grain_alpha);
        let color = if alpha > 0. {
            ThemeColor {
                red: (grain_color.red * grain_alpha + base.red * fill_alpha * (1. - grain_alpha)) / alpha,
                green: (grain_color.green * grain_alpha + base.green * fill_alpha * (1. - grain_alpha)) / alpha,
                blue: (grain_color.blue * grain_alpha + base.blue * fill_alpha * (1. - grain_alpha)) / alpha,
            }
        } else {
            base
        };
        image::Rgba([quantize(color.blue, true), quantize(color.green, true), quantize(color.red, true), quantize(alpha, true)])
    });
    let image = Arc::new(RenderImage::new(vec![image::Frame::new(image)]));
    CACHE.with(|cache| *cache.borrow_mut() = Some((key.clone(), width, height, image.clone())));
    image
}
/// The profile sheet previews each Space's actual pigments and texture.
/// It uses the sidebar's field and noise functions rather than a hue swatch.
#[derive(Clone, Default)]
pub(crate) struct ProfileCardPreview(Rc<RefCell<Option<(Surface, u32, u32, Arc<RenderImage>)>>>);
impl ProfileCardPreview {
    pub(crate) fn image(&self, surface: Surface) -> Arc<RenderImage> {
        self.image_sized(surface, 64, 96)
    }
    pub(crate) fn image_sized(&self, surface: Surface, width: u32, height: u32) -> Arc<RenderImage> {
        let (width, height) = (width.max(2), height.max(2));
        if let Some((_, _, _, image)) = self.0.borrow().as_ref().filter(|(key, w, h, _)| key == &surface && *w == width && *h == height) {
            return image.clone();
        }
        let image = image::RgbaImage::from_fn(width, height, |x, y| {
            let base = surface.sample(x as f32 / (width - 1) as f32, y as f32 / (height - 1) as f32);
            let (tint, alpha) = if surface.default_style {
                default_grain(&surface, x, y)
            } else {
                speckle(surface.theme.texture, x % NOISE_TILE, y % NOISE_TILE, surface.theme.noise)
            };
            let color = mix(base, tint, alpha);
            image::Rgba([
                quantize(color.blue, true),
                quantize(color.green, true),
                quantize(color.red, true),
                255,
            ])
        });
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(image)]));
        *self.0.borrow_mut() = Some((surface, width, height, image.clone()));
        image
    }
}

const SHADOW_MARGIN: u32 = 160;
#[derive(Clone, Default)]
pub(crate) struct ThemeShadowCache(Rc<RefCell<Option<(u32, u32, Arc<RenderImage>)>>>);
fn rounded_distance(width: f32, height: f32, x: f32, y: f32) -> f32 {
    let dx = (x - width * 0.5).abs() - (width * 0.5 - 12.);
    let dy = (y - height * 0.5).abs() - (height * 0.5 - 12.);
    dx.max(0.).hypot(dy.max(0.)) + dx.max(dy).min(0.) - 12.
}
fn command_shadow_alpha(width: f32, height: f32, x: f32, y: f32) -> f32 {
    if rounded_distance(width, height, x, y) <= 0. {
        return 0.;
    }
    let distance = rounded_distance(width, height, x, y - 22.);
    // Logistic approximation of the measured Gaussian edge response. Keeping
    // the center transparent avoids GPUI shadow culling and interior overdraw.
    0.54 / (1. + (1.702 * distance / 51.).exp())
}
impl ThemeShadowCache {
    fn image(&self, width: u32, height: u32) -> Arc<RenderImage> {
        let mut cache = self.0.borrow_mut();
        if let Some((w, h, image)) = cache
            .as_ref()
            .filter(|(w, h, _)| *w == width && *h == height)
        {
            let _ = (w, h);
            return image.clone();
        }
        let image = image::RgbaImage::from_fn(
            width + SHADOW_MARGIN * 2,
            height + SHADOW_MARGIN * 2,
            |x, y| {
                let alpha = command_shadow_alpha(
                    width as f32,
                    height as f32,
                    x as f32 + 0.5 - SHADOW_MARGIN as f32,
                    y as f32 + 0.5 - SHADOW_MARGIN as f32,
                );
                image::Rgba([0, 0, 0, (alpha * 255.).round() as u8])
            },
        );
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(image)]));
        *cache = Some((width, height, image.clone()));
        image
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

/// Device-pixel strips approximate the rounded boundary without repainting or
/// stretching the theme field. All coordinates stay relative to the window.
pub(crate) fn rounded_theme_mask(
    bounds: Bounds<gpui::Pixels>,
    radius: f32,
    scale: f32,
) -> Vec<Bounds<gpui::Pixels>> {
    let r = radius
        .min(f32::from(bounds.size.width) / 2.)
        .min(f32::from(bounds.size.height) / 2.)
        .max(0.);
    if r == 0. {
        return vec![bounds];
    }
    let count = (r * scale.max(1.)).ceil() as usize;
    let step = r / count as f32;
    let mut mask = vec![Bounds::new(
        bounds.origin + gpui::point(px(0.), px(r)),
        gpui::size(bounds.size.width, bounds.size.height - px(r * 2.)),
    )];
    for row in 0..count {
        let y = row as f32 * step;
        let dy = r - y - step / 2.;
        let inset = r - (r * r - dy * dy).max(0.).sqrt();
        let width = bounds.size.width - px(inset * 2.);
        mask.push(Bounds::new(
            bounds.origin + gpui::point(px(inset), px(y)),
            gpui::size(width, px(step)),
        ));
        mask.push(Bounds::new(
            gpui::point(bounds.left() + px(inset), bounds.bottom() - px(y + step)),
            gpui::size(width, px(step)),
        ));
    }
    mask
}

impl BrowserWindow {
    /// Arc keeps an empty document panel over the continuous Space field.
    pub(crate) fn theme_empty_panel_fill(&self) -> gpui::Hsla {
        // The masked field is opaque; compensate for the native window
        // material already included in Arc's underlying translucent field.
        let default = self.theme_default_fraction();
        // Over a blurred window the panel thins with the theme tint, so a
        // translucent Space stays see-through across the page area too.
        let tint = if self.theme_background_blurred == Some(true) {
            gradient_alpha(&self.theme_paint_key(true)) as f32 / 255.
        } else {
            1.
        };
        if self.theme_is_dark() {
            let alpha = 0.21 * (1. - default) + 0.10 * default;
            let luminance = (0.5 * 0.21 * (1. - default) + 0.10 * default) / alpha;
            // Over the blur the panel stays a little lighter than the field.
            // Over the blur the reference panel is a faint white veil.
            if tint < 1. { gpui::hsla(0., 0., 1., 0.07) } else { gpui::hsla(0., 0., luminance, alpha) }
        } else {
            let alpha = 0.70 * (1. - default) + (2. / 3.) * default;
            gpui::hsla(0., 0., 1., if tint < 1. { 0.12 + alpha * tint } else { alpha })
        }
    }
    /// Keep the shadow outside the translucent panel. Nested outlines leave
    /// its interior untouched, including the six-pixel corner cutouts.
    pub(crate) fn theme_empty_panel_shadow(&self) -> AnyElement {
        self.theme_surface_shadow(crate::pages::CONTENT_CORNER_RADIUS)
    }
    pub(crate) fn theme_surface_shadow(&self, radius: f32) -> AnyElement {
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                for distance in (1..=8).rev() {
                    let spread = distance as f32;
                    let alpha = 0.05 * (-0.5 * ((spread - 0.5) / 1.5).powi(2)).exp();
                    window.paint_quad(
                        gpui::outline(
                            bounds.dilate(px(spread)),
                            gpui::hsla(0., 0., 0., alpha),
                            Default::default(),
                        )
                        .corner_radii(px(radius + spread)),
                    );
                }
            },
        )
        .size_full()
        .into_any_element()
    }
    pub(crate) fn command_bar_shadow(&self) -> AnyElement {
        let cache = self
            .command_bar
            .as_ref()
            .expect("command bar owns its shadow")
            .shadow_cache
            .clone();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let image = cache.image(
                    f32::from(bounds.size.width).round() as u32,
                    f32::from(bounds.size.height).round() as u32,
                );
                let _ = window.paint_image(
                    bounds.dilate(px(SHADOW_MARGIN as f32)),
                    Default::default(),
                    image,
                    0,
                    false,
                );
            },
        )
        .size_full()
        .into_any_element()
    }
    pub(crate) fn theme_tab_selection_for(&self, space: browser_core::SpaceId) -> gpui::Hsla {
        gpui::hsla(
            0.,
            0.,
            1.,
            if self.theme_uses_light_ink_for(space) {
                0.16
            } else {
                0.80
            },
        )
    }
    pub(crate) fn theme_space_dot_fill(&self, active: bool) -> gpui::Hsla {
        let ink = if self.theme_uses_light_ink_for(self.browser.space().id) { 1. } else { 0. };
        gpui::hsla(0., 0., ink, if active { 0.55 } else { 0.25 })
    }
    pub(crate) fn theme_address_fill(&self) -> Option<gpui::Hsla> {
        self.theme_surface_for(self.browser.space()).default_style.then(||
            gpui::hsla(0., 0., if self.theme_is_dark() { 1. } else { 0. },
                if self.theme_is_dark() { 0.10 } else { 0.04 }))
    }
    /// Solid page-result selection, measured separately in each appearance.
    pub(crate) fn command_bar_page_selection(&self) -> gpui::Hsla {
        gpui::rgb(if self.theme_is_dark() { 0x464ddc } else { 0x8f95ff }).into()
    }
    pub(crate) fn command_bar_selection(&self) -> gpui::Hsla {
        let surface = self.theme_surface_for(self.browser.space());
        if !surface.picked {
            return gpui::hsla(0., 0., if self.theme_is_dark() { 1. } else { 0. }, 0.10);
        }
        let color = surface.theme.colors[0];
        let lift = |channel: f32| channel * 0.75 + 0.25;
        let mut fill: gpui::Hsla = gpui::Rgba {
            r: lift(color.red),
            g: lift(color.green),
            b: lift(color.blue),
            a: if self.theme_is_dark() { 0.65 } else { 0.30 },
        }
        .into();
        if !self.theme_is_dark() {
            fill.s = (fill.s * 1.6).min(1.);
            fill.l = (fill.l - 0.015).max(0.);
        }
        fill
    }
    pub(crate) fn command_bar_panel_fill(&self, blurred: bool) -> gpui::Hsla {
        let dark = self.theme_is_dark();
        gpui::hsla(
            self.accent(),
            if dark { 0.08 } else { 0. },
            if dark {
                if blurred { 0.055 } else { 0.16 }
            } else {
                1.
            },
            if blurred { 0.61 } else { 1. },
        )
    }
    pub(crate) fn theme_colors(&self) -> ChromeColors {
        let dark = self.theme_uses_light_ink_for(self.browser.space().id);
        let ink = if dark { 1. } else { 0. };
        let foreground = self.theme_foreground_for(self.browser.space().id);
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
        self.theme_is_dark_for(self.browser.space().id)
    }
    pub(crate) fn theme_appearance(&self) -> ThemeAppearance {
        if self.browser.is_private() {
            ThemeAppearance::Dark
        } else {
            self.browser.settings.theme_appearance
        }
    }
    pub(crate) fn theme_is_dark_for(&self, _space: browser_core::SpaceId) -> bool {
        match self.theme_appearance() {
            ThemeAppearance::Light => false,
            ThemeAppearance::Dark => true,
            ThemeAppearance::Automatic => self.theme_system_dark,
        }
    }
    pub(crate) fn theme_foreground_for(&self, space: browser_core::SpaceId) -> gpui::Hsla {
        let dark = self.theme_uses_light_ink_for(space);
        let hue = self
            .browser
            .spaces
            .iter()
            .find(|s| s.id == space)
            .map_or(self.accent(), |s| s.accent_hue() / 360.);
        gpui::hsla(
            hue,
            if dark { 0.12 } else { 0. },
            if dark { 0.92 } else { 0.1 },
            1.,
        )
    }
    pub(crate) fn theme_surface_for(&self, space: &browser_core::Space) -> Surface {
        let mut theme = self
            .theme_editor
            .as_ref()
            .filter(|e| e.space == space.id)
            .map_or_else(|| space.theme(), |e| e.draft.clone());
        let picked = space.theme.is_some()
            || self
                .theme_editor
                .as_ref()
                .is_some_and(|e| e.space == space.id && e.picked);
        if space.theme.is_none()
            && !self
                .theme_editor
                .as_ref()
                .is_some_and(|e| e.space == space.id && e.picked)
        {
            theme.intensity = 0.2;
            theme.noise = 0.12;
        }
        let dark = self.theme_is_dark_for(space.id);
        Surface {
            theme,
            dark,
            picked,
            default_style: !picked,
        }
    }
    pub(crate) fn theme_uses_light_ink_for(&self, space: browser_core::SpaceId) -> bool {
        let Some(target) = self.browser.spaces.iter().find(|s| s.id == space) else {
            return self.theme_is_dark_for(space);
        };
        let sample = self.theme_surface_for(target).sample(0.25, 0.5);
        sample.red * 0.2126 + sample.green * 0.7152 + sample.blue * 0.0722 < 0.5
    }
    fn theme_paint_key(&self, translucent: bool) -> PaintKey {
        use crate::sidebar_pages::SidebarPage;
        let surface = |space: &browser_core::Space| self.theme_surface_for(space);
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
                            picked: false,
                            default_style: false,
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
        PaintKey {
            from,
            to,
            fraction: position.fract(),
            translucent,
        }
    }
    pub(crate) fn theme_default_fraction(&self) -> f32 {
        let key = self.theme_paint_key(false);
        (if key.from.default_style { 1. } else { 0. }) * (1. - key.fraction)
            + (if key.to.default_style { 1. } else { 0. }) * key.fraction
    }
    pub(crate) fn theme_surface(&self, translucent: bool) -> AnyElement {
        self.theme_surface_with_origin(translucent, false)
    }

    /// A pane paints the same window field through its existing rounded clip.
    /// Gradient scaling and the grain tile origin remain fixed while it resizes.
    pub(crate) fn theme_window_field(&self) -> AnyElement {
        gpui::div().debug_selector(|| "theme-window-field".into()).absolute().inset_0().overflow_hidden()
            .child(self.theme_surface_with_origin(false, true)).into_any_element()
    }

    /// The Space field for a rounded floating panel, clipped to `radius`.
    /// Translucent when the window shows a native blur behind it.
    pub(crate) fn theme_surface_rounded(&self, radius: f32) -> AnyElement {
        let key = self.theme_paint_key(self.theme_background_blurred == Some(true));
        canvas(|_, _, _| (), move |bounds, _, window, _| {
            let image = panel_image(&key, f32::from(bounds.size.width).round() as u32, f32::from(bounds.size.height).round() as u32);
            let _ = window.paint_image(bounds, gpui::Corners::all(px(radius)), image, 0, false);
        })
        .absolute()
        .inset_0()
        .into_any_element()
    }

    fn theme_surface_with_origin(&self, translucent: bool, window_aligned: bool) -> AnyElement {
        let (gradient, texture) = self
            .theme_paint_cache
            .images(self.theme_paint_key(translucent));
        let private = self.browser.is_private();
        gpui::div()
            .absolute()
            .size_full()
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let bounds = if window_aligned { Bounds::new(gpui::point(px(0.), px(0.)), window.viewport_size()) } else { bounds };
                        if private {
                            window.paint_quad(gpui::fill(
                                bounds,
                                gpui::hsla(210. / 360., 0.06, 0.05, 1.),
                            ));
                        } else {
                            // Filtering fades the small field image toward
                            // transparency at its border; push that fade one
                            // texel outside the clip so edges stay fully tinted.
                            let clip = bounds;
                            let texel = gpui::size(bounds.size.width / 63., bounds.size.height / 63.);
                            let field = Bounds::new(
                                bounds.origin - gpui::point(texel.width, texel.height),
                                gpui::size(bounds.size.width + texel.width * 2., bounds.size.height + texel.height * 2.),
                            );
                            window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), |window| {
                                let _ = window.paint_image(field, Default::default(), gradient, 0, false);
                            });
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(
                gpui::div().absolute().inset_0().opacity(GRAIN_OPACITY).child(canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        if private {
                            return;
                        }
                        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                            let bounds = if window_aligned { Bounds::new(gpui::point(px(0.), px(0.)), window.viewport_size()) } else { bounds };
                            let cols =
                                (f32::from(bounds.size.width) / NOISE_TILE as f32).ceil() as u32;
                            let rows =
                                (f32::from(bounds.size.height) / NOISE_TILE as f32).ceil() as u32;
                            for row in 0..rows {
                                for col in 0..cols {
                                    let tile = Bounds::new(
                                        bounds.origin
                                            + gpui::point(
                                                px(col as f32 * NOISE_TILE as f32),
                                                px(row as f32 * NOISE_TILE as f32),
                                            ),
                                        gpui::size(px(NOISE_TILE as f32), px(NOISE_TILE as f32)),
                                    );
                                    if !tile.intersects(&window.content_mask().bounds) {
                                        continue;
                                    }
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
                .inset_0()),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_bar_shadow_excludes_the_translucent_rounded_interior() {
        for y in 0..50 {
            for x in 0..100 {
                let (x, y) = (x as f32 + 0.5, y as f32 + 0.5);
                if rounded_distance(100., 50., x, y) <= 0. {
                    assert_eq!(command_shadow_alpha(100., 50., x, y), 0.);
                }
            }
        }
        // The left tail continues smoothly across the panel's bottom; a
        // clipped shadow primitive previously left a horizontal seam here.
        for x in [-80., -40., -10.] {
            let above = command_shadow_alpha(764., 278., x, 277.5);
            let below = command_shadow_alpha(764., 278., x, 278.5);
            assert!(above > 0. && below > 0.);
            assert!((above - below).abs() < 0.01);
        }
        assert!(command_shadow_alpha(764., 278., 382., 279.) > 0.3);
        assert!(command_shadow_alpha(764., 278., 382., 390.) < 0.04);
    }

    #[test]
    fn appearance_intensity_and_third_color_affect_the_shared_background() {
        let mut s = Surface {
            theme: SpaceTheme::from_hue(0.),
            dark: false,
            picked: true,
            default_style: false,
        };
        s.theme.colors.push(ThemeColor::from_hsv(120., 0.8, 1.));
        s.theme.colors.push(ThemeColor::from_hsv(240., 0.8, 1.));
        assert_ne!(s.sample(0., 0.), s.sample(1., 1.));
        let light = s.sample(0., 0.);
        s.dark = true;
        assert_ne!(s.sample(0., 0.), light);
        s.theme.intensity = 0.;
        assert_ne!(s.sample(0., 0.), s.sample(1., 1.));
        s.theme.colors = vec![ThemeColor { red: 0.95, green: 0.95, blue: 0.95 }];
        assert!(s.sample(0., 0.).red < 0.3);
    }
    #[test]
    fn picked_dark_themes_keep_vivid_pigments_and_neutral_picks_stay_dark() {
        let mut s = Surface {
            theme: SpaceTheme::from_hue(280.),
            dark: true,
            picked: true,
            default_style: false,
        };
        s.theme.colors = vec![ThemeColor {
            red: 0.651,
            green: 0.447,
            blue: 0.616,
        }];
        let picked = s.sample(0.5, 0.5);
        assert!(picked.red < 0.4 && picked.red > picked.green);
        // A vivid pick keeps its saturation and most of its brightness.
        s.theme.colors = vec![ThemeColor { red: 0.12, green: 0.06, blue: 0.75 }];
        let vivid = s.sample(0.5, 0.5);
        assert!(vivid.hsv().1 > 0.85 && vivid.blue > 0.6);
        // A near-white pick stays dark chrome in Dark appearance.
        s.theme.colors = vec![ThemeColor { red: 0.95, green: 0.93, blue: 0.94 }];
        assert!(s.sample(0.5, 0.5).red < 0.25);
        s.picked = false;
        s.theme.intensity = 0.2;
        assert!((0.12..0.2).contains(&s.sample(0.5, 0.5).red));
    }
    #[test]
    fn default_field_ignores_legacy_hue_and_blends_with_picked_themes() {
        let mut from = Surface {
            theme: SpaceTheme::from_hue(0.),
            dark: true,
            picked: false,
            default_style: true,
        };
        let dark = from.sample(0.1, 0.5);
        from.theme = SpaceTheme::from_hue(180.);
        assert_eq!(from.sample(0.1, 0.5), dark);
        assert!(dark.blue > dark.red && dark.red < 0.1);
        let mut light = from.clone();
        light.dark = false;
        assert!(light.sample(0.1, 0.5).red > 0.85);
        let mut picked = from.clone();
        picked.default_style = false;
        picked.picked = true;
        let key = PaintKey {
            from: from.clone(),
            to: picked,
            fraction: 0.5,
            translucent: true,
        };
        assert_eq!(gradient_alpha(&key), 217);
        let mut faint = key.to.clone();
        faint.theme.intensity = 0.;
        let mut full = faint.clone();
        full.theme.intensity = 1.;
        let alpha = |s: Surface| gradient_alpha(&PaintKey { from: s.clone(), to: s, fraction: 0., translucent: true });
        // Tint opacity spans 0.30..0.80 in both appearances.
        assert_eq!((alpha(faint.clone()), alpha(full.clone())), (77, 204));
        let mut light = faint.clone();
        light.dark = false;
        assert_eq!(alpha(light), alpha(faint.clone()));
        // Low opacity lifts the pigment toward white; full keeps it pure.
        let pigment = ThemeColor { red: 0.1, green: 0.1, blue: 0.8 };
        assert!(veil_pigment(pigment, 0.3).red > 0.2);
        assert!((veil_pigment(pigment, 0.8).red - pigment.red).abs() < 1e-5);
        let cache = ThemePaintCache::default();
        let opaque = PaintKey {
            from: from.clone(),
            to: from,
            fraction: 0.,
            translucent: false,
        };
        let (a, grain_a) = cache.images(opaque.clone());
        let (b, grain_b) = cache.images(PaintKey {
            translucent: true,
            ..opaque
        });
        assert!(Arc::ptr_eq(&a, &b) && Arc::ptr_eq(&grain_a, &grain_b));
    }
    #[test]
    fn speckle_mixes_light_and_dark_grain_scaled_by_the_dial() {
        let samples: Vec<_> = (0..100)
            .flat_map(|y| (0..100).map(move |x| speckle(NoiseTexture::Grain, x, y, 1.)))
            .collect();
        let mean = samples.iter().map(|(_, a)| a).sum::<f32>() / samples.len() as f32;
        assert!((0.04..0.06).contains(&mean));
        assert!(samples.iter().any(|(c, _)| c.red > 0.75) && samples.iter().any(|(c, _)| c.red < 0.25));
        assert_eq!(speckle(NoiseTexture::Grain, 10, 20, 0.).1, 0.);
        assert_eq!(speckle(NoiseTexture::None, 10, 20, 1.).1, 0.);
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
