//! Deliver website-priority native keys before AppKit asks GPUI/menu key
//! equivalents. Chromium still arbitrates them through KeyboardHandler.
use crate::{keyboard_priority::Configuration, Shortcut, ShortcutPriority};
use objc2::{
    msg_send,
    runtime::{AnyClass, AnyObject, Bool, Imp, Sel},
    sel,
};
use objc2_foundation::NSString;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::c_void,
    sync::{Arc, Mutex, OnceLock},
};

struct Root {
    view: usize,
    shortcuts: Arc<Mutex<Configuration>>,
}
thread_local! {
    static ROOTS: RefCell<HashMap<i32, Root>> = RefCell::default();
    static ROUTING: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn register(browser: i32, view: *mut c_void, shortcuts: Arc<Mutex<Configuration>>) {
    if !view.is_null() {
        install();
        ROOTS.with_borrow_mut(|roots| {
            roots.insert(
                browser,
                Root {
                    view: view as usize,
                    shortcuts,
                },
            );
        });
    }
}

type Equivalent = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool;
static ORIGINAL: OnceLock<Equivalent> = OnceLock::new();
static ORIGINAL_MENU: OnceLock<Equivalent> = OnceLock::new();

fn install() {
    install_class(c"GPUIView", &ORIGINAL, equivalent);
    install_class(c"NSMenu", &ORIGINAL_MENU, menu_equivalent);
}

fn install_class(name: &std::ffi::CStr, saved: &OnceLock<Equivalent>, replacement: Equivalent) {
    unsafe {
        let Some(class) = AnyClass::get(name) else {
            return;
        };
        let Some(method) = class.instance_method(sel!(performKeyEquivalent:)) else {
            return;
        };
        let original = std::mem::transmute::<Imp, Equivalent>(method.implementation());
        if saved.set(original).is_err() {
            return;
        }
        let replacement = std::mem::transmute::<Equivalent, Imp>(replacement);
        objc2::ffi::class_replaceMethod(
            class as *const AnyClass as *mut AnyClass,
            sel!(performKeyEquivalent:),
            replacement,
            objc2::ffi::method_getTypeEncoding(method),
        );
    }
}

unsafe extern "C-unwind" fn equivalent(
    view: *mut AnyObject,
    selector: Sel,
    event: *mut AnyObject,
) -> Bool {
    unsafe { dispatch_equivalent(&ORIGINAL, view, selector, event) }
}

unsafe extern "C-unwind" fn menu_equivalent(
    view: *mut AnyObject,
    selector: Sel,
    event: *mut AnyObject,
) -> Bool {
    unsafe { dispatch_equivalent(&ORIGINAL_MENU, view, selector, event) }
}

unsafe fn dispatch_equivalent(
    original: &OnceLock<Equivalent>,
    view: *mut AnyObject,
    selector: Sel,
    event: *mut AnyObject,
) -> Bool {
    if ROUTING.get() {
        return Bool::NO;
    }
    let app: *mut AnyObject =
        unsafe { msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication] };
    if route(app, event) {
        return Bool::YES;
    }
    original.get().map_or(Bool::NO, |original| unsafe {
        original(view, selector, event)
    })
}
pub(crate) fn forget(browser: i32) {
    ROOTS.with_borrow_mut(|roots| {
        roots.remove(&browser);
    });
}

/// Registry entries are removed before native child destruction. Do not retain
/// a registry/configuration borrow while invoking Chromium: it can re-enter.
pub(crate) fn route(app: *mut AnyObject, event: *mut AnyObject) -> bool {
    if app.is_null() || event.is_null() || ROUTING.get() {
        return false;
    }
    unsafe {
        let kind: usize = msg_send![event, type];
        if kind != 10 {
            return false;
        } // NSEventTypeKeyDown
        let window: *mut AnyObject = msg_send![app, keyWindow];
        if window.is_null() {
            return false;
        }
        let responder: *mut AnyObject = msg_send![window, firstResponder];
        if responder.is_null() {
            return false;
        }
        let roots: Vec<_> = ROOTS.with_borrow(|roots| {
            roots
                .values()
                .map(|root| (root.view, root.shortcuts.clone()))
                .collect()
        });
        let mut ancestor = responder;
        let configuration = loop {
            if let Some((_, shortcuts)) = roots.iter().find(|(view, _)| *view == ancestor as usize)
            {
                let hidden: objc2::runtime::Bool = msg_send![ancestor, isHiddenOrHasHiddenAncestor];
                if hidden.as_bool() {
                    break None;
                }
                break Some(shortcuts.clone());
            }
            let can_walk: objc2::runtime::Bool =
                msg_send![ancestor, respondsToSelector: objc2::sel!(superview)];
            if !can_walk.as_bool() {
                break None;
            }
            ancestor = msg_send![ancestor, superview];
            if ancestor.is_null() {
                break None;
            }
        };
        let Some(configuration) = configuration else {
            return false;
        };
        let characters: *mut NSString = msg_send![event, charactersIgnoringModifiers];
        if characters.is_null() {
            return false;
        }
        let text = (&*characters).to_string();
        let flags: usize = msg_send![event, modifierFlags];
        let code: u16 = msg_send![event, keyCode];
        let key = native_shortcut(&text, flags, code, ascii_command_layout());

        let yield_to_website = {
            let configuration = configuration.lock().unwrap();

            configuration.contains(&key)
                && configuration.priority(&key) != ShortcutPriority::PreferBrowser
        };
        if !yield_to_website {
            return false;
        }
        ROUTING.set(true);
        let _: () = msg_send![responder, keyDown: event];
        ROUTING.set(false);
        true
    }
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *const c_void;
    fn TISGetInputSourceProperty(source: *const c_void, property: *const c_void) -> *const c_void;
    static kTISPropertyInputSourceIsASCIICapable: *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFBooleanGetValue(value: *const c_void) -> u8;
    fn CFRelease(value: *const c_void);
}
fn ascii_command_layout() -> bool {
    unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        if source.is_null() {
            return false;
        }
        let capable = TISGetInputSourceProperty(source, kTISPropertyInputSourceIsASCIICapable);
        let use_command_layout = !capable.is_null() && CFBooleanGetValue(capable) == 0;
        CFRelease(source);
        use_command_layout
    }
}

fn native_shortcut(text: &str, flags: usize, code: u16, ascii_commands: bool) -> Shortcut {
    // Command bindings keep their ASCII command-layout key on non-Latin
    // layouts, while an ASCII layout's logical key (including Dvorak) wins.
    const ASCII: &[(u16, &str)] = &[
        (0, "a"),
        (1, "s"),
        (2, "d"),
        (3, "f"),
        (4, "h"),
        (5, "g"),
        (6, "z"),
        (7, "x"),
        (8, "c"),
        (9, "v"),
        (11, "b"),
        (12, "q"),
        (13, "w"),
        (14, "e"),
        (15, "r"),
        (16, "y"),
        (17, "t"),
        (18, "1"),
        (19, "2"),
        (20, "3"),
        (21, "4"),
        (22, "6"),
        (23, "5"),
        (24, "="),
        (25, "9"),
        (26, "7"),
        (27, "-"),
        (28, "8"),
        (29, "0"),
        (30, "]"),
        (31, "o"),
        (32, "u"),
        (33, "["),
        (34, "i"),
        (35, "p"),
        (37, "l"),
        (38, "j"),
        (39, "'"),
        (40, "k"),
        (41, ";"),
        (42, "\\"),
        (43, ","),
        (44, "/"),
        (45, "n"),
        (46, "m"),
        (47, "."),
    ];
    let key = if let Some(function) = text
        .chars()
        .next()
        .filter(|key| ('\u{f704}'..='\u{f71b}').contains(key))
    {
        format!("f{}", function as u32 - 0xf704 + 1)
    } else {
        match text {
            "\u{f700}" => "up",
            "\u{f701}" => "down",
            "\u{f702}" => "left",
            "\u{f703}" => "right",
            "\u{1b}" => "escape",
            "\r" | "\u{3}" => "enter",
            "\u{7f}" => "backspace",
            "\u{f727}" => "insert",
            "\u{f728}" => "delete",
            "\u{f729}" => "home",
            "\u{f72b}" => "end",
            "\u{f72c}" => "pageup",
            "\u{f72d}" => "pagedown",
            "\t" => "tab",
            " " => "space",
            _ if (ascii_commands && !text.is_ascii()) || text.is_empty() => ASCII
                .iter()
                .find(|(candidate, _)| *candidate == code)
                .map_or(text, |(_, key)| *key),
            _ => text,
        }
        .to_lowercase()
    };
    Shortcut {
        key,
        cmd: flags & (1 << 20) != 0,
        ctrl: flags & (1 << 18) != 0,
        alt: flags & (1 << 19) != 0,
        shift: flags & (1 << 17) != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_latin_command_keys_match_the_binding_without_replacing_ascii_layouts() {
        assert_eq!(native_shortcut("с", 1 << 20, 8, true).key, "c");
        assert_eq!(native_shortcut("x", 1 << 20, 8, false).key, "x");
        assert_eq!(native_shortcut("ö", 1 << 20, 41, false).key, "ö");
    }
    #[test]
    fn native_equivalents_preserve_modifiers_and_chromium_key_names() {
        assert_eq!(
            native_shortcut("C", (1 << 20) | (1 << 17), 8, false),
            Shortcut {
                key: "c".into(),
                cmd: true,
                shift: true,
                ..Default::default()
            }
        );
        assert_eq!(
            native_shortcut("\u{f701}", (1 << 20) | (1 << 19), 125, false),
            Shortcut {
                key: "down".into(),
                cmd: true,
                alt: true,
                ..Default::default()
            }
        );
    }
}
