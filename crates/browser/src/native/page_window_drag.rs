//! AppKit host for `crate::page_window_drag`: a local event monitor feeds
//! raw left-button events to the shared [`Press`] policy and performs its
//! decisions. Pages are CEF subviews of the browser window, so the monitor
//! sees their presses before the page does.
use crate::page_window_drag::{Press, Rect, Step};
use objc2::{msg_send, runtime::{AnyClass, AnyObject, Bool}};
use objc2_foundation::{NSPoint, NSRect};
use std::{cell::RefCell, collections::HashMap, ffi::c_void};

const LEFT_MOUSE_DOWN: usize = 1;
const LEFT_MOUSE_UP: usize = 2;
const LEFT_MOUSE_DRAGGED: usize = 6;
const CONTROL: usize = 1 << 18;

thread_local! {
    /// Regions by window number; window numbers are not reused while the
    /// window server session lives, unlike object addresses.
    static REGIONS: RefCell<HashMap<isize, Vec<Rect>>> = RefCell::new(HashMap::new());
    static PRESS: RefCell<Press> = RefCell::new(Press::default());
    static MONITOR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn set_regions(view: *mut c_void, regions: &[Rect]) {
    let window: *mut AnyObject = unsafe { msg_send![view as *mut AnyObject, window] };
    if window.is_null() { return; }
    let number: isize = unsafe { msg_send![window, windowNumber] };
    REGIONS.with(|all| {
        let mut all = all.borrow_mut();
        if regions.is_empty() { all.remove(&number); } else { all.insert(number, regions.to_vec()); }
    });
    if !regions.is_empty() && !MONITOR.with(|installed| installed.replace(true)) { install(); }
}

fn install() {
    let handler = block2::RcBlock::new(|event: *mut AnyObject| -> *mut AnyObject {
        if event.is_null() { return event; }
        if unsafe { handle(event) } { std::ptr::null_mut() } else { event }
    });
    let mask: u64 = (1 << LEFT_MOUSE_DOWN) | (1 << LEFT_MOUSE_UP) | (1 << LEFT_MOUSE_DRAGGED);
    unsafe {
        let class = AnyClass::get(c"NSEvent").expect("AppKit is loaded");
        let monitor: *mut AnyObject = msg_send![class, addLocalMonitorForEventsMatchingMask: mask, handler: &*handler];
        // The monitor lives as long as the process.
        let _: *mut AnyObject = msg_send![monitor, retain];
    }
}

/// Top-left point in the window content, matching GPUI's coordinates.
unsafe fn content_point(window: *mut AnyObject, event: *mut AnyObject) -> Option<(f64, f64)> {
    unsafe {
        let content: *mut AnyObject = msg_send![window, contentView];
        if content.is_null() { return None; }
        let location: NSPoint = msg_send![event, locationInWindow];
        let mut point: NSPoint = msg_send![content, convertPoint: location, fromView: std::ptr::null_mut::<AnyObject>()];
        let flipped: Bool = msg_send![content, isFlipped];
        if !flipped.as_bool() {
            let bounds: NSRect = msg_send![content, bounds];
            point.y = bounds.size.height - point.y;
        }
        Some((point.x, point.y))
    }
}

/// Returns true when the event must not be delivered.
unsafe fn handle(event: *mut AnyObject) -> bool {
    unsafe {
        let kind: usize = msg_send![event, type];
        let window: *mut AnyObject = msg_send![event, window];
        if window.is_null() { return false; }
        let step = match kind {
            LEFT_MOUSE_DOWN => {
                let number: isize = msg_send![window, windowNumber];
                let regions = REGIONS.with(|all| all.borrow().get(&number).cloned()).unwrap_or_default();
                let flags: usize = msg_send![event, modifierFlags];
                let Some(point) = content_point(window, event) else { return false; };
                PRESS.with(|press| press.borrow_mut().down(point, flags & CONTROL == 0, &regions))
            }
            LEFT_MOUSE_DRAGGED => {
                let Some(point) = content_point(window, event) else { return false; };
                PRESS.with(|press| press.borrow_mut().dragged(point))
            }
            LEFT_MOUSE_UP => PRESS.with(|press| press.borrow_mut().up()),
            _ => Step::Pass,
        };
        match step {
            Step::Pass => false,
            Step::Swallow => true,
            Step::StartWindowDrag => {
                // The page keeps a balanced press, as in the reference.
                if let Some(up) = mouse_up_like(event) { let _: () = msg_send![window, sendEvent: up]; }
                let _: () = msg_send![window, performWindowDragWithEvent: event];
                true
            }
        }
    }
}

unsafe fn mouse_up_like(event: *mut AnyObject) -> Option<*mut AnyObject> {
    unsafe {
        let location: NSPoint = msg_send![event, locationInWindow];
        let flags: usize = msg_send![event, modifierFlags];
        let timestamp: f64 = msg_send![event, timestamp];
        let number: isize = msg_send![event, windowNumber];
        let up: *mut AnyObject = msg_send![
            AnyClass::get(c"NSEvent")?,
            mouseEventWithType: LEFT_MOUSE_UP,
            location: location,
            modifierFlags: flags,
            timestamp: timestamp,
            windowNumber: number,
            context: std::ptr::null_mut::<AnyObject>(),
            eventNumber: 0isize,
            clickCount: 1isize,
            pressure: 0f32
        ];
        (!up.is_null()).then_some(up)
    }
}
