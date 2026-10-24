//! Main-menu utility behavior on GPUI’s test platform.
use super::*;

fn check_about_closing_without_browser(cx: &mut TestAppContext, key: &str) {
    cx.update(init);
    cx.update(|cx| cx.dispatch_action(&crate::menus::About));
    cx.run_until_parked();
    let handle = cx.read(|cx| {
        assert_eq!(cx.windows().len(), 1);
        cx.windows()[0]
            .downcast::<crate::about::AboutWindow>()
            .unwrap()
    });
    cx.update(|cx| cx.dispatch_action(&crate::menus::About));
    cx.run_until_parked();
    assert_eq!(cx.read(|cx| cx.windows().len()), 1);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes(key);
    visual.run_until_parked();
    assert!(visual.cx.read(|cx| cx.windows().is_empty()));
}

#[gpui::test]
fn about_menu_opens_once_without_a_browser_and_escape_closes_it(cx: &mut TestAppContext) {
    check_about_closing_without_browser(cx, "escape");
}

#[gpui::test]
fn about_menu_without_a_browser_closes_with_command_w(cx: &mut TestAppContext) {
    check_about_closing_without_browser(cx, "cmd-w");
}

#[gpui::test]
fn essential_shortcuts_view_all_opens_shortcuts_settings_and_learning_reuses_panel(
    cx: &mut TestAppContext,
) {
    let (view, visual) = open(cx);
    let count = view.read_with(visual, |view, _| view.browser.all_tabs().count());
    visual
        .cx
        .update(|cx| cx.dispatch_action(&crate::essential_shortcuts::EssentialShortcuts));
    visual.run_until_parked();
    let help = visual.cx.read(|cx| {
        cx.windows()
            .into_iter()
            .find_map(|window| window.downcast::<crate::essential_shortcuts::ShortcutsWindow>())
            .unwrap()
    });
    let mut panel = VisualTestContext::from_window(help.into(), &visual.cx);
    panel.run_until_parked();
    click(&mut panel, "shortcuts-view-all");
    panel.run_until_parked();
    let settings = panel.cx.read(|cx| {
        cx.windows()
            .into_iter()
            .find_map(|window| window.downcast::<crate::settings::SettingsWindow>())
            .unwrap()
    });
    assert_eq!(
        panel.cx.read(|cx| settings.read(cx).unwrap().tab),
        crate::settings::Tab::Shortcuts
    );
    let mut settings_visual = VisualTestContext::from_window(settings.into(), &panel.cx);
    settings_visual.run_until_parked();
    click(&mut settings_visual, "learn-essential-shortcuts");
    settings_visual.run_until_parked();
    assert_eq!(
        settings_visual.cx.read(|cx| cx
            .windows()
            .into_iter()
            .filter(|window| window
                .downcast::<crate::essential_shortcuts::ShortcutsWindow>()
                .is_some())
            .count()),
        1
    );
    let reopened = settings_visual.cx.read(|cx| {
        cx.windows()
            .into_iter()
            .find_map(|window| window.downcast::<crate::essential_shortcuts::ShortcutsWindow>())
            .unwrap()
    });
    assert_ne!(reopened.window_id(), help.window_id());
    let mut reopened_visual = VisualTestContext::from_window(reopened.into(), &settings_visual.cx);
    reopened_visual.run_until_parked();
    reopened_visual.simulate_keystrokes("escape");
    reopened_visual.run_until_parked();
    assert!(reopened_visual
        .cx
        .read(|cx| cx.windows().into_iter().all(|window| window
            .downcast::<crate::essential_shortcuts::ShortcutsWindow>()
            .is_none())));
    assert_eq!(
        view.read_with(visual, |view, _| view.browser.all_tabs().count()),
        count
    );
}

#[gpui::test]
fn about_from_command_bar_dismisses_the_draft_without_late_page_focus(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    let original = active_url(&view, visual);
    fake.take_calls();
    visual.simulate_keystrokes("cmd-t");
    let mut command = bar(&view, visual);
    command.simulate_input("unfinished query");
    dispatch_menu(&mut command, "Echo", "About Echo");
    visual.executor().advance_clock(Duration::from_secs(1));
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
    let about = visual.cx.read(|cx| {
        cx.windows()
            .into_iter()
            .find_map(|window| window.downcast::<crate::about::AboutWindow>())
            .unwrap()
    });
    about
        .update(&mut visual.cx, |view, window, cx| {
            assert!(view.focus_handle(cx).is_focused(window))
        })
        .unwrap();
    assert_eq!(active_url(&view, visual), original);
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
}

#[gpui::test]
fn utility_open_cancels_a_popup_focus_restoration_already_queued_by_the_menu(
    cx: &mut TestAppContext,
) {
    let (view, visual, fake) = open_pages(cx);
    visual.simulate_keystrokes("cmd-t");
    fake.take_calls();
    view.update(visual, |view, cx| {
        view.close_command_bar(cx);
        view.open_menu_utility(crate::about::open, cx);
    });
    visual.run_until_parked();
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::FocusPage(_))));
    let about = visual.cx.read(|cx| {
        cx.windows()
            .into_iter()
            .find_map(|window| window.downcast::<crate::about::AboutWindow>())
            .unwrap()
    });
    about
        .update(&mut visual.cx, |view, window, cx| {
            assert!(view.focus_handle(cx).is_focused(window))
        })
        .unwrap();
}

#[gpui::test]
fn essential_shortcuts_without_a_browser_reuses_its_panel_and_command_w_closes_it(
    cx: &mut TestAppContext,
) {
    cx.update(init);
    cx.update(|cx| cx.dispatch_action(&crate::essential_shortcuts::EssentialShortcuts));
    cx.run_until_parked();
    let handle = cx.read(|cx| {
        cx.windows()[0]
            .downcast::<crate::essential_shortcuts::ShortcutsWindow>()
            .unwrap()
    });
    cx.update(|cx| cx.dispatch_action(&crate::essential_shortcuts::EssentialShortcuts));
    cx.run_until_parked();
    assert_eq!(cx.read(|cx| cx.windows().len()), 1);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("cmd-w");
    visual.run_until_parked();
    assert!(visual.cx.read(|cx| cx.windows().is_empty()));
}
