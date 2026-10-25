use std::ffi::c_void;
#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn SetProcessDpiAwarenessContext(context: isize) -> i32;
    fn GetClientRect(window: *mut c_void, rect: *mut Rect) -> i32;
    fn ClientToScreen(window: *mut c_void, point: *mut Point) -> i32;
    fn IsWindow(window: *mut c_void) -> i32;
    fn ShowWindow(window: *mut c_void, command: i32) -> i32;
    fn GetWindowLongPtrW(window: *mut c_void, index: i32) -> isize;
    fn SetWindowLongPtrW(window: *mut c_void, index: i32, value: isize) -> isize;
    fn SetWindowPos(
        window: *mut c_void,
        after: *mut c_void,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetLastError(error: u32);
    fn GetLastError() -> u32;
}
pub(super) fn rect(window: *mut c_void) -> Option<[i32; 4]> {
    let mut rect = Rect::default();
    let mut origin = Point::default();
    unsafe {
        if IsWindow(window) == 0
            || GetClientRect(window, &mut rect) == 0
            || ClientToScreen(window, &mut origin) == 0
        {
            return None;
        }
    }
    Some([
        origin.x,
        origin.y,
        rect.right - rect.left,
        rect.bottom - rect.top,
    ])
}
fn set_long(window: *mut c_void, index: i32, value: isize) -> Result<(), String> {
    unsafe {
        SetLastError(0);
        if SetWindowLongPtrW(window, index, value) == 0 && GetLastError() != 0 {
            return Err(format!("SetWindowLongPtrW({index}): {}", GetLastError()));
        }
    }
    Ok(())
}
pub(super) fn attach(parent: *mut c_void, child: *mut c_void) -> Result<(), String> {
    let content = rect(child).ok_or("Overlay has no client area")?;
    // GWLP_HWNDPARENT sets the OWNER of this top-level popup. Owned windows
    // stay above their owner, minimize with it, and preserve desktop coordinates.
    set_long(child, -8, parent as isize)?;
    let style = unsafe { GetWindowLongPtrW(child, -16) };
    // Remove caption/thick frame; GPUI draws the overlay surface.
    set_long(child, -16, (style & !0x00c40000) | 0x80000000u32 as isize)?;
    // SWP_FRAMECHANGED | NOMOVE | NOSIZE | NOACTIVATE, owned z-order retained.
    unsafe {
        SetWindowPos(child, std::ptr::null_mut(), 0, 0, 0, 0, 0x0037);
    }
    // Preserve GPUI's original client rectangle after removing the frame.
    // Owner placement uses client-area screen coordinates, not DWM borders.
    if !position(child, content) {
        return Err("Cannot preserve overlay client placement".into());
    }
    Ok(())
}
pub(super) fn position(child: *mut c_void, rect: [i32; 4]) -> bool {
    // NOZORDER | NOACTIVATE: passive overlays never steal keyboard focus.
    unsafe {
        SetWindowPos(
            child,
            std::ptr::null_mut(),
            rect[0],
            rect[1],
            rect[2],
            rect[3],
            0x14,
        ) != 0
    }
}
pub(super) fn passthrough(child: *mut c_void) -> Result<(), String> {
    // WS_EX_TRANSPARENT | WS_EX_NOACTIVATE. All GPUI/CEF windows are on the UI
    // thread, so Win32 skips this overlay when hit-testing same-thread siblings.
    let style = unsafe { GetWindowLongPtrW(child, -20) };
    set_long(child, -20, style | 0x08000020)
}

pub(super) fn prepare_windowing() -> Result<(), String> {
    // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, before any HWND is created.
    // ERROR_ACCESS_DENIED means a manifest/host already fixed process awareness.
    unsafe {
        if SetProcessDpiAwarenessContext(-4) == 0 && GetLastError() != 5 {
            return Err(format!(
                "Cannot initialize per-monitor DPI awareness: {}",
                GetLastError()
            ));
        }
    }
    Ok(())
}

pub(super) fn is_owned(parent: *mut c_void, child: *mut c_void) -> bool {
    unsafe { IsWindow(child) != 0 && GetWindowLongPtrW(child, -8) == parent as isize }
}

pub(super) fn set_visible(child: *mut c_void, visible: bool) -> Result<(), String> {
    if unsafe { IsWindow(child) } == 0 { return Err("Overlay was destroyed".into()); }
    // SW_SHOWNOACTIVATE / SW_HIDE; foreground focus is routed separately.
    unsafe { ShowWindow(child, if visible { 4 } else { 0 }); }
    Ok(())
}
