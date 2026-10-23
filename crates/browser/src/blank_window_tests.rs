//! Blank integration shares the production root/overlay/CEF routing.
use super::*;
use crate::window::Command;

fn roots(cx: &mut VisualTestContext) -> Vec<Entity<BrowserWindow>> {
    cx.cx.update(|cx| cx.windows().into_iter()
        .filter_map(|window| window.downcast::<BrowserWindow>())
        .filter_map(|window| window.root(cx).ok()).collect())
}

fn blank(source: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> (Entity<BrowserWindow>, VisualTestContext) {
    source.update(cx, |view, cx| view.blank_window(cx));
    cx.run_until_parked();
    let other = roots(cx).into_iter().filter(|view| view != source)
        .filter(|view| view.read_with(cx, |view, _| view.browser.is_blank()))
        .last().unwrap();
    let handle = other.read_with(cx, |view, _| view.main_window);
    (other, VisualTestContext::from_window(handle.into(), &cx.cx))
}

fn enter(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, url: &str) {
    if view.read_with(cx, |view, _| view.command_bar.is_none()) { cx.simulate_keystrokes("cmd-t"); }
    bar(view, cx).simulate_input(url);
    bar(view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn blank_root_overlay_and_cef_shortcuts_open_focused_independent_today(cx: &mut TestAppContext) {
    let (normal, visual, fake) = open_pages(cx);
    visual.simulate_keystrokes("ctrl-cmd-n");
    visual.run_until_parked();
    let first = roots(visual).into_iter().find(|view| view != &normal).unwrap();
    first.read_with(visual, |view, cx| {
        assert!(view.browser.is_blank());
        assert!(!view.browser.is_private());
        assert_eq!(view.browser.all_tabs().count(), 0);
        assert!(view.command_bar.as_ref().unwrap().input.read(cx).text().is_empty());
    });
    let mut first_visual = VisualTestContext::from_window(first.read_with(visual, |view, _| view.main_window).into(), &visual.cx);
    assert!(first_visual.debug_bounds("blank-profile").is_some());
    assert!(first_visual.debug_bounds("favorites").is_none());
    assert!(first_visual.debug_bounds("space-header").is_none());
    bar(&first, &mut first_visual).simulate_keystrokes("ctrl-cmd-n");
    visual.run_until_parked();
    assert_eq!(roots(visual).len(), 3);
    let shortcut = crate::window::page_shortcuts().into_iter().find(|s| s.ctrl && s.cmd && s.key == "n").unwrap();
    fake.send(visual, Event::Shortcut(shortcut));
    assert_eq!(roots(visual).len(), 4);
    assert_eq!(fake.0.borrow().forks.len(), 3);
    assert!(!normal.read_with(visual, |view, _| view.browser.is_blank()));
}

#[gpui::test]
fn blank_models_with_equal_ids_keep_independent_live_pages_and_archives(cx: &mut TestAppContext) {
    let (normal, visual, fake) = open_pages(cx);
    let (a, mut av) = blank(&normal, visual);
    let (b, mut bv) = blank(&normal, visual);
    enter(&a, &mut av, "https://blank-a.example/first");
    enter(&b, &mut bv, "https://blank-b.example/first");
    let aid = a.read_with(&av, |view, _| view.browser.active_tab().unwrap().id);
    assert_eq!(aid, b.read_with(&bv, |view, _| view.browser.active_tab().unwrap().id));
    let engines = fake.0.borrow().forks.clone();
    assert_eq!(engines.len(), 2);
    fake.take_calls();
    engines[0].take_calls();
    engines[1].take_calls();
    enter(&a, &mut av, "https://blank-a.example/second");
    av.simulate_keystrokes("cmd-w");
    av.run_until_parked();
    assert_eq!(a.read_with(&av, |view, _| view.browser.archive.len()), 1);
    av.simulate_keystrokes("cmd-shift-t");
    av.run_until_parked();
    assert_eq!(a.read_with(&av, |view, _| view.browser.all_tabs().count()), 2);
    let calls = engines[0].take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(_, url) if url == "https://blank-a.example/second")).count(), 2);
    assert!(engines[1].take_calls().is_empty());
    assert!(fake.take_calls().is_empty());
    bv.simulate_keystrokes("cmd-w");
    bv.run_until_parked();
    assert_eq!(roots(visual).len(), 2);
    assert!(normal.read_with(visual, |view, _| view.browser.archive.is_empty()));
    assert!(a.read_with(&av, |view, _| view.browser.archive.is_empty()));
}

#[gpui::test]
fn blank_persistent_routes_are_disabled_and_new_window_uses_retained_normal_space(cx: &mut TestAppContext) {
    let (normal, visual, _) = open_pages(cx);
    visual.simulate_keystrokes("ctrl-2");
    let space = normal.read_with(visual, |view, _| view.browser.space().id);
    let (a, mut av) = blank(&normal, visual);
    enter(&a, &mut av, "https://blank.example/page");
    av.simulate_keystrokes("cmd-d");
    av.update(|window, cx| window.dispatch_action(Box::new(crate::window::NewFolder), cx));
    a.update(&mut av, |view, cx| {
        for command in [Command::NewSpace, Command::NewFolder, Command::ToggleFavorite, Command::ToggleSpaceHeader, Command::ToggleFavoritesOrdering] {
            assert!(!view.blank_allows_command(command));
            view.run_command(command, cx);
        }
    });
    a.read_with(&av, |view, _| {
        assert!(view.browser.favorites.is_empty());
        assert!(view.browser.space().pinned.is_empty());
        assert!(view.browser.space().folders.is_empty());
        assert_eq!(view.browser.spaces.len(), 1);
        assert!(view.command_bar.is_none());
    });
    av.simulate_keystrokes("cmd-t");
    bar(&a, &mut av).simulate_input("pin tab");
    a.read_with(&av, |view, cx| assert!(!view.suggestions(cx).iter().any(|row| matches!(row, Suggestion::Action { command: Command::TogglePin, .. }))));
    bar(&a, &mut av).simulate_keystrokes("cmd-n");
    av.run_until_parked();
    let regular = roots(visual).into_iter().find(|view| view != &normal && view != &a).unwrap();
    regular.read_with(visual, |view, _| {
        assert!(!view.browser.is_blank());
        assert_eq!(view.browser.space().id, space);
        assert_eq!(view.browser.spaces.len(), 3);
        assert_eq!(view.browser.favorites.len(), normal.read_with(visual, |view, _| view.browser.favorites.len()));
    });
}

#[gpui::test]
fn blank_split_cancel_and_populated_pane_closure_keep_window_until_last_actual_tab(cx: &mut TestAppContext) {
    let (normal, visual, _) = open_pages(cx);
    let (a, mut av) = blank(&normal, visual);
    bar(&a, &mut av).simulate_keystrokes("escape");
    assert_eq!(roots(visual).len(), 2);
    enter(&a, &mut av, "https://blank.example/left");
    av.simulate_keystrokes("ctrl-shift-=");
    bar(&a, &mut av).simulate_keystrokes("escape");
    assert_eq!(roots(visual).len(), 2);
    assert_eq!(a.read_with(&av, |view, _| view.browser.all_tabs().count()), 1);
    av.simulate_keystrokes("ctrl-shift-=");
    bar(&a, &mut av).simulate_input("https://blank.example/right");
    bar(&a, &mut av).simulate_keystrokes("enter");
    av.simulate_keystrokes("cmd-w");
    av.run_until_parked();
    assert_eq!(a.read_with(&av, |view, _| view.browser.all_tabs().count()), 1);
    assert_eq!(roots(visual).len(), 2);
    av.simulate_keystrokes("cmd-w");
    av.run_until_parked();
    assert_eq!(roots(visual).len(), 1);
}

#[gpui::test]
fn blank_whole_split_and_bulk_removal_close_root_and_children(cx: &mut TestAppContext) {
    let (normal, visual, _) = open_pages(cx);
    for whole_group in [true, false] {
        let (a, mut av) = blank(&normal, visual);
        enter(&a, &mut av, "https://blank.example/left");
        if whole_group {
            av.simulate_keystrokes("ctrl-shift-=");
            enter(&a, &mut av, "https://blank.example/right");
            a.update(&mut av, |view, cx| { let id = view.browser.space().active_tab.unwrap(); view.close_tab(id, cx); });
        } else {
            // Clear Today retains active tabs. A window with no selection can
            // bulk archive every Today tab, and must run the same removal hook.
            a.update(&mut av, |view, cx| {
                view.edit_browser(|browser| browser.spaces[browser.active_space].active_tab = None);
                view.run_command(Command::ClearToday, cx);
            });
        }
        av.run_until_parked();
        assert_eq!(roots(visual).len(), 1);
        assert_eq!(visual.cx.update(|cx| cx.windows().len()), 1);
    }
}

#[gpui::test]
fn blank_from_private_forks_normal_engine_and_blank_drops_cannot_collide(cx: &mut TestAppContext) {
    let (normal, visual, fake) = open_pages(cx);
    normal.update(visual, |view, cx| view.retain_normal_source(cx));
    let private_fake = FakeEngine::default();
    let (private, pv) = visual.cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser_core::Browser::new_private(), Some(Box::new(private_fake.clone())), window, cx);
        view.focus(window); view
    });
    let (a, mut av) = blank(&private, pv);
    enter(&a, &mut av, "https://blank.example/a");
    assert!(private_fake.0.borrow().forks.is_empty());
    assert_eq!(fake.0.borrow().forks.len(), 1);
    let (b, mut bv) = blank(&a, &mut av);
    enter(&b, &mut bv, "https://blank.example/b");
    let drag = a.read_with(&av, |view, _| view.tab_drag(view.browser.active_tab().unwrap()));
    b.update(&mut bv, |view, cx| {
        let tab = view.browser.active_tab().unwrap().id;
        view.drop_tab_relative(&drag, tab, true, cx);
        view.drop_tab_on_section(&drag, view.browser.space().id, browser_core::Section::Pinned, cx);
    });
    b.read_with(&bv, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().url, "https://blank.example/b");
        assert!(view.browser.space().pinned.is_empty());
        assert_eq!(view.browser.all_tabs().count(), 1);
    });
}
