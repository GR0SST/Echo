//! Behavioral regressions from the second native parity audit.
use super::*;

#[gpui::test]
fn current_tab_is_history_and_inactive_tab_is_switchable(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    let current = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("second page");
    view.read_with(cx, |w, cx| {
        let rows = w.suggestions(cx);
        assert!(!rows.iter().any(|row| matches!(row, Suggestion::SwitchToTab { id, .. } if *id == current)));
        assert!(rows.iter().any(|row| matches!(row, Suggestion::History { url, .. } if url == "https://example.com/two")));
    });
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://unrelated.example");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("second page");
    view.read_with(cx, |w, cx| {
        let rows = w.suggestions(cx);
        assert!(rows.iter().any(|row| matches!(row, Suggestion::SwitchToTab { id, .. } if *id == current)));
        assert!(!rows.iter().any(|row| matches!(row, Suggestion::History { url, .. } if url == "https://example.com/two")));
    });
    fake.take_calls();
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), current);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn direct_search_settings_route_sets_metadata_and_keeps_a_custom_name(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("browser://settings/searchEngines?profile=1");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    let id = view.read_with(cx, |view, _| {
        let tab = view.browser.active_tab().unwrap();
        assert_eq!(tab.title, "Settings - Manage search engines and site search");
        assert_eq!(tab.page_title.as_deref(), Some("Settings - Manage search engines and site search"));
        tab.id
    });
    assert!(cx.debug_bounds("search-settings-page").is_some());
    assert!(cx.debug_bounds(test_selector(format!("settings-page-icon-{}", id.get()))).is_some());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(_, url) if url.starts_with("browser://"))));
    view.update(cx, |view, cx| { view.edit_browser(|browser| browser.rename_tab(id, "My search settings")); cx.notify(); });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let tab = view.browser.active_tab().unwrap();
        assert_eq!(tab.title, "My search settings");
        assert_eq!(tab.page_title.as_deref(), Some("Settings - Manage search engines and site search"));
    });
}

#[gpui::test]
fn shortcuts_footer_matches_reference_geometry_and_cancel_preserves_settings(cx: &mut TestAppContext) {
    let (browser, cx) = open(cx);
    let before = today_urls(&browser, cx);
    cx.simulate_keystrokes("cmd-,");
    let (view, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Shortcuts");
    assert_eq!(settings.debug_bounds("settings-keyboard-icon").unwrap().size, gpui::size(gpui::px(22.), gpui::px(22.)));
    let reset = settings.debug_bounds("reset-all-shortcuts").unwrap();
    let more = settings.debug_bounds("shortcut-footer-more").unwrap();
    assert_eq!(reset.size, gpui::size(gpui::px(140.), gpui::px(24.)));
    assert_eq!(more.size, gpui::size(gpui::px(24.), gpui::px(24.)));
    assert_eq!(reset.right() + gpui::px(4.), more.left());
    assert_eq!(reset.center().y, more.center().y);
    click(&mut settings, "shortcut-footer-more");
    let handle = view.read_with(&settings, |view, _| view.shortcut_footer_window()).unwrap();
    let mut menu = VisualTestContext::from_window(handle, &mut settings);
    assert_eq!(menu.debug_bounds("shortcut-footer-menu").unwrap().size, gpui::size(gpui::px(166.), gpui::px(34.)));
    // Without the GPUI extension editor registered, Enter must not navigate to Chromium UI.
    menu.simulate_keystrokes("enter");
    assert_eq!(today_urls(&browser, cx), before);
    assert!(view.read_with(&settings, |view, _| view.shortcut_footer_window()).is_some());
    menu.simulate_keystrokes("escape");
    settings.run_until_parked();
    assert!(view.read_with(&settings, |view, _| view.shortcut_footer_window()).is_none());
    click(&mut settings, "shortcut-footer-more");
    let handle = view.read_with(&settings, |view, _| view.shortcut_footer_window()).unwrap();
    let mut menu = VisualTestContext::from_window(handle, &mut settings);
    let outside = menu.debug_bounds("shortcut-footer-backdrop").unwrap().center();
    menu.simulate_mouse_move(outside, None, Modifiers::none());
    menu.simulate_mouse_down(outside, gpui::MouseButton::Left, Modifiers::none());
    settings.run_until_parked();
    // Mouse-down dismisses the popup; the real mouse-up goes to its parent.
    settings.simulate_mouse_up(outside, gpui::MouseButton::Left, Modifiers::none());
    assert!(view.read_with(&settings, |view, _| view.shortcut_footer_window()).is_none());
    assert_eq!(today_urls(&browser, cx), before);
}

#[gpui::test]
fn closing_settings_from_the_footer_menu_removes_both_windows(cx: &mut TestAppContext) {
    let (browser, cx) = open(cx);
    let before = today_urls(&browser, cx);
    cx.simulate_keystrokes("cmd-,");
    let (view, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Shortcuts");
    click(&mut settings, "shortcut-footer-more");
    let handle = view.read_with(&settings, |view, _| view.shortcut_footer_window()).unwrap();
    VisualTestContext::from_window(handle, &mut settings).simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.shortcut_footer_window()).is_none());
    assert_eq!(cx.cx.read(|cx| cx.windows().len()), 1);
    assert_eq!(today_urls(&browser, cx), before);
}
