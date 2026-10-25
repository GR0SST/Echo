//! General uses the same serialized OS-registration boundary as onboarding/menu actions.
use super::*;
use crate::browser_registration::{BrowserRegistration, DefaultState};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

struct Registration {
    state: AtomicUsize,
    requests: AtomicUsize,
    accept: bool,
}
impl BrowserRegistration for Registration {
    fn state(&self) -> DefaultState {
        match self.state.load(Ordering::Relaxed) {
            0 => DefaultState::Other, 1 => DefaultState::Ours, _ => DefaultState::Unavailable,
        }
    }
    fn request(&self) -> Result<DefaultState, String> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        if self.accept { self.state.store(1, Ordering::Relaxed); }
        Ok(self.state())
    }
}
#[gpui::test]
fn general_sets_default_through_registration_without_replacing_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    let registration = Arc::new(Registration { state: AtomicUsize::new(0), requests: AtomicUsize::new(0), accept: true });
    cx.cx.update(|cx| cx.set_global(crate::setup::Registration(registration.clone())));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    let before = settings.debug_bounds("settings-button-Previews Settings…").unwrap().origin.y;
    click(&mut settings, "settings-button-Set as default");
    assert_eq!(registration.requests.load(Ordering::Relaxed), 1);
    settings.update(|window, cx| { let _ = window.draw(cx); });
    assert!(settings.debug_bounds("settings-button-Previews Settings…").unwrap().origin.y < before);
    settings.simulate_keystrokes("cmd-w");
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::Close(..))));
}
#[gpui::test]
fn general_refreshes_external_status_and_keeps_the_action_after_declining(cx: &mut TestAppContext) {
    let (_, cx, _) = open_pages(cx);
    let registration = Arc::new(Registration { state: AtomicUsize::new(1), requests: AtomicUsize::new(0), accept: false });
    cx.cx.update(|cx| cx.set_global(crate::setup::Registration(registration.clone())));
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    assert!(settings.debug_bounds("settings-button-Set as default").is_none());
    for state in [2, 0] {
        registration.state.store(state, Ordering::Relaxed);
        cx.update(|window, _| window.activate_window());
        settings.update(|window, _| window.activate_window());
        settings.run_until_parked();
        assert_eq!(settings.debug_bounds("settings-button-Set as default").is_some(), state == 0);
    }
    let before = settings.debug_bounds("settings-button-Previews Settings…").unwrap().origin.y;
    click(&mut settings, "settings-button-Set as default");
    assert_eq!(registration.requests.load(Ordering::Relaxed), 1);
    settings.update(|window, cx| { let _ = window.draw(cx); });
    assert_eq!(settings.debug_bounds("settings-button-Previews Settings…").unwrap().origin.y, before);
}
