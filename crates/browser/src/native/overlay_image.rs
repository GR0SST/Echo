//! Snapshot only two explicitly owned windows, cropped to the child panel.
//! Other platform backends can supply the same operation without UI changes.
use std::ffi::c_void;
pub(crate) fn capture(parent: *mut c_void, child: *mut c_void) -> Option<Vec<u8>> {
    #[cfg(all(target_os = "macos", not(test)))]
    unsafe {
        return mac::capture(parent, child);
    }
    #[cfg(any(not(target_os = "macos"), test))]
    {
        let _ = (parent, child);
        None
    }
}
#[cfg(all(target_os = "macos", not(test)))]
mod mac {
    use super::*;
    use objc2::{msg_send, runtime::AnyObject};
    use objc2_foundation::{NSPoint, NSRect, NSString};
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowListCreateImageFromArray(
            bounds: NSRect,
            windows: *const c_void,
            options: u32,
        ) -> *mut c_void;
        fn CGImageRelease(image: *mut c_void);
        fn CGMainDisplayID() -> u32;
        fn CGDisplayPixelsHigh(display: u32) -> usize;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayCreate(
            a: *const c_void,
            v: *const *const c_void,
            n: isize,
            c: *const c_void,
        ) -> *const c_void;
        fn CFDataCreateMutable(a: *const c_void, capacity: isize) -> *mut c_void;
        fn CFDataGetLength(d: *const c_void) -> isize;
        fn CFDataGetBytePtr(d: *const c_void) -> *const u8;
        fn CFRelease(o: *const c_void);
    }
    #[link(name = "ImageIO", kind = "framework")]
    unsafe extern "C" {
        fn CGImageDestinationCreateWithData(
            d: *mut c_void,
            t: *const c_void,
            n: usize,
            o: *const c_void,
        ) -> *mut c_void;
        fn CGImageDestinationAddImage(d: *mut c_void, i: *mut c_void, p: *const c_void);
        fn CGImageDestinationFinalize(d: *mut c_void) -> bool;
    }
    pub(super) unsafe fn capture(parent: *mut c_void, child: *mut c_void) -> Option<Vec<u8>> {
        unsafe {
            if parent.is_null() || child.is_null() {
                return None;
            }
            let p: *mut AnyObject = msg_send![parent as *mut AnyObject, window];
            let c: *mut AnyObject = msg_send![child as *mut AnyObject, window];
            if p.is_null() || c.is_null() {
                return None;
            }
            let actual: *mut AnyObject = msg_send![c, parentWindow];
            if actual != p {
                return None;
            }
            let pn: isize = msg_send![p, windowNumber];
            let cn: isize = msg_send![c, windowNumber];
            let frame: NSRect = msg_send![c, frame];
            let rect = NSRect::new(
                NSPoint::new(
                    frame.origin.x,
                    CGDisplayPixelsHigh(CGMainDisplayID()) as f64
                        - frame.origin.y
                        - frame.size.height,
                ),
                frame.size,
            );
            let ids = [cn as usize as *const c_void, pn as usize as *const c_void];
            let array = CFArrayCreate(std::ptr::null(), ids.as_ptr(), 2, std::ptr::null());
            if array.is_null() {
                return None;
            }
            let image = CGWindowListCreateImageFromArray(rect, array, 1);
            CFRelease(array);
            if image.is_null() {
                return None;
            }
            let data = CFDataCreateMutable(std::ptr::null(), 0);
            if data.is_null() {
                CGImageRelease(image);
                return None;
            }
            let png = NSString::from_str("public.png");
            let dest = CGImageDestinationCreateWithData(
                data,
                (&*png as *const NSString).cast(),
                1,
                std::ptr::null(),
            );
            let mut result = None;
            if !dest.is_null() {
                CGImageDestinationAddImage(dest, image, std::ptr::null());
                if CGImageDestinationFinalize(dest) {
                    let len = CFDataGetLength(data);
                    let bytes = CFDataGetBytePtr(data);
                    if len > 0 && len <= 16 * 1024 * 1024 && !bytes.is_null() {
                        result = Some(std::slice::from_raw_parts(bytes, len as usize).to_vec());
                    }
                }
                CFRelease(dest);
            }
            CFRelease(data);
            CGImageRelease(image);
            result
        }
    }
}
