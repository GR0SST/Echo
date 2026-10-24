//! System-wide Little lookup entry point. Registration is owned by the UI loop,
//! so dropping the task unregisters it before the process finishes quitting.
use global_hotkey::{hotkey::{Code, HotKey, Modifiers}, GlobalHotKeyEvent, HotKeyState};
use gpui::App;

fn binding(test_control: bool) -> HotKey {
    let command = if cfg!(target_os = "macos") { Modifiers::META } else { Modifiers::CONTROL };
    let mut modifiers = command | Modifiers::ALT;
    if test_control { modifiers |= Modifiers::CONTROL; }
    HotKey::new(Some(modifiers), Code::KeyN)
}

struct Presses { id: u32, held: bool }
impl Presses {
    fn receive(&mut self, event: GlobalHotKeyEvent, enabled: bool) -> bool {
        if !enabled { self.held = false; return false; }
        if event.id != self.id { return false; }
        let pressed = event.state == HotKeyState::Pressed;
        let open = pressed && !self.held;
        self.held = pressed;
        open
    }
}

pub(crate) fn enabled(cx: &App) -> bool {
    use browser_core::settings::Toggle;
    cx.try_global::<crate::new_window::NormalProfile>().map_or(Toggle::LittleWindowShortcut.default_value(),
        |source| source.profile.read_settings(|browser| browser.settings.enabled(Toggle::LittleWindowShortcut)))
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
    if crate::quit::is_quitting() { return; }
    // Native automation uses an extra modifier because Arc owns the reference
    // default while both browsers are running on the shared machine.
    let test_control = std::env::var_os("BROWSER_SCRIPT").is_some()
        && std::env::var_os("BROWSER_TEST_GLOBAL_LITTLE_CONTROL").is_some();
    let hotkey = binding(test_control);
    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(error) => { eprintln!("[global-little] shortcut unavailable: {error}"); return; }
    };
    cx.spawn(async move |cx| {
        let mut registered = false;
        let mut retry = Instant::now();
        let mut last_error = None;
        let mut presses = Presses { id: hotkey.id(), held: false };
        loop {
            cx.background_executor().timer(Duration::from_millis(16)).await;
            if crate::quit::is_quitting() { break; }
            let Ok(enabled) = cx.update(|cx| enabled(cx)) else { break; };
            if !enabled {
                if registered && manager.unregister(hotkey).is_ok() { registered = false; }
                presses.held = false;
                retry = Instant::now();
            }
            if enabled && !registered && Instant::now() >= retry {
                match manager.register(hotkey) {
                    Ok(()) => {
                        registered = true; last_error = None;
                        eprintln!("[global-little] registered id={} control-test={test_control}", hotkey.id());
                    }
                    Err(error) => {
                        let error = error.to_string();
                        if last_error.as_ref() != Some(&error) { eprintln!("[global-little] shortcut unavailable: {error}"); }
                        last_error = Some(error);
                        retry = Instant::now() + Duration::from_secs(5);
                    }
                }
            }
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if presses.receive(event, enabled && registered)
                    && cx.update(open).is_err() { return; }
            }
        }
        // The manager unregisters on drop, including normal app termination.
    }).detach();
}

// GPUI tests must never register a real OS shortcut outside the screen lock.
#[cfg(test)]
pub(crate) fn init(_: &mut App) {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_binding_uses_the_same_command_option_n_as_the_reference() {
        let command = if cfg!(target_os = "macos") { Modifiers::META } else { Modifiers::CONTROL };
        assert_eq!(binding(false).id(), HotKey::new(Some(command | Modifiers::ALT), Code::KeyN).id());
    }
    #[test]
    fn one_lookup_per_press_ignores_repeat_release_and_unrelated_events() {
        let mut presses = Presses { id: 42, held: false };
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
        let mut presses = Presses { id: 42, held: false };
        let press = || GlobalHotKeyEvent { id: 42, state: HotKeyState::Pressed };
        assert!(presses.receive(press(), true));
        assert!(!presses.receive(press(), false));
        assert!(presses.receive(press(), true));
    }

}
