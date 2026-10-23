//! Native child-window placement. Reparenting preserves the live CEF browser.
use crate::Bounds;
use std::ffi::c_void;

pub(crate) fn parent_of(ui_view: *mut c_void) -> *mut c_void {
    #[cfg(target_os = "macos")]
    { crate::mac::content_view_of(ui_view) }
    #[cfg(not(target_os = "macos"))]
    { ui_view }
}

pub(crate) fn move_to(view: *mut c_void, parent: *mut c_void) -> bool {
    if view.is_null() || parent.is_null() { return false; }
    #[cfg(target_os = "macos")]
    { crate::mac::move_clip_view(view, parent); true }
    #[cfg(target_os = "windows")]
    { windows::move_to(view, parent) }
    #[cfg(target_os = "linux")]
    { x11::move_to(view, parent) }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    { false }
}

pub(crate) fn set_frame(view: *mut c_void, parent: *mut c_void, frame: Bounds) {
    if view.is_null() || parent.is_null() { return; }
    #[cfg(target_os = "macos")]
    crate::mac::set_frame(view, parent, frame);
    #[cfg(target_os = "windows")]
    windows::set_frame(view, frame);
    #[cfg(target_os = "linux")]
    x11::set_frame(view, frame);
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetParent(child: *mut c_void, parent: *mut c_void) -> *mut c_void;
        fn IsWindow(window: *mut c_void) -> i32;
        fn MoveWindow(window: *mut c_void, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetLastError(error: u32);
        fn GetLastError() -> u32;
    }
    pub(super) fn move_to(view: *mut c_void, parent: *mut c_void) -> bool {
        unsafe {
            if IsWindow(view) == 0 || IsWindow(parent) == 0 { return false; }
            SetLastError(0);
            !SetParent(view, parent).is_null() || GetLastError() == 0
        }
    }
    pub(super) fn set_frame(view: *mut c_void, frame: Bounds) {
        unsafe { MoveWindow(view, frame.x as i32, frame.y as i32, frame.width.max(1.) as i32, frame.height.max(1.) as i32, 1); }
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use super::*;
    use std::{cell::OnceCell, ffi::{c_char, c_int, c_uint, c_ulong}};
    type Open = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    type Close = unsafe extern "C" fn(*mut c_void) -> c_int;
    type Reparent = unsafe extern "C" fn(*mut c_void, c_ulong, c_ulong, c_int, c_int) -> c_int;
    type Resize = unsafe extern "C" fn(*mut c_void, c_ulong, c_int, c_int, c_uint, c_uint) -> c_int;
    type Flush = unsafe extern "C" fn(*mut c_void) -> c_int;
    struct Connection {
        display: *mut c_void,
        close: Close,
        reparent: Reparent,
        resize: Resize,
        flush: Flush,
        _library: libloading::Library,
    }
    impl Connection {
        fn open() -> Option<Self> {
            // Load the runtime SONAME; do not require the libX11 development
            // linker symlink on systems running the Wayland frontend.
            let library = unsafe { libloading::Library::new("libX11.so.6") }.ok()?;
            let open: Open = unsafe { *library.get(b"XOpenDisplay\0").ok()? };
            let close = unsafe { *library.get(b"XCloseDisplay\0").ok()? };
            let reparent = unsafe { *library.get(b"XReparentWindow\0").ok()? };
            let resize = unsafe { *library.get(b"XMoveResizeWindow\0").ok()? };
            let flush = unsafe { *library.get(b"XFlush\0").ok()? };
            let display = unsafe { open(std::ptr::null()) };
            if display.is_null() { return None; }
            Some(Self { display, close, reparent, resize, flush, _library: library })
        }
    }
    impl Drop for Connection {
        fn drop(&mut self) { unsafe { (self.close)(self.display); } }
    }
    thread_local! { static DISPLAY: OnceCell<Option<Connection>> = const { OnceCell::new() }; }
    fn with_display<R>(run: impl FnOnce(&Connection) -> R) -> Option<R> {
        DISPLAY.with(|display| display.get_or_init(Connection::open).as_ref().map(run))
    }
    pub(super) fn move_to(view: *mut c_void, parent: *mut c_void) -> bool {
        with_display(|connection| unsafe {
            let moved = (connection.reparent)(connection.display, view as usize as c_ulong, parent as usize as c_ulong, 0, 0);
            (connection.flush)(connection.display);
            moved != 0
        }).unwrap_or(false)
    }
    pub(super) fn set_frame(view: *mut c_void, frame: Bounds) {
        let _ = with_display(|connection| unsafe {
            (connection.resize)(connection.display, view as usize as c_ulong, frame.x as c_int, frame.y as c_int,
                frame.width.max(1.) as c_uint, frame.height.max(1.) as c_uint);
            (connection.flush)(connection.display);
        });
    }
}
