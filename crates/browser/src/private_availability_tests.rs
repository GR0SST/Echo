//! Incognito actions cannot hide a tab in sidebar sections it never renders.
use super::*;
use crate::window::{Command, Target};

fn private(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new_private();
    browser.new_tab("https://private.example/retained", std::time::SystemTime::UNIX_EPOCH);
    let (view, visual) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.pages.as_ref().unwrap().borrow_mut().set_clock(cx.background_executor().clone());
        view.focus(window);
        view
    });
    visual.run_until_parked();
    fake.take_calls();
    (view, visual, fake)
}

fn assert_retained(view: &Entity<BrowserWindow>, cx: &VisualTestContext) {
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.spaces.len(), 1);
        assert_eq!(view.browser.space().name, "Incognito");
        assert!(view.browser.space().folders.is_empty());
        assert!(view.browser.space().pinned.is_empty());
        assert!(view.browser.active_user_profile().favorites.is_empty());
        assert_eq!(view.browser.space().today.len(), 1);
        assert_eq!(view.browser.active_tab().unwrap().url, "https://private.example/retained");
    });
}

#[gpui::test]
fn private_unrendered_destinations_are_absent_from_typed_commands(cx: &mut TestAppContext) {
    let (view, cx, _) = private(cx);
    for query in ["pin tab", "favorite tab", "new folder", "pin to new folder", "new space", "rename space", "change space icon", "show space header", "move to pinned"] {
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        view.read_with(cx, |view, cx| assert!(view.suggestions(cx).iter().all(|row|
            !matches!(row, Suggestion::Action { command, .. } if !view.private_allows_command(*command))), "{query}"));
        bar(&view, cx).simulate_keystrokes("escape");
    }
    assert_retained(&view, cx);
}

#[gpui::test]
fn private_pin_root_overlay_and_renderer_shortcuts_keep_the_visible_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = private(cx);
    cx.simulate_keystrokes("cmd-d");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_keystrokes("cmd-d");
    assert!(view.read_with(cx, |view, _| matches!(view.command_bar.as_ref().unwrap().target, Target::NewTab)));
    bar(&view, cx).simulate_keystrokes("escape");
    let shortcut = crate::window::page_shortcuts().into_iter().find(|s| s.cmd && !s.shift && !s.alt && s.key == "d").unwrap();
    fake.send(cx, fake.shortcut(shortcut));
    assert_retained(&view, cx);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn private_direct_commands_and_folder_entries_recheck_availability(cx: &mut TestAppContext) {
    let (view, cx, fake) = private(cx);
    let tab = view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id);
    view.update(cx, |view, cx| {
        for command in [Command::TogglePin, Command::ToggleFavorite, Command::NewFolder,
            Command::NewFolderWithTab, Command::MoveToFolder, Command::NewSpace,
            Command::RenameSpace, Command::ToggleSpaceHeader, Command::ToggleFavoritesOrdering] {
            assert!(!view.private_allows_command(command));
            view.run_command(command, cx);
        }
    });
    cx.update(|window, cx| view.update(cx, |view, cx| {
        view.open_command_action(Command::NewFolder, window, cx);
        view.open_folder_creator(None, Some(tab), window, cx);
        view.toggle_new_menu(window, cx);
    }));
    view.read_with(cx, |view, _| {
        assert!(view.command_bar.is_none());
        assert!(view.new_menu.is_none());
    });
    assert_retained(&view, cx);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn private_supported_tab_commands_still_duplicate_close_and_restore(cx: &mut TestAppContext) {
    let (view, cx, _) = private(cx);
    run_named_command(&view, cx, "duplicate current tab");
    assert_eq!(view.read_with(cx, |view, _| view.browser.space().today.len()), 2);
    cx.simulate_keystrokes("cmd-w");
    cx.simulate_keystrokes("cmd-shift-t");
    assert_eq!(view.read_with(cx, |view, _| view.browser.space().today.len()), 2);
    assert!(view.read_with(cx, |view, _| view.browser.active_user_profile().history.is_empty()));
}
