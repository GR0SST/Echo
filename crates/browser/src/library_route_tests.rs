//! Library entry routes share the existing sidebar navigation and focus lifecycle.
use super::*;
use crate::{library::LibrarySection, sidebar_pages::SidebarPage, window::Command};

fn route(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, route: usize) {
    match route {
        0 => cx.simulate_keystrokes("cmd-shift-l"),
        1 => dispatch_menu(cx, "Window", "Open Library…"),
        _ => run_named_command(view, cx, "open library"),
    }
    cx.run_until_parked();
}

fn assert_library(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.sidebar_navigation.controller.selected(),
            SidebarPage::Library
        );
        assert!(!view.page_content_visible());
        assert!(view.command_bar.is_none());
        assert!(view.find_bar.is_none());
        assert!(view.new_menu.is_none());
        assert_eq!(view.sidebar_navigation.library_section == LibrarySection::Media, view
            .pages
            .as_ref()
            .unwrap()
            .borrow()
            .is_visible(view.browser.active_tab().unwrap().id));
    });
    assert!(cx.update(|window, cx| view.read(cx).focus_handle.is_focused(window)));
}

fn assert_no_document_changes(calls: &[Call]) {
    assert!(
        !calls.iter().any(|call| matches!(
            call,
            Call::Create(..)
                | Call::Close(..)
                | Call::Navigate(..)
                | Call::Reload(..)
                | Call::ForceReload(..)
        )),
        "Library routing must preserve live documents: {calls:?}"
    );
}

fn return_to_origin(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    click(cx, "library-nav-spaces");
    click(cx, "library-space-select-0");
    assert!(view.read_with(cx, |view, _| view.page_content_visible()));
}

#[gpui::test]
fn library_shortcut_menu_and_command_preserve_the_original_page_and_spaces(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |view, _| view.session_json());
    let original = view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id);
    let document = fake.0.borrow().focused_page.unwrap();
    for entry in 0..3 {
        fake.take_calls();
        route(&view, cx, entry);
        assert_library(&view, cx);
        assert_eq!(
            view.read_with(cx, |view, _| view.sidebar_navigation.library_section),
            LibrarySection::Media
        );
        let calls = fake.take_calls();
        assert!(!calls.contains(&Call::Visible(raw(document), false)));
        assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))));
        assert_no_document_changes(&calls);
        assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
        return_to_origin(&view, cx);
        let calls = fake.take_calls();
        assert_no_document_changes(&calls);
        assert!(calls.contains(&Call::Visible(raw(document), true)));
        assert!(calls.contains(&Call::FocusPage(raw(document))));
        assert_eq!(fake.0.borrow().focused_page, Some(document));
        assert_eq!(
            view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id),
            original
        );
        assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
    }
}

#[gpui::test]
fn library_shortcut_from_cef_uses_the_registered_window_route(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |view, _| view.session_json());
    let document = fake.0.borrow().focused_page.unwrap();
    let shortcut = crate::window::page_shortcuts()
        .into_iter()
        .find(|shortcut| {
            shortcut.cmd && shortcut.shift && !shortcut.alt && !shortcut.ctrl && shortcut.key == "l"
        })
        .expect("Library shortcut must be intercepted while CEF owns focus");
    fake.take_calls();
    fake.send(cx, fake.shortcut(shortcut));
    assert_library(&view, cx);
    let calls = fake.take_calls();
    assert!(!calls.contains(&Call::Visible(raw(document), false)));
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))));
    assert_no_document_changes(&calls);
    return_to_origin(&view, cx);
    let calls = fake.take_calls();
    assert_no_document_changes(&calls);
    assert!(calls.contains(&Call::FocusPage(raw(document))));
    assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
}

#[gpui::test]
fn library_routes_keep_the_selected_section_when_library_is_already_open(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |view, _| view.session_json());
    route(&view, cx, 0);
    click(cx, "library-nav-spaces");
    for entry in 0..3 {
        fake.take_calls();
        route(&view, cx, entry);
        assert_library(&view, cx);
        assert_eq!(
            view.read_with(cx, |view, _| view.sidebar_navigation.library_section),
            LibrarySection::Spaces
        );
        let calls = fake.take_calls();
        assert_no_document_changes(&calls);
        assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))));
        assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
    }
}

#[gpui::test]
fn library_shortcut_closes_command_and_find_overlays_without_stealing_media_focus(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    for find in [false, true] {
        cx.simulate_keystrokes(if find { "cmd-f" } else { "cmd-t" });
        cx.run_until_parked();
        fake.take_calls();
        if find {
            find_overlay(&view, cx).simulate_keystrokes("cmd-shift-l");
        } else {
            bar(&view, cx).simulate_keystrokes("cmd-shift-l");
        }
        cx.run_until_parked();
        assert_library(&view, cx);
        let calls = fake.take_calls();
        assert_no_document_changes(&calls);
        assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))));
        return_to_origin(&view, cx);
    }
}

fn unavailable(cx: &mut TestAppContext, browser: browser_core::Browser) {
    cx.update(init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, None, window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    let before = view.read_with(cx, |view, _| view.session_json());
    assert!(!cx.update(|window, cx| window.is_action_available(&crate::window::ViewLibrary, cx)));
    assert!(!cx.update(|window, cx| window.is_action_available(&crate::window::ViewDownloads, cx)));
    assert!(!cx.update(|window, cx| window.is_action_available(&crate::menus::ManageSpaces, cx)));
    dispatch_menu(cx, "Spaces", "Manage Spaces…");
    cx.simulate_keystrokes("cmd-shift-j");
    dispatch_menu(cx, "Window", "View Downloads…");
    cx.simulate_keystrokes("cmd-shift-l");
    dispatch_menu(cx, "Window", "Open Library…");
    assert!(view.read_with(cx, |view, _| view.page_content_visible()));
    assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("open library");
    assert!(
        !view.read_with(cx, |view, cx| view.suggestions(cx).iter().any(
            |suggestion| matches!(
                suggestion,
                Suggestion::Action {
                    command: Command::OpenLibrary,
                    ..
                }
            )
        ))
    );
    assert!(!bar(&view, cx)
        .update(|window, cx| window.is_action_available(&crate::window::ViewLibrary, cx)));
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("manage spaces");
    assert!(!view.read_with(cx, |view, cx| view.suggestions(cx).iter().any(|row|
        matches!(row, Suggestion::Action { command: Command::ManageSpaces, .. }))));
    assert!(!bar(&view, cx)
        .update(|window, cx| window.is_action_available(&crate::menus::ManageSpaces, cx)));
    view.update(cx, |view, cx| {
        view.run_command(Command::OpenLibrary, cx);
        view.run_command(Command::ViewDownloads, cx);
        view.run_command(Command::ManageSpaces, cx);
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.page_content_visible()));
    assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
}

#[gpui::test]
fn private_windows_do_not_offer_library_routes(cx: &mut TestAppContext) {
    unavailable(cx, browser_core::Browser::new_private());
}

#[gpui::test]
fn blank_windows_do_not_offer_library_routes(cx: &mut TestAppContext) {
    unavailable(cx, browser_core::Browser::new_blank());
}

#[test]
fn library_menu_uses_the_observed_window_menu_position() {
    let menu = crate::menus::build()
        .into_iter()
        .find(|menu| menu.name.as_ref() == "Window")
        .unwrap();
    let names: Vec<_> = menu
        .items
        .into_iter()
        .filter_map(|item| match item {
            gpui::MenuItem::Action { name, .. } => Some(name.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        [
            "Minimize Window",
            "Zoom",
            "Open Library…",
            "View Downloads…",
            "View Media…",
            "View Boosts…",
            "Bring All to Front"
        ]
    );
}

#[gpui::test]
fn manage_spaces_menu_and_command_target_spaces_and_preserve_the_live_document(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |view, _| view.session_json());
    let document = fake.0.borrow().focused_page.unwrap();
    for route in 0..3 {
        fake.take_calls();
        match route {
            0 => dispatch_menu(cx, "Spaces", "Manage Spaces…"),
            1 => run_named_command(&view, cx, "manage spaces"),
            _ => run_named_command(&view, cx, "organize"),
        }
        cx.run_until_parked();
        assert_library(&view, cx);
        assert_eq!(
            view.read_with(cx, |view, _| view.sidebar_navigation.library_section),
            LibrarySection::Spaces
        );
        let calls = fake.take_calls();
        assert_no_document_changes(&calls);
        assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))));
        assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
        return_to_origin(&view, cx);
        let calls = fake.take_calls();
        assert_no_document_changes(&calls);
        assert_eq!(fake.0.borrow().focused_page, Some(document));
        assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
    }
}

#[gpui::test]
fn manage_spaces_replaces_downloads_and_routes_from_both_popup_windows(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |view, _| view.session_json());
    run_named_command(&view, cx, "view downloads");
    assert_eq!(
        view.read_with(cx, |view, _| view.sidebar_navigation.library_section),
        LibrarySection::Downloads
    );
    for source in 0..3 {
        match source {
            1 => cx.simulate_keystrokes("cmd-f"),
            2 => cx.simulate_keystrokes("cmd-t"),
            _ => {}
        }
        cx.run_until_parked();
        fake.take_calls();
        match source {
            1 => dispatch_menu(&mut find_overlay(&view, cx), "Spaces", "Manage Spaces…"),
            2 => dispatch_menu(&mut bar(&view, cx), "Spaces", "Manage Spaces…"),
            _ => dispatch_menu(cx, "Spaces", "Manage Spaces…"),
        }
        cx.run_until_parked();
        assert_library(&view, cx);
        assert_eq!(
            view.read_with(cx, |view, _| view.sidebar_navigation.library_section),
            LibrarySection::Spaces
        );
        assert_no_document_changes(&fake.take_calls());
        assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
        return_to_origin(&view, cx);
    }
}

#[gpui::test]
fn manage_spaces_keeps_the_window_local_creation_draft_without_allocating_a_space(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |view, _| view.session_json());
    run_named_command(&view, cx, "new space");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    click(cx, "space-creation-name");
    cx.simulate_input("Retained draft");
    fake.take_calls();
    dispatch_menu(cx, "Spaces", "Manage Spaces…");
    cx.run_until_parked();
    assert_library(&view, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_navigation.library_section, LibrarySection::Spaces);
        assert_eq!(view.sidebar_navigation.controller.draft().unwrap().name, "Retained draft");
    });
    assert_no_document_changes(&fake.take_calls());
    assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
}

#[gpui::test]
fn manage_spaces_releases_a_focused_library_native_preview_to_the_manager(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    route(&view, cx, 0);
    fake.send(cx, Event::Focused { view: document });
    assert!(view.read_with(cx, |view, _| view.pages.as_ref().unwrap()
        .borrow().capture_resize_focus().has_native_focus()));
    // Native focus legitimately updates tab activity before this route starts.
    let before = view.read_with(cx, |view, _| view.session_json());
    fake.take_calls();
    dispatch_menu(cx, "Spaces", "Manage Spaces…");
    cx.run_until_parked();
    assert_library(&view, cx);
    assert_eq!(view.read_with(cx, |view, _| view.sidebar_navigation.library_section),
        LibrarySection::Spaces);
    assert!(!view.read_with(cx, |view, _| view.pages.as_ref().unwrap()
        .borrow().capture_resize_focus().has_native_focus()));
    let calls = fake.take_calls();
    assert_no_document_changes(&calls);
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))));
    return_to_origin(&view, cx);
    assert_eq!(fake.0.borrow().focused_page, Some(document));
    assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
}

#[test]
fn manage_spaces_menu_follows_the_space_switching_group() {
    let menu = crate::menus::build()
        .into_iter()
        .find(|menu| menu.name.as_ref() == "Spaces")
        .unwrap();
    let previous = menu.items.iter().position(|item| {
        matches!(item, gpui::MenuItem::Action { name, .. } if name.as_ref() == "Previous Space")
    }).unwrap();
    assert!(matches!(menu.items[previous + 1], gpui::MenuItem::Separator));
    assert!(matches!(&menu.items[previous + 2], gpui::MenuItem::Action { name, .. }
        if name.as_ref() == "Manage Spaces…"));
}
