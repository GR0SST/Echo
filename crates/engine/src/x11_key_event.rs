//! Xlib key-event ABI adapter for CEF's opaque Linux XEvent pointer.
use crate::keyboard_priority::{EventIdentity, EventStamp};
use std::ffi::{c_int, c_ulong, c_void};

// The common XKeyEvent prefix through `time`, from X11/Xlib.h. XEvent is
// a union; only KeyPress/KeyRelease members have a meaningful key timestamp.
#[repr(C)]
struct KeyEventPrefix {
    kind: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    window: c_ulong,
    root: c_ulong,
    subwindow: c_ulong,
    time: c_ulong,
}

/// `event` must be null or a live, aligned Xlib XEvent supplied by CEF.
pub(crate) unsafe fn stamp(event: *const c_void) -> EventStamp {
    if event.is_null() {
        return EventStamp::default();
    }
    // Read the discriminant before interpreting the key-event union member.
    if !matches!(unsafe { *event.cast::<c_int>() }, 2 | 3) {
        return EventStamp::default();
    }
    let key = unsafe { &*event.cast::<KeyEventPrefix>() };
    EventStamp {
        identity: (key.time != 0).then_some(EventIdentity(key.time as u64, key.serial as u64)),
        milliseconds: (key.time != 0).then_some(key.time as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xlib_layout_and_key_identity() {
        assert_eq!(
            std::mem::offset_of!(KeyEventPrefix, time),
            if cfg!(target_pointer_width = "64") {
                56
            } else {
                28
            }
        );
        let mut event = KeyEventPrefix {
            kind: 2,
            serial: 91,
            send_event: 0,
            display: std::ptr::null_mut(),
            window: 0,
            root: 0,
            subwindow: 0,
            time: 1234,
        };
        for kind in [2, 3] {
            event.kind = kind;
            let result = unsafe { stamp((&event as *const KeyEventPrefix).cast()) };
            assert_eq!(result.identity, Some(EventIdentity(1234, 91)));
            assert_eq!(result.milliseconds, Some(1234));
        }
        event.time = 0;
        let result = unsafe { stamp((&event as *const KeyEventPrefix).cast()) };
        assert_eq!(result.identity, None);
        assert_eq!(result.milliseconds, None);
    }

    #[test]
    fn null_and_non_key_events_have_no_stamp() {
        for event in [std::ptr::null(), (&12 as *const c_int).cast()] {
            let result = unsafe { stamp(event) };
            assert_eq!(result.identity, None);
            assert_eq!(result.milliseconds, None);
        }
    }
}
