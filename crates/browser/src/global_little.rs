//! System-wide Little lookup entry point. Registration is owned by the UI loop,
//! so dropping the task unregisters it before the process finishes quitting.
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, HotKeyState};
use gpui::App;

const ACTION: &str = "New Little Echo Window";

fn binding_from_key(key: &str, test_control: bool) -> Option<HotKey> {
    let key = gpui::Keystroke::parse(key).ok()?;
    let m = key.modifiers;
    // The OS registration API has no Fn modifier; never drop it and register
    // an unrelated, less-modified system-wide key.
    if m.function {
        return None;
    }
    let mut parts = Vec::new();
    if m.platform {
        parts.push(if cfg!(target_os = "macos") {
            "Super"
        } else {
            "Control"
        });
    }
    if m.control || test_control {
        parts.push("Control");
    }
    if m.alt {
        parts.push("Alt");
    }
    if m.shift {
        parts.push("Shift");
    }
    parts.push(match key.key.as_str() {
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "enter" => "Enter",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        other => other,
    });
    parts.join("+").parse().ok()
}

pub(crate) fn configured_binding(cx: &App, test_control: bool) -> Option<HotKey> {
    // A recorder must receive even the currently registered global chord.
    if cx.windows().into_iter().any(|window| {
        window
            .downcast::<crate::settings::SettingsWindow>()
            .is_some_and(|window| window.read(cx).is_ok_and(|view| view.recording_shortcut()))
    }) {
        return None;
    }
    let value = cx
        .try_global::<crate::new_window::NormalProfile>()
        .map_or_else(
            || Some("cmd-alt-n".to_owned()),
            |source| {
                source.profile.read_settings(|browser| {
                    browser
                        .settings
                        .shortcuts
                        .effective(ACTION, Some("cmd-alt-n"))
                        .map(str::to_owned)
                })
            },
        )?;
    let key = gpui::Keystroke::parse(&value).ok()?;
    // A conflicting assignment may be recorded but not installed. The OS
    // route must follow the same winner as the app, rather than bypass it.
    use gpui::AsKeystroke;
    let installed = cx.key_bindings();
    if !installed.borrow().bindings().any(|binding| {
        binding.keystrokes().len() == 1
            && binding.keystrokes()[0].as_keystroke() == &key
            && binding
                .action()
                .partial_eq(&crate::transient::NewLittleWindow)
    }) {
        return None;
    }
    binding_from_key(&value, test_control)
}

struct Presses {
    id: u32,
    held: bool,
}
impl Presses {
    fn receive(&mut self, event: GlobalHotKeyEvent, enabled: bool) -> bool {
        if !enabled {
            self.held = false;
            return false;
        }
        if event.id != self.id {
            return false;
        }
        let pressed = event.state == HotKeyState::Pressed;
        let open = pressed && !self.held;
        self.held = pressed;
        open
    }
}

pub(crate) fn enabled(cx: &App) -> bool {
    use browser_core::settings::Toggle;
    cx.try_global::<crate::new_window::NormalProfile>().map_or(
        Toggle::LittleWindowShortcut.default_value(),
        |source| {
            source
                .profile
                .read_settings(|browser| browser.settings.enabled(Toggle::LittleWindowShortcut))
        },
    )
}

pub(crate) fn open(cx: &mut App) {
    // Reuse the ordinary action route: a foreground window/overlay owns its
    // Space and private/Blank fallback; the app handler covers no-window entry.
    cx.dispatch_action(&crate::transient::NewLittleWindow);
}

#[cfg(not(test))]
pub(crate) fn init(cx: &mut App) {
    use global_hotkey::GlobalHotKeyManager;
    use gpui::prelude::*;
    use std::time::{Duration, Instant};
    if crate::quit::is_quitting() {
        return;
    }
    // Native automation uses an extra modifier because Arc owns the reference
    // default while both browsers are running on the shared machine.
    let test_control = std::env::var_os("BROWSER_SCRIPT").is_some()
        && std::env::var_os("BROWSER_TEST_GLOBAL_LITTLE_CONTROL").is_some();
    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(error) => {
            eprintln!("[global-little] shortcut unavailable: {error}");
            return;
        }
    };
    cx.spawn(async move |cx| {
        let mut registered: Option<HotKey> = None;
        let mut retry = Instant::now();
        let mut last_error = None;
        let mut presses = Presses { id: 0, held: false };
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            if crate::quit::is_quitting() {
                break;
            }
            let Ok(desired) = cx.update(|cx| {
                enabled(cx)
                    .then(|| configured_binding(cx, test_control))
                    .flatten()
            }) else {
                break;
            };
            if registered != desired {
                if let Some(old) = registered {
                    if manager.unregister(old).is_ok() {
                        registered = None;
                        presses.held = false;
                        retry = Instant::now();
                    }
                }
            }
            if registered.is_none() && Instant::now() >= retry {
                if let Some(hotkey) = desired {
                    match manager.register(hotkey) {
                        Ok(()) => {
                            registered = Some(hotkey);
                            last_error = None;
                            presses = Presses {
                                id: hotkey.id(),
                                held: false,
                            };
                            eprintln!(
                                "[global-little] registered id={} control-test={test_control}",
                                hotkey.id()
                            );
                        }
                        Err(error) => {
                            let error = error.to_string();
                            if last_error.as_ref() != Some(&error) {
                                eprintln!("[global-little] shortcut unavailable: {error}");
                            }
                            last_error = Some(error);
                            retry = Instant::now() + Duration::from_secs(5);
                        }
                    }
                }
            }
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if presses.receive(event, registered.is_some() && registered == desired)
                    && cx.update(open).is_err()
                {
                    return;
                }
            }
        }
        // The manager unregisters on drop, including normal app termination.
    })
    .detach();
}

// GPUI tests must never register a real OS shortcut outside the screen lock.
#[cfg(test)]
pub(crate) fn init(_: &mut App) {}

#[cfg(test)]
mod tests {
    use super::*;
    use global_hotkey::hotkey::{Code, Modifiers};
    #[test]
    fn default_binding_uses_the_same_command_option_n_as_the_reference() {
        let command = if cfg!(target_os = "macos") {
            Modifiers::META
        } else {
            Modifiers::CONTROL
        };
        assert_eq!(
            binding_from_key("cmd-alt-n", false).unwrap().id(),
            HotKey::new(Some(command | Modifiers::ALT), Code::KeyN).id()
        );
    }
    #[test]
    fn custom_global_keys_and_test_modifier_preserve_the_recorded_chord() {
        assert_eq!(
            binding_from_key("ctrl-alt-k", false).unwrap().id(),
            HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyK).id()
        );
        assert_eq!(
            binding_from_key("cmd-shift-f8", true).unwrap().id(),
            HotKey::new(
                Some(if cfg!(target_os = "macos") {
                    Modifiers::META | Modifiers::SHIFT | Modifiers::CONTROL
                } else {
                    Modifiers::CONTROL | Modifiers::SHIFT
                }),
                Code::F8
            )
            .id()
        );
        assert!(binding_from_key("not-a-key", false).is_none());
        assert!(binding_from_key("fn-n", false).is_none());
    }
    #[test]
    fn one_lookup_per_press_ignores_repeat_release_and_unrelated_events() {
        let mut presses = Presses {
            id: 42,
            held: false,
        };
        let event = |id, state| GlobalHotKeyEvent { id, state };
        assert!(!presses.receive(event(1, HotKeyState::Pressed), true));
        assert!(presses.receive(event(42, HotKeyState::Pressed), true));
        assert!(!presses.receive(event(42, HotKeyState::Pressed), true));
        assert!(!presses.receive(event(1, HotKeyState::Released), true));
        assert!(!presses.receive(event(42, HotKeyState::Pressed), true));
        assert!(!presses.receive(event(42, HotKeyState::Released), true));
        assert!(presses.receive(event(42, HotKeyState::Pressed), true));
    }
    #[test]
    fn disabling_the_binding_discards_queued_input_and_rearms_the_next_press() {
        let mut presses = Presses {
            id: 42,
            held: false,
        };
        let press = || GlobalHotKeyEvent {
            id: 42,
            state: HotKeyState::Pressed,
        };
        assert!(presses.receive(press(), true));
        assert!(!presses.receive(press(), false));
        assert!(presses.receive(press(), true));
    }
}
