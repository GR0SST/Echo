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
