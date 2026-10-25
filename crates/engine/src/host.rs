//! Native child-window placement. Reparenting preserves the live CEF browser.
use crate::Bounds;
use std::ffi::c_void;

// CEF's generated bindings use an HWND newtype on Windows, an XID integer
// on Linux and an NSView pointer on macOS. Keep those ABI details here.
pub(crate) fn cef_handle(view: *mut c_void) -> cef::sys::cef_window_handle_t {
    #[cfg(target_os = "windows")]
    { cef::sys::HWND(view.cast()) }
    #[cfg(not(target_os = "windows"))]
    { view as _ }
}

pub(crate) fn native_handle(handle: cef::sys::cef_window_handle_t) -> *mut c_void {
    #[cfg(target_os = "windows")]
    { handle.0.cast() }
    #[cfg(not(target_os = "windows"))]
    { handle as _ }
}

#[cfg(target_os = "macos")]
pub(crate) type KeyEvent<'a> = *mut u8;
#[cfg(target_os = "windows")]
pub(crate) type KeyEvent<'a> = Option<&'a mut cef::sys::MSG>;
#[cfg(target_os = "linux")]
pub(crate) type KeyEvent<'a> = Option<&'a mut cef::sys::XEvent>;

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

/// A platform mask affects presentation only; no CEF resize notification.
pub(crate) fn set_viewport_clip(view: *mut c_void, clip: Option<Bounds>) {
    if view.is_null() { return; }
    #[cfg(target_os = "macos")]
    crate::mac::set_viewport_clip(view, clip);
    #[cfg(not(target_os = "macos"))]
    let _ = (view, clip); // Native region adapters remain platform-specific.
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn CreateWindowExW(ex_style: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32, width: i32, height: i32, parent: *mut c_void, menu: *mut c_void, instance: *mut c_void, param: *mut c_void) -> *mut c_void;
        fn GetParent(window: *mut c_void) -> *mut c_void;
        fn SetParent(child: *mut c_void, parent: *mut c_void) -> *mut c_void;
        fn IsWindow(window: *mut c_void) -> i32;
        fn DestroyWindow(window: *mut c_void) -> i32;
        fn MoveWindow(window: *mut c_void, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetLastError(error: u32);
        fn GetLastError() -> u32;
    }
    pub(super) fn create_retired_parent() -> Result<(*mut c_void, *mut c_void), String> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW, WS_POPUP, no WS_VISIBLE.
        let owner = unsafe { CreateWindowExW(0x08000080, class.as_ptr(), std::ptr::null(), 0x80000000,
            0, 0, 16, 16, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
        if owner.is_null() { return Err(format!("Hidden HWND creation failed: {}", unsafe { GetLastError() })); }
        Ok((owner, owner))
    }
    pub(super) fn has_parent(child: *mut c_void, parent: *mut c_void) -> bool {
        unsafe { GetParent(child) == parent }
    }
    pub(super) fn release_retired_parent(owner: *mut c_void) {
        unsafe { DestroyWindow(owner); }
    }
    pub(super) fn destroy_child(child: *mut c_void) -> Result<(), String> {
        // CEF154 WM_NCDESTROY calls WindowDestroyed(), which completes close.
        // DestroyWindow is synchronous and only accepts windows on this thread.
        unsafe {
            if DestroyWindow(child) != 0 { Ok(()) }
            else { Err(format!("DestroyWindow failed: {}", GetLastError())) }
        }
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
    use std::{cell::OnceCell, ffi::{c_char, c_int}};
    use crate::native_requests::check_request;

    // ABI verified against xcb.h/xproto.h. Checked requests return errors for
    // exactly their own connection/cookie; no Xlib process-global handler exists.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Cookie { sequence: u32 }
    #[repr(C)]
    struct Screen {
        root: u32, default_colormap: u32, white_pixel: u32, black_pixel: u32,
        current_input_masks: u32, width_pixels: u16, height_pixels: u16,
        width_mm: u16, height_mm: u16, min_maps: u16, max_maps: u16,
        root_visual: u32, backing_stores: u8, save_unders: u8,
        root_depth: u8, allowed_depths: u8,
    }
    #[repr(C)]
    struct Screens { data: *mut Screen, remaining: c_int, index: c_int }
    #[repr(C)]
    struct RequestError {
        response_type: u8, error_code: u8, sequence: u16, resource_id: u32,
        minor_code: u16, major_code: u8, pad0: u8, pad: [u32; 5], full_sequence: u32,
    }
    type Connect = unsafe extern "C" fn(*const c_char, *mut c_int) -> *mut c_void;
    type Disconnect = unsafe extern "C" fn(*mut c_void);
    type ConnectionError = unsafe extern "C" fn(*mut c_void) -> c_int;
    type Setup = unsafe extern "C" fn(*mut c_void) -> *const c_void;
    type Roots = unsafe extern "C" fn(*const c_void) -> Screens;
    type ScreenNext = unsafe extern "C" fn(*mut Screens);
    type GenerateId = unsafe extern "C" fn(*mut c_void) -> u32;
    type Create = unsafe extern "C" fn(*mut c_void, u8, u32, u32, i16, i16, u16, u16, u16, u16, u32, u32, *const c_void) -> Cookie;
    type Reparent = unsafe extern "C" fn(*mut c_void, u32, u32, i16, i16) -> Cookie;
    type Destroy = unsafe extern "C" fn(*mut c_void, u32) -> Cookie;
    type Configure = unsafe extern "C" fn(*mut c_void, u32, u16, *const c_void) -> Cookie;
    type Check = unsafe extern "C" fn(*mut c_void, Cookie) -> *mut RequestError;
    unsafe extern "C" { fn free(value: *mut c_void); }
    struct Connection {
        connection: *mut c_void, root: u32, disconnect: Disconnect,
        connection_error: ConnectionError, generate_id: GenerateId,
        create: Create, reparent: Reparent, destroy: Destroy, configure: Configure,
        check: Check, _library: libloading::Library,
    }
    impl Connection {
        fn open() -> Option<Self> {
            // No development linker symlink and no process-global Xlib state.
            let library = unsafe { libloading::Library::new("libxcb.so.1") }.ok()?;
            let connect: Connect = unsafe { *library.get(b"xcb_connect\0").ok()? };
            let disconnect: Disconnect = unsafe { *library.get(b"xcb_disconnect\0").ok()? };
            let connection_error: ConnectionError = unsafe { *library.get(b"xcb_connection_has_error\0").ok()? };
            let setup: Setup = unsafe { *library.get(b"xcb_get_setup\0").ok()? };
            let roots: Roots = unsafe { *library.get(b"xcb_setup_roots_iterator\0").ok()? };
            let next: ScreenNext = unsafe { *library.get(b"xcb_screen_next\0").ok()? };
            let generate_id = unsafe { *library.get(b"xcb_generate_id\0").ok()? };
            let create = unsafe { *library.get(b"xcb_create_window_checked\0").ok()? };
            let reparent = unsafe { *library.get(b"xcb_reparent_window_checked\0").ok()? };
            let destroy = unsafe { *library.get(b"xcb_destroy_window_checked\0").ok()? };
            let configure = unsafe { *library.get(b"xcb_configure_window_checked\0").ok()? };
            let check = unsafe { *library.get(b"xcb_request_check\0").ok()? };
            let mut preferred = 0;
            let connection = unsafe { connect(std::ptr::null(), &mut preferred) };
            if connection.is_null() { return None; }
            let root = unsafe {
                if connection_error(connection) != 0 { disconnect(connection); return None; }
                let setup = setup(connection);
                if setup.is_null() { disconnect(connection); return None; }
                let mut screens = roots(setup);
                for _ in 0..preferred.max(0) {
                    if screens.remaining <= 0 { break; }
                    next(&mut screens);
                }
                if screens.remaining <= 0 || screens.data.is_null() { disconnect(connection); return None; }
                (*screens.data).root
            };
            Some(Self { connection, root, disconnect, connection_error, generate_id,
                create, reparent, destroy, configure, check, _library: library })
        }
        fn checked<R>(&self, cookie: Cookie, value: R) -> Result<R, String> {
            check_request(self.connection as usize, cookie.sequence, value, |connection, sequence| unsafe {
                let error = (self.check)(connection as *mut c_void, Cookie { sequence });
                if !error.is_null() {
                    let message = format!("XCB native host request {sequence} failed: code {}, opcode {}:{}",
                        (*error).error_code, (*error).major_code, (*error).minor_code);
                    free(error.cast());
                    return Err(message);
                }
                let status = (self.connection_error)(self.connection);
                if status != 0 { Err(format!("XCB native host connection failed: {status}")) }
                else { Ok(()) }
            })
        }
    }
    impl Drop for Connection {
        fn drop(&mut self) { unsafe { (self.disconnect)(self.connection); } }
    }
    thread_local! { static DISPLAY: OnceCell<Option<Connection>> = const { OnceCell::new() }; }
    fn with_display<R>(run: impl FnOnce(&Connection) -> R) -> Option<R> {
        DISPLAY.with(|display| display.get_or_init(Connection::open).as_ref().map(run))
    }
    pub(super) fn create_retired_parent() -> Result<(*mut c_void, *mut c_void), String> {
        with_display(|connection| unsafe {
            let id = (connection.generate_id)(connection.connection);
            if id == 0 || id == u32::MAX { return Err("Hidden X11 parent ID allocation failed".into()); }
            let cookie = (connection.create)(connection.connection, 0, id, connection.root,
                0, 0, 16, 16, 0, 1, 0, 0, std::ptr::null());
            let id = connection.checked(cookie, id)?;
            // Never mapped. RetiredHost takes cleanup ownership only after this
            // exact CreateWindow request is acknowledged, not an unrelated error.
            let owner = id as usize as *mut c_void;
            Ok((owner, owner))
        }).unwrap_or_else(|| Err("Download retirement requires an available X11/XCB connection; Wayland-only hosts are unsupported".into()))
    }
    pub(super) fn release_retired_parent(owner: *mut c_void) {
        let _ = with_display(|connection| unsafe {
            connection.checked((connection.destroy)(connection.connection, owner as usize as u32), ())
        });
    }
    pub(super) fn move_to(view: *mut c_void, parent: *mut c_void) -> bool {
        with_display(|connection| unsafe {
            connection.checked((connection.reparent)(connection.connection,
                view as usize as u32, parent as usize as u32, 0, 0), ()).is_ok()
        }).unwrap_or(false)
    }
    pub(super) fn set_frame(view: *mut c_void, frame: Bounds) {
        let values = [frame.x as i32 as u32, frame.y as i32 as u32,
            frame.width.max(1.) as u32, frame.height.max(1.) as u32];
        let _ = with_display(|connection| unsafe {
            // XCB_CONFIG_WINDOW_X | Y | WIDTH | HEIGHT, protocol value order.
            connection.checked((connection.configure)(connection.connection,
                view as usize as u32, 0x0f, values.as_ptr().cast()), ())
        });
    }
}

/// Complete the Alloy DoClose(true) handshake by destroying ONLY the native
/// browser child after returning from the callback. Never target its parent.
pub(crate) fn destroy_child(child: *mut c_void) -> Result<(), String> {
    if child.is_null() { return Err("missing native child handle".into()); }
    #[cfg(target_os = "macos")]
    { crate::mac::destroy_browser_child(child); Ok(()) }
    #[cfg(target_os = "windows")]
    { windows::destroy_child(child) }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    { Err("explicit Alloy child destruction is unsupported on this platform".into()) }
}

/// Owned native parent, retained alongside the exact clip/child until Closed.
/// Never wraps a user window. Platform failures are explicit, never cancel.
pub(crate) struct RetiredHost {
    owner: *mut c_void,
    parent: *mut c_void,
}
impl RetiredHost {
    pub(crate) fn parent(&self) -> *mut c_void { self.parent }
    pub(crate) fn new() -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        let (owner, parent) = crate::mac::create_retired_window()?;
        #[cfg(target_os = "windows")]
        let (owner, parent) = windows::create_retired_parent()?;
        #[cfg(target_os = "linux")]
        let (owner, parent) = x11::create_retired_parent()?;
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        return Err("Download retirement native hosts are unsupported on this platform".into());
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            Ok(Self { owner, parent })
        }
    }
    pub(crate) fn attach(&self, child_or_clip: *mut c_void) -> Result<(), String> {
        if !move_to(child_or_clip, self.parent) {
            return Err("Cannot reparent the original download browser host".into());
        }
        #[cfg(target_os = "macos")]
        if !crate::mac::clip_has_parent(child_or_clip, self.parent) {
            return Err("Original clip did not attach to hidden NSWindow".into());
        }
        #[cfg(target_os = "windows")]
        if !windows::has_parent(child_or_clip, self.parent) {
            return Err("Original child did not attach to hidden HWND".into());
        }
        Ok(())
    }
}

impl Drop for RetiredHost {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        crate::mac::release_retired_window(self.owner);
        #[cfg(target_os = "windows")]
        windows::release_retired_parent(self.owner);
        #[cfg(target_os = "linux")]
        x11::release_retired_parent(self.owner);
    }
}

/// Install the independent lease before native reparenting. A partial native
/// failure must never drop the only possible parent and destroy its live child.
/// This seam is shared by production lifecycle and CEF-free failure tests.
pub(crate) fn retain_and_attach<H>(
    host: std::rc::Rc<H>,
    retain: impl FnOnce(std::rc::Rc<H>) -> Result<(), String>,
    attach: impl FnOnce(&H) -> Result<(), String>,
) -> Result<(), String> {
    retain(host.clone())?;
    attach(&host)
}

#[cfg(test)]
mod retirement_tests {
    use super::retain_and_attach;
    use std::{cell::{Cell, RefCell}, rc::Rc};
    struct Host(Rc<Cell<usize>>);
    impl Drop for Host { fn drop(&mut self) { self.0.set(self.0.get() + 1); } }
    #[test]
    fn failed_attach_retains_exact_host_until_closed_and_drops_outside_owner_borrow() {
        let owner: RefCell<Option<Rc<Host>>> = RefCell::new(None);
        let drops = Rc::new(Cell::new(0));
        let host = Rc::new(Host(drops.clone()));
        let weak = Rc::downgrade(&host);
        let result = retain_and_attach(host, |host| { *owner.borrow_mut() = Some(host); Ok(()) }, |host| {
            assert!(owner.try_borrow_mut().is_ok());
            assert!(std::ptr::eq(host, owner.borrow().as_deref().unwrap()));
            Err("native reparent rejected".into())
        });
        assert_eq!(result, Err("native reparent rejected".into()));
        assert!(weak.upgrade().is_some());
        assert_eq!(drops.get(), 0);
        let released = owner.borrow_mut().take(); // Closed dispatcher extracts.
        assert_eq!(drops.get(), 0);
        drop(released);
        assert!(weak.upgrade().is_none());
        assert_eq!(drops.get(), 1);
    }
    #[test]
    fn rejected_lease_installation_never_attempts_native_attach() {
        let drops = Rc::new(Cell::new(0));
        let result = retain_and_attach(Rc::new(Host(drops.clone())), |_| Err("already closed".into()), |_| {
            panic!("must not touch a native child after lease loss")
        });
        assert!(result.is_err());
        assert_eq!(drops.get(), 1);
    }
}
