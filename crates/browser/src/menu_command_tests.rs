//! Existing application-menu flows are reachable by name without page navigation.
use super::*;
use crate::window::Command;
use crate::browser_registration::{BrowserRegistration, DefaultState};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

struct Registration {
    state: AtomicUsize,
    reads: AtomicUsize,
    requests: AtomicUsize,
}
impl BrowserRegistration for Registration {
    fn state(&self) -> DefaultState {
        self.reads.fetch_add(1, Ordering::Relaxed);
        match self.state.load(Ordering::Relaxed) { 0 => DefaultState::Other, 1 => DefaultState::Ours, _ => DefaultState::Unavailable }
    }
    fn request(&self) -> Result<DefaultState, String> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        Err("Cancelled".into())
    }
}
fn offers(view: &Entity<BrowserWindow>, cx: &VisualTestContext, command: Command) -> bool {
    view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|s| matches!(s, Suggestion::Action { command: found, .. } if *found == command)))
}
#[gpui::test]
fn default_browser_command_snapshots_availability_and_uses_serialized_registration(cx: &mut TestAppContext) {
    let (view,cx)=open(cx);
    let registration=Arc::new(Registration { state:AtomicUsize::new(0),reads:AtomicUsize::new(0),requests:AtomicUsize::new(0) });
    cx.cx.update(|cx|cx.set_global(crate::setup::Registration(registration.clone())));
    cx.simulate_keystrokes("cmd-t");
    let mut input=bar(&view,cx);
    input.simulate_input("default browser");
    assert!(offers(&view,cx,Command::SetDefaultBrowser));
    input.simulate_keystrokes("cmd-a");input.simulate_input("set as default browser");
    assert!(offers(&view,cx,Command::SetDefaultBrowser));
    assert_eq!(registration.reads.load(Ordering::Relaxed),1);
    input.simulate_keystrokes("enter");cx.run_until_parked();
    assert!(view.read_with(cx,|w,_|w.command_bar.is_none()));
    assert_eq!(registration.requests.load(Ordering::Relaxed),1);
    for state in [1,2] {
        registration.state.store(state,Ordering::Relaxed);
        cx.simulate_keystrokes("cmd-t");bar(&view,cx).simulate_input("set as default browser");
        assert!(!offers(&view,cx,Command::SetDefaultBrowser));
        bar(&view,cx).simulate_keystrokes("escape");
    }
    assert_eq!(registration.requests.load(Ordering::Relaxed),1);
}
#[gpui::test]
fn import_command_opens_the_existing_picker_and_cancel_keeps_the_live_page(cx: &mut TestAppContext) {
    let (view,cx,fake)=open_pages(cx);let _fixture=import_fixture(cx);
    let active=active_url(&view,cx);fake.take_calls();
    run_named_command(&view,cx,"import from another browser");
    assert!(view.read_with(cx,|w,_|w.command_bar.is_none() && w.setup.is_some()));
    let (_,mut panel)=local_setup(&view,cx);panel.simulate_keystrokes("escape");cx.run_until_parked();
    assert!(view.read_with(cx,|w,_|w.setup.is_none()));
    assert_eq!(active_url(&view,cx),active);
    assert!(!fake.take_calls().iter().any(|call|matches!(call,Call::Create(..)|Call::Close(..)|Call::Navigate(..))));
}
#[gpui::test]
fn about_command_opens_the_existing_utility_and_dismisses_the_query(cx: &mut TestAppContext) {
    let (view,cx,fake)=open_pages(cx);let active=active_url(&view,cx);fake.take_calls();
    run_named_command(&view,cx,"about echo");cx.run_until_parked();
    assert!(view.read_with(cx,|w,_|w.command_bar.is_none()));
    assert!(cx.cx.update(|cx|cx.windows().iter().any(|window|window.downcast::<crate::about::AboutWindow>().is_some())));
    assert_eq!(active_url(&view,cx),active);
    assert!(!fake.take_calls().iter().any(|call|matches!(call,Call::Create(..)|Call::Close(..)|Call::Navigate(..))));
}
fn import_unavailable(cx: &mut TestAppContext, browser: browser_core::Browser) {
    cx.update(init);
    let (view,cx)=cx.add_window_view(|window,cx| { let view=BrowserWindow::new(browser,None,window,cx); view.focus(window);view });
    cx.run_until_parked();cx.simulate_keystrokes("cmd-t");bar(&view,cx).simulate_input("import from another browser");
    assert!(!offers(&view,cx,Command::ImportBrowser));
    let before=view.read_with(cx,|w,_|w.session_json());
    view.update(cx,|w,cx|w.run_command(Command::ImportBrowser,cx));cx.run_until_parked();
    assert!(view.read_with(cx,|w,_|w.setup.is_none()));assert_eq!(view.read_with(cx,|w,_|w.session_json()),before);
}
#[gpui::test]
fn import_command_is_unavailable_in_private_windows(cx: &mut TestAppContext) { import_unavailable(cx,browser_core::Browser::new_private()); }
#[gpui::test]
fn import_command_is_unavailable_in_blank_windows(cx: &mut TestAppContext) { import_unavailable(cx,browser_core::Browser::new_blank()); }
