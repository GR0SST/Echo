//! Original Echo icon variants. No reference-browser artwork is embedded.
use browser_core::settings::AppIcon;
use gpui::{prelude::*, px, AnyElement};

pub(crate) fn artwork(icon: AppIcon) -> (gpui::ImageFormat, Vec<u8>) {
    let bytes: &[u8] = match icon {
        AppIcon::Original => include_bytes!("../assets/echo-icon.png"),
        AppIcon::Candy => include_bytes!("../assets/app-icons/candy.png"),
        AppIcon::Neon => include_bytes!("../assets/app-icons/neon.png"),
        AppIcon::Glass => include_bytes!("../assets/app-icons/glass.png"),
        AppIcon::Schoolbook => include_bytes!("../assets/app-icons/schoolbook.png"),
        AppIcon::Colorful => include_bytes!("../assets/app-icons/colorful.png"),
    };
    (gpui::ImageFormat::Png, bytes.to_vec())
}

pub(crate) fn preview_image(icon: AppIcon, cx: &gpui::App) -> std::sync::Arc<gpui::RenderImage> {
    let (format, bytes) = artwork(icon);
    let decoded = gpui::Image::from_bytes(format, bytes)
        .to_image_data(cx.svg_renderer())
        .expect("generated Echo artwork");
    let size = decoded.size(0);
    let pixels = decoded.as_bytes(0).expect("first icon frame").to_vec();
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
    // Pre-filter the high-resolution artwork for a 52 pt Retina thumbnail.
    let buffer = if icon == AppIcon::Original {
        buffer
    } else {
        image::imageops::resize(&buffer, 208, 208, image::imageops::FilterType::Lanczos3)
    };
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
