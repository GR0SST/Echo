//! Library entry routes share the existing sidebar navigation and focus lifecycle.
use super::*;
use crate::{library::LibrarySection, sidebar_pages::SidebarPage, window::Command};

fn route(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, route: usize) {
    match route {
        0 => cx.simulate_keystrokes("cmd-shift-l"),
        1 => dispatch_menu(cx, "Window", "View Library"),
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
        assert!(!view
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
        assert!(calls.contains(&Call::Visible(raw(document), false)));
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
    assert!(calls.contains(&Call::Visible(raw(document), false)));
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
fn library_shortcut_closes_command_and_find_overlays_without_restoring_hidden_page_focus(
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
    cx.simulate_keystrokes("cmd-shift-l");
    dispatch_menu(cx, "Window", "View Library");
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
    view.update(cx, |view, cx| view.run_command(Command::OpenLibrary, cx));
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
            "View Library",
            "Bring All to Front"
        ]
    );
}
