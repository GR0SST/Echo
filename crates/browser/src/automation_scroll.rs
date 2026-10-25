//! A window-bound precise NSEvent for native automation only. No OS event tap
//! or private AppKit setter is needed; Chromium receives the regular event.
use objc2::{
    ffi, msg_send,
    runtime::{AnyClass, AnyObject, ClassBuilder, Sel},
    sel,
};
use objc2_foundation::NSPoint;
use std::ffi::c_void;
static WINDOW_KEY: u8 = 0;
static POINT_KEY: u8 = 1;
unsafe extern "C-unwind" fn event_window(event: *mut AnyObject, _sel: Sel) -> *mut AnyObject {
    unsafe {
        ffi::objc_getAssociatedObject(event, (&WINDOW_KEY as *const u8).cast()) as *mut AnyObject
    }
}
unsafe extern "C-unwind" fn event_number(event: *mut AnyObject, sel: Sel) -> isize {
    unsafe {
        let window = event_window(event, sel);
        if window.is_null() {
            0
        } else {
            msg_send![window, windowNumber]
        }
    }
}
unsafe extern "C-unwind" fn event_point(event: *mut AnyObject, _sel: Sel) -> NSPoint {
    unsafe {
        let value = ffi::objc_getAssociatedObject(event, (&POINT_KEY as *const u8).cast());
        msg_send![value, pointValue]
    }
}
pub(super) unsafe fn bind(event: *mut AnyObject, window: *mut AnyObject, point: NSPoint) -> bool {
    unsafe {
        if event.is_null() || window.is_null() {
            return false;
        }
        let base = (*event).class();
        let class = if let Some(class) = AnyClass::get(c"EchoAutomationPreciseScrollEvent") {
            class
        } else {
            let Some(mut builder) = ClassBuilder::new(c"EchoAutomationPreciseScrollEvent", base)
            else {
                return false;
            };
            builder.add_method(
                sel!(window),
                event_window as unsafe extern "C-unwind" fn(_, _) -> *mut AnyObject,
            );
            builder.add_method(
                sel!(windowNumber),
                event_number as unsafe extern "C-unwind" fn(_, _) -> isize,
            );
            builder.add_method(
                sel!(locationInWindow),
                event_point as unsafe extern "C-unwind" fn(_, _) -> NSPoint,
            );
            builder.register()
        };
        if class.superclass().map(|c| c as *const AnyClass) != Some(base as *const AnyClass) {
            return false;
        }
        let Some(values) = AnyClass::get(c"NSValue") else {
            return false;
        };
        let value: *mut AnyObject = msg_send![values,valueWithPoint:point];
        if value.is_null() {
            return false;
        }
        ffi::objc_setAssociatedObject(
            event,
            (&WINDOW_KEY as *const u8).cast::<c_void>(),
            window,
            ffi::OBJC_ASSOCIATION_RETAIN_NONATOMIC,
        );
        ffi::objc_setAssociatedObject(
            event,
            (&POINT_KEY as *const u8).cast::<c_void>(),
            value,
            ffi::OBJC_ASSOCIATION_RETAIN_NONATOMIC,
        );
        // No ivars are added: the concrete CG-backed NSEvent layout is unchanged.
        AnyObject::set_class(&*event, class);
        true
    }
}
