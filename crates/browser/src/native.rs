//! Native window glue the toolkit does not provide.

use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The root NSView, HWND or X11 XID. Wayland surfaces cannot parent a CEF
/// windowed browser. `None` on GPUI's
/// test platform, whose windows have no native counterpart.
pub fn view_of(window: &gpui::Window) -> Option<*mut c_void> {
    if cfg!(test) {
        return None;
    }
    let view = match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr(),
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as *mut c_void,
        RawWindowHandle::Xlib(handle) => handle.window as usize as *mut c_void,
        RawWindowHandle::Xcb(handle) => handle.window.get() as usize as *mut c_void,
        _ => return None,
    };
    let previous_scale = engine::native_parent_scale(view);
    engine::configure_native_parent(view, f64::from(window.scale_factor()));
    if previous_scale != engine::native_parent_scale(view) { sync_overlays(view); }
    Some(view)
}

/// Native minimized state, which GPUI does not currently expose.
pub(crate) fn window_minimized(window: &gpui::Window) -> bool {
    #[cfg(target_os = "macos")]
    { return scripting::state(view_of(window)).1; }
    #[cfg(not(target_os = "macos"))]
    { let _ = window; false }
}

/// GPUI's macOS Floating kind currently uses the normal window level. Keep
/// utility-panel stacking at the native seam; other backends own their kind.
pub(crate) fn float_utility_window(window: &gpui::Window) {
    #[cfg(target_os = "macos")]
    if let Some(view) = view_of(window) { mac::float_utility_window(view); }
    #[cfg(not(target_os = "macos"))]
    let _ = window;
}

/// Modifier state can change while a borderless child is being attached,
/// before that child has received its first flags-changed event. The macOS
/// query is process-independent AppKit state; other backends retain GPUI's
/// event-derived behavior. No native view exists on the test backend.
pub(crate) fn physical_control_is_held(window: &gpui::Window) -> Option<bool> {
    #[cfg(target_os = "macos")]
    if view_of(window).is_some() {
        let flags: usize = unsafe { objc2::msg_send![objc2::class!(NSEvent), modifierFlags] };
        return Some(flags & (1 << 18) != 0);
    }
    let _ = window;
    None
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    use objc2::runtime::{AnyObject, Bool, Sel};
    use objc2::{msg_send, sel};

    pub fn float_utility_window(view: *mut c_void) {
        unsafe {
            let window = window(view);
            let _: () = msg_send![window, setLevel: 3isize]; // NSFloatingWindowLevel
        }
    }

    #[repr(C)]
    struct SecTrust { _opaque: [u8; 0] }
    unsafe impl objc2::RefEncode for SecTrust {
        const ENCODING_REF: objc2::encode::Encoding = objc2::encode::Encoding::Pointer(
            &objc2::encode::Encoding::Struct("__SecTrust", &[]));
    }

    // Native certificate presentation is an OS sheet, matching the reference.
    // Its input is owned DER data; no CEF object escapes the engine crate.
    #[link(name = "SecurityInterface", kind = "framework")]
    unsafe extern "C" {}
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecCertificateCreateWithData(allocator: *const c_void, data: *const c_void) -> *const c_void;
        fn SecPolicyCreateSSL(server: u8, hostname: *const c_void) -> *const c_void;
        fn SecTrustCreateWithCertificates(certificates: *const c_void, policies: *const c_void, trust: *mut *const SecTrust) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" { fn CFRelease(value: *const c_void); }

    pub fn show_certificate(view: *mut c_void, chain: &[Vec<u8>], host: &str) -> bool {
        use objc2::runtime::AnyClass;
        use objc2_foundation::NSString;
        if chain.is_empty() || host.is_empty() { return false; }
        unsafe {
            let owner = window(view);
            if owner.is_null() { return false; }
            let Some(panel_class) = AnyClass::get(c"SFCertificatePanel") else { return false; };
            let panel: *mut AnyObject = msg_send![panel_class, sharedCertificatePanel];
            if panel.is_null() { return false; }
            let attached: *mut AnyObject = msg_send![panel, sheetParent];
            if !attached.is_null() { return false; }
            let array: *mut AnyObject = msg_send![AnyClass::get(c"NSMutableArray").expect("Foundation is loaded"), array];
            for der in chain {
                let data: *mut AnyObject = msg_send![AnyClass::get(c"NSData").expect("Foundation is loaded"), dataWithBytes: der.as_ptr().cast::<c_void>() length: der.len()];
                let certificate = SecCertificateCreateWithData(std::ptr::null(), data.cast());
                if certificate.is_null() { return false; }
                let _: () = msg_send![array, addObject: certificate as *mut AnyObject];
                CFRelease(certificate);
            }
            let name = NSString::from_str(host);
            let policy = SecPolicyCreateSSL(1, (&*name as *const NSString).cast());
            if policy.is_null() { return false; }
            let mut trust = std::ptr::null();
            let result = SecTrustCreateWithCertificates(array.cast(), policy, &mut trust);
            CFRelease(policy);
            if result != 0 || trust.is_null() { return false; }
            let _: () = msg_send![panel,
                beginSheetForWindow: owner
                modalDelegate: std::ptr::null_mut::<AnyObject>()
                didEndSelector: None::<Sel>
                contextInfo: std::ptr::null_mut::<c_void>()
                trust: trust
                showGroup: Bool::YES];
            CFRelease(trust.cast());
            true
        }
    }

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

    /// CEF consumes page input before GPUI's cached pointer can be updated.
    pub fn pointer_in_view(view: *mut c_void) -> Option<(f32, f32)> {
        use objc2_foundation::{NSPoint, NSRect};
        unsafe {
            let owner = window(view);
            if owner.is_null() { return None; }
            let position: NSPoint = msg_send![owner, mouseLocationOutsideOfEventStream];
            let mut local: NSPoint = msg_send![view as *mut AnyObject, convertPoint: position fromView: std::ptr::null_mut::<AnyObject>()];
            let flipped: Bool = msg_send![view as *mut AnyObject, isFlipped];
            if !flipped.as_bool() {
                let bounds: NSRect = msg_send![view as *mut AnyObject, bounds];
                local.y = bounds.size.height - local.y;
            }
            (local.x.is_finite() && local.y.is_finite()).then_some((local.x as f32, local.y as f32))
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

    // AppKit raw enum values are isolated here, never in shared theme logic.
    fn effect_material(material: crate::material::EffectMaterial) -> isize {
        use crate::material::EffectMaterial::*;
        match material { Hud => 13, Menu => 5, Popover => 6, WindowBackground => 12,
            ReferenceLight => 1, ReferenceDark => 2 }
    }

    struct BoundedEffect {
        view: *mut AnyObject,
        parent: *mut AnyObject,
        effect: *mut AnyObject,
        identifier: String,
        above_content: bool,
    }

    impl super::BackdropHost for BoundedEffect {
        fn exists(&self) -> bool { !self.effect.is_null() }

        fn install(&mut self) -> bool {
            use objc2_foundation::NSString;
            unsafe {
                let allocated: *mut AnyObject = msg_send![objc2::class!(NSVisualEffectView), alloc];
                if allocated.is_null() { return false; }
                let effect: *mut AnyObject = msg_send![allocated, init];
                if effect.is_null() { return false; }
                let identifier = NSString::from_str(&self.identifier);
                let _: () = msg_send![effect, setIdentifier: &*identifier];
                let relative: *mut AnyObject = if self.above_content { std::ptr::null_mut() } else { self.view };
                let order = if self.above_content { 1isize } else { -1isize };
                let _: () = msg_send![self.parent, addSubview: effect, positioned: order, relativeTo: relative];
                let parent: *mut AnyObject = msg_send![effect, superview];
                let installed = parent == self.parent;
                if installed { self.effect = effect; }
                objc2::ffi::objc_release(effect as *mut _);
                installed
            }
        }

        fn update(&mut self, rect: [f64; 4], radius: f64, config: crate::material::BackdropConfig) -> bool {
            use crate::material::{EffectAppearance, EffectBlending, EffectState};
            use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
            unsafe {
                if self.effect.is_null() { return false; }
                let parent: *mut AnyObject = msg_send![self.effect, superview];
                if parent != self.parent { return false; }
                if self.above_content {
                    let subviews: *mut AnyObject = msg_send![self.parent, subviews];
                    let last: *mut AnyObject = msg_send![subviews, lastObject];
                    if last != self.effect {
                        let relative: *mut AnyObject = std::ptr::null_mut();
                        let _: () = msg_send![self.parent, addSubview: self.effect, positioned: 1isize, relativeTo: relative];
                    }
                }
                let nil: *mut AnyObject = std::ptr::null_mut();
                let appearance = match config.appearance {
                    EffectAppearance::Inherited => nil,
                    explicit => {
                        let name = NSString::from_str(match explicit {
                            EffectAppearance::Light => "NSAppearanceNameVibrantLight",
                            EffectAppearance::Dark => "NSAppearanceNameVibrantDark",
                            EffectAppearance::Inherited => unreachable!(),
                        });
                        let value: *mut AnyObject = msg_send![objc2::class!(NSAppearance), appearanceNamed: &*name];
                        if value.is_null() { return false; }
                        value
                    }
                };
                // Apply every parameter on reused views too. nil clears an old
                // explicit override, allowing effective appearance inheritance.
                let _: () = msg_send![self.effect, setAppearance: appearance];
                let _: () = msg_send![self.effect, setMaterial: effect_material(config.material)];
                let state: isize = match config.state { EffectState::FollowWindow => 0, EffectState::Active => 1, EffectState::Inactive => 2 };
                let blending: isize = match config.blending { EffectBlending::BehindWindow => 0, EffectBlending::WithinWindow => 1 };
                let _: () = msg_send![self.effect, setState: state];
                let _: () = msg_send![self.effect, setBlendingMode: blending];
                let _: () = msg_send![self.effect, setEmphasized: Bool::new(config.emphasized)];
                let _: () = msg_send![self.effect, setAlphaValue: config.effect_alpha];
                let _: () = msg_send![self.effect, setWantsLayer: Bool::YES];
                let [x, y, width, height] = rect;
                let bounds: NSRect = msg_send![self.parent, bounds];
                let flipped: Bool = msg_send![self.parent, isFlipped];
                let y = if flipped.as_bool() { y } else { bounds.size.height - y - height };
                let _: () = msg_send![self.effect, setFrame: NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))];
                let layer: *mut AnyObject = msg_send![self.effect, layer];
                if layer.is_null() { return false; }
                let _: () = msg_send![layer, setCornerRadius: radius];
                let _: () = msg_send![layer, setMasksToBounds: Bool::YES];
                true
            }
        }

        fn remove(&mut self) -> bool {
            unsafe {
                if self.effect.is_null() { return true; }
                let _: () = msg_send![self.effect, removeFromSuperview];
                // Removing may release the last owner; never dereference again.
                self.effect = std::ptr::null_mut();
                true
            }
        }
    }

    pub fn set_backdrop_configured(view: *mut c_void, key: &str, rect: Option<[f64; 4]>, radius: f64, config: crate::material::BackdropConfig, above_content: bool) -> crate::material::BackdropInstallation {
        use crate::material::BackdropInstallation;
        use objc2_foundation::NSString;
        let view = view as *mut AnyObject;
        if view.is_null() { return BackdropInstallation::Unavailable; }
        unsafe {
            let parent: *mut AnyObject = msg_send![view, superview];
            if parent.is_null() { return BackdropInstallation::Unavailable; }
            let identifier = format!("echo.backdrop.{key}");
            let subviews: *mut AnyObject = msg_send![parent, subviews];
            let count: usize = msg_send![subviews, count];
            let mut effect = std::ptr::null_mut();
            for index in 0..count {
                let subview: *mut AnyObject = msg_send![subviews, objectAtIndex: index];
                let id: *mut NSString = msg_send![subview, identifier];
                if !id.is_null() && (*id).to_string() == identifier {
                    let is_effect: Bool = msg_send![subview, isKindOfClass: objc2::class!(NSVisualEffectView)];
                    if !is_effect.as_bool() { return BackdropInstallation::Unavailable; }
                    effect = subview;
                    break;
                }
            }
            let mut host = BoundedEffect { view, parent, effect, identifier, above_content };
            super::reconcile_backdrop(&mut host, rect, radius, config)
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

    pub fn prepare_toast_window(view: *mut c_void) {
        let overlay = window(view);
        if !overlay.is_null() {
            unsafe {
                // GPUI creates titled NSWindows even without a titlebar. A
                // padded transient surface must not expose AppKit's frame rim.
                let _: () = msg_send![overlay, setStyleMask: 0usize];
                let _: () = msg_send![overlay, setHasShadow: Bool::YES];
            }
        }
    }

    pub fn set_overlay_passthrough(view: *mut c_void) {
        let overlay = window(view);
        if !overlay.is_null() {
            unsafe { let _: () = msg_send![overlay, setIgnoresMouseEvents: Bool::YES]; }
        }
    }
}

/// Borderless toast windows use a shadow that follows the rendered surface.
pub fn prepare_toast_window(view: *mut c_void) {
    #[cfg(target_os = "macos")]
    mac::prepare_toast_window(view);
    #[cfg(not(target_os = "macos"))]
    let _ = view;
}

/// Passive overlays must let the page receive pointer events through them.
pub fn set_overlay_passthrough(view: *mut c_void) {
    #[cfg(target_os = "macos")]
    mac::set_overlay_passthrough(view);
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    desktop::passthrough(view);
}

/// Makes `overlay_view`'s window a borderless child of `parent_view`'s
/// window: it stays above it and moves with it. With `traffic_lights` the
/// overlay shows window buttons that act on the parent window.
pub fn attach_overlay(parent_view: *mut c_void, overlay_view: *mut c_void, traffic_lights: bool) {
    #[cfg(target_os = "macos")]
    mac::attach_overlay(parent_view, overlay_view, traffic_lights);
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    { desktop::attach(parent_view, overlay_view); let _ = traffic_lights; }
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

/// Legacy HUD and menu roles. Existing callers retain dark, always-active,
/// behind-window effects; no authored tint or foreground values change.
#[derive(Clone, Copy)]
pub enum Material { Hud, Menu }

impl Material {
    fn legacy_config(self) -> crate::material::BackdropConfig {
        use crate::material::*;
        BackdropConfig { appearance: EffectAppearance::Dark, state: EffectState::Active,
            blending: EffectBlending::BehindWindow, emphasized: false, effect_alpha: 1.,
            material: match self { Self::Hud => EffectMaterial::Hud, Self::Menu => EffectMaterial::Menu } }
    }
}

/// Compatibility wrapper: true only when the named effect is installed and
/// configured. Not proof of rendered blur. Removal/absence/failure returns false.
pub fn set_backdrop(view: *mut c_void, key: &str, rect: Option<[f64; 4]>, radius: f64, material: Material) -> bool {
    set_backdrop_configured(view, key, rect, radius, material.legacy_config()) == crate::material::BackdropInstallation::Installed
}

/// Bounded effect only; no window/content alpha or palette changes. Geometry
/// uses top-left coordinates. None removes only the effect named by key.
/// Installed means an attached/configured effect view, not verified blur output.
pub(crate) fn set_backdrop_configured(view: *mut c_void, key: &str, rect: Option<[f64; 4]>, radius: f64, config: crate::material::BackdropConfig) -> crate::material::BackdropInstallation {
    #[cfg(all(target_os = "macos", not(test)))]
    { mac::set_backdrop_configured(view, key, rect, radius, config, false) }
    #[cfg(any(not(target_os = "macos"), test))]
    { let _ = (view, key, rect, radius, config); crate::material::BackdropInstallation::Unavailable }
}

/// A bounded material above embedded content, below a separate GPUI overlay.
/// WithinWindow blending can sample the live document without grouping the
/// sampling view into the transparent child window that owns the controls.
pub(crate) fn set_content_backdrop(view: *mut c_void, key: &str, rect: Option<[f64; 4]>, radius: f64, config: crate::material::BackdropConfig) -> crate::material::BackdropInstallation {
    #[cfg(all(target_os = "macos", not(test)))]
    { mac::set_backdrop_configured(view, key, rect, radius, config, true) }
    #[cfg(any(not(target_os = "macos"), test))]
    { let _ = (view, key, rect, radius, config); crate::material::BackdropInstallation::Unavailable }
}

// The reconciliation seam is also exercised with an in-memory host: native
// setters must not accidentally be confined to the initial creation path.
trait BackdropHost {
    fn exists(&self) -> bool;
    fn install(&mut self) -> bool;
    fn update(&mut self, rect: [f64; 4], radius: f64, config: crate::material::BackdropConfig) -> bool;
    fn remove(&mut self) -> bool;
}

fn reconcile_backdrop(host: &mut impl BackdropHost, rect: Option<[f64; 4]>, radius: f64, config: crate::material::BackdropConfig) -> crate::material::BackdropInstallation {
    use crate::material::BackdropInstallation::*;
    let Some(rect) = rect else {
        return if !host.exists() { Absent } else if host.remove() { Removed } else { Unavailable };
    };
    if !config.is_valid() || !radius.is_finite() || radius < 0. || rect.iter().any(|v| !v.is_finite()) || rect[2] <= 0. || rect[3] <= 0. {
        return InvalidRequest;
    }
    if !host.exists() && !host.install() { return Unavailable; }
    if !host.exists() { return Unavailable; }
    if host.update(rect, radius, config) { Installed } else {
        host.remove();
        Unavailable
    }
}

/// Legacy compile-time hint for existing palette callers. Not runtime effect
/// installation status, compositor support, or evidence of rendered blur.
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

#[cfg(all(target_os = "macos", not(test)))]
pub(crate) mod termination;

#[cfg(test)]
mod backdrop_lifecycle_tests {
    use super::*;
    use crate::material::*;

    #[derive(Default)]
    struct MemoryHost {
        installed: bool,
        creates: usize,
        removes: usize,
        fail_install: bool,
        fail_update: bool,
        applied: Vec<([f64; 4], f64, BackdropConfig)>,
    }
    impl BackdropHost for MemoryHost {
        fn exists(&self) -> bool { self.installed }
        fn install(&mut self) -> bool {
            self.creates += 1;
            self.installed = !self.fail_install;
            self.installed
        }
        fn update(&mut self, rect: [f64; 4], radius: f64, config: BackdropConfig) -> bool {
            self.applied.push((rect, radius, config));
            !self.fail_update
        }
        fn remove(&mut self) -> bool {
            self.removes += 1;
            self.installed = false;
            true
        }
    }

    #[test]
    fn reusing_a_key_applies_new_parameters_and_clears_explicit_appearance() {
        let mut host = MemoryHost::default();
        let rect = [5., 6., 120., 80.];
        let legacy = Material::Hud.legacy_config();
        assert_eq!(reconcile_backdrop(&mut host, Some(rect), 12., legacy), BackdropInstallation::Installed);
        let reference = BackdropConfig { appearance: EffectAppearance::Light,
            state: EffectState::FollowWindow, blending: EffectBlending::WithinWindow,
            emphasized: true, effect_alpha: 0.37, material: EffectMaterial::WindowBackground };
        assert_eq!(reconcile_backdrop(&mut host, Some(rect), 10., reference), BackdropInstallation::Installed);
        let inherited = BackdropConfig { appearance: EffectAppearance::Inherited,
            state: EffectState::Inactive, material: EffectMaterial::ReferenceDark, ..reference };
        assert_eq!(reconcile_backdrop(&mut host, Some(rect), 10., inherited), BackdropInstallation::Installed);
        assert_eq!(host.creates, 1);
        assert_eq!(host.applied, vec![(rect, 12., legacy), (rect, 10., reference), (rect, 10., inherited)]);
        assert_eq!(host.removes, 0);
    }

    #[test]
    fn removal_is_idempotent_and_reopening_creates_a_fresh_effect() {
        let mut host = MemoryHost::default();
        let config = Material::Menu.legacy_config();
        assert_eq!(reconcile_backdrop(&mut host, None, 10., config), BackdropInstallation::Absent);
        let rect = Some([0., 0., 120., 80.]);
        assert_eq!(reconcile_backdrop(&mut host, rect, 10., config), BackdropInstallation::Installed);
        assert_eq!(reconcile_backdrop(&mut host, None, 10., config), BackdropInstallation::Removed);
        assert_eq!(reconcile_backdrop(&mut host, None, 10., config), BackdropInstallation::Absent);
        assert_eq!((host.creates, host.removes), (1, 1));
        assert_eq!(reconcile_backdrop(&mut host, rect, 10., config), BackdropInstallation::Installed);
        assert_eq!(host.creates, 2);
    }

    #[test]
    fn failed_install_or_update_never_reports_installed_and_cleans_stale_effect() {
        let rect = Some([0., 0., 120., 80.]);
        let config = Material::Menu.legacy_config();
        let mut host = MemoryHost { fail_install: true, ..MemoryHost::default() };
        assert_eq!(reconcile_backdrop(&mut host, rect, 10., config), BackdropInstallation::Unavailable);
        assert!(host.applied.is_empty());
        host.fail_install = false;
        assert_eq!(reconcile_backdrop(&mut host, rect, 10., config), BackdropInstallation::Installed);
        host.fail_update = true;
        assert_eq!(reconcile_backdrop(&mut host, rect, 10., config), BackdropInstallation::Unavailable);
        assert!(!host.installed);
        assert_eq!(host.removes, 1);
    }

    #[test]
    fn invalid_request_does_not_replace_the_existing_authored_effect() {
        let mut host = MemoryHost::default();
        let config = Material::Hud.legacy_config();
        let rect = Some([0., 0., 120., 80.]);
        reconcile_backdrop(&mut host, rect, 10., config);
        for alpha in [f64::NAN, -0.1, 1.1] {
            let invalid = BackdropConfig { effect_alpha: alpha, ..config };
            assert_eq!(reconcile_backdrop(&mut host, rect, 10., invalid), BackdropInstallation::InvalidRequest);
        }
        assert!(host.installed);
        assert_eq!((host.creates, host.removes, host.applied.len()), (1, 0, 1));
    }
}

/// Current native page pointer, in GPUI view coordinates. Headless and other
/// platforms use the toolkit's pointer until their native bridge supplies one.
pub(crate) fn pointer_in_view(view: *mut c_void) -> Option<(f32, f32)> {
    #[cfg(all(target_os = "macos", not(test)))]
    { mac::pointer_in_view(view) }
    #[cfg(any(not(target_os = "macos"), test))]
    { let _ = view; None }
}

/// Platform-owned sharing destination menu.
pub(crate) mod sharing;

pub(crate) mod link_source;
/// Displays the platform certificate sheet without retaining engine handles.
pub(crate) fn show_certificate(view: *mut c_void, chain: &[Vec<u8>], host: &str) -> bool {
    #[cfg(all(target_os = "macos", not(test)))]
    { mac::show_certificate(view, chain, host) }
    #[cfg(any(not(target_os = "macos"), test))]
    { let _ = (view, chain, host); false }
}

pub(crate) mod overlay_image;

/// Current native modifier state, including flagsChanged events consumed by an
/// embedded page before GPUI/CEF's ordinary key callbacks. None uses the
/// backend's normal key-event state; the test platform never reads the OS.
pub(crate) fn shift_pressed() -> Option<bool> {
    #[cfg(all(target_os = "macos", not(test)))]
    unsafe {
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" { fn CGEventSourceFlagsState(state: i32) -> u64; }
        Some(CGEventSourceFlagsState(0) & (1 << 17) != 0)
    }
    #[cfg(any(not(target_os = "macos"), test))]
    { None }
}

pub(crate) fn application_active() -> bool {
    #[cfg(all(target_os = "macos", not(test)))]
    unsafe {
        use objc2::{msg_send, runtime::{AnyClass,AnyObject,Bool}};
        let Some(class)=AnyClass::get(c"NSApplication") else{return false;};
        let app:*mut AnyObject=msg_send![class,sharedApplication];
        let active:Bool=msg_send![app,isActive];active.as_bool()
    }
    #[cfg(any(not(target_os = "macos"), test))]
    { true }
}
#[cfg(target_os = "macos")]
pub(crate) mod scripting;

/// The toolkit's menu model does not yet expose checked items. Keep the
/// platform's native checkmark presentation at this isolated boundary.
pub(crate) fn set_appearance_menu_selection(selected: usize) {
    #[cfg(all(target_os = "macos", not(test)))]
    menu_appearance::set_selected(selected);
    #[cfg(any(not(target_os = "macos"), test))]
    let _ = selected;
}
#[cfg(all(target_os = "macos", not(test)))]
#[path = "native/menu_appearance.rs"]
mod menu_appearance;

pub(crate) fn set_menu_minimum_width(title: &str, width: f64) {
    #[cfg(all(target_os = "macos", not(test)))]
    menu_appearance::set_minimum_width(title, width);
    #[cfg(any(not(target_os = "macos"), test))]
    let _ = (title, width);
}

/// Alignment feedback at an accepted reorder boundary; other platforms are silent.
pub(crate) fn reorder_haptic() {
    #[cfg(test)] { HAPTIC_PULSES.with(|count|count.set(count.get()+1)); return; }
    #[cfg(all(target_os="macos",not(test)))] unsafe {
        use objc2::{msg_send,runtime::{AnyClass,AnyObject}};
        let Some(class)=AnyClass::get(c"NSHapticFeedbackManager")else{return;};
        let performer:*mut AnyObject=msg_send![class,defaultPerformer];
        if !performer.is_null(){
            let _:()=msg_send![performer,performFeedbackPattern:1isize,performanceTime:0usize];
            if std::env::var_os("BROWSER_SCRIPT").is_some(){eprintln!("[reorder-haptic] Alignment");}
        }
    }
}
#[cfg(test)] thread_local!{pub(crate) static HAPTIC_PULSES:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}

/// OS-owned diagnostic text behind the shared utility interface.
pub(crate) fn operating_system_version() -> String {
    #[cfg(all(target_os = "macos", not(test)))]
    unsafe {
        use objc2::{msg_send, runtime::AnyObject};
        let info: *mut AnyObject = msg_send![objc2::class!(NSProcessInfo), processInfo];
        let version: *mut AnyObject = msg_send![info, operatingSystemVersionString];
        if !version.is_null() {
            let text: *const std::ffi::c_char = msg_send![version, UTF8String];
            if !text.is_null() { return std::ffi::CStr::from_ptr(text).to_string_lossy().into_owned(); }
        }
    }
    std::env::consts::OS.to_owned()
}

/// Standard text commands remain in the platform responder chain; native menus
/// own enablement and checked state rather than advertising unsupported edits.
pub(crate) fn configure_standard_edit_menu() {
    #[cfg(all(target_os = "macos", not(test)))]
    standard_edit::configure();
}

pub(crate) fn perform_standard_edit(command: crate::standard_edit::Command, view: Option<*mut c_void>) -> bool {
    #[cfg(test)] { let _ = (command, view); true }
    #[cfg(all(target_os = "macos", not(test)))] {
        view.is_some_and(|view| standard_edit::perform(command, view))
    }
    #[cfg(all(not(target_os = "macos"), not(test)))] { let _ = (command, view); false }
}

#[cfg(all(target_os = "macos", not(test)))]
#[path = "native/standard_edit.rs"]
mod standard_edit;

/// Run before GPUI/CEF creates threads. Windowed CEF uses an X11 parent XID;
/// a Wayland wl_surface is not interchangeable with it. XWayland is supported.
pub(crate) fn prepare_windowing() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    desktop::prepare_windowing()?;
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("DISPLAY").is_none_or(|value| value.is_empty()) {
            return Err("Echo's windowed CEF embedding requires X11 or XWayland (DISPLAY); native Wayland embedding is unavailable".into());
        }
        // GPUI 0.2 chooses Wayland whenever WAYLAND_DISPLAY is present. Select
        // its X11 backend before Application::new and before spawning workers.
        unsafe { std::env::remove_var("WAYLAND_DISPLAY"); }
    }
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "linux", test))]
mod overlay_geometry;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod desktop;
pub(crate) fn sync_overlays(parent: *mut c_void) {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    desktop::sync(parent);
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let _ = parent;
}
pub(crate) fn forget_overlay(child: *mut c_void) {
    engine::forget_native_parent(child);
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    desktop::forget(child);
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let _ = child;
}

pub(crate) fn attach_extension_view(parent: usize, view: usize, width: i32, height: i32) {
    #[cfg(all(target_os="macos",not(test)))]
    if parent != 0 && view != 0 { unsafe {
        let parent=parent as *mut objc2::runtime::AnyObject;
        let view=view as *mut objc2::runtime::AnyObject;
        let _: ()=objc2::msg_send![parent,addSubview:view];
        let _: ()=objc2::msg_send![view,setFrame:objc2_foundation::NSRect::new(objc2_foundation::NSPoint::new(0.,0.),objc2_foundation::NSSize::new(width as f64,height as f64))];
        let _: ()=objc2::msg_send![view,setAutoresizingMask:18usize];
        let window:*mut objc2::runtime::AnyObject=objc2::msg_send![parent,window];
        let _: bool=objc2::msg_send![window,makeFirstResponder:view];
    }}
    let _=(parent,view,width,height);
}
pub(crate) fn detach_extension_view(view:usize) {
    #[cfg(all(target_os="macos",not(test)))]
    if view != 0 {unsafe{let _: ()=objc2::msg_send![view as *mut objc2::runtime::AnyObject,removeFromSuperview];}}
    let _=view;
}

/// GPUI exposes resize but no programmatic move. Keep a borderless action
/// popup anchored while its extension changes the preferred content size.
pub(crate) fn set_popup_bounds(window: &mut gpui::Window, bounds: gpui::Bounds<gpui::Pixels>) {
    let previous = window.bounds();
    let Some(view) = view_of(window) else { window.resize(bounds.size); return; };
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2::{msg_send, runtime::AnyObject};
        use objc2_foundation::{NSPoint,NSRect,NSSize};
        let native: *mut AnyObject = msg_send![view as *mut AnyObject, window];
        if native.is_null() {return;}
        let frame: NSRect = msg_send![native, frame];
        let width = f32::from(bounds.size.width) as f64;
        let height = f32::from(bounds.size.height) as f64;
        let x = frame.origin.x + f32::from(bounds.left()-previous.left()) as f64;
        let y = frame.origin.y + frame.size.height - height - f32::from(bounds.top()-previous.top()) as f64;
        let _: () = msg_send![native, setFrame:NSRect::new(NSPoint::new(x,y),NSSize::new(width,height)), display:true];
    }
    #[cfg(any(target_os = "windows",target_os = "linux"))]
    desktop::set_popup_bounds(view, previous, bounds);
    #[cfg(not(any(target_os = "macos",target_os = "windows",target_os = "linux")))]
    {let _=(view,previous);window.resize(bounds.size);}
}
