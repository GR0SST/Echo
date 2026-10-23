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

    /// Keep native controls on the same horizontal track as the GPUI sidebar.
    pub fn position_traffic_lights(view: *mut c_void, left: f32) {
        use objc2_foundation::{NSPoint, NSRect};
        let window = window(view);
        if window.is_null() { return; }
        unsafe {
            let buttons = [button(window, CLOSE), button(window, MINIATURIZE), button(window, ZOOM)];
            if buttons.iter().any(|button| button.is_null()) { return; }
            let frames: Vec<NSRect> = buttons.iter().map(|button| msg_send![*button, frame]).collect();
            let spacing = frames[2].origin.x - frames[1].origin.x;
            // Hide the group together: hiding individual AppKit buttons can
            // trigger titlebar layout between frame updates and alter spacing.
            let hidden = f64::from(left) + spacing * 2. + frames[2].size.width <= 0.;
            set_traffic_lights_hidden(view, hidden);
            for (index, button) in buttons.into_iter().enumerate() {
                let x = f64::from(left) + spacing * index as f64;
                let _: () = msg_send![button, setFrameOrigin: NSPoint::new(x, frames[index].origin.y)];
            }
        }
    }

    pub fn set_traffic_lights_alpha(view: *mut c_void, alpha: f32) {
        let window = window(view);
        if window.is_null() { return; }
        // Fade the buttons' shared container: changing the buttons themselves
        // makes AppKit lay them out again at their default position.
        let button = button(window, CLOSE);
        if button.is_null() { return; }
        unsafe {
            let container: *mut AnyObject = msg_send![button, superview];
            if !container.is_null() { let _: () = msg_send![container, setAlphaValue: f64::from(alpha)]; }
        }
    }

    pub fn traffic_light_frames(view: *mut c_void) -> Option<[[f32; 4]; 3]> {
        use objc2_foundation::NSRect;
        let window = window(view);
        if window.is_null() { return None; }
        unsafe {
            let content: *mut AnyObject = msg_send![window, contentView];
            let bounds: NSRect = msg_send![content, bounds];
            let mut frames = [[0.; 4]; 3];
            for (index, kind) in [CLOSE, MINIATURIZE, ZOOM].into_iter().enumerate() {
                let button = button(window, kind);
                if button.is_null() { return None; }
                let frame: NSRect = msg_send![button, bounds];
                let nil: *mut AnyObject = std::ptr::null_mut();
                let frame: NSRect = msg_send![button, convertRect: frame, toView: nil];
                frames[index] = [frame.origin.x as f32, (bounds.size.height - frame.origin.y - frame.size.height) as f32, frame.size.width as f32, frame.size.height as f32];
            }
            Some(frames)
        }
    }

    /// Places a rounded dark vibrancy view behind the GPUI content of
    /// `view`'s window at `rect` (x, y, width, height; top-left origin),
    /// creating it on first use. `None` removes it.
    pub fn set_backdrop(view: *mut c_void, rect: Option<[f64; 4]>, radius: f64) {
        use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
        const HUD_WINDOW: isize = 13;
        const BEHIND_WINDOW: isize = 0;
        const ACTIVE: isize = 1;
        let view = view as *mut AnyObject;
        if view.is_null() { return; }
        unsafe {
            let frame_view: *mut AnyObject = msg_send![view, superview];
            if frame_view.is_null() { return; }
            let identifier = NSString::from_str("echo.backdrop");
            let subviews: *mut AnyObject = msg_send![frame_view, subviews];
            let count: usize = msg_send![subviews, count];
            let mut backdrop: *mut AnyObject = std::ptr::null_mut();
            for index in 0..count {
                let subview: *mut AnyObject = msg_send![subviews, objectAtIndex: index];
                let id: *mut NSString = msg_send![subview, identifier];
                if !id.is_null() && (*id).to_string() == "echo.backdrop" { backdrop = subview; break; }
            }
            let Some([x, y, width, height]) = rect else {
                if !backdrop.is_null() { let _: () = msg_send![backdrop, removeFromSuperview]; }
                return;
            };
            if backdrop.is_null() {
                backdrop = msg_send![objc2::class!(NSVisualEffectView), alloc];
                backdrop = msg_send![backdrop, init];
                let _: () = msg_send![backdrop, setIdentifier: &*identifier];
                let _: () = msg_send![backdrop, setMaterial: HUD_WINDOW];
                let _: () = msg_send![backdrop, setBlendingMode: BEHIND_WINDOW];
                let _: () = msg_send![backdrop, setState: ACTIVE];
                let name = NSString::from_str("NSAppearanceNameVibrantDark");
                let appearance: *mut AnyObject = msg_send![objc2::class!(NSAppearance), appearanceNamed: &*name];
                let _: () = msg_send![backdrop, setAppearance: appearance];
                let _: () = msg_send![backdrop, setWantsLayer: Bool::YES];
                let below: isize = -1;
                let _: () = msg_send![frame_view, addSubview: backdrop, positioned: below, relativeTo: view];
                objc2::ffi::objc_release(backdrop as *mut _);
            }
            let bounds: NSRect = msg_send![frame_view, bounds];
            let flipped: Bool = msg_send![frame_view, isFlipped];
            let y = if flipped.as_bool() { y } else { bounds.size.height - y - height };
            let _: () = msg_send![backdrop, setFrame: NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))];
            let layer: *mut AnyObject = msg_send![backdrop, layer];
            if !layer.is_null() {
                let _: () = msg_send![layer, setCornerRadius: radius];
                let _: () = msg_send![layer, setMasksToBounds: Bool::YES];
            }
        }
    }

    pub fn replay_pointer(view: *mut c_void) {
        use objc2_foundation::NSPoint;
        const MOUSE_MOVED: usize = 5;
        let window = window(view);
        if window.is_null() { return; }
        unsafe {
            let location: NSPoint = msg_send![window, mouseLocationOutsideOfEventStream];
            let number: isize = msg_send![window, windowNumber];
            let nil: *mut AnyObject = std::ptr::null_mut();
            let event: *mut AnyObject = msg_send![
                objc2::class!(NSEvent),
                mouseEventWithType: MOUSE_MOVED,
                location: location,
                modifierFlags: 0usize,
                timestamp: 0.0f64,
                windowNumber: number,
                context: nil,
                eventNumber: 0isize,
                clickCount: 0isize,
                pressure: 0.0f32
            ];
            if event.is_null() { return; }
            // Deliver from the run loop, outside the toolkit's event handling.
            let _: () = msg_send![view as *mut AnyObject, performSelector: sel!(mouseMoved:), withObject: event, afterDelay: 0.0f64];
        }
    }

    pub fn pointer_position(view: *mut c_void) -> Option<[f32; 2]> {
        use objc2_foundation::{NSPoint, NSRect};
        let window = window(view);
        if window.is_null() { return None; }
        unsafe {
            let point: NSPoint = msg_send![window, mouseLocationOutsideOfEventStream];
            let content: *mut AnyObject = msg_send![window, contentView];
            let bounds: NSRect = msg_send![content, bounds];
            Some([point.x as f32, (bounds.size.height - point.y) as f32])
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

/// Position native window controls along the sidebar's horizontal track.
pub fn position_traffic_lights(view: *mut c_void, left: f32) {
    #[cfg(target_os = "macos")]
    mac::position_traffic_lights(view, left);
    #[cfg(not(target_os = "macos"))]
    let _ = (view, left);
}

/// Fades the native window controls without removing them from hit testing,
/// so a drawn idle state can show through until the pointer reaches them.
pub fn set_traffic_lights_alpha(view: *mut c_void, alpha: f32) {
    #[cfg(target_os = "macos")]
    mac::set_traffic_lights_alpha(view, alpha);
    #[cfg(not(target_os = "macos"))]
    let _ = (view, alpha);
}

/// Window control frames (x, y, width, height; top-left window coordinates).
pub fn traffic_light_frames(view: *mut c_void) -> Option<[[f32; 4]; 3]> {
    #[cfg(target_os = "macos")]
    { mac::traffic_light_frames(view) }
    #[cfg(not(target_os = "macos"))]
    { let _ = view; None }
}

/// A blurred, translucent backdrop behind a floating panel, as macOS
/// vibrancy. Returns whether the platform draws one; elsewhere panels stay
/// opaque.
pub fn set_backdrop(view: *mut c_void, rect: Option<[f64; 4]>, radius: f64) -> bool {
    #[cfg(target_os = "macos")]
    { mac::set_backdrop(view, rect, radius); true }
    #[cfg(not(target_os = "macos"))]
    { let _ = (view, rect, radius); false }
}

/// Whether panels get a translucent native backdrop.
pub const HAS_BACKDROP: bool = cfg!(target_os = "macos");

/// Sends the window's view a move to where the pointer is now, so the
/// toolkit refreshes hover after the pointer went over a native page.
pub fn replay_pointer(view: *mut c_void) {
    #[cfg(target_os = "macos")]
    mac::replay_pointer(view);
    #[cfg(not(target_os = "macos"))]
    let _ = view;
}

/// Pointer in window coordinates, including positions over native web pages.
pub fn pointer_position(view: *mut c_void) -> Option<[f32; 2]> {
    #[cfg(target_os = "macos")]
    { mac::pointer_position(view) }
    #[cfg(not(target_os = "macos"))]
    { let _ = view; None }
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
