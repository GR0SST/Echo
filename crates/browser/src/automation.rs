//! Debug automation for the real app (macOS): runs a script of keystrokes,
//! clicks and screenshots against the app's own window. Events are posted as
//! real `NSEvent`s, so they take the same AppKit path as user input, and
//! screenshots include the Chromium views.
//!
//! `BROWSER_SCRIPT="wait 3; shot /tmp/a.png; key cmd-t; type github.com; key enter; dump; quit"`
//!
//! Commands: `reopen` (Dock request), `window <index|last>` (select a browser root),
//! `transfer-new-host`, `transfer-back` (diagnostic live-page host transfer),
//! `snapshot <file.png>` (capture the active document through CEF),
//! `inspector-open`, `inspector-status`, `inspector-eval`, `inspector-close`
//! (native diagnostic right dock; eval is fixed document URL + lifetime UUID,
//! submitted without waiting so a following snapshot can run concurrently),
//! `activate`, `require-focus` (abort on focus loss or occlusion),
//! `wait <seconds>`, `key <keystroke>`, `type <text>`,
//! `menu <menu title> > <item title>` (invoke an enabled native menu item),
//! `compose <text>` (native IME/emoji text insertion into the focused responder),
//! `click <x> <y>`, `doubleclick <x> <y>`, `rightclick <x> <y>` (logical px from the browser's top-left;
//! clicks and moves hit its child overlays when present),
//! `pointer <x> <y>` (also moves the physical pointer for native hover checks),
//! `drag <x1> <y1> <x2> <y2> [hold_ms]` (hold at the destination before releasing),
//! `swipe <x> <y> <dx>` (two-finger trackpad swipe), `resize <w> <h>`,
//! `position <x> <y>` (native screen coordinates), `shot <file.png>`,
//! `dump` (prints the model), `quit`.

use std::ffi::{c_void, CString};
use std::time::Duration;

use gpui::{App, AppContext, AsyncApp, WeakEntity};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::msg_send;
use objc2::encode::{Encode, Encoding};
use objc2_foundation::{NSPoint, NSRange, NSRect, NSString, NSNotFound};

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

struct HostTransfer {
    source: WeakEntity<BrowserWindow>,
    source_view: usize,
    source_window: gpui::AnyWindowHandle,
    target: gpui::WindowHandle<BrowserWindow>,
    pages: std::rc::Rc<std::cell::RefCell<crate::pages::Pages>>,
    tab: browser_core::TabId,
}

async fn transfer_new_host(source: &WeakEntity<BrowserWindow>, cx: &mut AsyncApp) -> Option<(HostTransfer, WeakEntity<BrowserWindow>, usize)> {
    let (source_window, source_view, profile, selection, pages, tab, frame) = source.read_with(cx, |view, _| {
        let pages = view.pages.clone()?;
        let layout = pages.borrow().surface_layout();
        if layout.len() != 1 { return None; }
        let (tab, frame) = layout[0];
        Some((view.main_window, view.native_view? as usize, view.profile.handle.clone(), view.browser.window_selection(), pages, tab, frame))
    }).ok().flatten()?;
    let (target, target_view, target_entity) = cx.update(|cx| {
        let bounds = source_window.update(cx, |_, window, _| window.bounds()).ok()?;
        let target = cx.open_window(gpui::WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions { title: Some("Echo transfer probe".into()), appears_transparent: true,
                traffic_light_position: Some(gpui::point(gpui::px(crate::window::TRAFFIC_LIGHT_X), gpui::px(crate::window::TRAFFIC_LIGHT_Y))) }),
            ..Default::default()
        }, |window, cx| {
            let parent = crate::native::view_of(window).unwrap_or(std::ptr::null_mut());
            let shared_pages = pages.borrow().for_host(parent);
            cx.new(|cx| BrowserWindow::with_profile_pages(profile, Some(selection), Some(shared_pages), window, cx))
        }).ok()?;
        let (parent, entity) = target.update(cx, |_, window, cx| {
            crate::native::view_of(window).map(|parent| (parent, cx.weak_entity()))
        }).ok().flatten()?;
        target.update(cx, |view, _, _| {
            let mut pages = view.pages.as_ref().unwrap().borrow_mut();
            pages.sync(&view.browser, true);
            pages.set_bounds(frame);
            pages.activate_host(&view.browser);
        }).ok()?;
        let _ = target.update(cx, |_, window, _| window.activate_window());
        let _ = target.update(cx, |view, _, _| view.pages.as_ref().unwrap().borrow_mut().focus_shown());
        Some((target, parent as usize, entity))
    }).ok().flatten()?;
    if !wait_for_page(&target_entity, tab, cx).await {
        let _ = cx.update(|cx| target.update(cx, |_, window, _| window.remove_window()));
        return None;
    }
    Some((HostTransfer { source: source.clone(), source_view, source_window, target, pages, tab }, target_entity, target_view))
}

async fn transfer_back(transfer: &HostTransfer, cx: &mut AsyncApp) -> bool {
    if transfer.source.upgrade().is_none() { return false; }
    if !cx.update(|cx| transfer.source_window.update(cx, |_, _, _| ()).is_ok()).unwrap_or(false) { return false; }
    let restored_page = cx.update(|cx| transfer.source.update(cx, |view, _| {
        view.refresh_profile_projection();
        let mut pages = transfer.pages.borrow_mut();
        pages.activate_host(&view.browser);
    }).is_ok()).unwrap_or(false);
    if !restored_page || !wait_for_page(&transfer.source, transfer.tab, cx).await { return false; }
    let restored = cx.update(|cx| {
        let _ = transfer.target.update(cx, |_, window, _| window.remove_window());
        let _ = transfer.source_window.update(cx, |_, window, _| window.activate_window());
        transfer.pages.borrow_mut().focus_shown();
    }).is_ok();
    restored
}

// Native handoff may wait briefly for the source viewport capture. Never hold
// a window or Pages borrow while waiting for the regular engine pump.
async fn wait_for_page(view: &WeakEntity<BrowserWindow>, tab: browser_core::TabId, cx: &mut AsyncApp) -> bool {
    for _ in 0..100 {
        if view.read_with(cx, |view, _| view.pages.as_ref().is_some_and(|pages| pages.borrow().is_visible(tab))).unwrap_or(false) { return true; }
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
    false
}

pub fn start(script: String, mut view: WeakEntity<BrowserWindow>, ns_view: *mut c_void, cx: &mut App) {
    let ns_view = ns_view as usize;
    cx.spawn(async move |cx: &mut AsyncApp| {
        let mut ns_view = ns_view as *mut AnyObject;
        unsafe { make_key(ns_view) };
        let mut require_focus = false;
        let mut transfer = None;
        for command in script.split(';').map(str::trim).filter(|c| !c.is_empty()) {
            eprintln!("[automation] {command}");
            let (name, arg) = command.split_once(' ').unwrap_or((command, ""));
            if name == "require-focus" { require_focus = true; }
            // Read-only waits let native fullscreen/Spaces transitions settle.
            // Input and screenshots still require focus after the wait.
            // Explicit window selection/activation also remains usable after
            // a deliberate window switch or Close Window.
            let after_close = matches!(name, "wait" | "windows" | "reopen" | "window" | "activate" | "quit");
            if require_focus && !after_close && (view.upgrade().is_none() || !unsafe { has_browser_focus(ns_view) }) {
                eprintln!("[automation] aborted: browser lost focus or is not on screen");
                let _ = cx.update(|cx| cx.quit());
                return;
            }
            match name {
                "require-focus" => {},
                "transfer-new-host" => {
                    let moved = if transfer.is_none() { transfer_new_host(&view, cx).await } else { None };
                    let Some((state, target, parent)) = moved else {
                        eprintln!("[automation] aborted: live host transfer needs one loaded standalone page and a supported native host");
                        let _ = cx.update(|cx| cx.quit());
                        return;
                    };
                    transfer = Some(state);
                    view = target;
                    ns_view = parent as *mut AnyObject;
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                }
                "transfer-back" => {
                    let state = transfer.take();
                    let restored = if let Some(state) = &state { transfer_back(state, cx).await } else { false };
                    let Some(state) = state.filter(|_| restored) else {
                        eprintln!("[automation] aborted: live host transfer could not restore the source");
                        let _ = cx.update(|cx| cx.quit());
                        return;
                    };
                    view = state.source;
                    ns_view = state.source_view as *mut AnyObject;
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                }
                "window" => {
                    let selected = cx.update(|cx| {
                        let mut roots: Vec<_> = cx.windows().into_iter().filter_map(|handle| handle.downcast::<BrowserWindow>()).collect();
                        // App::windows iterates a hash map. Native window
                        // numbers give these disposable roots creation order.
                        roots.sort_by_key(|handle| handle.update(cx, |_, window, _| {
                            crate::native::view_of(window).map(|parent| unsafe {
                                let root = window_of(parent.cast());
                                let number: isize = msg_send![root, windowNumber];
                                number
                            }).unwrap_or_default()
                        }).unwrap_or_default());
                        let handle = if arg == "last" { roots.last() } else { arg.parse::<usize>().ok().and_then(|index| roots.get(index)) }?;
                        handle.update(cx, |view, window, cx| crate::native::view_of(window).map(|parent| {
                            unsafe {
                                let root = window_of(parent.cast());
                                let keyboard = keyboard_window(parent.cast());
                                let root_number: isize = msg_send![root, windowNumber];
                                let keyboard_number: isize = msg_send![keyboard, windowNumber];
                                eprintln!("[automation] selected_window private={} root={root_number} keyboard={keyboard_number}", view.browser.is_private());
                            }
                            (cx.weak_entity(), parent)
                        })).ok().flatten()
                    }).ok().flatten();
                    let Some((new_view, parent)) = selected else {
                        eprintln!("[automation] aborted: browser window selector did not match");
                        let _ = cx.update(|cx| cx.quit());
                        return;
                    };
                    view = new_view;
                    ns_view = parent.cast();
                    // Bind only. A just-opened root may not have its final
                    // occlusion state yet; making it key would steal focus
                    // from its already-focused Command Bar child.
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                }
                "menu" => {
                    let invoked = arg.split_once(" > ").is_some_and(|(menu, item)| unsafe { perform_menu_action(menu, item) });
                    if !invoked {
                        eprintln!("[automation] aborted: native menu item is missing or disabled");
                        let _ = cx.update(|cx| cx.quit());
                        return;
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "reopen" => {
                    // Deliver the same delegate request AppKit sends for a Dock click.
                    unsafe {
                        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
                        let delegate: *mut AnyObject = msg_send![app, delegate];
                        let _: () = msg_send![delegate, applicationShouldHandleReopen: app, hasVisibleWindows: false];
                    }
                    if let Ok(Some((new_view, parent))) = cx.update(|cx| {
                        cx.windows().into_iter().find_map(|handle| {
                            let handle = handle.downcast::<BrowserWindow>()?;
                            handle.update(cx, |_, window, cx| {
                                crate::native::view_of(window).map(|parent| (cx.weak_entity(), parent))
                            }).ok().flatten()
                        })
                    }) {
                        view = new_view;
                        ns_view = parent.cast();
                    }
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                }
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
                        if require_focus && (view.upgrade().is_none() || !unsafe { has_browser_focus(ns_view) }) {
                            eprintln!("[automation] aborted: browser lost focus or is not on screen");
                            let _ = cx.update(|cx| cx.quit());
                            return;
                        }
                        unsafe { post_key(ns_view, &ch.to_string()) };
                        cx.background_executor().timer(Duration::from_millis(15)).await;
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "compose" => {
                    let overlay = view.update(cx, |this, _| this.command_bar.as_ref().and_then(|bar| bar.overlay)).ok().flatten();
                    let target = overlay.and_then(|handle| handle.update(cx, |_, window, _| crate::native::view_of(window)).ok().flatten()).map_or(ns_view, |view| view.cast());
                    unsafe { compose_text(target, arg); }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "pointer" | "move" => {
                    let v: Vec<f64> = arg.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                    if let [x, y] = v[..] {
                        unsafe {
                            if name == "pointer" { warp_pointer(ns_view, x, y); }
                            post_mouse(ns_view, MOUSE_MOVED, x, y);
                        };
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "drag" => {
                    // drag <x1> <y1> <x2> <y2> [hold_ms]: press, move and release the mouse
                    // (window coordinates, may lie on the window's edge).
                    let v: Vec<f64> = arg.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                    let hold = v.get(4).copied().filter(|ms| ms.is_finite() && *ms >= 0.).map(|ms| Duration::from_secs_f64(ms / 1000.)).unwrap_or_default();
                    if v.len() == 4 || v.len() == 5 {
                        unsafe { post_drag(ns_view, (v[0], v[1]), (v[2], v[3]), hold) };
                    }
                    cx.background_executor().timer(Duration::from_millis(500) + hold).await;
                }
                "doubleclick" => {
                    let coords: Vec<f64> = arg.split_whitespace().filter_map(|value| value.parse().ok()).collect();
                    if let [x, y] = coords[..] { unsafe { post_double_click(ns_view, x, y) }; }
                    cx.background_executor().timer(EVENT_GAP).await;
                }
                "rightclick" => {
                    let coords: Vec<f64> = arg.split_whitespace().filter_map(|value| value.parse().ok()).collect();
                    if let [x, y] = coords[..] {
                        unsafe { send_right_click(ns_view, x, y); }
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
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
                "menus" => {
                    let _ = view.update(cx, |_, cx| {
                        if let Some(menus) = cx.get_menus() {
                            for menu in menus {
                                let names: Vec<_> = menu.items.into_iter().filter_map(|item| match item {
                                    gpui::OwnedMenuItem::Action { name, .. } => Some(name.to_string()), _ => None,
                                }).collect();
                                eprintln!("[menus] {}: {}", menu.name, names.join(" | "));
                            }
                        }
                    });
                }
                "windows" => unsafe {
                    let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
                    let windows: *mut AnyObject = msg_send![app, windows];
                    let count: usize = msg_send![windows, count];
                    let main: *mut AnyObject = msg_send![app, mainWindow];
                    for i in 0..count {
                        let window: *mut AnyObject = msg_send![windows, objectAtIndex: i];
                        let class: *const AnyClass = msg_send![window, class];
                        let frame: NSRect = msg_send![window, frame];
                        let visible: Bool = msg_send![window, isVisible];
                        let ignores: Bool = msg_send![window, ignoresMouseEvents];
                        let key: Bool = msg_send![window, isKeyWindow];
                        let minimized: Bool = msg_send![window, isMiniaturized];
                        // NSWindowOcclusionStateVisible: macOS stops drawing
                        // windows hidden behind others, e.g. other apps'.
                        let occlusion: usize = msg_send![window, occlusionState];
                        eprintln!(
                            "[windows] {i} {} frame=({:.0},{:.0} {:.0}x{:.0}) visible={} ignoresMouse={} key={} main={} minimized={} onScreen={}",
                            (*class).name().to_string_lossy(),
                            frame.origin.x, frame.origin.y, frame.size.width, frame.size.height,
                            visible.as_bool(), ignores.as_bool(), key.as_bool(), window == main, minimized.as_bool(), occlusion & 2 != 0,
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
                "inspector-open" | "inspector-status" | "inspector-eval" | "inspector-close" => {
                    // No arbitrary JS, protocol JSON, view IDs or parent pointers
                    // are accepted from the script. Events are logged centrally
                    // by Pages::take_events after the regular engine pump.
                    if !arg.is_empty() {
                        eprintln!("[automation] {name} rejected: this command accepts no arguments");
                        continue;
                    }
                    let pages = view.read_with(cx, |view, _| view.pages.clone()).ok().flatten();
                    if let Some(pages) = pages {
                        let mut pages = pages.borrow_mut();
                        match name {
                            "inspector-open" => match pages.open_inspector_probe() {
                                Ok(status) => eprintln!("[inspector-status] {status:?}"),
                                Err(error) => eprintln!("[inspector-error] {error}"),
                            },
                            "inspector-status" => eprintln!("[inspector-status] {:?}", pages.inspector_probe_status()),
                            "inspector-eval" => match pages.eval_inspector_probe() {
                                Ok(id) => eprintln!("[inspector-eval] submitted id={id}; result follows in inspector-event Protocol"),
                                Err(error) => eprintln!("[inspector-error] {error}"),
                            },
                            "inspector-close" => {
                                pages.close_inspector_probe();
                                eprintln!("[inspector-status] closed; inspected page retained");
                            },
                            _ => unreachable!(),
                        }
                    } else {
                        eprintln!("[inspector-error] No live page engine");
                    }
                },
                "snapshot" => {
                    let page = view.read_with(cx, |view, _| Some((view.pages.clone()?, view.browser.active_tab()?.id))).ok().flatten();
                    let requested = page.and_then(|(pages, tab)| {
                        let request = pages.borrow_mut().request_snapshot(tab)?;
                        Some((pages, tab, request))
                    });
                    let Some((pages, tab, request)) = requested else {
                        eprintln!("[automation] aborted: active page snapshot request was rejected");
                        let _ = cx.update(|cx| cx.quit());
                        return;
                    };
                    let mut captured = None;
                    for _ in 0..300 {
                        cx.background_executor().timer(Duration::from_millis(20)).await;
                        let snapshot = pages.borrow().snapshot(tab).filter(|(id, _)| *id == request);
                        if let Some((_, png)) = snapshot { captured = Some(png); break; }
                        if !pages.borrow().snapshot_pending(tab, request) { break; }
                    }
                    if let Some(png) = captured {
                        match std::fs::write(arg, png.as_ref()) {
                            Ok(()) => eprintln!("[automation] saved page snapshot {arg}"),
                            Err(error) => eprintln!("[automation] page snapshot write failed: {error}"),
                        }
                    } else {
                        eprintln!("[automation] aborted: active page snapshot failed or timed out");
                        let _ = cx.update(|cx| cx.quit());
                        return;
                    }
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
                "position" => unsafe {
                    let mut origin = arg.split_whitespace().filter_map(|v| v.parse::<f64>().ok());
                    if let (Some(x), Some(y)) = (origin.next(), origin.next()) {
                        let window = window_of(ns_view);
                        let _: () = msg_send![window, setFrameOrigin: NSPoint::new(x, y)];
                    }
                    cx.background_executor().timer(EVENT_GAP).await;
                },
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
                "surfaces" => {
                    let _ = view.read_with(cx, |this, _| {
                        if let Some(pages) = &this.pages {
                            for (id, bounds) in pages.borrow().surface_layout() {
                                eprintln!("[surface] tab={} x={:.1} y={:.1} width={:.1} height={:.1}", id.get(), bounds.x, bounds.y, bounds.width, bounds.height);
                            }
                        }
                    });
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
    out += &format!("[dump] history_count={}\n", browser.history.len());
    out += &format!("[dump] inline_rename={:?}\n", this.tab_rename.as_ref().map(|editor| (editor.tab.get(), editor.input.read(cx).text().to_string())));
    out += &format!("[dump] link_status={:?}\n", this.link_status.as_ref().map(|status| (status.tab.get(), &status.text, status.expanded)));
    if this.history_is_open() { out += &format!("[dump] history_query={:?}\n", this.history_page.input.read(cx).text()); }

    if let Some(bar) = &this.command_bar {
        out += &format!("[dump] command_target={:?} selected={} suggestions={}\n", bar.target, bar.selected, this.suggestions(cx).len());
    }
    if let Some(menu) = &this.tab_menu { out += &format!("[dump] sidebar_menu={:?} selected={:?}\n", menu.target, menu.selected); }
    out += &format!("[dump] archive_after_seconds={:?}\n", browser.archive_after.map(|duration| duration.as_secs()));
    if let Some(toast) = &this.toast {
        out += &format!("[dump] toast={:?}\n", toast.message);
    }
    if let Some(find) = &this.find_bar {
        out += &format!("[dump] find={:?} count={:?} active={} searching={}\n", find.input.read(cx).text(), find.count, find.active, find.searching);
    }
    let shown = this.pages.as_ref().and_then(|pages| pages.borrow().shown());
    for (section, tabs) in [("favorite", &this.browser.favorites), ("pinned", &space.pinned), ("today", &space.today)] {
        for tab in tabs {
            let active = space.active_tab == Some(tab.id);
            let state = this
                .pages
                .as_ref()
                .map(|pages| pages.borrow().state(tab.id))
                .unwrap_or_default();
            out += &format!(
                "[dump] {section:6} {}{} {:?} {} loading={} back={} icon_bytes={} muted={} engine_muted={:?} visible={}\n",
                if active { "*" } else { " " },
                if shown == Some(tab.id) { "shown" } else { "     " },
                tab.title,
                tab.url,
                state.loading,
                state.can_go_back,
                this.browser.favicon(&tab.url).map_or(0, |png| png.len()),
                tab.muted,
                this.pages.as_ref().and_then(|pages| pages.borrow().audio_muted(tab.id)),
                this.pages.as_ref().is_some_and(|pages| pages.borrow().is_visible(tab.id)),
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

/// Keyboard events belong to the focused child window, while script points
/// remain relative to the browser's main window.
unsafe fn keyboard_window(view: *mut AnyObject) -> *mut AnyObject {
    unsafe {
        let root = window_of(view);
        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let key: *mut AnyObject = msg_send![app, keyWindow];
        let mut ancestor = key;
        while !ancestor.is_null() {
            if ancestor == root { return key; }
            ancestor = msg_send![ancestor, parentWindow];
        }
        root
    }
}

unsafe fn perform_menu_action(menu: &str, item: &str) -> bool {
    unsafe {
        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let main: *mut AnyObject = msg_send![app, mainMenu];
        let menu_item: *mut AnyObject = msg_send![main, itemWithTitle: &*NSString::from_str(menu)];
        if menu_item.is_null() { return false; }
        let submenu: *mut AnyObject = msg_send![menu_item, submenu];
        if submenu.is_null() { return false; }
        let _: () = msg_send![submenu, update];
        let index: isize = msg_send![submenu, indexOfItemWithTitle: &*NSString::from_str(item)];
        if index < 0 { return false; }
        let item: *mut AnyObject = msg_send![submenu, itemAtIndex: index];
        let enabled: Bool = msg_send![item, isEnabled];
        if !enabled.as_bool() { return false; }
        let _: () = msg_send![submenu, performActionForItemAtIndex: index];
        true
    }
}

/// Command Bar and menu child windows belong to the same test browser.
unsafe fn has_browser_focus(view: *mut AnyObject) -> bool {
    unsafe {
        let root = window_of(view);
        if root.is_null() { return false; }
        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let active: Bool = msg_send![app, isActive];
        let visible: Bool = msg_send![root, isVisible];
        let occlusion: usize = msg_send![root, occlusionState];
        if !active.as_bool() || !visible.as_bool() || occlusion & 2 == 0 { return false; }
        let mut key: *mut AnyObject = msg_send![app, keyWindow];
        while !key.is_null() {
            if key == root { return true; }
            key = msg_send![key, parentWindow];
        }
        false
    }
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

// GPUI 0.2 registers Cocoa's legacy `NSRange` encoding; Foundation uses
// `_NSRange`. They share an ABI, but objc2 validates the structure tag too.
#[repr(C)]
struct LegacyTextRange { location: usize, length: usize }
unsafe impl Encode for LegacyTextRange {
    const ENCODING: Encoding = Encoding::Struct("NSRange", &[usize::ENCODING, usize::ENCODING]);
}

/// Supplementary Unicode and emoji are delivered by the system's text input
/// client, not by a physical keyboard's virtual key code. Keep this separate
/// from `type`, which exercises the NSEvent shortcut/key routing path.
unsafe fn compose_text(view: *mut AnyObject, text: &str) {
    unsafe {
        let window = window_of(view);
        let responder: *mut AnyObject = msg_send![window, firstResponder];
        if responder.is_null() { eprintln!("[automation] no text responder"); return; }
        let accepts: Bool = msg_send![responder, respondsToSelector: objc2::sel!(insertText:replacementRange:)];
        if !accepts.as_bool() { eprintln!("[automation] focused responder does not accept composed text"); return; }
        let text = NSString::from_str(text);
        let legacy_range = (*responder).class().instance_method(objc2::sel!(insertText:replacementRange:))
            .and_then(|method| method.argument_type(3)).is_some_and(|encoding| encoding.to_bytes().starts_with(b"{NSRange="));
        if legacy_range {
            let _: () = msg_send![responder, insertText: &*text, replacementRange: LegacyTextRange { location: NSNotFound as usize, length: 0 }];
        } else {
            let _: () = msg_send![responder, insertText: &*text, replacementRange: NSRange::new(NSNotFound as usize, 0)];
        }
    }
}

unsafe fn post_key(view: *mut AnyObject, spec: &str) {
    let (flags, chars, code) = parse_key(spec);
    let shifted = if flags & SHIFT != 0 { chars.to_uppercase() } else { chars.clone() };
    unsafe {
        let window = keyboard_window(view);
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
        let (window, location) = mouse_target(view, x, y);
        let number: isize = msg_send![window, windowNumber];
        let location = (location.x, location.y);
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

unsafe fn post_double_click(view: *mut AnyObject, x: f64, y: f64) {
    unsafe {
        post_click(view, x, y, None);
        let (window, location) = mouse_target(view, x, y);
        let number: isize = msg_send![window, windowNumber];
        let location = (location.x, location.y);
        post_mouse_button_count(MOUSE_DOWN, location, number, 2);
        post_mouse_button_count(MOUSE_UP, location, number, 2);
    }
}

unsafe fn post_mouse_button(kind: usize, location: (f64, f64), window_number: isize) {
    unsafe { post_mouse_button_count(kind, location, window_number, 1); }
}

unsafe fn post_mouse_button_count(kind: usize, (x, y): (f64, f64), window_number: isize, count: isize) {
    unsafe { post_mouse_button_modified(kind, (x, y), window_number, count, 0); }
}

unsafe fn post_mouse_button_modified(kind: usize, (x, y): (f64, f64), window_number: isize, count: isize, flags: usize) {
    unsafe {
        let nil: *mut AnyObject = std::ptr::null_mut();
        let event: *mut AnyObject = msg_send![
            AnyClass::get(c"NSEvent").unwrap(),
            mouseEventWithType: kind,
            location: NSPoint::new(x, y),
            modifierFlags: flags,
            timestamp: 0.0f64,
            windowNumber: window_number,
            context: nil,
            eventNumber: 0isize,
            clickCount: count,
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
    fn CGWarpMouseCursorPosition(position: NSPoint) -> i32;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
}

/// AppKit's synthetic mouseEventWithType reports buttonNumber=0 even for
/// RightMouseDown. Use macOS's standard control-click gesture instead, keeping
/// the event bound to its window and routed through AppKit's normal hit-test.
unsafe fn send_right_click(view: *mut AnyObject, x: f64, y: f64) {
    unsafe {
        let (window, point) = mouse_target(view, x, y);
        let number: isize = msg_send![window, windowNumber];
        post_mouse_button_modified(MOUSE_DOWN, (point.x, point.y), number, 1, CONTROL);
        post_mouse_button_modified(MOUSE_UP, (point.x, point.y), number, 1, CONTROL);
    }
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

/// Align the physical pointer for native hover/tracking checks over CEF.
unsafe fn warp_pointer(view: *mut AnyObject, x: f64, y: f64) {
    unsafe {
        let window = window_of(view);
        let content: *mut AnyObject = msg_send![window, contentView];
        let bounds: NSRect = msg_send![content, bounds];
        let screen: NSPoint = msg_send![window, convertPointToScreen: NSPoint::new(x, bounds.size.height - y)];
        let screens: *mut AnyObject = msg_send![AnyClass::get(c"NSScreen").unwrap(), screens];
        let primary: *mut AnyObject = msg_send![screens, objectAtIndex: 0usize];
        let frame: NSRect = msg_send![primary, frame];
        CGWarpMouseCursorPosition(NSPoint::new(screen.x, frame.size.height - screen.y));
    }
}

unsafe fn toolkit_view(view: *mut AnyObject) -> Option<*mut AnyObject> {
    unsafe {
        let class = AnyClass::get(c"GPUIView")?;
        let toolkit: Bool = msg_send![view, isKindOfClass: class];
        if toolkit.as_bool() { return Some(view); }
        let children: *mut AnyObject = msg_send![view, subviews];
        let count: usize = msg_send![children, count];
        for index in 0..count {
            let child: *mut AnyObject = msg_send![children, objectAtIndex: index];
            if let Some(view) = toolkit_view(child) { return Some(view); }
        }
        None
    }
}

/// A mouse event at window point (x, y), top-left origin.
unsafe fn post_mouse(view: *mut AnyObject, kind: usize, x: f64, y: f64) {
    unsafe {
        let (window, location) = mouse_target(view, x, y);
        let number: isize = msg_send![window, windowNumber];
        let content: *mut AnyObject = msg_send![window, contentView];
        let nil: *mut AnyObject = std::ptr::null_mut();
        let event: *mut AnyObject = msg_send![
            AnyClass::get(c"NSEvent").unwrap(),
            mouseEventWithType: kind,
            location: location,
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
            // A synthetic move must reach the view under the pointer. Sending
            // every move to GPUI makes it look like the pointer left CEF and
            // dismisses page hover UI, even after a real cursor warp.
            let hit: *mut AnyObject = msg_send![content, hitTest: location];
            let target = if hit.is_null() { toolkit_view(content).unwrap_or(content) } else { hit };
            let _: () = msg_send![target, mouseMoved: event];
        }
    }
}

/// Script coordinates are relative to the browser, but real pointer events
/// belong to the child window under that point (Command Bar, sidebar, Find).
/// Restrict hit-testing to this browser's descendants, ignoring passive toasts.
unsafe fn mouse_target(view: *mut AnyObject, x: f64, y: f64) -> (*mut AnyObject, NSPoint) {
    unsafe {
        let root = window_of(view);
        let content: *mut AnyObject = msg_send![root, contentView];
        let bounds: NSRect = msg_send![content, bounds];
        let local = NSPoint::new(x, bounds.size.height - y);
        let screen: NSPoint = msg_send![root, convertPointToScreen: local];
        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let ordered: *mut AnyObject = msg_send![app, orderedWindows];
        let count: usize = msg_send![ordered, count];
        for index in 0..count {
            let candidate: *mut AnyObject = msg_send![ordered, objectAtIndex: index];
            let mut parent: *mut AnyObject = msg_send![candidate, parentWindow];
            while !parent.is_null() && parent != root {
                parent = msg_send![parent, parentWindow];
            }
            if parent != root { continue; }
            let visible: Bool = msg_send![candidate, isVisible];
            let ignores: Bool = msg_send![candidate, ignoresMouseEvents];
            let frame: NSRect = msg_send![candidate, frame];
            if visible.as_bool() && !ignores.as_bool()
                && screen.x >= frame.origin.x && screen.x < frame.origin.x + frame.size.width
                && screen.y >= frame.origin.y && screen.y < frame.origin.y + frame.size.height {
                let point: NSPoint = msg_send![candidate, convertPointFromScreen: screen];
                return (candidate, point);
            }
        }
        (root, local)
    }
}

/// Presses at `from`, moves to `to` and releases, paced like a hand from
/// another thread, so AppKit's tracking loops (e.g. resizing) see it.
unsafe fn post_drag(view: *mut AnyObject, from: (f64, f64), to: (f64, f64), hold: Duration) {
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
            std::thread::sleep(Duration::from_millis(30) + hold);
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
