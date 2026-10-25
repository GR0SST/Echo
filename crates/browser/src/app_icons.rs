//! Original Echo icon variants. No reference-browser artwork is embedded.
use browser_core::settings::AppIcon;
use gpui::{prelude::*, px, AnyElement};

pub(crate) fn artwork(icon: AppIcon) -> (gpui::ImageFormat, Vec<u8>) {
    if icon == AppIcon::Original {
        return (
            gpui::ImageFormat::Png,
            include_bytes!("../assets/echo-icon.png").to_vec(),
        );
    }
    let (top, bottom, ink) = match icon {
        AppIcon::Candy => ("#f8d3e7", "#a7b8ec", "#fff5ff"),
        AppIcon::Neon => ("#171039", "#370b54", "#f88cfa"),
        AppIcon::Glass => ("#ece9e5", "#c2c8cc", "#555568"),
        AppIcon::Schoolbook => ("#fff2ad", "#efcd76", "#3164cd"),
        AppIcon::Colorful => ("#160b3d", "#321052", "#ff6aaf"),
        AppIcon::Original => unreachable!(),
    };
    // Echo's concentric, six-lobed echo mark, expressed as portable geometry.
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="192" height="192" viewBox="0 0 192 192"><defs><linearGradient id="b" x2="0" y2="1"><stop stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient></defs><rect x="15" y="15" width="162" height="162" rx="32" fill="url(#b)"/>"##
    );
    if icon == AppIcon::Glass {
        for x in (23..170).step_by(7) {
            svg.push_str(&format!(r#"<rect x="{x}" y="38" width="2" height="116" rx="1" fill="white" opacity="0.34"/>"#));
        }
    }
    if icon == AppIcon::Schoolbook {
        for y in (39..160).step_by(13) {
            svg.push_str(&format!(
                r##"<path d="M25 {y}H167" stroke="#be9d54" opacity="0.25"/>"##
            ));
        }
    }
    for ring in 0..6 {
        let ink = if icon == AppIcon::Colorful {
            [
                "#ffe487", "#ffb344", "#ff6aaf", "#c765f4", "#6759ee", "#3c87e8",
            ][ring]
        } else {
            ink
        };
        let radius = 15. + ring as f32 * 8.;
        let opacity = 1. - ring as f32 * 0.12;
        let mut path = String::new();
        for step in 0..=180 {
            let angle = step as f32 / 180. * std::f32::consts::TAU;
            let r = radius * (1. + 0.085 * (6. * angle).cos());
            path.push_str(&format!(
                "{} {:.2} {:.2} ",
                if step == 0 { "M" } else { "L" },
                96. + r * angle.cos(),
                96. + r * angle.sin()
            ));
        }
        svg.push_str(&format!(
            r#"<path d="{path}Z" fill="none" stroke="{ink}" stroke-width="4" opacity="{opacity}"/>"#
        ));
    }
    svg.push_str("</svg>");
    (gpui::ImageFormat::Svg, svg.into_bytes())
}

pub(crate) fn preview_image(icon: AppIcon, cx: &gpui::App) -> std::sync::Arc<gpui::RenderImage> {
    let (format, bytes) = artwork(icon);
    let decoded = gpui::Image::from_bytes(format, bytes)
        .to_image_data(cx.svg_renderer())
        .expect("generated Echo artwork");
    let size = decoded.size(0);
    let mut pixels = decoded.as_bytes(0).expect("first icon frame").to_vec();
    // GPUI 0.2's encoded-image SVG path omits its normal RGBA-to-BGRA
    // conversion. Supply straight-alpha BGRA, as its PNG path already does.
    if icon != AppIcon::Original {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            if pixel[3] > 0 {
                let alpha = pixel[3] as f32 / 255.;
                for channel in &mut pixel[..3] {
                    *channel = (*channel as f32 / alpha) as u8;
                }
            }
        }
    }
    let buffer = image::RgbaImage::from_raw(size.width.0 as u32, size.height.0 as u32, pixels)
        .expect("decoder dimensions");
    // Dock artwork includes transparent outer padding; radio rows use a 52 px
    // optical frame so the visible icon, rather than its canvas, fills the row.
    let (mut left, mut top, mut right, mut bottom) = (buffer.width(), buffer.height(), 0, 0);
    for (x, y, pixel) in buffer.enumerate_pixels() {
        if pixel[3] != 0 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    let buffer =
        image::imageops::crop_imm(&buffer, left, top, right - left, bottom - top).to_image();
    std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(buffer)]))
}
pub(crate) fn preview(icon: AppIcon, cx: &gpui::App) -> AnyElement {
    gpui::img(preview_image(icon, cx))
        .size(px(52.))
        .flex_none()
        .into_any_element()
}

/// Platform integration is isolated from preference and artwork logic.
pub(crate) fn apply(icon: AppIcon) -> bool {
    if cfg!(test) {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        use objc2::{
            msg_send,
            runtime::{AnyClass, AnyObject},
        };
        unsafe {
            let Some(data_class) = AnyClass::get(c"NSData") else {
                return false;
            };
            let Some(image_class) = AnyClass::get(c"NSImage") else {
                return false;
            };
            let Some(app_class) = AnyClass::get(c"NSApplication") else {
                return false;
            };
            let app: *mut AnyObject = msg_send![app_class, sharedApplication];
            if app.is_null() {
                return false;
            }
            if icon == AppIcon::Original {
                // Restore the bundle's full-resolution icon rather than the small UI preview.
                let _: () =
                    msg_send![app, setApplicationIconImage: std::ptr::null_mut::<AnyObject>()];
                return true;
            }
            let (_, bytes) = artwork(icon);
            let data: *mut AnyObject = msg_send![data_class, dataWithBytes: bytes.as_ptr().cast::<std::ffi::c_void>() length: bytes.len()];
            let allocated: *mut AnyObject = msg_send![image_class, alloc];
            let image: *mut AnyObject = msg_send![allocated, initWithData: data];
            if image.is_null() {
                return false;
            }
            let _: () = msg_send![app, setApplicationIconImage: image];
            let _: () = msg_send![image, release];
            true
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = icon;
        false
    }
}
