//! AppKit glue: CEF's NSApplication requirements and child NSView layout.

use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr};
use std::sync::OnceLock;

use objc2::ffi;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{msg_send, sel};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::Bounds;

thread_local! {
    static HANDLING_SEND_EVENT: Cell<bool> = const { Cell::new(false) };
}

type SendEvent = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);
static ORIGINAL_SEND_EVENT: OnceLock<SendEvent> = OnceLock::new();

#[cfg(target_arch = "aarch64")]
const BOOL_ENCODING: &str = "B";
#[cfg(not(target_arch = "aarch64"))]
const BOOL_ENCODING: &str = "c";

extern "C-unwind" fn is_handling_send_event(_: *mut AnyObject, _: Sel) -> Bool {
    Bool::new(HANDLING_SEND_EVENT.get())
}

extern "C-unwind" fn set_handling_send_event(_: *mut AnyObject, _: Sel, value: Bool) {
    HANDLING_SEND_EVENT.set(value.as_bool());
}

extern "C-unwind" fn send_event(this: *mut AnyObject, sel: Sel, event: *mut AnyObject) {
    let was_handling = HANDLING_SEND_EVENT.replace(true);
    if let Some(original) = ORIGINAL_SEND_EVENT.get() {
        unsafe { original(this, sel, event) };
    }
    HANDLING_SEND_EVENT.set(was_handling);
}

/// Chromium requires `NSApp` to implement `CrAppControlProtocol`. The
/// NSApplication subclass belongs to the UI toolkit, so the methods are
/// added to whatever class `NSApp` already is.
pub fn install_app_protocols() {
    unsafe {
        let ns_application = AnyClass::get(c"NSApplication").expect("AppKit is not loaded");
        let app: *mut AnyObject = msg_send![ns_application, sharedApplication];
        let class: *const AnyClass = msg_send![app, class];
        let class = class as *mut AnyClass;

        let original = (*class)
            .instance_method(sel!(sendEvent:))
            .expect("NSApplication has no sendEvent:")
            .implementation();
        let _ = ORIGINAL_SEND_EVENT.set(std::mem::transmute::<Imp, SendEvent>(original));

        let getter_types = format!("{BOOL_ENCODING}@:\0");
        let setter_types = format!("v@:{BOOL_ENCODING}\0");
        add_method(
            class,
            sel!(isHandlingSendEvent),
            std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel) -> Bool, Imp>(
                is_handling_send_event,
            ),
            &getter_types,
        );
        add_method(
            class,
            sel!(setHandlingSendEvent:),
            std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel, Bool), Imp>(
                set_handling_send_event,
            ),
            &setter_types,
        );
        // Overrides NSApplication's implementation; the original is called above.
        add_method(
            class,
            sel!(sendEvent:),
            std::mem::transmute::<SendEvent, Imp>(send_event),
            "v@:@\0",
        );

        for name in [c"CrAppProtocol", c"CrAppControlProtocol", c"CefAppProtocol"] {
            add_protocol(class, name);
        }
    }
}

unsafe fn add_method(class: *mut AnyClass, sel: Sel, imp: Imp, types: &str) {
    let added = unsafe { ffi::class_addMethod(class, sel, imp, types.as_ptr() as *const c_char) };
    if !added.as_bool() {
        // Already defined directly on the class: replace it.
        unsafe { ffi::class_replaceMethod(class, sel, imp, types.as_ptr() as *const c_char) };
    }
}

unsafe fn add_protocol(class: *mut AnyClass, name: &CStr) {
    unsafe {
        let protocol = ffi::objc_getProtocol(name.as_ptr());
        if !protocol.is_null() {
            ffi::class_addProtocol(class, protocol);
        }
    }
}

/// Whether the main run loop runs in AppKit's event tracking mode, i.e.
/// inside a mouse tracking loop such as a window resize.
pub fn is_tracking_events() -> bool {
    unsafe {
        let Some(class) = AnyClass::get(c"NSRunLoop") else {
            return false;
        };
        let run_loop: *mut AnyObject = msg_send![class, currentRunLoop];
        let mode: *mut AnyObject = msg_send![run_loop, currentMode];
        if mode.is_null() {
            return false;
        }
        let name: *const c_char = msg_send![mode, UTF8String];
        !name.is_null() && CStr::from_ptr(name) == c"NSEventTrackingRunLoopMode"
    }
}

/// Whether native events wait in the queue for the toolkit to dispatch.
pub fn has_pending_events() -> bool {
    const ANY: u64 = u64::MAX;
    unsafe {
        let (Some(app_class), Some(date_class), Some(string_class)) = (
            AnyClass::get(c"NSApplication"),
            AnyClass::get(c"NSDate"),
            AnyClass::get(c"NSString"),
        ) else {
            return false;
        };
        let app: *mut AnyObject = msg_send![app_class, sharedApplication];
        let past: *mut AnyObject = msg_send![date_class, distantPast];
        let mode: *mut AnyObject = msg_send![string_class, stringWithUTF8String: c"kCFRunLoopDefaultMode".as_ptr()];
        let event: *mut AnyObject = msg_send![
            app,
            nextEventMatchingMask: ANY,
            untilDate: past,
            inMode: mode,
            dequeue: Bool::NO
        ];
        !event.is_null()
    }
}

/// Positions `view` inside `parent` using top-left-origin coordinates.
pub fn set_frame(view: *mut c_void, parent: *mut c_void, frame: Bounds) {
    if view.is_null() || parent.is_null() {
        return;
    }
    unsafe {
        let view = view as *mut AnyObject;
        let parent = parent as *mut AnyObject;
        let flipped: Bool = msg_send![parent, isFlipped];
        let bounds: NSRect = msg_send![parent, bounds];
        let y = if flipped.as_bool() {
            frame.y
        } else {
            bounds.size.height - frame.y - frame.height
        };
        let rect = NSRect::new(NSPoint::new(frame.x, y), NSSize::new(frame.width, frame.height));
        let _: () = msg_send![view, setFrame: rect];
    }
}

pub fn set_hidden(view: *mut c_void, hidden: bool) {
    if view.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![view as *mut AnyObject, setHidden: Bool::new(hidden)];
    }
}

/// A plain view inside `parent` that clips its subviews to rounded corners.
/// Pages live in one, since AppKit manages the layers of CEF's own views.
/// The engine owns the returned reference until `release_clip_view`: the
/// window's superview may be destroyed before the engine is dropped.
pub fn create_clip_view(parent: *mut c_void, radius: f64) -> *mut c_void {
    unsafe {
        let class = AnyClass::get(c"NSView").expect("AppKit is not loaded");
        let view: *mut AnyObject = msg_send![class, alloc];
        let view: *mut AnyObject = msg_send![view, initWithFrame: NSRect::ZERO];
        let _: () = msg_send![view, setWantsLayer: Bool::YES];
        let layer: *mut AnyObject = msg_send![view, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: radius];
            let _: () = msg_send![layer, setMasksToBounds: Bool::YES];
        }
        let _: () = msg_send![parent as *mut AnyObject, addSubview: view];
        view as *mut c_void
    }
}

/// Moves the retained clip host, keeping CEF's browser and renderer alive.
pub fn move_clip_view(view: *mut c_void, parent: *mut c_void) {
    unsafe {
        let _: () = msg_send![view as *mut AnyObject, removeFromSuperview];
        let _: () = msg_send![parent as *mut AnyObject, addSubview: view as *mut AnyObject];
    }
}

/// The clip host is an unflipped NSView: minimum-Y corners are its bottom edge.
/// Keep those rounded when the page joins a GPUI split header along its top edge.
pub fn set_top_corners_rounded(view: *mut c_void, rounded: bool) {
    unsafe {
        let layer: *mut AnyObject = msg_send![view as *mut AnyObject, layer];
        if !layer.is_null() {
            let corners: usize = if rounded { 0b1111 } else { 0b0011 };
            let _: () = msg_send![layer, setMaskedCorners: corners];
        }
    }
}

/// Makes `view` fill its superview and follow its size.
pub fn fill_superview(view: *mut c_void) {
    const WIDTH_SIZABLE: usize = 1 << 1;
    const HEIGHT_SIZABLE: usize = 1 << 4;
    if view.is_null() {
        return;
    }
    unsafe {
        let view = view as *mut AnyObject;
        let superview: *mut AnyObject = msg_send![view, superview];
        if superview.is_null() {
            return;
        }
        let bounds: NSRect = msg_send![superview, bounds];
        let _: () = msg_send![view, setFrame: bounds];
        let _: () = msg_send![view, setAutoresizingMask: WIDTH_SIZABLE | HEIGHT_SIZABLE];
    }
}

pub fn release_clip_view(view: *mut c_void) {
    if !view.is_null() {
        unsafe {
            let _: () = msg_send![view as *mut AnyObject, removeFromSuperview];
            ffi::objc_release(view as *mut _);
        }
    }
}

/// Gives keyboard focus back to the toolkit's view (e.g. for a text field
/// drawn over the page).
pub fn make_first_responder(view: *mut c_void) {
    if view.is_null() {
        return;
    }
    unsafe {
        let view = view as *mut AnyObject;
        let window: *mut AnyObject = msg_send![view, window];
        if !window.is_null() {
            let _: Bool = msg_send![window, makeFirstResponder: view];
        }
    }
}

/// The window's content view that contains `view`. Pages are added there,
/// next to the toolkit's view: AppKit does not composite subviews of a
/// layer-hosting view such as GPUI's Metal view.
pub fn content_view_of(view: *mut c_void) -> *mut c_void {
    if view.is_null() {
        return view;
    }
    unsafe {
        let window: *mut AnyObject = msg_send![view as *mut AnyObject, window];
        if window.is_null() {
            return view;
        }
        let content: *mut AnyObject = msg_send![window, contentView];
        if content.is_null() { view } else { content as *mut c_void }
    }
}

/// CEF154 CefBrowserHostView::dealloc calls WindowDestroyed(). The owning
/// superview supplies the native child's strong reference; detach ONLY this
/// child, never the clip host or NSWindow. Do not retain the child ourselves:
/// that would prevent dealloc and therefore OnBeforeClose. The clip remains
/// retained by the runtime lease until the callback has returned.
pub(crate) fn destroy_browser_child(child: *mut c_void) {
    unsafe {
        let _: () = msg_send![child as *mut AnyObject, removeFromSuperview];
    }
}

/// Independently owned, never ordered front. The existing clip remains CEF's
/// original parent_view; only its superview changes. No GPUI owner is retained.
pub(crate) fn create_retired_window() -> Result<(*mut c_void, *mut c_void), String> {
    unsafe {
        let class = AnyClass::get(c"NSWindow").ok_or("AppKit NSWindow unavailable")?;
        let window: *mut AnyObject = msg_send![class, alloc];
        let rect = NSRect::new(NSPoint::new(0., 0.), NSSize::new(16., 16.));
        let window: *mut AnyObject = msg_send![window, initWithContentRect: rect, styleMask: 0usize, backing: 2usize, defer: Bool::NO];
        if window.is_null() { return Err("Hidden NSWindow creation failed".into()); }
        let _: () = msg_send![window, setReleasedWhenClosed: Bool::NO];
        let _: () = msg_send![window, setExcludedFromWindowsMenu: Bool::YES];
        let _: () = msg_send![window, orderOut: std::ptr::null_mut::<AnyObject>()];
        let content: *mut AnyObject = msg_send![window, contentView];
        if content.is_null() {
            ffi::objc_release(window);
            return Err("Hidden NSWindow has no content view".into());
        }
        Ok((window.cast(), content.cast()))
    }
}

pub(crate) fn clip_has_parent(clip: *mut c_void, parent: *mut c_void) -> bool {
    unsafe {
        let actual: *mut AnyObject = msg_send![clip as *mut AnyObject, superview];
        actual.cast::<c_void>() == parent
    }
}

pub(crate) fn release_retired_window(window: *mut c_void) {
    unsafe {
        let window = window as *mut AnyObject;
        let _: () = msg_send![window, close];
        ffi::objc_release(window);
    }
}
