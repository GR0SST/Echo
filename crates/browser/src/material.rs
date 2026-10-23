//! Browser chrome materials. No persistence, profile policy, or native handles.
//!
//! The host supplies a resolved palette; alpha belongs to the surface fill,
//! never to a container containing text, icons, or a native web page.
use gpui::{Hsla, Rgba, WindowAppearance, WindowBackgroundAppearance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Appearance {
    Light,
    Dark,
}

impl From<WindowAppearance> for Appearance {
    fn from(value: WindowAppearance) -> Self {
        match value {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
        }
    }
}

/// Actual backing at this surface, not a compile-time OS guess. Unknown,
/// failed, transparent-only, and unsupported backends use Opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Backdrop {
    Opaque,
    Blurred,
}

impl Backdrop {
    /// Whole-window request only. Sparse overlay windows must remain transparent
    /// and use bounded native materials, otherwise empty overlay areas blur too.
    pub(crate) fn window_background(self) -> WindowBackgroundAppearance {
        match self {
            Self::Opaque => WindowBackgroundAppearance::Opaque,
            Self::Blurred => WindowBackgroundAppearance::Blurred,
        }
    }
}

/// Bounded native effect parameters, independent of foreground/tint colors.
/// No automatic appearance-to-material mapping is implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EffectAppearance {
    Inherited,
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EffectState {
    FollowWindow,
    Active,
    Inactive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EffectBlending {
    BehindWindow,
    WithinWindow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EffectMaterial {
    Hud,
    Menu,
    WindowBackground,
    /// Explicit reference-only looks; deprecated by AppKit. Not theme modes.
    ReferenceLight,
    ReferenceDark,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BackdropConfig {
    pub appearance: EffectAppearance,
    pub state: EffectState,
    pub blending: EffectBlending,
    pub emphasized: bool,
    /// Alpha of this effect view alone, never a container or website alpha.
    pub effect_alpha: f64,
    pub material: EffectMaterial,
}

impl BackdropConfig {
    pub(crate) fn is_valid(self) -> bool {
        self.effect_alpha.is_finite() && (0. ..=1.).contains(&self.effect_alpha)
    }
}

/// Installation says nothing about the compositor's actual blur output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackdropInstallation {
    Installed,
    Removed,
    Absent,
    Unavailable,
    InvalidRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    Sidebar,
    LibraryRail,
    LibraryList,
    LibraryPreview,
    Menu,
    Hud,
}

/// All colors are authored by the host theme. No Arc opacity is inferred here.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfaceStyle {
    /// RGB is the opaque fallback color; alpha is the desired tint over blur.
    pub fill: Hsla,
    pub foreground: Hsla,
    /// Authored secondary text, independent of the disabled-control treatment.
    pub secondary: Hsla,
    pub border: Hsla,
    /// Overlays drawn once above fill, below text/icons.
    pub hover: Hsla,
    pub selected: Hsla,
}

/// A render-time value. Its owner/precedence is intentionally outside this type.
/// Future profile appearance and Space colors can produce this value without
/// changing renderers or adding fields to the browser/session model today.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MaterialTheme {
    pub appearance: Appearance,
    pub sidebar: SurfaceStyle,
    pub library_rail: SurfaceStyle,
    pub library_list: SurfaceStyle,
    pub library_preview: SurfaceStyle,
    pub menu: SurfaceStyle,
    pub hud: SurfaceStyle,
}

impl MaterialTheme {
    pub(crate) fn style(self, role: Surface) -> SurfaceStyle {
        match role {
            Surface::Sidebar => self.sidebar,
            Surface::LibraryRail => self.library_rail,
            Surface::LibraryList => self.library_list,
            Surface::LibraryPreview => self.library_preview,
            Surface::Menu => self.menu,
            Surface::Hud => self.hud,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaterialError {
    InvalidColor,
    TranslucentText,
    UnreadablePalette,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfaceMaterial {
    pub fill: Hsla,
    pub foreground: Hsla,
    pub secondary: Hsla,
    pub border: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
}

impl SurfaceStyle {
    /// Preserve every authored color and alpha when blur is available.
    /// Only actual unsupported blur or Reduce Transparency makes fill opaque.
    /// Contrast validation is explicit and never changes rendering values.
    pub(crate) fn resolve(self, backdrop: Backdrop, reduce_transparency: bool) -> SurfaceMaterial {
        let fill = if reduce_transparency || backdrop == Backdrop::Opaque {
            Hsla { a: 1., ..self.fill }
        } else {
            self.fill
        };
        SurfaceMaterial {
            fill,
            foreground: self.foreground,
            secondary: self.secondary,
            border: self.border,
            hover: self.hover,
            selected: self.selected,
        }
    }
}

impl SurfaceMaterial {
    /// Optional authoring check for normal text at 4.5:1 over any sRGB
    /// backdrop, in idle and mutually exclusive hover/selected states.
    /// This conservative diagnostic is not called by resolve, has no side
    /// effects, and must not gate rendering an authored reference material.
    /// Translucent text requires separate rendered-pixel validation.
    pub(crate) fn validate_text_contrast(&self) -> Result<(), MaterialError> {
        for color in [
            self.fill,
            self.foreground,
            self.secondary,
            self.border,
            self.hover,
            self.selected,
        ] {
            if !valid(color) {
                return Err(MaterialError::InvalidColor);
            }
        }
        if self.foreground.a != 1. || self.secondary.a != 1. {
            return Err(MaterialError::TranslucentText);
        }
        for text in [self.foreground, self.secondary] {
            for overlay in [None, Some(self.hover), Some(self.selected)] {
                if worst_contrast(text.into(), self.fill.into(), overlay.map(Rgba::from)) < 4.5 {
                    return Err(MaterialError::UnreadablePalette);
                }
            }
        }
        Ok(())
    }
}

fn valid(color: Hsla) -> bool {
    [color.h, color.s, color.l, color.a]
        .into_iter()
        .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
}

fn luminance(color: Rgba) -> f32 {
    let linear = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

fn worst_contrast(text: Rgba, fill: Rgba, overlay: Option<Rgba>) -> f32 {
    // Channel-wise source-over is monotone, so black and white bound the
    // luminance of every sRGB backdrop, including saturated desktop colors.
    let composite = |level| {
        let base = Rgba {
            r: level,
            g: level,
            b: level,
            a: 1.,
        }
        .blend(fill);
        luminance(overlay.map_or(base, |overlay| base.blend(overlay)))
    };
    let (low, high, text) = (composite(0.), composite(1.), luminance(text));
    if text < low {
        (low + 0.05) / (text + 0.05)
    } else if text > high {
        (text + 0.05) / (high + 0.05)
    } else {
        1.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::hsla;

    fn style(dark: bool, alpha: f32, hue: f32) -> SurfaceStyle {
        // Test fixtures only; not proposed product or Arc palette values.
        let text = if dark { 1. } else { 0. };
        SurfaceStyle {
            fill: hsla(hue, 0.2, if dark { 0.08 } else { 0.96 }, alpha),
            foreground: hsla(0., 0., text, 1.),
            secondary: hsla(0., 0., if dark { 0.8 } else { 0.25 }, 1.),
            border: hsla(0., 0., text, 0.15),
            hover: hsla(0., 0., text, 0.04),
            selected: hsla(0., 0., text, 0.08),
        }
    }

    fn assert_authored_colors(input: SurfaceStyle, output: SurfaceMaterial, fill: Hsla) {
        assert_eq!(output.fill, fill);
        assert_eq!(output.foreground, input.foreground);
        assert_eq!(output.secondary, input.secondary);
        assert_eq!(output.border, input.border);
        assert_eq!(output.hover, input.hover);
        assert_eq!(output.selected, input.selected);
    }

    #[test]
    fn fallback_and_reduced_transparency_change_only_fill_alpha() {
        for dark in [false, true] {
            let input = style(dark, 0.4, 0.6);
            let opaque = Hsla {
                a: 1.,
                ..input.fill
            };
            assert_authored_colors(input, input.resolve(Backdrop::Opaque, false), opaque);
            assert_authored_colors(input, input.resolve(Backdrop::Blurred, true), opaque);
            assert_authored_colors(input, input.resolve(Backdrop::Opaque, true), opaque);
        }
    }

    #[test]
    fn resolution_preserves_authored_materials_including_low_contrast() {
        for dark in [false, true] {
            for degree in 0..360 {
                for alpha in [0., 0.2, 0.5, 0.99, 1.] {
                    let input = style(dark, alpha, degree as f32 / 360.);
                    assert_authored_colors(
                        input,
                        input.resolve(Backdrop::Blurred, false),
                        input.fill,
                    );
                }
            }
        }
        let mut input = style(false, 0.2, 0.3);
        input.foreground.a = 0.65;
        input.secondary.a = 0.5;
        assert_authored_colors(input, input.resolve(Backdrop::Blurred, false), input.fill);
    }

    #[test]
    fn validation_reports_contrast_without_changing_material() {
        for dark in [false, true] {
            for degree in 0..360 {
                let input = style(dark, 0.2, degree as f32 / 360.);
                let output = input.resolve(Backdrop::Blurred, false);
                assert_eq!(
                    output.validate_text_contrast(),
                    Err(MaterialError::UnreadablePalette)
                );
                assert_authored_colors(input, output, input.fill);
                let opaque = input.resolve(Backdrop::Opaque, false);
                assert_eq!(opaque.validate_text_contrast(), Ok(()));
            }
        }
    }

    #[test]
    fn validator_reports_invalid_colors_and_translucent_text() {
        let mut input = style(false, 0.5, 0.3);
        input.foreground.a = 0.5;
        let output = input.resolve(Backdrop::Blurred, false);
        assert_eq!(
            output.validate_text_contrast(),
            Err(MaterialError::TranslucentText)
        );
        assert_authored_colors(input, output, input.fill);
        input.fill.h = f32::NAN;
        assert_eq!(
            input
                .resolve(Backdrop::Blurred, false)
                .validate_text_contrast(),
            Err(MaterialError::InvalidColor)
        );
    }

    #[test]
    fn a_backdrop_with_the_same_luminance_as_text_is_not_readable() {
        let text = Rgba::from(hsla(0., 0., 0.5, 1.));
        let fill = Rgba::from(hsla(0., 0., 0., 0.));
        assert_eq!(worst_contrast(text, fill, None), 1.);
    }
}
