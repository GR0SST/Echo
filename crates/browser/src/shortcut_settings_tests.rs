use super::*;
fn search_shortcuts(
    settings: &Entity<crate::settings::SettingsWindow>,
    visual: &mut VisualTestContext,
    query: &str,
) {
    click(visual, "settings-tab-Shortcuts");
    click(visual, "shortcut-search");
    visual.simulate_keystrokes("cmd-a");
    visual.simulate_input(query);
    visual.run_until_parked();
    assert_eq!(
        settings.read_with(visual, |s, cx| s.shortcut_query(cx)),
        query
    );
}
#[gpui::test]
fn custom_shortcut_records_dispatches_and_reset_restores_an_unbound_action(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx).len();
    cx.simulate_keystrokes("cmd-,");
    let (settings, mut visual) = settings_window(cx);
    search_shortcuts(&settings, &mut visual, "Duplicate");
    click(&mut visual, "shortcut-record-Duplicate");
    visual.simulate_keystrokes("ctrl-alt-1");
    assert_eq!(
        view.read_with(cx, |v, _| v
            .browser
            .settings
            .shortcuts
            .bindings
            .get("Duplicate")
            .cloned()),
        Some(Some("ctrl-alt-1".into()))
    );
    visual.simulate_keystrokes("cmd-w");
    cx.update(|window, cx| view.read(cx).focus_handle.focus(window));
    cx.simulate_keystrokes("ctrl-alt-1");
    assert_eq!(today_urls(&view, cx).len(), before + 1);
    cx.simulate_keystrokes("cmd-,");
    let (settings, mut visual) = settings_window(cx);
    search_shortcuts(&settings, &mut visual, "Duplicate");
    click(&mut visual, "shortcut-select-Duplicate");
    click(&mut visual, "reset-one-shortcut");
    assert!(!view.read_with(cx, |v, _| v
        .browser
        .settings
        .shortcuts
        .bindings
        .contains_key("Duplicate")));
    visual.simulate_keystrokes("cmd-w");
    cx.update(|window, cx| view.read(cx).focus_handle.focus(window));
    cx.simulate_keystrokes("ctrl-alt-1");
    assert_eq!(today_urls(&view, cx).len(), before + 1);
}
#[gpui::test]
fn recorder_cancels_without_mutation_clears_with_delete_and_marks_a_conflict(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-,");
    let (settings, mut visual) = settings_window(cx);
    search_shortcuts(&settings, &mut visual, "Copy URL");
    click(&mut visual, "shortcut-record-Copy URL");
    visual.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |v, _| v.browser.settings.shortcuts.bindings.is_empty()));
    click(&mut visual, "shortcut-record-Copy URL");
    visual.simulate_keystrokes("cmd-1");
    assert!(settings.read_with(&visual, |s, _| s.shortcut_has_conflict("Copy URL")));
    assert_eq!(
        view.read_with(cx, |v, _| v
            .browser
            .settings
            .shortcuts
            .bindings
            .get("Copy URL")
            .cloned()),
        Some(Some("cmd-1".into()))
    );
    click(&mut visual, "shortcut-record-Copy URL");
    visual.simulate_keystrokes("backspace");
    assert_eq!(
        view.read_with(cx, |v, _| v
            .browser
            .settings
            .shortcuts
            .bindings
            .get("Copy URL")
            .cloned()),
        Some(None)
    );
    assert!(!settings.read_with(&visual, |s, _| s.shortcut_has_conflict("Copy URL")));
    click(&mut visual, "reset-one-shortcut");
    assert!(view.read_with(cx, |v, _| v.browser.settings.shortcuts.bindings.is_empty()));
}

#[gpui::test]
fn a_released_default_can_be_reassigned_and_reset_all_restores_both_actions(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-,");
    let (settings, mut visual) = settings_window(cx);
    search_shortcuts(&settings, &mut visual, "Copy URL");
    click(&mut visual, "shortcut-record-Copy URL");
    visual.simulate_keystrokes("cmd-r");
    assert!(settings.read_with(&visual, |s, _| s.shortcut_has_conflict("Copy URL")));
    search_shortcuts(&settings, &mut visual, "Refresh the Page");
    click(&mut visual, "shortcut-record-Refresh the Page");
    visual.simulate_keystrokes("ctrl-alt-1");
    assert!(!settings.read_with(&visual, |s, _| s.shortcut_has_conflict("Copy URL")));
    visual.simulate_keystrokes("cmd-w");
    cx.update(|window, cx| view.read(cx).focus_handle.focus(window));
    cx.simulate_keystrokes("cmd-r");
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert!(clipboard.is_some_and(|text| text.starts_with("https://")));
    cx.simulate_keystrokes("cmd-,");
    let (_, mut visual) = settings_window(cx);
    click(&mut visual, "settings-tab-Shortcuts");
    click(&mut visual, "reset-all-shortcuts");
    assert!(view.read_with(cx, |v, _| v.browser.settings.shortcuts.bindings.is_empty()));
}

fn footer_menu(
    settings: &Entity<crate::settings::SettingsWindow>,
    visual: &mut VisualTestContext,
) -> VisualTestContext {
    click(visual, "shortcut-footer-more");
    let handle = settings
        .read_with(visual, |s, _| s.shortcut_footer_window())
        .unwrap();
    let menu = VisualTestContext::from_window(handle, &visual.cx);
    menu.run_until_parked();
    menu
}
#[gpui::test]
fn shortcut_menu_removes_the_other_default_and_the_reassigned_key_dispatches_the_selected_action(
    cx: &mut TestAppContext,
) {
    let (browser, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-,");
    let (settings, mut visual) = settings_window(cx);
    search_shortcuts(&settings, &mut visual, "Copy URL");
    click(&mut visual, "shortcut-record-Copy URL");
    visual.simulate_keystrokes("cmd-r");
    let mut menu = footer_menu(&settings, &mut visual);
    assert_eq!(
        menu.debug_bounds("shortcut-footer-menu").unwrap().size,
        gpui::size(gpui::px(206.), gpui::px(117.))
    );
    menu.simulate_keystrokes("down enter");
    assert!(!settings.read_with(&visual, |s, _| s.shortcut_has_conflict("Copy URL")));
    browser.read_with(cx, |v, _| {
        assert_eq!(
            v.browser.settings.shortcuts.bindings.get("Copy URL"),
            Some(&Some("cmd-r".into()))
        );
        assert_eq!(
            v.browser
                .settings
                .shortcuts
                .bindings
                .get("Refresh the Page"),
            Some(&None)
        );
    });
    visual.simulate_keystrokes("cmd-w");
    cx.update(|w, cx| browser.read(cx).focus_handle.focus(w));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-r");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(copied, active_url(&browser, cx));
    assert!(!fake
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::Reload(..))));
    cx.simulate_keystrokes("cmd-,");
    let (settings, mut visual) = settings_window(cx);
    search_shortcuts(&settings, &mut visual, "Copy URL");
    click(&mut visual, "shortcut-select-Copy URL");
    let mut menu = footer_menu(&settings, &mut visual);
    assert_eq!(
        menu.debug_bounds("shortcut-footer-menu").unwrap().size,
        gpui::size(gpui::px(164.), gpui::px(93.))
    );
    click(&mut menu, "shortcut-menu-reset-all");
    assert!(browser.read_with(cx, |v, _| v.browser.settings.shortcuts.bindings.is_empty()));
    visual.simulate_keystrokes("cmd-w");
    cx.update(|w, cx| browser.read(cx).focus_handle.focus(w));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-r");
    assert!(fake
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::Reload(..))));
}
#[gpui::test]
fn shortcut_menu_removes_all_other_custom_conflicts_without_changing_the_selected_binding(
    cx: &mut TestAppContext,
) {
    let (browser, cx) = open(cx);
    cx.simulate_keystrokes("cmd-,");
    let (settings, mut visual) = settings_window(cx);
    for action in ["Duplicate", "Refresh the Page", "Copy URL"] {
        search_shortcuts(&settings, &mut visual, action);
        click(
            &mut visual,
            test_selector(format!("shortcut-record-{action}")),
        );
        visual.simulate_keystrokes("ctrl-alt-1");
    }
    let mut menu = footer_menu(&settings, &mut visual);
    assert_eq!(
        menu.debug_bounds("shortcut-footer-menu").unwrap().size,
        gpui::size(gpui::px(212.), gpui::px(117.))
    );
    click(&mut menu, "shortcut-menu-remove-conflicts");
    browser.read_with(cx, |v, _| {
        assert_eq!(
            v.browser.settings.shortcuts.bindings.get("Copy URL"),
            Some(&Some("ctrl-alt-1".into()))
        );
        assert_eq!(
            v.browser.settings.shortcuts.bindings.get("Duplicate"),
            Some(&None)
        );
        assert_eq!(
            v.browser
                .settings
                .shortcuts
                .bindings
                .get("Refresh the Page"),
            Some(&None)
        );
    });
    assert!(!settings.read_with(&visual, |s, _| s.shortcut_has_conflict("Copy URL")));
    let mut menu = footer_menu(&settings, &mut visual);
    click(&mut menu, "shortcut-menu-remove");
    browser.read_with(cx, |v, _| {
        assert_eq!(
            v.browser.settings.shortcuts.bindings.get("Copy URL"),
            Some(&None)
        )
    });
}
#[gpui::test]
fn stale_shortcut_conflict_menu_does_not_delete_a_new_assignment(cx: &mut TestAppContext) {
    let (browser, cx) = open(cx);
    cx.simulate_keystrokes("cmd-,");
    let (settings, mut visual) = settings_window(cx);
    search_shortcuts(&settings, &mut visual, "Copy URL");
    click(&mut visual, "shortcut-record-Copy URL");
    visual.simulate_keystrokes("cmd-r");
    let mut menu = footer_menu(&settings, &mut visual);
    browser.update(cx, |v, _| {
        v.edit_profile(|b| {
            b.settings
                .shortcuts
                .set("Refresh the Page", Some("ctrl-alt-9".into()))
        })
    });
    click(&mut menu, "shortcut-menu-remove-conflicts");
    browser.read_with(cx, |v, _| {
        assert_eq!(
            v.browser.settings.shortcuts.bindings.get("Copy URL"),
            Some(&Some("cmd-r".into()))
        );
        assert_eq!(
            v.browser
                .settings
                .shortcuts
                .bindings
                .get("Refresh the Page"),
            Some(&Some("ctrl-alt-9".into()))
        );
    });
}
