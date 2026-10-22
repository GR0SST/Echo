//! Debug automation for the real app (macOS): runs a script of keystrokes,
//! clicks and screenshots against the app's own window. Events are posted as
//! real `NSEvent`s, so they take the same AppKit path as user input, and
//! screenshots include the Chromium views.
//!
//! `BROWSER_SCRIPT="wait 3; shot /tmp/a.png; key cmd-t; type github.com; key enter; dump; quit"`
//!
//! Commands: `activate`, `wait <seconds>`, `key <keystroke>`, `type <text>`,
//! `click <x> <y>` (logical px from the window's top-left),
//! `swipe <x> <y> <dx>` (two-finger trackpad swipe), `resize <w> <h>`, `shot <file.png>`,
//! `dump` (prints the model), `quit`.

use std::ffi::{c_void, CString};
use std::time::Duration;

use gpui::{App, AsyncApp, WeakEntity};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::msg_send;
use objc2_foundation::{NSPoint, NSRect, NSString};

use crate::window::BrowserWindow;

const KEY_DOWN: usize = 10;
const KEY_UP: usize = 11;
const MOUSE_DOWN: usize = 1;
const MOUSE_UP: usize = 2;
const SHIFT: usize = 1 << 17;
const CONTROL: usize = 1 << 18;
const OPTION: usize = 1 << 19;
const COMMAND: usize = 1 << 20;
const EVENT_GAP: Duration = Duration::from_millis(60);

pub fn start(script: String, view: WeakEntity<BrowserWindow>, ns_view: *mut c_void, cx: &mut App) {
    let ns_view = ns_view as usize;
    cx.spawn(async move |cx: &mut AsyncApp| {
        let ns_view = ns_view as *mut AnyObject;
        unsafe { make_key(ns_view) };
        for command in script.split(';').map(str::trim).filter(|c| !c.is_empty()) {
            eprintln!("[automation] {command}");
            let (name, arg) = command.split_once(' ').unwrap_or((command, ""));
            match name {
                "activate" => {
                    unsafe { make_key(ns_view) };
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                }
                "wait" => {
                    let secs: f64 = arg.parse().unwrap_or(1.0);
                    cx.background_executor()
                        .timer(Duration::from_secs_f64(secs))
                        .await;
                }
                "key" => {
                    unsafe { post_key(ns_view, arg) };
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "type" => {
                    for ch in arg.chars() {
                        unsafe { post_key(ns_view, &ch.to_string()) };
                        cx.background_executor().timer(Duration::from_millis(15)).await;
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "move" => {
                    let v: Vec<f64> = arg.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                    if let [x, y] = v[..] {
                        unsafe { post_mouse(ns_view, MOUSE_MOVED, x, y) };
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "drag" => {
                    // drag <x1> <y1> <x2> <y2>: press, move and release the mouse
                    // (window coordinates, may lie on the window's edge).
                    let v: Vec<f64> = arg.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                    if let [x1, y1, x2, y2] = v[..] {
                        unsafe { post_drag(ns_view, (x1, y1), (x2, y2)) };
                    }
                    cx.background_executor().timer(Duration::from_millis(500)).await;
                }
                "click" => {
                    // click <x> <y> [hold ms]
                    let mut coords = arg.split_whitespace().filter_map(|v| v.parse::<f64>().ok());
                    if let (Some(x), Some(y)) = (coords.next(), coords.next()) {
                        let hold = coords.next().map(|ms| Duration::from_secs_f64(ms / 1000.0));
                        unsafe { post_click(ns_view, x, y, hold) };
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "shot" => match unsafe { screenshot(ns_view, arg) } {
                    Ok(()) => eprintln!("[automation] saved {arg}"),
                    Err(error) => eprintln!("[automation] screenshot failed: {error}"),
                },
                "windows" => unsafe {
                    let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
                    let windows: *mut AnyObject = msg_send![app, windows];
                    let count: usize = msg_send![windows, count];
                    for i in 0..count {
                        let window: *mut AnyObject = msg_send![windows, objectAtIndex: i];
                        let class: *const AnyClass = msg_send![window, class];
                        let frame: NSRect = msg_send![window, frame];
                        let visible: Bool = msg_send![window, isVisible];
                        let ignores: Bool = msg_send![window, ignoresMouseEvents];
                        let key: Bool = msg_send![window, isKeyWindow];
                        // NSWindowOcclusionStateVisible: macOS stops drawing
                        // windows hidden behind others, e.g. other apps'.
                        let occlusion: usize = msg_send![window, occlusionState];
                        eprintln!(
                            "[windows] {i} {} frame=({:.0},{:.0} {:.0}x{:.0}) visible={} ignoresMouse={} key={} onScreen={}",
                            (*class).name().to_string_lossy(),
                            frame.origin.x, frame.origin.y, frame.size.width, frame.size.height,
                            visible.as_bool(), ignores.as_bool(), key.as_bool(), occlusion & 2 != 0,
                        );
                    }
                },
                "views" => unsafe {
                    let window = window_of(ns_view);
                    let mask: usize = msg_send![window, styleMask];
                    let min: objc2_foundation::NSSize = msg_send![window, minSize];
                    let max: objc2_foundation::NSSize = msg_send![window, maxSize];
                    eprintln!("[views] styleMask={mask:#x} resizable={} min={min:?} max={max:?}", mask & 8 != 0);
                    let content: *mut AnyObject = msg_send![window, contentView];
                    let frame_view: *mut AnyObject = msg_send![content, superview];
                    print_views(if frame_view.is_null() { content } else { frame_view }, 0);
                },
                "shotstack" => unsafe {
                    // All our windows composited over the main window's frame,
                    // e.g. to see overlays such as the command bar.
                    match screenshot_stack(ns_view, arg) {
                        Ok(()) => eprintln!("[automation] saved {arg}"),
                        Err(error) => eprintln!("[automation] screenshot failed: {error}"),
                    }
                },
                "shotall" => unsafe {
                    let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
                    let windows: *mut AnyObject = msg_send![app, windows];
                    let count: usize = msg_send![windows, count];
                    for i in 0..count {
                        let window: *mut AnyObject = msg_send![windows, objectAtIndex: i];
                        let number: isize = msg_send![window, windowNumber];
                        let path = format!("{arg}-{i}.png");
                        match capture_window(number as u32, &path) {
                            Ok(()) => eprintln!("[automation] saved {path}"),
                            Err(error) => eprintln!("[automation] window {i}: {error}"),
                        }
                    }
                },
                "swipe" => {
                    // swipe <x> <y> <dx> [hold]: two-finger horizontal trackpad swipe;
                    // with `hold` the fingers stay down until `release <x> <y>`.
                    let mut words = arg.split_whitespace();
                    let v: Vec<f64> = words.by_ref().take(3).filter_map(|v| v.parse().ok()).collect();
                    let hold = words.next() == Some("hold");
                    if let [x, y, dx] = v[..] {
                        const STEPS: usize = 12;
                        unsafe { send_scroll(ns_view, x, y, 0.0, SCROLL_BEGAN) };
                        for _ in 0..STEPS {
                            cx.background_executor().timer(Duration::from_millis(16)).await;
                            unsafe { send_scroll(ns_view, x, y, dx / STEPS as f64, SCROLL_CHANGED) };
                        }
                        if !hold {
                            cx.background_executor().timer(Duration::from_millis(16)).await;
                            unsafe { send_scroll(ns_view, x, y, 0.0, SCROLL_ENDED) };
                        }
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "release" => {
                    let v: Vec<f64> = arg.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                    if let [x, y] = v[..] {
                        unsafe { send_scroll(ns_view, x, y, 0.0, SCROLL_ENDED) };
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "resize" => unsafe {
                    let mut size = arg.split_whitespace().filter_map(|v| v.parse::<f64>().ok());
                    if let (Some(w), Some(h)) = (size.next(), size.next()) {
                        let window = window_of(ns_view);
                        let _: () = msg_send![window, setContentSize: objc2_foundation::NSSize::new(w, h)];
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                },
                "dump" => {
                    let _ = view.update(cx, |this, cx| eprintln!("{}", dump(this, cx)));
                }
                "quit" => {
                    let _ = cx.update(|cx| cx.quit());
                }
                _ => eprintln!("[automation] unknown command"),
            }
        }
    })
    .detach();
}

fn dump(this: &BrowserWindow, cx: &App) -> String {
    let browser = &this.browser;
    let space = browser.space();
    let mut out = format!(
        "[dump] space={:?} sidebar={} command_bar={:?}\n",
        space.name,
        this.sidebar_visible,
        this.command_bar
            .as_ref()
            .map(|bar| bar.input.read(cx).text().to_string()),
    );
    out += &format!("[dump] archived_count={}\n", browser.archive.len());
    if let Some(bar) = &this.command_bar {
        out += &format!("[dump] command_target={:?} selected={} suggestions={}\n", bar.target, bar.selected, this.suggestions(cx).len());
    }
    out += &format!("[dump] archive_after_seconds={:?}\n", browser.archive_after.map(|duration| duration.as_secs()));
    if let Some(toast) = &this.toast {
        out += &format!("[dump] toast={:?}\n", toast.message);
    }
    if let Some(find) = &this.find_bar {
        out += &format!("[dump] find={:?} count={:?} active={} searching={}\n", find.input.read(cx).text(), find.count, find.active, find.searching);
    }
    let shown = this.pages.as_ref().and_then(|pages| pages.borrow().shown());
    for (section, tabs) in [("pinned", &space.pinned), ("today", &space.today)] {
        for tab in tabs {
            let active = space.active_tab == Some(tab.id);
            let state = this
                .pages
                .as_ref()
                .map(|pages| pages.borrow().state(tab.id))
                .unwrap_or_default();
            out += &format!(
                "[dump] {section:6} {}{} {:?} {} loading={} back={}\n",
                if active { "*" } else { " " },
                if shown == Some(tab.id) { "shown" } else { "     " },
                tab.title,
                tab.url,
                state.loading,
                state.can_go_back,
            );
        }
    }
    out
}

unsafe fn print_views(view: *mut AnyObject, depth: usize) {
    unsafe {
        let class: *const AnyClass = msg_send![view, class];
        let frame: NSRect = msg_send![view, frame];
        let hidden: Bool = msg_send![view, isHidden];
        let layer: *mut AnyObject = msg_send![view, layer];
        let layer_class = if layer.is_null() {
            "-".to_string()
        } else {
            let c: *const AnyClass = msg_send![layer, class];
            (*c).name().to_string_lossy().into_owned()
        };
        eprintln!(
            "[views] {}{} frame=({:.0},{:.0} {:.0}x{:.0}) hidden={} layer={}",
            "  ".repeat(depth),
            (*class).name().to_string_lossy(),
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
            hidden.as_bool(),
            layer_class,
        );
        if !layer.is_null() && layer_class != "CAMetalLayer" {
            print_layers(layer, depth + 2);
        }
        let subviews: *mut AnyObject = msg_send![view, subviews];
        let count: usize = msg_send![subviews, count];
        for i in 0..count {
            let child: *mut AnyObject = msg_send![subviews, objectAtIndex: i];
            print_views(child, depth + 1);
        }
    }
}

unsafe fn print_layers(layer: *mut AnyObject, depth: usize) {
    unsafe {
        let sublayers: *mut AnyObject = msg_send![layer, sublayers];
        if sublayers.is_null() {
            return;
        }
        let count: usize = msg_send![sublayers, count];
        for i in 0..count {
            let child: *mut AnyObject = msg_send![sublayers, objectAtIndex: i];
            let class: *const AnyClass = msg_send![child, class];
            let name = (*class).name().to_string_lossy().into_owned();
            let bounds: NSRect = msg_send![child, bounds];
            let context = if name == "CALayerHost" {
                let id: u32 = msg_send![child, contextId];
                format!(" contextId={id}")
            } else {
                String::new()
            };
            eprintln!(
                "[views] {}layer {name} {:.0}x{:.0}{context}",
                "  ".repeat(depth),
                bounds.size.width,
                bounds.size.height
            );
            print_layers(child, depth + 1);
        }
    }
}

unsafe fn window_of(view: *mut AnyObject) -> *mut AnyObject {
    unsafe { msg_send![view, window] }
}

unsafe fn make_key(view: *mut AnyObject) {
    unsafe {
        let window = window_of(view);
        let nil: *mut AnyObject = std::ptr::null_mut();
        let _: () = msg_send![window, makeKeyAndOrderFront: nil];
        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let _: () = msg_send![app, activateIgnoringOtherApps: Bool::YES];
    }
}

/// Parses `cmd-shift-t`, `enter`, `down`, or a single character.
fn parse_key(spec: &str) -> (usize, String, u16) {
    let mut flags = 0;
    let mut key = spec;
    if spec.chars().count() > 1 {
        let mut parts: Vec<&str> = spec.split('-').collect();
        // A trailing "-" means the minus key itself, e.g. "cmd--".
        if spec.ends_with("--") {
            parts.pop();
            parts.pop();
            parts.push("-");
        }
        key = parts.pop().unwrap_or("");
        for modifier in parts {
            flags |= match modifier {
                "cmd" => COMMAND,
                "ctrl" => CONTROL,
                "alt" => OPTION,
                "shift" => SHIFT,
                _ => 0,
            };
        }
    }
    let special = |chars: &str, code| (flags, chars.to_string(), code);
    match key {
        "enter" => special("\r", 36),
        "tab" => special("\t", 48),
        "space" => special(" ", 49),
        "backspace" => special("\u{7f}", 51),
        "escape" => special("\u{1b}", 53),
        "left" => special("\u{f702}", 123),
        "right" => special("\u{f703}", 124),
        "down" => special("\u{f701}", 125),
        "up" => special("\u{f700}", 126),
        _ => {
            let ch = key.chars().next().unwrap_or(' ');
            (flags, ch.to_string(), key_code(ch))
        }
    }
}

fn key_code(ch: char) -> u16 {
    const CODES: &[(char, u16)] = &[
        ('a', 0), ('s', 1), ('d', 2), ('f', 3), ('h', 4), ('g', 5), ('z', 6), ('x', 7),
        ('c', 8), ('v', 9), ('b', 11), ('q', 12), ('w', 13), ('e', 14), ('r', 15),
        ('y', 16), ('t', 17), ('1', 18), ('2', 19), ('3', 20), ('4', 21), ('6', 22),
        ('5', 23), ('=', 24), ('9', 25), ('7', 26), ('-', 27), ('8', 28), ('0', 29),
        (']', 30), ('o', 31), ('u', 32), ('[', 33), ('i', 34), ('p', 35), ('l', 37),
        ('j', 38), ('\'', 39), ('k', 40), (';', 41), ('\\', 42), (',', 43), ('/', 44),
        ('n', 45), ('m', 46), ('.', 47), (' ', 49),
    ];
    let lower = ch.to_ascii_lowercase();
    CODES
        .iter()
        .find(|(c, _)| *c == lower)
        .map_or(0, |(_, code)| *code)
}

unsafe fn post_key(view: *mut AnyObject, spec: &str) {
    let (flags, chars, code) = parse_key(spec);
    let shifted = if flags & SHIFT != 0 { chars.to_uppercase() } else { chars.clone() };
    unsafe {
        let window = window_of(view);
        let number: isize = msg_send![window, windowNumber];
        let characters = NSString::from_str(&shifted);
        let unmodified = NSString::from_str(&chars);
        let nil: *mut AnyObject = std::ptr::null_mut();
        for kind in [KEY_DOWN, KEY_UP] {
            let event: *mut AnyObject = msg_send![
                AnyClass::get(c"NSEvent").unwrap(),
                keyEventWithType: kind,
                location: NSPoint::new(0.0, 0.0),
                modifierFlags: flags,
                timestamp: 0.0f64,
                windowNumber: number,
                context: nil,
                characters: &*characters,
                charactersIgnoringModifiers: &*unmodified,
                isARepeat: Bool::NO,
                keyCode: code
            ];
            post(event);
        }
    }
}

/// Clicks at window point (x, y). With `hold`, the button is released
/// that much later from another thread, like a real hand: AppKit's own
/// tracking loops (window buttons, resizing) then wait for it.
unsafe fn post_click(view: *mut AnyObject, x: f64, y: f64, hold: Option<Duration>) {
    unsafe {
        let window = window_of(view);
        let number: isize = msg_send![window, windowNumber];
        let content: *mut AnyObject = msg_send![window, contentView];
        let bounds: NSRect = msg_send![content, bounds];
        let location = (x, bounds.size.height - y);
        post_mouse_button(MOUSE_DOWN, location, number);
        match hold {
            None => post_mouse_button(MOUSE_UP, location, number),
            Some(hold) => {
                std::thread::spawn(move || {
                    std::thread::sleep(hold);
                    post_mouse_button(MOUSE_UP, location, number);
                });
            }
        }
    }
}

unsafe fn post_mouse_button(kind: usize, (x, y): (f64, f64), window_number: isize) {
    unsafe {
        let nil: *mut AnyObject = std::ptr::null_mut();
        let event: *mut AnyObject = msg_send![
            AnyClass::get(c"NSEvent").unwrap(),
            mouseEventWithType: kind,
            location: NSPoint::new(x, y),
            modifierFlags: 0usize,
            timestamp: 0.0f64,
            windowNumber: window_number,
            context: nil,
            eventNumber: 0isize,
            clickCount: 1isize,
            pressure: 1.0f32
        ];
        post(event);
    }
}

/// `CGEventRef`, typed so objc2's message checks accept it.
#[repr(transparent)]
struct CGEventRef(*mut c_void);

unsafe impl objc2::Encode for CGEventRef {
    const ENCODING: objc2::Encoding =
        objc2::Encoding::Pointer(&objc2::Encoding::Struct("__CGEvent", &[]));
}

const SCROLL_BEGAN: i64 = 1;
const SCROLL_CHANGED: i64 = 2;
const SCROLL_ENDED: i64 = 4;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateScrollWheelEvent2(source: *const c_void, units: u32, count: u32, wheel1: i32, wheel2: i32, wheel3: i32) -> *mut c_void;
    fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
    fn CGEventSetDoubleValueField(event: *mut c_void, field: u32, value: f64);
    fn CGEventSetLocation(event: *mut c_void, location: NSPoint);
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
}

/// Hands a trackpad scroll event at window point (x, y) (top-left origin)
/// straight to the toolkit view: AppKit drops synthesized scroll events
/// posted to the queue, since they belong to no window.
unsafe fn send_scroll(view: *mut AnyObject, x: f64, y: f64, dx: f64, phase: i64) {
    const UNIT_PIXEL: u32 = 0;
    const IS_CONTINUOUS: u32 = 88;
    const FIXED_DELTA_AXIS_2: u32 = 94;
    const POINT_DELTA_AXIS_2: u32 = 97;
    const SCROLL_PHASE: u32 = 99;
    unsafe {
        let window = window_of(view);
        let content: *mut AnyObject = msg_send![window, contentView];
        let bounds: NSRect = msg_send![content, bounds];
        // A window-less event's locationInWindow is its screen position,
        // so place it where screen coordinates equal the window point.
        let display_height = CGDisplayPixelsHigh(CGMainDisplayID()) as f64;
        let global = NSPoint::new(x, display_height - (bounds.size.height - y));
        let event = CGEventCreateScrollWheelEvent2(std::ptr::null(), UNIT_PIXEL, 2, 0, dx as i32, 0);
        if event.is_null() {
            return;
        }
        CGEventSetLocation(event, global);
        CGEventSetIntegerValueField(event, IS_CONTINUOUS, 1);
        CGEventSetIntegerValueField(event, POINT_DELTA_AXIS_2, dx as i64);
        CGEventSetDoubleValueField(event, FIXED_DELTA_AXIS_2, dx);
        CGEventSetIntegerValueField(event, SCROLL_PHASE, phase);
        let ns_event: *mut AnyObject =
            msg_send![AnyClass::get(c"NSEvent").unwrap(), eventWithCGEvent: CGEventRef(event)];
        if !ns_event.is_null() {
            let _: () = msg_send![view, scrollWheel: ns_event];
        }
        CFRelease(event);
    }
}

const MOUSE_MOVED: usize = 5;

/// A mouse event at window point (x, y), top-left origin.
unsafe fn post_mouse(view: *mut AnyObject, kind: usize, x: f64, y: f64) {
    unsafe {
        let window = window_of(view);
        let number: isize = msg_send![window, windowNumber];
        let content: *mut AnyObject = msg_send![window, contentView];
        let bounds: NSRect = msg_send![content, bounds];
        let nil: *mut AnyObject = std::ptr::null_mut();
        let event: *mut AnyObject = msg_send![
            AnyClass::get(c"NSEvent").unwrap(),
            mouseEventWithType: kind,
            location: NSPoint::new(x, bounds.size.height - y),
            modifierFlags: 0usize,
            timestamp: 0.0f64,
            windowNumber: number,
            context: nil,
            eventNumber: 0isize,
            clickCount: 0isize,
            pressure: 0.0f32
        ];
        // Queued mouse moves only reach the first responder (often the page);
        // real ones also reach tracking-area owners like this view.
        if !event.is_null() {
            let _: () = msg_send![view, mouseMoved: event];
        }
    }
}

/// Presses at `from`, moves to `to` and releases, paced like a hand from
/// another thread, so AppKit's tracking loops (e.g. resizing) see it.
unsafe fn post_drag(view: *mut AnyObject, from: (f64, f64), to: (f64, f64)) {
    const DRAGGED: usize = 6;
    const STEPS: usize = 10;
    unsafe {
        let window = window_of(view);
        let number: isize = msg_send![window, windowNumber];
        let content: *mut AnyObject = msg_send![window, contentView];
        let bounds: NSRect = msg_send![content, bounds];
        let height = bounds.size.height;
        let at = move |t: f64| (from.0 + (to.0 - from.0) * t, height - (from.1 + (to.1 - from.1) * t));
        post_mouse_button(MOUSE_DOWN, at(0.0), number);
        std::thread::spawn(move || {
            for i in 1..=STEPS {
                std::thread::sleep(Duration::from_millis(30));
                post_mouse_button(DRAGGED, at(i as f64 / STEPS as f64), number);
            }
            std::thread::sleep(Duration::from_millis(30));
            post_mouse_button(MOUSE_UP, at(1.0), number);
        });
    }
}

unsafe fn post(event: *mut AnyObject) {
    if event.is_null() {
        eprintln!("[automation] could not create event");
        return;
    }
    unsafe {
        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let _: () = msg_send![app, postEvent: event, atStart: Bool::NO];
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCreateImage(bounds: NSRect, options: u32, window: u32, image_options: u32) -> *mut c_void;
    fn CGImageRelease(image: *mut c_void);
}

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageDestinationCreateWithURL(url: *const c_void, kind: *const c_void, count: usize, options: *const c_void) -> *mut c_void;
    fn CGImageDestinationAddImage(dest: *mut c_void, image: *mut c_void, properties: *const c_void);
    fn CGImageDestinationFinalize(dest: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(alloc: *const c_void, path: *const u8, len: isize, is_dir: bool) -> *const c_void;
    fn CFRelease(object: *const c_void);
}

const INCLUDING_WINDOW: u32 = 1 << 3;
const BOUNDS_IGNORE_FRAMING: u32 = 1 << 0;

/// Captures this app's own window, which needs no screen recording permission.
/// Layers hosted by Chromium's GPU process are not included; run the app with
/// `--in-process-gpu` (as `scripts/drive.sh` does) to see page content.
unsafe fn screenshot(view: *mut AnyObject, path: &str) -> Result<(), String> {
    unsafe {
        let window = window_of(view);
        let number: isize = msg_send![window, windowNumber];
        capture_window(number as u32, path)
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCreateImageFromArray(bounds: NSRect, windows: *const c_void, image_options: u32) -> *mut c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFArrayCreate(alloc: *const c_void, values: *const *const c_void, count: isize, callbacks: *const c_void) -> *const c_void;
}

/// Composites only this app's visible windows (front to back) over the main
/// window's frame, e.g. to see overlays. Never includes other apps.
unsafe fn screenshot_stack(view: *mut AnyObject, path: &str) -> Result<(), String> {
    unsafe {
        let window = window_of(view);
        let frame: NSRect = msg_send![window, frame];
        let display_height = CGDisplayPixelsHigh(CGMainDisplayID()) as f64;
        // CoreGraphics rects are top-left based.
        let rect = NSRect::new(
            NSPoint::new(frame.origin.x, display_height - frame.origin.y - frame.size.height),
            frame.size,
        );
        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let ordered: *mut AnyObject = msg_send![app, orderedWindows];
        let count: usize = msg_send![ordered, count];
        let mut ids: Vec<*const c_void> = Vec::new();
        for i in 0..count {
            let w: *mut AnyObject = msg_send![ordered, objectAtIndex: i];
            let visible: Bool = msg_send![w, isVisible];
            if visible.as_bool() {
                let number: isize = msg_send![w, windowNumber];
                ids.push(number as usize as *const c_void);
            }
        }
        // A CFArray of raw CGWindowID values, as the API expects.
        let array = CFArrayCreate(std::ptr::null(), ids.as_ptr(), ids.len() as isize, std::ptr::null());
        let image = CGWindowListCreateImageFromArray(rect, array, BOUNDS_IGNORE_FRAMING);
        CFRelease(array);
        write_png(image, path)
    }
}

unsafe fn capture_window(number: u32, path: &str) -> Result<(), String> {
    unsafe {
        let null_rect = NSRect::new(NSPoint::new(f64::INFINITY, f64::INFINITY), Default::default());
        let image = CGWindowListCreateImage(null_rect, INCLUDING_WINDOW, number, BOUNDS_IGNORE_FRAMING);
        write_png(image, path)
    }
}

/// Writes and releases `image`.
unsafe fn write_png(image: *mut c_void, path: &str) -> Result<(), String> {
    unsafe {
        if image.is_null() {
            return Err("CGWindowListCreateImage returned nothing".into());
        }
        let c_path = CString::new(path).map_err(|e| e.to_string())?;
        let url = CFURLCreateFromFileSystemRepresentation(
            std::ptr::null(),
            c_path.as_ptr() as *const u8,
            path.len() as isize,
            false,
        );
        let png = NSString::from_str("public.png");
        let dest = CGImageDestinationCreateWithURL(url, &*png as *const NSString as *const c_void, 1, std::ptr::null());
        let ok = !dest.is_null() && {
            CGImageDestinationAddImage(dest, image, std::ptr::null());
            CGImageDestinationFinalize(dest)
        };
        if !dest.is_null() {
            CFRelease(dest);
        }
        CFRelease(url);
        CGImageRelease(image);
        if ok { Ok(()) } else { Err("could not write PNG".into()) }
    }
}
