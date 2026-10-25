use std::{cell::OnceCell, ffi::c_void};
use x11rb::{
    connection::Connection,
    protocol::{
        shape::{ConnectionExt as _, SK, SO},
        xproto::{ConfigureWindowAux, ConnectionExt, PropMode, StackMode},
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};
thread_local! { static DISPLAY: OnceCell<Option<(RustConnection, u32)>> = const { OnceCell::new() }; }
fn connection<T>(run: impl FnOnce(&RustConnection, u32) -> Result<T, String>) -> Result<T, String> {
    DISPLAY.with(|display| {
        let (connection, root) = display
            .get_or_init(|| {
                let (connection, screen) = x11rb::connect(None).ok()?;
                let root = connection.setup().roots[screen].root;
                Some((connection, root))
            })
            .as_ref()
            .ok_or("No X11 display for overlay ownership")?;
        run(connection, *root)
    })
}
fn xid(window: *mut c_void) -> u32 {
    window as usize as u32
}
pub(super) fn rect(window: *mut c_void) -> Option<[i32; 4]> {
    connection(|c, root| {
        let geometry = c
            .get_geometry(xid(window))
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        let origin = c
            .translate_coordinates(xid(window), root, 0, 0)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        Ok([
            i32::from(origin.dst_x),
            i32::from(origin.dst_y),
            i32::from(geometry.width),
            i32::from(geometry.height),
        ])
    })
    .ok()
}
pub(super) fn attach(parent: *mut c_void, child: *mut c_void) -> Result<(), String> {
    connection(|c, _| {
        let atom = |name: &[u8]| {
            c.intern_atom(false, name)
                .map_err(|e| e.to_string())?
                .reply()
                .map(|r| r.atom)
                .map_err(|e| e.to_string())
        };
        // ICCCM transient ownership: window manager keeps this above its owner
        // and handles workspace/minimize policy. Never use global always-on-top.
        c.change_property32(
            PropMode::REPLACE,
            xid(child),
            atom(b"WM_TRANSIENT_FOR")?,
            x11rb::protocol::xproto::AtomEnum::WINDOW,
            &[xid(parent)],
        )
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
        c.change_property32(
            PropMode::REPLACE,
            xid(child),
            atom(b"_NET_WM_WINDOW_TYPE")?,
            x11rb::protocol::xproto::AtomEnum::ATOM,
            &[atom(b"_NET_WM_WINDOW_TYPE_DIALOG")?],
        )
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
        // Motif decorations flag, decorations = 0. GPUI owns the chrome.
        let motif = atom(b"_MOTIF_WM_HINTS")?;
        c.change_property32(
            PropMode::REPLACE,
            xid(child),
            motif,
            motif,
            &[2, 0, 0, 0, 0],
        )
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
        c.configure_window(
            xid(child),
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
        c.flush().map_err(|e| e.to_string())
    })
}
pub(super) fn position(child: *mut c_void, rect: [i32; 4]) -> bool {
    connection(|c, _| {
        c.configure_window(
            xid(child),
            &ConfigureWindowAux::new()
                .x(rect[0])
                .y(rect[1])
                .width(rect[2] as u32)
                .height(rect[3] as u32),
        )
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
        c.flush().map_err(|e| e.to_string())
    })
    .is_ok()
}
pub(super) fn passthrough(child: *mut c_void) -> Result<(), String> {
    connection(|c, _| {
        // Empty ShapeInput region, preserving the visual/bounding shape.
        c.shape_rectangles(
            SO::SET,
            SK::INPUT,
            x11rb::protocol::xproto::ClipOrdering::UNSORTED,
            xid(child),
            0,
            0,
            &[],
        )
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
        c.flush().map_err(|e| e.to_string())
    })
}

pub(super) fn is_owned(parent: *mut c_void, child: *mut c_void) -> bool {
    connection(|c, _| {
        let atom = c
            .intern_atom(false, b"WM_TRANSIENT_FOR")
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?
            .atom;
        let property = c
            .get_property(
                false,
                xid(child),
                atom,
                x11rb::protocol::xproto::AtomEnum::WINDOW,
                0,
                1,
            )
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        Ok(property.value32().and_then(|mut ids| ids.next()) == Some(xid(parent)))
    })
    .unwrap_or(false)
}

pub(super) fn set_visible(child: *mut c_void, visible: bool) -> Result<(), String> {
    connection(|c, _| {
        let request = if visible { c.map_window(xid(child)) } else { c.unmap_window(xid(child)) };
        request.map_err(|e| e.to_string())?.check().map_err(|e| e.to_string())?;
        c.flush().map_err(|e| e.to_string())
    })
}
