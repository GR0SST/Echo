//! Derived favicon looks: the accent the reference tints an active Favorite
//! with, and recolored monochrome marks (e.g. the GitHub mark) so they stay
//! readable on both the light sidebar and the dark Command Bar.
use std::{collections::HashMap, hash::{Hash, Hasher}, sync::Arc};

use gpui::{hsla, Hsla};

#[derive(Clone)]
pub(crate) struct FaviconStyle {
    /// What to draw on a light surface: light marks are darkened.
    pub image: Arc<gpui::Image>,
    /// What to draw on a dark surface: dark marks are lightened.
    pub on_dark: Arc<gpui::Image>,
    /// The icon's dominant color; `None` for gray or black-and-white icons.
    pub accent: Option<Hsla>,
}

#[derive(Default)]
pub(crate) struct FaviconStyles(HashMap<u64, FaviconStyle>);

impl FaviconStyles {
    pub(crate) fn get(&mut self, png: &[u8]) -> FaviconStyle {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        png.hash(&mut hasher);
        self.0.entry(hasher.finish()).or_insert_with(|| analyze(png)).clone()
    }
}

fn analyze(png: &[u8]) -> FaviconStyle {
    let original = || Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, png.to_vec()));
    let Ok(decoded) = image::load_from_memory_with_format(png, image::ImageFormat::Png) else {
        return FaviconStyle { image: original(), on_dark: original(), accent: None };
    };
    let mut rgba = decoded.to_rgba8();
    let (mut weight, mut light, mut opaque) = (0f32, 0f32, 0f32);
    let (mut r, mut g, mut b) = (0f32, 0f32, 0f32);
    for pixel in rgba.pixels() {
        let [pr, pg, pb, pa] = pixel.0.map(|value| value as f32 / 255.);
        if pa < 0.5 { continue; }
        opaque += 1.;
        let max = pr.max(pg).max(pb);
        let min = pr.min(pg).min(pb);
        light += (pr * 0.299 + pg * 0.587 + pb * 0.114) * pa;
        // Saturated pixels decide the accent.
        let saturation = if max > 0. { (max - min) / max } else { 0. };
        let w = saturation * saturation * max;
        weight += w;
        r += pr * w; g += pg * w; b += pb * w;
    }
    if opaque == 0. { return FaviconStyle { image: original(), on_dark: original(), accent: None }; }
    let colorful = weight / opaque > 0.08;
    let accent = colorful.then(|| {
        let rgb = gpui::Rgba { r: r / weight, g: g / weight, b: b / weight, a: 1. };
        let color: Hsla = rgb.into();
        hsla(color.h, color.s.max(0.55), color.l.clamp(0.45, 0.6), 1.)
    });
    // A mark with no color would vanish on a surface of its own lightness.
    let lightness = light / opaque;
    let image = if !colorful && lightness > 0.78 { recolor(&rgba, [38, 38, 42]) } else { None };
    let on_dark = if !colorful && lightness < 0.3 { recolor(&rgba, [236, 236, 240]) } else { None };
    FaviconStyle { image: image.unwrap_or_else(original), on_dark: on_dark.unwrap_or_else(original), accent }
}

fn recolor(rgba: &image::RgbaImage, [r, g, b]: [u8; 3]) -> Option<Arc<gpui::Image>> {
    let mut rgba = rgba.clone();
    for pixel in rgba.pixels_mut() {
        pixel.0 = [r, g, b, pixel.0[3]];
    }
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(rgba).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).ok()?;
    Some(Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(color: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(8, 8, image::Rgba(color));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }

    #[test]
    fn colorful_icons_get_an_accent_and_monochrome_marks_follow_the_surface() {
        let mut styles = FaviconStyles::default();
        let red = styles.get(&png([230, 30, 30, 255]));
        assert!(red.accent.is_some_and(|accent| accent.h < 0.05 || accent.h > 0.95));
        let white = png([250, 250, 250, 255]);
        let style = styles.get(&white);
        assert!(style.accent.is_none());
        assert_ne!(style.image.bytes(), white.as_slice());
        assert_eq!(style.on_dark.bytes(), white.as_slice());
        let black = png([20, 20, 20, 255]);
        assert_eq!(styles.get(&black).image.bytes(), black.as_slice());
        assert_ne!(styles.get(&black).on_dark.bytes(), black.as_slice());
    }
}
