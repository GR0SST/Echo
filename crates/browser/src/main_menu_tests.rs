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


#[gpui::test]
fn appearance_menu_updates_retained_documents_and_shared_settings_without_navigation(cx: &mut TestAppContext) {
    use browser_core::themes::ThemeAppearance;
    let (view, visual, fake) = open_pages(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    let original = active_url(&view, visual);
    let themes = view.read_with(visual, |view, _| view.browser.spaces.iter().map(|space| space.theme.clone()).collect::<Vec<_>>());
    assert!(!visual.update(|window, cx| window.is_action_available(&crate::menus::AppearanceHeading, cx)));
    for (label, mode, scheme) in [("Light", ThemeAppearance::Light, Some(false)), ("Dark", ThemeAppearance::Dark, Some(true)), ("Automatic", ThemeAppearance::Automatic, None)] {
        fake.take_calls();
        dispatch_menu(visual, "View", label);
        assert_eq!(view.read_with(visual, |view, _| view.browser.settings.theme_appearance), mode);
        let calls = fake.take_calls();
        assert!(calls.iter().any(|call| matches!(call, Call::ColorScheme(1, value) if *value == scheme)), "{calls:?}");
        assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::Close(..))));
        assert_eq!(active_url(&view, visual), original);
    }
    assert_eq!(view.read_with(visual, |view, _| view.browser.spaces.iter().map(|space| space.theme.clone()).collect::<Vec<_>>()), themes);
}

#[gpui::test]
fn appearance_command_updates_other_window_and_survives_sidebar_undo(cx: &mut TestAppContext) {
    use browser_core::themes::ThemeAppearance;
    let (view, visual, fake) = open_pages(cx);
    let (other, mut other_visual) = another_profile_window(&view, visual);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    run_named_command(&view, visual, "switch to light mode");
    other_visual.run_until_parked();
    assert_eq!(other.read_with(&other_visual, |view, _| view.browser.settings.theme_appearance), ThemeAppearance::Light);
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
    fake.take_calls();
    visual.simulate_keystrokes("cmd-z");
    visual.run_until_parked();
    assert_eq!(view.read_with(visual, |view, _| view.browser.settings.theme_appearance), ThemeAppearance::Light);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::ColorScheme(..))));
}


#[gpui::test]
fn media_and_boosts_menu_routes_retain_the_original_page_and_dismiss_command_bar_drafts(cx: &mut TestAppContext) {
    use crate::{library::LibrarySection, sidebar_pages::SidebarPage};
    let (view, visual, fake) = open_pages(cx);
    let before = active_url(&view, visual);
    let count = view.read_with(visual, |view, _| view.browser.all_tabs().count());
    for (label, section) in [("View Boosts…", LibrarySection::Boosts), ("View Media…", LibrarySection::Media)] {
        visual.simulate_keystrokes("cmd-t");
        bar(&view, visual).simulate_input("unfinished query");
        fake.take_calls();
        dispatch_menu(&mut bar(&view, visual), "Window", label);
        assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
        assert_eq!(view.read_with(visual, |view, _| view.sidebar_navigation.controller.selected()), SidebarPage::Library);
        assert_eq!(view.read_with(visual, |view, _| view.sidebar_navigation.library_section), section);
        assert!(view.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(view.browser.active_tab().unwrap().id)));
        assert_eq!(active_url(&view, visual), before);
        assert_eq!(view.read_with(visual, |view, _| view.browser.all_tabs().count()), count);
        assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Close(..) | Call::Reload(..))));
    }
    run_named_command(&view, visual, "view media");
    assert_eq!(view.read_with(visual, |view, _| view.sidebar_navigation.library_section), LibrarySection::Media);
}

#[gpui::test]
fn clear_site_menu_and_commands_target_the_active_document_origin_without_replacing_it(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    let original = active_url(&view, visual);
    let origin = browser_core::site_settings::SiteSettings::origin(original.as_ref().unwrap()).unwrap();
    fake.0.borrow_mut().site_infos.insert(1, engine::SiteInfo {
        origin: origin.clone(), host: crate::window::address_host(&origin), secure: true,
        certificate: None, zoom_percent: 100, permissions: Vec::new(),
    });
    for (label, kind) in [("Clear Cookies and Refresh", engine::ClearSiteData::Cookies), ("Clear Cache and Refresh", engine::ClearSiteData::Cache)] {
        fake.take_calls();
        dispatch_menu(visual, "View", label);
        let calls = fake.take_calls();
        assert!(calls.contains(&Call::ClearSiteData(1, origin.clone(), kind)), "{label}: {calls:?}");
        assert_eq!(active_url(&view, visual), original);
        fake.take_calls();
        run_named_command(&view, visual, label);
        assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
        let calls = fake.take_calls();
        assert!(calls.contains(&Call::ClearSiteData(1, origin.clone(), kind)));
        assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Close(..))));
    }
}

#[gpui::test]
fn clear_site_menu_actions_follow_retained_library_pages_and_reject_internal_pages(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    dispatch_menu(visual, "Window", "Open Library…");
    click(visual, "library-nav-spaces");
    assert!(visual.update(|window, cx| window.is_action_available(&crate::menus::ClearCookiesAndRefresh, cx)));
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("browser://history");
    bar(&view, visual).simulate_keystrokes("enter");
    fake.take_calls();
    assert!(!visual.update(|window, cx| window.is_action_available(&crate::menus::ClearCacheAndRefresh, cx)));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::ClearSiteData(..))));
}


#[gpui::test]
fn file_share_captures_the_existing_document_and_command_bar_dismisses_without_navigation(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    let url = active_url(&view, visual).unwrap();
    fake.take_calls();
    dispatch_menu(visual, "File", "Share…");
    visual.executor().advance_clock(Duration::from_millis(30));
    visual.run_until_parked();
    assert_eq!(view.read_with(visual, |view, _| view.share_picker.as_ref().unwrap().urls.clone()), vec![url.clone()]);
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("share url");
    bar(&view, visual).simulate_keystrokes("enter");
    visual.executor().advance_clock(Duration::from_millis(30));
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
    assert_eq!(view.read_with(visual, |view, _| view.share_picker.as_ref().unwrap().urls.clone()), vec![url]);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::Close(..))));
}

#[gpui::test]
fn file_share_pending_presentation_rejects_a_same_address_reload(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    dispatch_menu(visual, "File", "Share…");
    visual.simulate_keystrokes("cmd-r");
    visual.executor().advance_clock(Duration::from_millis(40));
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.share_picker.is_none()));
}

#[gpui::test]
fn file_share_owner_close_cancels_a_pending_native_presenter(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    dispatch_menu(visual, "File", "Share…");
    dispatch_menu(visual, "File", "Close Window");
    visual.executor().advance_clock(Duration::from_millis(40));
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.share_picker.is_none()));
}


#[gpui::test]
fn file_share_anchor_stays_over_the_live_page_after_collapsing_the_sidebar(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-s");
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(500));
    visual.run_until_parked();
    dispatch_menu(visual, "File", "Share…");
    visual.executor().advance_clock(Duration::from_millis(40));
    visual.run_until_parked();
    view.read_with(visual, |view, _| {
        let anchor = view.share_picker.as_ref().unwrap().anchor;
        let tab = view.browser.active_tab().unwrap().id;
        let page = view.pages.as_ref().unwrap().borrow().surface_layout().into_iter().find(|(id, _)| *id == tab).unwrap().1;
        assert!((f64::from(anchor[0] + anchor[2] / 2.) - page.x - page.width / 2.).abs() < 0.1);
        assert!(anchor[2] > 0. && anchor[3] > 0., "Native picker anchors must be non-empty");
    });
}

#[gpui::test]
fn copy_info_menu_reports_retained_documents_without_account_fields_or_page_navigation(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    let original = active_url(&view, visual).unwrap();
    let tabs = view.read_with(visual, |view, _| view.browser.all_tabs().count());
    fake.take_calls();
    dispatch_menu(visual, "Help", "Copy Echo Info");
    let copied = visual.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    let info: serde_json::Value = serde_json::from_str(&copied).unwrap();
    assert_eq!(info["engineInfo"]["browser_tab_count"], tabs);
    assert_eq!(info["engineInfo"]["native_web_contents_count"], 1);
    assert_eq!(info["engineInfo"]["active_profiles_count"], 1);
    assert_eq!(info["numProfiles"], 1);
    assert!(info["deviceInfo"]["osVersion"].as_str().is_some_and(|version| !version.is_empty()));
    assert!(info.get("memberInfo").is_none());
    assert!(info.get("sentrySearchLink").is_none());
    assert!(info["deviceInfo"].get("deviceId").is_none());
    assert!(!copied.contains(&original));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::Close(..))));
    visual.executor().advance_clock(Duration::from_millis(700));
    visual.run_until_parked();
    let overlay = view.read_with(visual, |view, _| view.toast.as_ref().unwrap().overlay.unwrap());
    let mut toast = VisualTestContext::from_window(overlay.into(), &visual.cx);
    toast.run_until_parked();
    assert_eq!(f32::from(toast.debug_bounds("toast").unwrap().size.height), 40.);
    assert!(toast.debug_bounds("toast-share").is_none());
}

#[gpui::test]
fn copy_info_command_closes_its_draft_and_keeps_the_same_page_and_clipboard_schema(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    let original = active_url(&view, visual);
    fake.take_calls();
    run_named_command(&view, visual, "copy info");
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
    assert_eq!(active_url(&view, visual), original);
    let copied = visual.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    let info: serde_json::Value = serde_json::from_str(&copied).unwrap();
    assert_eq!(info["echoVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(info["engineInfo"]["chromiumVersion"], engine::chromium_version());
    assert!(view.read_with(visual, |view, _| !view.toast.as_ref().unwrap().copied_link));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::Close(..))));
}


#[gpui::test]
fn reveal_data_menu_opens_the_configured_folder_without_navigation_or_late_focus(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Data with spaces #1");
    std::fs::create_dir(&path).unwrap();
    let expected = url::Url::from_directory_path(&path).unwrap().to_string();
    let (view, visual, fake) = open_pages(cx);
    visual.update(|_, cx| cx.set_global(crate::diagnostics::DataDirectory(path)));
    let original = active_url(&view, visual);
    visual.simulate_keystrokes("cmd-t");
    let mut command = bar(&view, visual);
    command.simulate_input("unfinished query");
    fake.take_calls();
    dispatch_menu(&mut command, "Help", "Reveal Echo Data");
    visual.executor().advance_clock(Duration::from_secs(1));
    visual.run_until_parked();
    assert_eq!(visual.cx.opened_url(), Some(expected));
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
    assert_eq!(active_url(&view, visual), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::FocusPage(_))));
}

#[gpui::test]
fn reveal_data_command_uses_the_startup_directory(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let expected = url::Url::from_directory_path(directory.path()).unwrap().to_string();
    let (view, visual, _) = open_pages(cx);
    visual.update(|_, cx| cx.set_global(crate::diagnostics::DataDirectory(directory.path().into())));
    run_named_command(&view, visual, "reveal echo data");
    assert_eq!(visual.cx.opened_url(), Some(expected.clone()));
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
}

#[gpui::test]
fn reveal_data_without_a_browser_uses_the_configured_directory(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("another folder");
    let expected = url::Url::from_directory_path(&path).unwrap().to_string();
    cx.update(init);
    cx.update(|cx| {
        cx.set_global(crate::diagnostics::DataDirectory(path));
        cx.dispatch_action(&crate::menus::RevealData);
    });
    cx.run_until_parked();
    assert_eq!(cx.opened_url(), Some(expected));
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn standard_edit_menu_dispatches_to_the_text_responder_without_page_navigation(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    let original = active_url(&view, visual);
    fake.take_calls();
    dispatch_menu(visual, "Edit", "Make Upper Case");
    visual.run_until_parked();
    assert_eq!(visual.cx.read(|cx| cx.global::<crate::standard_edit::Requests>().commands.clone()), vec![crate::standard_edit::Command::Uppercase]);
    assert_eq!(active_url(&view, visual), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::Close(..))));
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn standard_edit_menus_survive_sidebar_undo_label_rebuilds(cx: &mut TestAppContext) {
    let (_, visual, _) = open_pages(cx);
    visual.update(|_, cx| crate::menus::sync_sidebar_undo(Some("moving a tab"), None, false, cx));
    dispatch_menu(visual, "Edit", "Underline");
    dispatch_menu(visual, "Edit", "Show Spelling and Grammar");
    visual.run_until_parked();
    assert_eq!(visual.cx.read(|cx| cx.global::<crate::standard_edit::Requests>().commands.clone()), vec![crate::standard_edit::Command::Underline, crate::standard_edit::Command::ShowSpelling]);
}


#[cfg(target_os = "macos")]
#[gpui::test]
fn standard_edit_from_a_command_draft_targets_its_overlay_window(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    visual.simulate_keystrokes("cmd-t");
    let mut command = bar(&view, visual);
    command.simulate_input("Mispelled command draft");
    let target = command.update(|window, _| window.window_handle().window_id());
    fake.take_calls();
    dispatch_menu(&mut command, "Edit", "Show Spelling and Grammar");
    command.run_until_parked();
    assert_eq!(command.cx.read(|cx| cx.global::<crate::standard_edit::Requests>().windows.clone()), vec![target]);
    assert_eq!(bar_text(&view, visual).as_deref(), Some("Mispelled command draft"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_) | Call::Create(..) | Call::Navigate(..))));
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn saved_custom_shortcuts_keep_standard_edit_actions_routable(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    let before = today_urls(&view, visual).len();
    view.update(visual, |view, cx| {
        view.edit_browser(|browser| { browser.settings.shortcuts.bindings.insert("Duplicate".into(), Some("ctrl-alt-1".into())); });
        crate::shortcuts::apply(&view.browser.settings.shortcuts, cx);
    });
    visual.simulate_keystrokes("ctrl-alt-1");
    visual.run_until_parked();
    assert_eq!(today_urls(&view, visual).len(), before + 1);
    dispatch_menu(visual, "Edit", "Underline");
    visual.run_until_parked();
    assert_eq!(visual.cx.read(|cx| cx.global::<crate::standard_edit::Requests>().commands.clone()), vec![crate::standard_edit::Command::Underline]);
}


#[gpui::test]
fn find_panel_matches_reference_inset_and_only_shows_navigation_for_a_query(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    visual.simulate_keystrokes("cmd-f");
    let mut find = find_overlay(&view, visual);
    let panel = find.debug_bounds("find-bar").unwrap();
    let main = visual.update(|window, _| window.bounds());
    let overlay = find.update(|window, _| window.bounds());
    assert_eq!(panel.size, gpui::size(gpui::px(300.), gpui::px(32.)));
    assert_eq!(overlay.origin.x + panel.origin.x, main.origin.x + main.size.width - gpui::px(320.));
    assert_eq!(overlay.origin.y + panel.origin.y, main.origin.y + gpui::px(20.));
    assert!(find.debug_bounds("find-status").is_none());
    assert!(find.debug_bounds("find-next").is_none());
    assert!(find.debug_bounds("find-previous").is_none());
    find.simulate_input("needle");
    assert!(find.debug_bounds("find-status").is_some());
    click(&mut find, "find-next");
    assert!(fake.take_calls().contains(&Call::Find(1, "needle".into(), true, true)));
    fake.send(visual, Event::FindResult { view: page(1), count: 0, active: 0, final_update: true });
    assert!(find.debug_bounds("find-not-found").is_some());
    find.simulate_keystrokes("cmd-a backspace");
    find.run_until_parked();
    assert!(view.read_with(visual, |view, cx| view.find_bar.as_ref().unwrap().input.read(cx).text().is_empty()));
    assert!(fake.take_calls().contains(&Call::StopFinding(1)));
    find.simulate_keystrokes("escape");
    assert!(view.read_with(visual, |view, _| view.find_bar.is_none()));
}

#[gpui::test]
fn command_page_navigation_previews_urls_restores_the_draft_and_edits_the_preview(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    record_history_fixture(&fake, visual);
    fake.take_calls();
    visual.simulate_keystrokes("cmd-t");
    let mut command = bar(&view, visual);
    command.simulate_input("first page");
    let original = view.read_with(visual, |view, cx| view.suggestions(cx));
    command.simulate_keystrokes("down");
    assert_eq!(bar_text(&view, visual).as_deref(), Some("example.com/one"));
    view.read_with(visual, |view, cx| {
        assert_eq!(view.command_bar.as_ref().unwrap().selected, 1);
        assert_eq!(view.command_bar.as_ref().unwrap().query(cx), "first page");
        assert_eq!(view.suggestions(cx), original);
    });
    command.simulate_keystrokes("up");
    assert_eq!(bar_text(&view, visual).as_deref(), Some("first page"));
    command.simulate_keystrokes("down");
    command.simulate_input("-edited");
    assert_eq!(bar_text(&view, visual).as_deref(), Some("example.com/one-edited"));
    view.read_with(visual, |view, cx| {
        let bar = view.command_bar.as_ref().unwrap();
        assert_eq!(bar.selected, 0);
        assert!(bar.navigation_query.is_none());
        assert!(matches!(&view.suggestions(cx)[0], Suggestion::Load(browser_core::omnibox::Resolved::Url(url)) if url == "https://example.com/one-edited"));
    });
    command.simulate_keystrokes("escape");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Navigate(..) | Call::Create(..) | Call::Reload(..))));
}

#[gpui::test]
fn command_open_tab_preview_keeps_the_original_result_and_confirms_the_tab(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    record_history_fixture(&fake, visual);
    let tab = view.read_with(visual, |view, _| view.browser.active_tab().unwrap().id);
    view.update(visual, |w, _| w.edit_browser(|b| b.new_tab("https://unrelated.example", std::time::SystemTime::now())));
    visual.simulate_keystrokes("cmd-t");
    let mut command = bar(&view, visual);
    command.simulate_input("second page");
    assert!(view.read_with(visual, |view, cx| matches!(&view.suggestions(cx)[1], Suggestion::SwitchToTab { id, .. } if *id == tab)));
    fake.take_calls();
    command.simulate_keystrokes("down");
    assert_eq!(bar_text(&view, visual).as_deref(), Some("example.com/two"));
    command.simulate_keystrokes("enter");
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none() && view.browser.active_tab().unwrap().id == tab));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Navigate(..) | Call::Create(..) | Call::Reload(..))));
}
