//! New Window actions exercise the same root and overlay routing as Cmd+N.
use crate::window::BrowserWindow;
use gpui::{Entity, TestAppContext, VisualTestContext};

fn open(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext) {
    cx.update(crate::init);
    let (view, visual) = cx.add_window_view(|window, cx| {
        let mut view = BrowserWindow::new(crate::demo_browser(), None, window, cx);
        // Keep the production writer away from the user's session in tests.
        let path = std::env::temp_dir().join(format!(
            "echo-new-window-{}-{:?}.json",
            std::process::id(),
            cx.entity_id()
        ));
        view.save_session_to(path, cx);
        view.focus(window);
        view
    });
    visual.run_until_parked();
    (view, visual)
}

fn roots(cx: &mut VisualTestContext) -> Vec<Entity<BrowserWindow>> {
    cx.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|window| window.downcast::<BrowserWindow>())
            .filter_map(|window| window.root(cx).ok())
            .collect()
    })
}

fn bar(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay = view.read_with(cx, |view, _| {
        view.command_bar.as_ref().unwrap().overlay.unwrap()
    });
    VisualTestContext::from_window(overlay.into(), cx)
}

#[gpui::test]
fn new_window_shortcut_shares_tabs_and_inherits_only_the_source_space(cx: &mut TestAppContext) {
    let (source, visual) = open(cx);
    visual.simulate_keystrokes("ctrl-2");
    visual.simulate_keystrokes("cmd-n");
    visual.run_until_parked();
    let other = roots(visual)
        .into_iter()
        .find(|view| view != &source)
        .unwrap();
    let (space, original_selection) = source.read_with(visual, |view, _| {
        (view.browser.space().id, view.browser.space().active_tab)
    });
    other.read_with(visual, |view, cx| {
        assert_eq!(view.browser.space().id, space);
        assert!(view
            .browser
            .spaces
            .iter()
            .all(|space| space.active_tab.is_none()));
        assert_eq!(view.browser.spaces.len(), 3);
        assert!(view
            .command_bar
            .as_ref()
            .unwrap()
            .input
            .read(cx)
            .text()
            .is_empty());
    });
    bar(&other, visual).simulate_input("https://new-window.example/shared");
    bar(&other, visual).simulate_keystrokes("enter");
    visual.run_until_parked();
    source.read_with(visual, |view, _| {
        assert_eq!(view.browser.space().active_tab, original_selection);
        assert!(view
            .browser
            .all_tabs()
            .any(|tab| tab.url == "https://new-window.example/shared"));
    });
}

#[gpui::test]
fn new_window_shortcut_from_command_overlay_uses_its_source(cx: &mut TestAppContext) {
    let (source, visual) = open(cx);
    visual.simulate_keystrokes("cmd-t");
    bar(&source, visual).simulate_keystrokes("cmd-n");
    visual.run_until_parked();
    assert_eq!(roots(visual).len(), 2);
    assert!(source.read_with(visual, |view, _| view.command_bar.is_none()));
    let other = roots(visual)
        .into_iter()
        .find(|view| view != &source)
        .unwrap();
    bar(&other, visual).simulate_input("typed into the new window");
    other.read_with(visual, |view, cx| {
        assert_eq!(
            view.command_bar.as_ref().unwrap().input.read(cx).text(),
            "typed into the new window"
        );
    });
}

#[gpui::test]
fn new_window_from_private_uses_the_retained_normal_profile(cx: &mut TestAppContext) {
    let (source, visual) = open(cx);
    visual.simulate_keystrokes("cmd-n");
    visual.run_until_parked();
    // The first New Window adopts this fixture as the retained normal profile.
    visual.simulate_keystrokes("cmd-shift-n");
    visual.run_until_parked();
    let private = roots(visual)
        .into_iter()
        .find(|view| view.read_with(visual, |view, _| view.browser.is_private()))
        .unwrap();
    bar(&private, visual).simulate_input("https://private.example/secret");
    bar(&private, visual).simulate_keystrokes("enter");
    let handle = private.read_with(visual, |view, _| view.main_window);
    let mut private_visual = VisualTestContext::from_window(handle, &visual.cx);
    private_visual.simulate_keystrokes("cmd-n");
    private_visual.run_until_parked();
    let windows = roots(&mut private_visual);
    assert_eq!(windows.len(), 4);
    let expected_tabs = source.read_with(&private_visual, |source, _| {
        source.browser.all_tabs().count()
    });
    let normal_count = windows
        .iter()
        .filter(|view| {
            view.read_with(&private_visual, |view, _| {
                if view.browser.is_private() {
                    return false;
                }
                assert!(view
                    .browser
                    .all_tabs()
                    .all(|tab| tab.url != "https://private.example/secret"));
                assert_eq!(view.browser.all_tabs().count(), expected_tabs);
                true
            })
        })
        .count();
    assert_eq!(normal_count, 3);
}

#[gpui::test]
fn named_new_window_command_opens_a_regular_window(cx: &mut TestAppContext) {
    let (source, visual) = open(cx);
    visual.simulate_keystrokes("cmd-t");
    bar(&source, visual).simulate_input("new window");
    bar(&source, visual).simulate_keystrokes("enter");
    visual.run_until_parked();
    let windows = roots(visual);
    assert_eq!(windows.len(), 2);
    assert!(windows
        .iter()
        .all(|view| view.read_with(visual, |view, _| !view.browser.is_private())));
    assert!(source.read_with(visual, |view, _| view.command_bar.is_none()));
}

#[gpui::test]
fn dock_reopen_retains_the_normal_profile_after_all_windows_close(cx: &mut TestAppContext) {
    let (source, visual) = open(cx);
    source.update(visual, |view, _| {
        view.edit_browser(|browser| {
            browser.new_tab("https://retained.example/", std::time::SystemTime::now())
        });
    });
    visual.simulate_keystrokes("cmd-n");
    visual.run_until_parked();
    for view in roots(visual) {
        view.update(visual, |view, cx| view.close_window(cx));
    }
    visual.run_until_parked();
    visual.cx.update(crate::reopen);
    visual.run_until_parked();
    let windows = roots(visual);
    assert_eq!(windows.len(), 1);
    windows[0].read_with(visual, |view, _| {
        assert!(view
            .browser
            .all_tabs()
            .any(|tab| tab.url == "https://retained.example/"));
        assert!(!view.browser.is_private());
    });
}

#[gpui::test]
fn file_new_window_menu_routes_to_the_source_window(cx: &mut TestAppContext) {
    let (source, visual) = open(cx);
    let action = crate::menus::build()
        .into_iter()
        .find(|menu| menu.name.as_ref() == "File")
        .unwrap()
        .items
        .into_iter()
        .find_map(|item| match item {
            gpui::MenuItem::Action { name, action, .. } if name.as_ref() == "New Window" => {
                Some(action)
            }
            _ => None,
        })
        .unwrap();
    visual.update(|window, cx| window.dispatch_action(action, cx));
    visual.run_until_parked();
    assert_eq!(roots(visual).len(), 2);
    assert!(source.read_with(visual, |view, _| view.command_bar.is_none()));
}
