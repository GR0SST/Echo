//! Command aliases use the same app-owned updater utility as the app menu.
use super::*;
use crate::{updates::{Status, Updates, UpdateWindow}, window::Command};

fn utility(cx: &mut VisualTestContext) -> gpui::WindowHandle<UpdateWindow> {
    cx.cx.update(|cx| {
        let windows: Vec<_> = cx.windows().into_iter().filter_map(|window| window.downcast::<UpdateWindow>()).collect();
        assert_eq!(windows.len(), 1);
        windows[0]
    })
}

#[gpui::test]
fn update_command_aliases_open_the_existing_utility_and_cancel_without_navigation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let source = view.read_with(cx, |view, _| (view.browser.active_tab().unwrap().id, view.browser.active_tab().unwrap().url.clone()));
    for query in ["check for updates", "software update", "update echo"] {
        cx.update(|window, _| window.activate_window());
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        assert!(view.read_with(cx, |view, cx| matches!(view.suggestions(cx).first(), Some(Suggestion::Action {command: Command::CheckForUpdates, ..}))));
        fake.take_calls();
        bar(&view, cx).simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(view.read_with(cx, |view, _| view.command_bar.is_none()));
        assert_eq!(cx.cx.read(|cx| cx.global::<Updates>().status.clone()), Status::Checking);
        let window = utility(cx);
        let mut update = VisualTestContext::from_window(window.into(), &cx.cx);
        update.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(!cx.cx.read(|cx| cx.windows().iter().any(|handle| handle.downcast::<UpdateWindow>().is_some())));
        assert_eq!(view.read_with(cx, |view, _| (view.browser.active_tab().unwrap().id, view.browser.active_tab().unwrap().url.clone())), source);
        assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    }
}

#[gpui::test]
fn repeated_update_commands_reuse_the_app_menu_window_and_retry_failure(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.update(|window, cx| window.dispatch_action(Box::new(crate::updates::CheckForUpdates), cx));
    cx.run_until_parked();
    let original = utility(cx);
    for status in [Status::Checking, Status::Failed] {
        cx.cx.update(|cx| cx.global_mut::<Updates>().status = status);
        cx.update(|window, _| window.activate_window());
        fake.take_calls();
        run_named_command(&view, cx, "check updates");
        assert_eq!(utility(cx), original);
        assert_eq!(cx.cx.read(|cx| cx.global::<Updates>().status.clone()), Status::Checking);
        assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    }
}
