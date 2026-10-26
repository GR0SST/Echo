//! First-run welcome intro. There is no window at first: a glowing orb grows
//! on the dimming desktop, then a glass pane appears where the window will be,
//! the orb swells into it and the pane resolves into a drifting color field
//! around the music's downbeat. The Echo mark rises in and leaves, then the
//! headline letters rise out of a blur and the continue arrow appears. Timing
//! follows Arc's intro, measured frame by frame, in music time.
use crate::icons::{self, Icon};
use gpui::{AnyElement, Pixels, RenderImage, Size, div, hsla, img, prelude::*, px};
use std::sync::Arc;

/// Music time after which the orb is no longer drawn.
pub(crate) const ORB_GONE: f32 = 4.4;
/// Music time at which the continue arrow is fully shown.
pub(crate) const READY: f32 = 10.2;
// Beats measured from Arc's intro recording at 30 fps, aligned to its music
// by cross-correlating the recorded audio (the recording leads by 2.572 s).
const ORB_IN: (f32, f32) = (0.3, 1.0);
const DIM: (f32, f32) = (0.3, 1.6);
const DIM_ALPHA: f32 = 0.86;
const GLASS: (f32, f32) = (3.85, 4.0);
const BLOOM: (f32, f32) = (3.95, 4.3);
const FIELD_IN: (f32, f32) = (4.0, 4.45);
const BRIGHTEN: (f32, f32) = (4.3, 4.8);
const UNDIM: (f32, f32) = (5.3, 7.6);
/// The mark rises into place out of a blur just after the downbeat, then
/// leaves upward with a short accelerating blur.
const MARK_IN: (f32, f32) = (4.72, 5.45);
const MARK_OUT: (f32, f32) = (6.65, 6.92);
const MARK_TRAVEL: f32 = 80.;
const MARK_SIDE: f32 = 108.;
const CORNER: f32 = 10.;
/// Headline lines and their start times. Letters arrive left to right, each
/// rising out of a blur.
const LINES: [(&str, f32); 2] = [("Meet the", 7.5), ("internet again", 7.8)];
const LETTER_STAGGER: f32 = 0.035;
const LETTER_TIME: f32 = 0.55;
const RISE: f32 = 36.;
const BLUR: f32 = 12.;
const ARROW_IN: (f32, f32) = (9.7, READY);

const BASE: u32 = 0x4a3ce6;
/// Field colors: light sky, pale lavender, violet, magenta, periwinkle, deep indigo.
const FIELD: [(u32, f32, f32, f32, f32, f32); 6] = [
    // color, size (× width), x center, y center, period seconds, phase
    (0xa8cdf3, 1.05, 0.18, 0.22, 13., 0.0),
    (0xc6c2f2, 0.75, 0.62, 0.05, 11., 1.7),
    (0x6a28c8, 0.95, 0.88, 0.72, 12., 3.1),
    (0xbc52c2, 0.55, 0.20, 0.98, 10., 4.4),
    (0x93b9f0, 0.85, 0.80, 1.02, 14., 2.2),
    (0x3d2fe0, 0.80, 0.50, 0.50, 9., 5.0),
];
/// Orb sub-glows: color, size, orbit radius, angular speed, phase, weight.
/// Sizes are in points at full orb scale. A vivid indigo core with a pale
/// cyan rim drifting around it and a smaller pink edge, as in Arc.
const ORB: [(u32, f32, f32, f32, f32, f32); 4] = [
    (0x3a2cf2, 380., 10., -1.1, 1.2, 1.0),
    (0x9fd8f0, 360., 48., 0.9, 2.4, 0.55),
    (0xb7a4ee, 320., 40., -0.7, 0.3, 0.3),
    (0xe0609a, 260., 62., 1.2, 4.6, 0.6),
];
/// Half the orb image's extent, in the same units as `ORB`.
const ORB_EXTENT: f32 = 290.;
const ORB_PIXELS: u32 = 160;

fn smooth(range: (f32, f32), t: f32) -> f32 {
    let x = ((t - range.0) / (range.1 - range.0)).clamp(0., 1.);
    x * x * (3. - 2. * x)
}

/// The orb at `t`, composited like a Screen/Add layer: the glows' colors are
/// summed and the alpha is the brightest channel, so blending it over the
/// desktop can only lighten it, never leave a dark fringe. GPUI has no blend
/// modes, so this is baked on the CPU for each frame of the short orb phase.
pub(crate) fn orb_image(t: f32) -> Arc<RenderImage> {
    let unit = 2. * ORB_EXTENT / ORB_PIXELS as f32;
    let glows: Vec<([f32; 3], f32, f32, f32)> = ORB
        .iter()
        .map(|&(color, side, radius, speed, phase, weight)| {
            let a = t * speed + phase;
            let rgb = unpack(color).map(|c| c * weight);
            (rgb, radius * a.cos(), radius * a.sin(), side / 2.)
        })
        .collect();
    let buffer = image::RgbaImage::from_fn(ORB_PIXELS, ORB_PIXELS, |x, y| {
        let px_ = (x as f32 + 0.5) * unit - ORB_EXTENT;
        let py_ = (y as f32 + 0.5) * unit - ORB_EXTENT;
        let mut sum = [0f32; 3];
        for (rgb, gx, gy, half) in &glows {
            let d = (((px_ - gx).powi(2) + (py_ - gy).powi(2)).sqrt() / half).min(1.);
            let a = (1. - d * d * (3. - 2. * d)).powf(1.5);
            for c in 0..3 {
                sum[c] += rgb[c] * a;
            }
        }
        let alpha = sum.iter().fold(0f32, |m, v| m.max(*v)).min(1.);
        let straight = |v: f32| {
            if alpha > 0. { (v.min(1.) / alpha * 255.).round().min(255.) as u8 } else { 0 }
        };
        image::Rgba([straight(sum[2]), straight(sum[1]), straight(sum[0]), (alpha * 255.).round() as u8])
    });
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

const FIELD_PIXELS: (u32, u32) = (320, 200);

fn unpack(color: u32) -> [f32; 3] {
    [
        ((color >> 16) & 0xff) as f32 / 255.,
        ((color >> 8) & 0xff) as f32 / 255.,
        (color & 0xff) as f32 / 255.,
    ]
}

/// The window's color field at `t`, flattened into one opaque image so it can
/// be drawn with the window's rounded corners (GPUI clips children only to
/// rectangles). `aspect` is the window's width / height.
pub(crate) fn field_image(t: f32, aspect: f32) -> Arc<RenderImage> {
    let (iw, ih) = FIELD_PIXELS;
    let glows: Vec<([f32; 3], f32, f32, f32)> = FIELD
        .iter()
        .map(|&(color, scale, x, y, period, phase)| {
            let a = std::f32::consts::TAU * t / period + phase;
            // Center in window-width units on both axes; radius likewise.
            (unpack(color), x + 0.10 * a.sin(), (y + 0.12 * (a * 0.8).cos()) / aspect, scale * 0.8)
        })
        .collect();
    let dark = 0.45 * (1. - smooth(BRIGHTEN, t));
    let shade = unpack(0x1b1442);
    let base = unpack(BASE);
    let buffer = image::RgbaImage::from_fn(iw, ih, |x, y| {
        let u = (x as f32 + 0.5) / iw as f32;
        let v = (y as f32 + 0.5) / ih as f32 / aspect;
        let mut color = base;
        for (rgb, gx, gy, half) in &glows {
            let d = (((u - gx).powi(2) + (v - gy).powi(2)).sqrt() / half).min(1.);
            let a = (1. - d * d * (3. - 2. * d)).powf(1.5);
            for c in 0..3 {
                color[c] += (rgb[c] - color[c]) * a;
            }
        }
        for c in 0..3 {
            color[c] += (shade[c] - color[c]) * dark;
        }
        let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
        image::Rgba([byte(color[2]), byte(color[1]), byte(color[0]), 255])
    });
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

thread_local! {
    static MARK: std::cell::RefCell<Option<(u32, Arc<RenderImage>)>> = const { std::cell::RefCell::new(None) };
}

/// The Echo mark resampled once to exactly `pixels` with a Lanczos filter.
/// GPUI samples images bilinearly without mipmaps, so drawing the 896 px
/// source at ~100 pt aliases its thin rings.
fn mark_image(pixels: u32) -> Arc<RenderImage> {
    MARK.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, image)) = cache.as_ref().filter(|(size, _)| *size == pixels) {
            return image.clone();
        }
        let source = image::load_from_memory(include_bytes!("../../bundle/assets/echo-mark.png"))
            .expect("bundled Echo mark")
            .to_rgba8();
        let mut scaled = image::imageops::resize(
            &source,
            pixels,
            pixels,
            image::imageops::FilterType::Lanczos3,
        );
        for pixel in scaled.pixels_mut() {
            pixel.0.swap(0, 2);
        }
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(scaled)]));
        *cache = Some((pixels, image.clone()));
        image
    })
}

/// Draws a square image whose edges are transparent, centered at (`x`, `y`).
/// GPUI packs images into a shared atlas without gutters, so bilinear
/// sampling at an image's outermost texels picks up its atlas neighbor as a
/// thin line. Those texels are transparent here, so clip them away.
fn placed(image: Arc<RenderImage>, x: f32, y: f32, side: f32) -> AnyElement {
    const GUTTER: f32 = 2.;
    div()
        .absolute()
        .left(px(x - side / 2. + GUTTER))
        .top(px(y - side / 2. + GUTTER))
        .size(px((side - 2. * GUTTER).max(0.)))
        .overflow_hidden()
        .child(
            img(image)
                .absolute()
                .left(px(-GUTTER))
                .top(px(-GUTTER))
                .size(px(side)),
        )
        .into_any_element()
}

fn ease_out(range: (f32, f32), t: f32) -> f32 {
    let x = ((t - range.0) / (range.1 - range.0)).clamp(0., 1.);
    1. - (1. - x).powi(3)
}

/// One letter rising out of a blur. GPUI cannot blur text, so the blur is a
/// ring of faint offset copies that converge onto the sharp glyph.
fn letter(text: String, start: f32, t: f32) -> AnyElement {
    let p = ease_out((start, start + LETTER_TIME), t);
    let alpha = smooth((start, start + LETTER_TIME * 0.6), t);
    let blur = (1. - p) * BLUR;
    let mut glyph = div().relative().top(px((1. - p) * RISE));
    if blur > 0.3 {
        glyph = glyph.child(div().opacity(alpha * 0.25).child(text.clone()));
        for ring in [0.5f32, 1.] {
            for step in 0..8 {
                let a = std::f32::consts::TAU * step as f32 / 8. + ring;
                glyph = glyph.child(
                    div()
                        .absolute()
                        .left(px(blur * ring * a.cos()))
                        .top(px(blur * ring * a.sin()))
                        .opacity(alpha * 0.11)
                        .child(text.clone()),
                );
            }
        }
    } else {
        glyph = glyph.child(div().opacity(alpha).child(text));
    }
    glyph.into_any_element()
}

fn headline_line(text: &str, start: f32, t: f32) -> AnyElement {
    let mut row = div().flex();
    for (index, ch) in text.chars().enumerate() {
        let at = start + index as f32 * LETTER_STAGGER;
        row = row.child(if ch == ' ' {
            div().w(px(17.)).into_any_element()
        } else {
            letter(ch.to_string(), at, t)
        });
    }
    row.into_any_element()
}

/// The Echo mark centered at (`x`, `y`), blurred by `blur` points with the
/// same converging-copies technique as the headline letters.
fn mark(x: f32, y: f32, alpha: f32, blur: f32, scale: f32) -> AnyElement {
    let image = mark_image((MARK_SIDE * scale).round().max(1.) as u32);
    let copy = |dx: f32, dy: f32, opacity: f32| {
        div()
            .absolute()
            .inset_0()
            .opacity(opacity)
            .child(placed(image.clone(), x + dx, y + dy, MARK_SIDE))
    };
    let mut layer = div().absolute().inset_0();
    if blur > 0.3 {
        layer = layer.child(copy(0., 0., alpha * 0.25));
        for ring in [0.5f32, 1.] {
            for step in 0..8 {
                let a = std::f32::consts::TAU * step as f32 / 8. + ring;
                layer = layer.child(copy(blur * ring * a.cos(), blur * ring * a.sin(), alpha * 0.11));
            }
        }
    } else {
        layer = layer.child(copy(0., 0., alpha * 0.95));
    }
    layer.into_any_element()
}

/// The intro at music time `t` in a `size` intro window. `frame` is where
/// the browser window sits inside it (the whole intro window when absent).
/// `arrow` is the caller's interactive continue control.
pub(crate) fn render(
    t: f32,
    size: Size<Pixels>,
    frame: Option<gpui::Bounds<Pixels>>,
    orb: Option<Arc<RenderImage>>,
    field: Option<Arc<RenderImage>>,
    scale: f32,
    arrow: gpui::Stateful<gpui::Div>,
) -> AnyElement {
    let frame = frame.unwrap_or(gpui::Bounds::new(gpui::point(px(0.), px(0.)), size));
    let (w, h) = (f32::from(frame.size.width), f32::from(frame.size.height));
    let (cx, cy) = (w / 2., h / 2.);
    let bloom = smooth(BLOOM, t);
    // Field: the window's color surface, revealed as the orb blooms and
    // brightening into its final colors on the downbeat.
    let field = div()
        .absolute()
        .inset_0()
        .opacity(smooth(FIELD_IN, t))
        .when_some(field, |layer, image| {
            layer.child(
                img(image)
                    .absolute()
                    .inset_0()
                    .size_full()
                    // Stretch: the field is resolution-free, and letterboxing
                    // would detach it from the window's edges.
                    .object_fit(gpui::ObjectFit::Fill)
                    .rounded(px(CORNER)),
            )
        });
    // Arc's window shows its inactive buttons faintly over the field.
    let lights = (0..3).fold(
        div().absolute().inset_0().opacity(smooth(FIELD_IN, t) * 0.22),
        |row, i| {
            row.child(
                div()
                    .absolute()
                    .left(px(18. + 18. * i as f32))
                    .top(px(10.))
                    .size(px(12.))
                    .rounded_full()
                    .bg(gpui::white()),
            )
        },
    );
    // Orb: grows in from nothing on the desktop, churns, then swells to
    // fill the pane and dissolves into the field.
    let orb_scale = (0.35 + 0.65 * smooth(ORB_IN, t))
        * (1. + 0.05 * (t * 2.1).sin())
        * (1. + 3.2 * bloom);
    let orb = div()
        .absolute()
        .inset_0()
        .opacity(smooth(ORB_IN, t) * (1. - smooth((4.15, ORB_GONE), t)))
        .when_some(orb, |layer, image| {
            layer.child(placed(image, cx, cy, 2. * ORB_EXTENT * orb_scale))
        });
    let glass = smooth(GLASS, t);
    let rise = ease_out(MARK_IN, t);
    let leave = {
        let x = ((t - MARK_OUT.0) / (MARK_OUT.1 - MARK_OUT.0)).clamp(0., 1.);
        x * x
    };
    let mark_alpha = smooth((MARK_IN.0, MARK_IN.0 + 0.45), t) * (1. - leave);
    let mark_y = cy + MARK_TRAVEL * (1. - rise) - MARK_TRAVEL * leave;
    let mark_blur = 10. * ((1. - rise) + leave).min(1.);
    let arrow_alpha = smooth(ARROW_IN, t);
    // The pane clips the bloom like a window; before it exists, the orb
    // floats free on the desktop.
    let pane = div()
        .absolute()
        .left(frame.origin.x)
        .top(frame.origin.y)
        .w(px(w))
        .h(px(h))
        .rounded(px(CORNER * glass))
        .when(glass > 0., |s| s.overflow_hidden())
        .bg(hsla(0., 0., 0.55, 0.16 * glass * (1. - smooth(FIELD_IN, t))))
        .child(field)
        .child(lights)
        .child(orb);

    div()
        .id("welcome-intro")
        .debug_selector(|| "welcome-intro".into())
        .relative()
        .size_full()
        .overflow_hidden()
        .bg(hsla(0., 0., 0., DIM_ALPHA * smooth(DIM, t) * (1. - smooth(UNDIM, t))))
        .child(
            pane.when(mark_alpha > 0.005, |s| {
                s.child(mark(cx, mark_y, mark_alpha, mark_blur, scale))
            })
            .child(
                div()
                    .absolute()
                    .left_0()
                    .w(px(w))
                    .top(px(cy - 96.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .text_color(gpui::white())
                    .text_size(px(68.))
                    .line_height(px(66.))
                    .font_weight(gpui::FontWeight::BLACK)
                    .child(headline_line(LINES[0].0, LINES[0].1, t))
                    .child(headline_line(LINES[1].0, LINES[1].1, t))
                    .child(
                        arrow
                            .mt(px(60.))
                            .w(px(62.))
                            .h(px(40.))
                            .rounded(px(7.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(hsla(0.66, 0.55, 0.32, 0.72))
                            .hover(|s| s.bg(hsla(0.66, 0.55, 0.26, 0.82)))
                            .opacity(arrow_alpha)
                            .when(arrow_alpha > 0.5, |s| s.cursor_pointer())
                            .child(icons::control_sized(Icon::Arrow, 16.)),
                    ),
            ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mark_is_resampled_to_its_drawn_pixels() {
        let image = mark_image(216);
        assert_eq!(image.size(0), gpui::size(gpui::DevicePixels(216), gpui::DevicePixels(216)));
        assert!(std::sync::Arc::ptr_eq(&image, &mark_image(216)));
    }
    #[test]
    fn intro_beats_follow_arc_order() {
        assert!(ORB_IN.1 < GLASS.0 && GLASS.0 < BLOOM.0 && BRIGHTEN.1 < MARK_IN.1);
        assert!(MARK_OUT.1 < LINES[0].1 && LINES[0].1 < LINES[1].1);
        let last = LINES[1].1 + LINES[1].0.len() as f32 * LETTER_STAGGER + LETTER_TIME;
        assert!(last < ARROW_IN.0 && ARROW_IN.1 == READY);
        assert!(!LINES[1].0.ends_with('.'));
    }
}

#[cfg(test)]
mod orb_tests {
    use super::*;
    /// Screen-like compositing: every pixel's straight color has a full
    /// brightest channel (or is transparent), so the orb never darkens.
    #[test]
    fn orb_pixels_only_lighten_what_is_beneath() {
        let image = orb_image(1.3);
        let bytes = image.as_bytes(0).unwrap();
        let mut opaque = 0;
        for px_ in bytes.chunks_exact(4) {
            if px_[3] > 8 {
                opaque += 1;
                assert!(px_[..3].iter().copied().max().unwrap() >= 250, "{px_:?}");
            }
        }
        assert!(opaque > 1000);
    }
}

