//! Native window glue the toolkit does not provide.

use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The window's root native view (`NSView*` on macOS). `None` on gpui's
/// test platform, whose windows have no native counterpart.
pub fn view_of(window: &gpui::Window) -> Option<*mut c_void> {
    if cfg!(test) {
        return None;
    }
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr()),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    use objc2::runtime::{AnyObject, Bool, Sel};
    use objc2::{msg_send, sel};

    const CLOSE: usize = 0;
    const MINIATURIZE: usize = 1;
    const ZOOM: usize = 2;
    const WINDOW_ABOVE: isize = 1;

    fn window(view: *mut c_void) -> *mut AnyObject {
        unsafe { msg_send![view as *mut AnyObject, window] }
    }

    fn button(window: *mut AnyObject, kind: usize) -> *mut AnyObject {
        unsafe { msg_send![window, standardWindowButton: kind] }
    }

    #[cfg(not(test))]
    pub fn prefers_reduced_motion() -> bool {
        unsafe {
            let workspace: *mut AnyObject = msg_send![objc2::runtime::AnyClass::get(c"NSWorkspace").expect("AppKit is loaded"), sharedWorkspace];
            let reduced: Bool = msg_send![workspace, accessibilityDisplayShouldReduceMotion];
            reduced.as_bool()
        }
    }

    pub fn set_traffic_lights_hidden(view: *mut c_void, hidden: bool) {
        let window = window(view);
        if window.is_null() {
            return;
        }
        for kind in [CLOSE, MINIATURIZE, ZOOM] {
            let button = button(window, kind);
            if !button.is_null() {
                unsafe {
                    let _: () = msg_send![button, setHidden: Bool::new(hidden)];
                }
            }
        }
    }

    /// Replaces `areas` with tracking areas covering `view` except `holes`
    /// (x, y, width, height; top-left origin), so the view gets mouse moves
    /// there even while a page (inside the hole) is first responder.
    pub fn track_mouse_outside(view: *mut c_void, holes: &[[f64; 4]], areas: &mut Vec<usize>) {
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        const ENTERED_AND_EXITED: usize = 0x01;
        const MOVED: usize = 0x02;
        const ACTIVE_IN_ACTIVE_APP: usize = 0x40;
        let view = view as *mut AnyObject;
        unsafe {
            for area in areas.drain(..) {
                let area = area as *mut AnyObject;
                let _: () = msg_send![view, removeTrackingArea: area];
                objc2::ffi::objc_release(area as *mut _);
            }
            let bounds: NSRect = msg_send![view, bounds];
            let flipped: Bool = msg_send![view, isFlipped];
            let (w, h) = (bounds.size.width, bounds.size.height);
            let rects = super::outside_regions(w, h, holes);
            for [rx, ry, rw, rh] in rects {
                if rw <= 0.0 || rh <= 0.0 {
                    continue;
                }
                let ry = if flipped.as_bool() { ry } else { h - ry - rh };
                let rect = NSRect::new(NSPoint::new(rx, ry), NSSize::new(rw, rh));
                let area: *mut AnyObject = msg_send![objc2::class!(NSTrackingArea), alloc];
                let nil: *mut AnyObject = std::ptr::null_mut();
                let area: *mut AnyObject = msg_send![
                    area,
                    initWithRect: rect,
                    options: ENTERED_AND_EXITED | MOVED | ACTIVE_IN_ACTIVE_APP,
                    owner: view,
                    userInfo: nil
                ];
                let _: () = msg_send![view, addTrackingArea: area];
                areas.push(area as usize);
            }
        }
    }

    pub fn attach_overlay(parent_view: *mut c_void, overlay_view: *mut c_void, traffic_lights: bool) {
        let (parent, overlay) = (window(parent_view), window(overlay_view));
        if parent.is_null() || overlay.is_null() {
            return;
        }
        set_traffic_lights_hidden(overlay_view, !traffic_lights);
        if traffic_lights {
            // The overlay's buttons act on the browser window it stands in for.
            let actions: [(usize, Sel); 3] = [
                (CLOSE, sel!(performClose:)),
                (MINIATURIZE, sel!(performMiniaturize:)),
                (ZOOM, sel!(performZoom:)),
            ];
            for (kind, action) in actions {
                let button = button(overlay, kind);
                if !button.is_null() {
                    unsafe {
                        let _: () = msg_send![button, setTarget: parent];
                        let _: () = msg_send![button, setAction: action];
                    }
                }
            }
        }
        unsafe {
            let _: () = msg_send![overlay, setHasShadow: Bool::NO];
            let _: () = msg_send![parent, addChildWindow: overlay, ordered: WINDOW_ABOVE];
        }
    }
}

/// Makes `overlay_view`'s window a borderless child of `parent_view`'s
/// window: it stays above it and moves with it. With `traffic_lights` the
/// overlay shows window buttons that act on the parent window.
pub fn attach_overlay(parent_view: *mut c_void, overlay_view: *mut c_void, traffic_lights: bool) {
    #[cfg(target_os = "macos")]
    mac::attach_overlay(parent_view, overlay_view, traffic_lights);
    #[cfg(not(target_os = "macos"))]
    let _ = (parent_view, overlay_view, traffic_lights);
}

/// Shows or hides the window's close/minimize/zoom buttons.
pub fn set_traffic_lights_hidden(view: *mut c_void, hidden: bool) {
    #[cfg(target_os = "macos")]
    mac::set_traffic_lights_hidden(view, hidden);
    #[cfg(not(target_os = "macos"))]
    let _ = (view, hidden);
}

/// Makes `view` receive mouse moves everywhere except over native page surfaces,
/// which the engine handles. `areas` keeps what was added, for replacing.
pub fn track_mouse_outside(view: *mut c_void, holes: &[[f64; 4]], areas: &mut Vec<usize>) {
    #[cfg(target_os = "macos")]
    mac::track_mouse_outside(view, holes, areas);
    #[cfg(not(target_os = "macos"))]
    let _ = (view, holes, areas);
}

#[cfg(any(target_os = "macos", test))]
fn outside_regions(width: f64, height: f64, holes: &[[f64; 4]]) -> Vec<[f64; 4]> {
    let mut regions = vec![[0., 0., width, height]];
    for &[hx, hy, hw, hh] in holes {
        let mut next = Vec::new();
        for [x, y, w, h] in regions {
            let (left, top) = (x.max(hx), y.max(hy));
            let (right, bottom) = ((x + w).min(hx + hw), (y + h).min(hy + hh));
            if right <= left || bottom <= top { next.push([x, y, w, h]); continue; }
            for rectangle in [[x, y, left - x, h], [right, y, x + w - right, h], [left, y, right - left, top - y], [left, bottom, right - left, y + h - bottom]] {
                if rectangle[2] > 0. && rectangle[3] > 0. { next.push(rectangle); }
            }
        }
        regions = next;
    }
    regions.retain(|rect| rect[2] > 0. && rect[3] > 0.);
    regions
}

#[cfg(test)]
mod tracking_tests {
    use super::*;
    #[test]
    fn split_headers_and_gaps_are_tracked_without_covering_any_native_page() {
        let holes = [[24., 8., 32., 28.], [60., 8., 32., 28.], [24., 44., 32., 28.], [60., 44., 32., 28.]];
        let regions = outside_regions(100., 80., &holes);
        let contains = |rect: &[f64; 4], x: f64, y: f64| x >= rect[0] && x < rect[0] + rect[2] && y >= rect[1] && y < rect[1] + rect[3];
        for y in 0..80 { for x in 0..100 {
            let (x, y) = (x as f64 + 0.5, y as f64 + 0.5);
            let covered = regions.iter().filter(|region| contains(region, x, y)).count();
            let native = holes.iter().any(|hole| contains(hole, x, y));
            assert_eq!(covered, usize::from(!native), "point ({x}, {y})");
        }}
    }
}


/// Platform motion preference, isolated from sidebar interaction logic.
pub fn prefers_reduced_motion() -> bool {
    #[cfg(all(target_os = "macos", not(test)))]
    { mac::prefers_reduced_motion() }
    #[cfg(any(not(target_os = "macos"), test))]
    { false }
}

/// App-wide visibility is a native menu convention supported by macOS.
/// GPUI's Windows implementation of Hide Others currently panics.
pub fn supports_app_visibility() -> bool { cfg!(target_os = "macos") }

pub enum AppVisibility { Hide, HideOthers, ShowAll }

pub fn set_app_visibility(action: AppVisibility, cx: &gpui::App) {
    if supports_app_visibility() {
        match action {
            AppVisibility::Hide => cx.hide(),
            AppVisibility::HideOthers => cx.hide_other_apps(),
            AppVisibility::ShowAll => cx.unhide_other_apps(),
        }
    }
}
