//! Native ownership for GPUI overlays. Keep them top-level windows: SetParent
//! would change GPUI's coordinate system and swapchain assumptions on Windows.
use super::overlay_geometry::Placement;
use std::{cell::RefCell, collections::HashMap, ffi::c_void};
#[cfg(target_os = "windows")]
#[path = "desktop_windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "desktop_x11.rs"]
mod platform;
struct Attachment {
    parent: usize,
    placement: Placement,
}
thread_local! { static OVERLAYS: RefCell<HashMap<usize, Attachment>> = RefCell::default(); }

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
            Attachment {
                parent: parent as usize,
                placement: Placement::new(
                    owner_rect,
                    child_rect,
                    engine::native_parent_scale(parent),
                ),
            },
        );
    });
}
pub(super) fn sync(parent: *mut c_void) {
    let Some(owner_rect) = platform::rect(parent) else {
        return;
    };
    // No GPUI/entity callbacks while holding this private registry borrow.
    OVERLAYS.with(|overlays| {
        overlays.borrow_mut().retain(|child, attachment| {
            if attachment.parent != parent as usize {
                return true;
            }
            let child = *child as *mut c_void;
            // A destroyed HWND/XID can be reused. Never reposition a new,
            // unrelated window merely because its numeric handle matches.
            if !platform::is_owned(parent, child) {
                return false;
            }
            let Some(current) = platform::rect(child) else {
                return false;
            };
            let target = attachment
                .placement
                .follow(owner_rect, engine::native_parent_scale(parent));
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
