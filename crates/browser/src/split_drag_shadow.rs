//! Hollow shadow: GPUI's additive shadow alpha must not make the card opaque.
use std::sync::{Arc, OnceLock};

pub(super) const PAD: f32 = 32.;
pub(super) const WIDTH: f32 = 192.;
pub(super) const HEIGHT: f32 = 314.;
const SCALE: u32 = 2;

fn coverage(x: f32, y: f32, offset: f32) -> f32 {
    let inside = |x: f32, y: f32| {
        let x = x - PAD;
        let y = y - PAD - offset;
        if !(0. ..128.).contains(&x) || !(0. ..250.).contains(&y) {
            return false;
        }
        let dx = x - x.clamp(20., 108.);
        let dy = y - y.clamp(20., 230.);
        dx * dx + dy * dy <= 400.
    };
    let mut samples = 0;
    for sy in 0..2 {
        for sx in 0..2 {
            if inside(x + (sx as f32 + 0.5) / 4., y + (sy as f32 + 0.5) / 4.) {
                samples += 1;
            }
        }
    }
    samples as f32 / 4.
}

fn raster() -> image::RgbaImage {
    let width = WIDTH as u32 * SCALE;
    let height = HEIGHT as u32 * SCALE;
    let shape = image::GrayImage::from_fn(width, height, |x, y| {
        image::Luma([(255. * coverage(x as f32 / 2., y as f32 / 2., 6.)).round() as u8])
    });
    let blurred = image::imageops::blur(&shape, 16.);
    image::RgbaImage::from_fn(width, height, |x, y| {
        let outside = 1. - coverage(x as f32 / 2., y as f32 / 2., 0.);
        image::Rgba([
            0,
            0,
            0,
            (f32::from(blurred.get_pixel(x, y)[0]) * 0.3 * outside).round() as u8,
        ])
    })
}

pub(super) fn image() -> Arc<gpui::Image> {
    static IMAGE: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            let mut bytes = Vec::new();
            image::DynamicImage::ImageRgba8(raster())
                .write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                )
                .expect("procedural drag shadow PNG");
            Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_drag_shadow_has_no_alpha_under_the_card_and_keeps_its_exterior_falloff() {
        let shadow = raster();
        assert_eq!(shadow.dimensions(), (384, 628));
        assert!(shadow.pixels().all(|pixel| pixel[3] <= 77));
        assert_eq!(shadow.get_pixel(192, 300)[3], 0);
        assert!(shadow.get_pixel(192, 570)[3] > 0);
        assert!(
            shadow.get_pixel(68, 68)[3] > 0,
            "rounded outside corners retain their shadow"
        );
        assert_eq!(shadow.get_pixel(0, 0)[3], 0);
    }
}
