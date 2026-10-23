//! Derived favicon looks: the accent the reference tints an active Favorite
//! with, and a dark variant of light monochrome icons (e.g. a dark-mode
//! GitHub mark) so they stay readable on a light sidebar.
use std::{collections::HashMap, hash::{Hash, Hasher}, sync::Arc};

use gpui::{hsla, Hsla};

#[derive(Clone)]
pub(crate) struct FaviconStyle {
    /// What to draw: the original PNG, or a darkened one for light marks.
    pub image: Arc<gpui::Image>,
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
        return FaviconStyle { image: original(), accent: None };
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
    if opaque == 0. { return FaviconStyle { image: original(), accent: None }; }
    let colorful = weight / opaque > 0.08;
    let accent = colorful.then(|| {
        let rgb = gpui::Rgba { r: r / weight, g: g / weight, b: b / weight, a: 1. };
        let color: Hsla = rgb.into();
        hsla(color.h, color.s.max(0.55), color.l.clamp(0.45, 0.6), 1.)
    });
    // A light mark with no color would vanish on the light sidebar.
    if !colorful && light / opaque > 0.78 {
        for pixel in rgba.pixels_mut() {
            let [_, _, _, a] = pixel.0;
            pixel.0 = [38, 38, 42, a];
        }
        let mut bytes = Vec::new();
        if image::DynamicImage::ImageRgba8(rgba).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).is_ok() {
            return FaviconStyle { image: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes)), accent };
        }
    }
    FaviconStyle { image: original(), accent }
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
    fn colorful_icons_get_an_accent_and_light_marks_are_darkened() {
        let mut styles = FaviconStyles::default();
        let red = styles.get(&png([230, 30, 30, 255]));
        assert!(red.accent.is_some_and(|accent| accent.h < 0.05 || accent.h > 0.95));
        let white = png([250, 250, 250, 255]);
        let style = styles.get(&white);
        assert!(style.accent.is_none());
        assert_ne!(style.image.bytes(), white.as_slice());
        let black = png([20, 20, 20, 255]);
        assert_eq!(styles.get(&black).image.bytes(), black.as_slice());
    }
}
