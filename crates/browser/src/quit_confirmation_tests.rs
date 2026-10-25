use super::*;
fn warning(cx: &mut VisualTestContext) -> VisualTestContext {
    let handle = cx.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find_map(|w| w.downcast::<crate::quit_confirmation::Confirmation>())
            .expect("quit confirmation")
    });
    let visual = VisualTestContext::from_window(handle.into(), &cx.cx);
    visual.run_until_parked();
    visual
}
#[gpui::test]
fn quit_confirmation_cancel_and_escape_preserve_tabs_and_leave_producers_running(
    cx: &mut TestAppContext,
) {
    let (view, visual) = open(cx);
    let before = today_urls(&view, visual);
    visual.simulate_keystrokes("cmd-q");
    let mut sheet = warning(visual);
    assert!(!crate::quit::is_quitting());
    let card = sheet.debug_bounds("quit-confirmation-card").unwrap();
    assert_eq!(card.size, gpui::size(gpui::px(450.), gpui::px(248.)));
    assert_eq!(
        card.center(),
        sheet.update(|w, _| gpui::point(
            w.viewport_size().width / 2.,
            w.viewport_size().height / 2.
        ))
    );
    // Repeated intent raises the existing sheet instead of starting shutdown.
    sheet.cx.update(crate::quit_confirmation::request);
    assert_eq!(
        sheet.cx.update(|cx| cx
            .windows()
            .into_iter()
            .filter(|w| w
                .downcast::<crate::quit_confirmation::Confirmation>()
                .is_some())
            .count()),
        1
    );
    click(&mut sheet, "quit-cancel");
    visual.run_until_parked();
    assert!(!crate::quit::is_quitting());
    assert_eq!(today_urls(&view, visual), before);
    assert!(
        view.read_with(visual, |v, _| v.profile.handle.read_settings(|b| b
            .settings
            .enabled(browser_core::settings::Toggle::WarnBeforeQuitting)))
    );
    visual.simulate_keystrokes("cmd-q");
    let mut sheet = warning(visual);
    sheet.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(!crate::quit::is_quitting());
    assert_eq!(today_urls(&view, visual), before);
}
#[gpui::test]
fn quit_confirmation_remember_disables_the_setting_before_starting_shutdown(
    cx: &mut TestAppContext,
) {
    let (view, visual) = open(cx);
    visual.simulate_keystrokes("cmd-q");
    let mut sheet = warning(visual);
    click(&mut sheet, "quit-dont-ask");
    assert!(crate::quit::is_quitting());
    assert!(
        !view.read_with(visual, |v, _| v.profile.handle.read_settings(|b| b
            .settings
            .enabled(browser_core::settings::Toggle::WarnBeforeQuitting)))
    );
    visual.cx.update(crate::quit::reset_for_test);
}
#[gpui::test]
fn disabled_quit_warning_and_direct_shutdown_bypass_confirmation(cx: &mut TestAppContext) {
    let (view, visual) = open(cx);
    view.update(visual, |v, _| {
        v.edit_profile(|b| {
            b.settings
                .set_enabled(browser_core::settings::Toggle::WarnBeforeQuitting, false)
        })
    });
    visual.simulate_keystrokes("cmd-q");
    assert!(crate::quit::is_quitting());
    assert!(visual.cx.update(|cx| cx.windows().into_iter().all(|w| w
        .downcast::<crate::quit_confirmation::Confirmation>()
        .is_none())));
    visual.cx.update(crate::quit::reset_for_test);
}

#[gpui::test]
fn quit_from_a_private_window_uses_the_install_preference_and_its_own_host(
    cx: &mut TestAppContext,
) {
    let (regular, visual) = open(cx);
    regular.update(visual, |v, cx| v.retain_normal_source(cx));
    visual.simulate_keystrokes("cmd-shift-n");
    visual.run_until_parked();
    let handle = visual.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|w| w.downcast::<BrowserWindow>())
            .find(|w| w.read(cx).is_ok_and(|v| v.browser.is_private()))
            .unwrap()
    });
    let mut private = VisualTestContext::from_window(handle.into(), &visual.cx);
    private.update(|window, _| window.activate_window());
    private.simulate_keystrokes("cmd-q");
    let mut sheet = warning(&mut private);
    sheet.cx.update(|cx| {
        let warning = cx
            .windows()
            .into_iter()
            .find_map(|w| w.downcast::<crate::quit_confirmation::Confirmation>())
            .unwrap();
        assert_eq!(warning.read(cx).unwrap().owner_id(), handle.window_id());
    });
    click(&mut sheet, "quit-dont-ask");
    assert!(crate::quit::is_quitting());
    assert!(
        !regular.read_with(visual, |v, _| v.profile.handle.read_settings(|b| b
            .settings
            .enabled(browser_core::settings::Toggle::WarnBeforeQuitting)))
    );
    visual.cx.update(crate::quit::reset_for_test);
}

#[gpui::test]
fn settings_quit_confirmation_uses_its_browser_instead_of_the_first_window(
    cx: &mut TestAppContext,
) {
    let (first, visual) = open(cx);
    let (second, mut browser) = another_profile_window(&first, visual);
    let second_window = second.read_with(&mut browser, |view, _| view.main_window);
    browser.update(|window, _| window.activate_window());
    browser.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(&mut browser);
    settings.update(|window, _| window.activate_window());
    settings.cx.update(crate::quit_confirmation::request);
    let mut sheet = warning(&mut settings);
    sheet.cx.update(|cx| {
        let handle = cx
            .windows()
            .into_iter()
            .find_map(|w| w.downcast::<crate::quit_confirmation::Confirmation>())
            .unwrap();
        assert_eq!(
            handle.read(cx).unwrap().owner_id(),
            second_window.window_id()
        );
    });
    sheet.simulate_keystrokes("escape");
    assert!(!crate::quit::is_quitting());
}
