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

pub(super) fn set_popup_bounds(view: *mut c_void, previous: gpui::Bounds<gpui::Pixels>, bounds: gpui::Bounds<gpui::Pixels>) {
    let Some(mut rect) = platform::rect(view) else {return;};
    let scale = engine::native_parent_scale(view);
    rect[0] += (f32::from(bounds.left()-previous.left()) as f64*scale).round() as i32;
    rect[1] += (f32::from(bounds.top()-previous.top()) as f64*scale).round() as i32;
    rect[2] = (f32::from(bounds.size.width) as f64*scale).round() as i32;
    rect[3] = (f32::from(bounds.size.height) as f64*scale).round() as i32;
    // Publish the new placement before SetWindowPos can synchronously dispatch
    // WM_SIZE/WM_MOVE. Never hold the registry across a native operation.
    let parent = OVERLAYS.with(|registry|registry.borrow().get(&(view as usize)).map(|attachment|attachment.parent));
    if let Some((parent,owner)) = parent.and_then(|parent|platform::rect(parent as *mut c_void).map(|owner|(parent,owner))) {
        let owner_scale=engine::native_parent_scale(parent as *mut c_void);
        OVERLAYS.with(|registry| {
            if let Some(attachment) = registry.borrow_mut().get_mut(&(view as usize)).filter(|attachment|attachment.parent==parent) {
                attachment.placement=Placement::new(owner,rect,owner_scale);
            }
        });
    }
    if !platform::position(view,rect) {eprintln!("[extension-popup] native positioning failed");}
}
