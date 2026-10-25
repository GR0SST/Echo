//! Behavioral regressions from the second native parity audit.
use super::*;
use browser_core::omnibox::Resolved;

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

#[gpui::test]
fn site_search_hint_follows_completion_and_profile_activation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("wiki");
    cx.run_until_parked();
    assert!(bar(&view, cx).debug_bounds("site-search-tab-hint").is_some());
    bar(&view, cx).simulate_keystrokes("tab");
    cx.run_until_parked();
    assert!(bar(&view, cx).debug_bounds("site-search-pill").is_some());
    assert!(view.read_with(cx, |w, _| w.command_bar.as_ref().unwrap().site_search.is_some()));
    bar(&view, cx).simulate_keystrokes("backspace");
    bar(&view, cx).simulate_input("unrelated query");
    cx.run_until_parked();
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("wikipedia");
    view.update(cx, |view, cx| { view.edit_browser(|browser| { browser.active_user_profile_mut().settings.search.providers.iter_mut().find(|p| p.id == 7).unwrap().active = false; }); cx.notify(); });
    cx.run_until_parked();
    view.update(cx, |w, cx| assert!(!w.scope_site_search(cx)));
}

#[gpui::test]
fn editing_a_pin_keeps_tab_identity_and_navigates_its_existing_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "tab-4");
    let id = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    fake.take_calls();
    right_click(cx, "tab-4");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-edit-pinned-page");
    click(&mut menu, "tab-menu-edit-pinned-url");
    let mut editor = bar(&view, cx);
    assert_eq!(editor.debug_bounds("pinned-url-editor").unwrap().size, gpui::size(gpui::px(394.), gpui::px(265.)));
    editor.simulate_input("https://github.com/edited-pin");
    editor.simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        let tab = w.browser.active_tab().unwrap();
        assert_eq!(tab.id, id);
        assert_eq!(tab.url, "https://github.com/edited-pin");
        assert_eq!(tab.pinned_url.as_deref(), Some(tab.url.as_str()));
    });
    let calls = fake.take_calls();
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..))));
    assert!(calls.iter().any(|call| matches!(call, Call::Navigate(_, url) if url == "https://github.com/edited-pin")));
    click(cx, "close-4");
    click(cx, "tab-4");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com/edited-pin"));
}

#[gpui::test]
fn pin_editor_cancel_and_superseded_pin_cannot_mutate_a_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = active_url(&view, cx);
    for superseded in [false, true] {
        right_click(cx, "tab-4");
        let mut menu = tab_menu(&view, cx);
        click(&mut menu, "tab-menu-edit-pinned-page");
        click(&mut menu, "tab-menu-edit-pinned-url");
        bar(&view, cx).simulate_input("https://example.org/edited");
        if superseded {
            view.update(cx, |w, cx| { w.edit_browser(|b| { b.move_tab(b.space().pinned[0].id, b.space().id, browser_core::Section::Today); }); cx.notify(); });
            fake.take_calls();
            bar(&view, cx).simulate_keystrokes("enter");
            assert!(view.read_with(cx, |w, _| w.browser.all_tabs().all(|t| t.url != "https://example.org/edited")));
            assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Navigate(..))));
            assert!(view.read_with(cx, |w, _| w.command_bar.is_none()));
        } else {
            fake.take_calls();
            bar(&view, cx).simulate_keystrokes("escape");
            assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Navigate(..))));
        }
        assert_eq!(active_url(&view, cx), before);
    }
}

#[gpui::test]
fn domain_completion_keeps_the_typed_search_row_during_navigation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("git");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("github.com"));
    let rows = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(matches!(&rows[0], Suggestion::Load(Resolved::Url(url)) if url == "https://github.com"));
    assert!(matches!(&rows[1], Suggestion::Load(Resolved::Search { query, .. }) if query == "git"));
    bar(&view, cx).simulate_keystrokes("down");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("git"));
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx)), rows);
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.google.com/search?q=git"));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("git");
    bar(&view, cx).simulate_keystrokes("right enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com"));
}

#[gpui::test]
fn command_bar_shortcuts_action_opens_the_existing_settings_section(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = active_url(&view, cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("edit keyboard shortcuts");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    let (settings, _) = settings_window(cx);
    assert_eq!(settings.read_with(cx, |s, _| s.tab), crate::settings::Tab::Shortcuts);
    assert_eq!(active_url(&view, cx), before);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn suggestions_disclosure_settings_and_dismiss_retain_the_source_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let id = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rust language");
    click(&mut bar(&view, cx), "suggestions-info");
    let disclosure = bar(&view, cx).debug_bounds("suggestions-privacy").unwrap();
    assert_eq!(disclosure.size.height, gpui::px(60.));
    assert!(bar(&view, cx).debug_bounds("suggestions-privacy-link").is_some());
    fake.take_calls();
    click(&mut bar(&view, cx), "dismiss-suggestions-privacy");
    view.read_with(cx, |w, _| {
        assert!(!w.command_bar.as_ref().unwrap().suggestions_privacy);
        assert!(w.browser.active_user_profile().settings.search.notice_dismissed);
        assert_eq!(w.browser.active_tab().unwrap().id, id);
    });
    click(&mut bar(&view, cx), "suggestions-info");
    click(&mut bar(&view, cx), "suggestions-search-settings");
    cx.run_until_parked();
    assert!(active_url(&view, cx).unwrap().starts_with("browser://settings/searchEngines"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn page_suggestions_match_separate_words_in_title_and_url(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |w, cx| {
        w.edit_browser(|b| {
            let profile = b.active_user_profile_id();
            b.record_visit(profile, "https://en.wikipedia.org/wiki/Rust_(programming_language)", std::time::SystemTime::now());
            b.update_history_title(profile, "https://en.wikipedia.org/wiki/Rust_(programming_language)", "Rust (programming language)");
            let id = b.new_tab("https://example.org/rust/language-guide", std::time::SystemTime::now());
            b.set_title(id, "A language guide");
        }); cx.notify();
    });
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rust programming language");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|row| matches!(row, Suggestion::History { title, .. } if title == "Rust (programming language)"))));
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("rust absent");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|row| matches!(row, Suggestion::History { .. }))));
}

#[gpui::test]
fn empty_command_bar_uses_recency_instead_of_favorite_section_order(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let recent = view.update(cx, |w, cx| {
        let id = w.edit_browser(|b| {
            let now = std::time::SystemTime::now();
            for space in &mut b.spaces { for tab in space.pinned.iter_mut().chain(&mut space.today) { tab.last_active = now - std::time::Duration::from_secs(60); } }
            let favorite = b.new_tab("https://unused-favorite.example", now - std::time::Duration::from_secs(30));
            b.favorite_tab(favorite);
            let recent = b.new_tab("https://recent.example", now + std::time::Duration::from_secs(20));
            b.set_title(recent, "Recent local page");
            b.new_tab("https://active.example", now + std::time::Duration::from_secs(30));
            for favorite in &mut b.active_user_profile_mut().favorites { favorite.last_active = now - std::time::Duration::from_secs(30); }
            recent
        }); cx.notify(); id
    });
    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |w, cx| assert!(matches!(w.suggestions(cx).first(), Some(Suggestion::SwitchToTab { id, .. }) if *id == recent)));
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), recent);
}

#[gpui::test]
fn command_history_routes_replay_appearance_and_respect_an_editable_page(cx: &mut TestAppContext) {
    use browser_core::themes::ThemeAppearance;
    let (view, cx, fake) = open_pages(cx);
    view.update(cx, |w, cx| {
        w.edit_profile(|b| { b.clear_sidebar_history(); b.settings.theme_appearance = ThemeAppearance::Automatic; }); cx.notify();
    });
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("switch to dark mode");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    fake.take_calls();
    for (query, expected) in [("undo", ThemeAppearance::Automatic), ("redo", ThemeAppearance::Dark)] {
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        bar(&view, cx).simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |w, _| { assert!(w.command_bar.is_none()); assert_eq!(w.theme_appearance(), expected); });
    }
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::Undo(..) | Call::Redo(..))));
    fake.send(cx, Event::EditFocusChanged { view: page(1), editable: true });
    fake.take_calls();
    for query in ["undo", "redo"] {
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        bar(&view, cx).simulate_keystrokes("enter");
        cx.run_until_parked();
    }
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Undo(1)) && calls.contains(&Call::Redo(1)));
    assert_eq!(view.read_with(cx, |w, _| w.theme_appearance()), ThemeAppearance::Dark);
}
