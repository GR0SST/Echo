//! Native ownership for GPUI overlays. Keep them top-level windows: SetParent
//! would change GPUI's coordinate system and swapchain assumptions on Windows.
use super::overlay_geometry::{synchronize, Attachment, Placement, Registry};
use std::ffi::c_void;
#[cfg(target_os = "windows")]
#[path = "desktop_windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "desktop_x11.rs"]
mod platform;
thread_local! { static OVERLAYS: Registry = Registry::default(); }

pub(super) fn attach(parent: *mut c_void, child: *mut c_void) {
    if parent.is_null() || child.is_null() || parent == child {
        return;
    }
    let (Some(owner_rect), Some(child_rect)) = (platform::rect(parent), platform::rect(child))
    else {
        return;
    };
    if let Err(error) = platform::attach(parent, child) {
        eprintln!("[overlay] native attachment failed: {error}");
        return;
    }
    OVERLAYS.with(|overlays| {
        overlays.borrow_mut().insert(
            child as usize,
            Attachment::new(
                parent as usize,
                Placement::new(owner_rect, child_rect, engine::native_parent_scale(parent)),
            ),
        );
    });
}
pub(super) fn sync(parent: *mut c_void) {
    let Some(owner_rect) = platform::rect(parent) else {
        return;
    };
    OVERLAYS.with(|overlays| {
        synchronize(overlays, parent as usize, |child, placement| {
            let child = child as *mut c_void;
            // A destroyed HWND/XID can be reused by an unrelated native window.
            if !platform::is_owned(parent, child) {
                return false;
            }
            let Some(current) = platform::rect(child) else {
                return false;
            };
            let target = placement.follow(owner_rect, engine::native_parent_scale(parent));
            current == target || platform::position(child, target)
        })
    });
}

pub(super) fn forget(child: *mut c_void) {
    OVERLAYS.with(|overlays| {
        overlays.borrow_mut().retain(|handle, attachment| {
            *handle != child as usize && attachment.parent != child as usize
        });
    });
}
pub(super) fn passthrough(child: *mut c_void) {
    if let Err(error) = platform::passthrough(child) {
        eprintln!("[overlay] input passthrough failed: {error}");
    }
}

#[cfg(target_os = "windows")]
pub(super) fn prepare_windowing() -> Result<(), String> {
    platform::prepare_windowing()
}
