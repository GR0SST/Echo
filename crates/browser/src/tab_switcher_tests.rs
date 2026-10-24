//! Recent Items is driven through the same shortcuts and child window as users.
use super::*;

fn visits(
    view: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
    count: usize,
) -> Vec<browser_core::TabId> {
    (0..count)
        .map(|index| {
            cx.simulate_keystrokes(if index == 0 { "cmd-l" } else { "cmd-t" });
            bar(view, cx).simulate_input(&format!("https://recent-{index}.example/"));
            bar(view, cx).simulate_keystrokes("enter");
            view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id)
        })
        .collect()
}
fn hold_control_tab(cx: &mut VisualTestContext) {
    cx.simulate_modifiers_change(Modifiers {
        control: true,
        ..Modifiers::none()
    });
    cx.simulate_keystrokes("ctrl-tab");
}
fn panel(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay = view.read_with(cx, |view, _| {
        view.tab_switcher
            .as_ref()
            .and_then(|state| state.overlay)
            .unwrap()
    });
    let panel = VisualTestContext::from_window(overlay.into(), &cx.cx);
    panel.run_until_parked();
    panel
}
fn selected(view: &Entity<BrowserWindow>, cx: &VisualTestContext) -> Option<browser_core::TabId> {
    view.read_with(cx, |view, _| {
        view.tab_switcher
            .as_ref()
            .and_then(|state| state.selection.selected())
    })
}
fn no_document_replacement(calls: &[Call]) {
    assert!(
        !calls.iter().any(|call| matches!(
            call,
            Call::Create(..) | Call::Navigate(..) | Call::Close(..)
        )),
        "{calls:?}"
    );
}

#[gpui::test]
fn mru_preview_cycles_without_navigation_and_control_release_commits_the_existing_page(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let ids = visits(&view, cx, 3);
    fake.take_calls();
    hold_control_tab(cx);
    let mut popup = panel(&view, cx);
    assert_eq!(selected(&view, cx), Some(ids[1]));
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id),
        ids[2]
    );
    let bounds = popup.debug_bounds("recent-items-card").unwrap();
    assert_eq!(bounds.size, gpui::size(gpui::px(578.), gpui::px(190.)));
    popup.simulate_keystrokes("ctrl-tab");
    assert_eq!(selected(&view, cx), Some(ids[0]));
    popup.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(selected(&view, cx), Some(ids[1]));
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id),
        ids[2]
    );
    popup.simulate_modifiers_change(Modifiers::none());
    popup.run_until_parked();
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.tab_switcher.is_none()));
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id),
        ids[1]
    );
    no_document_replacement(&fake.take_calls());
    hold_control_tab(cx);
    let mut again = panel(&view, cx);
    assert_eq!(selected(&view, cx), Some(ids[2]));
    again.simulate_keystrokes("ctrl-escape");
    again.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.tab_switcher.is_none()));
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id),
        ids[1]
    );
}

#[gpui::test]
fn mru_limits_the_overlay_to_five_recent_items_and_wraps_within_them(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let ids = visits(&view, cx, 6);
    hold_control_tab(cx);
    let mut popup = panel(&view, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v
            .tab_switcher
            .as_ref()
            .unwrap()
            .selection
            .items()
            .to_vec()),
        [ids[5], ids[4], ids[3], ids[2], ids[1]]
    );
    assert_eq!(
        popup.debug_bounds("recent-items-card").unwrap().size,
        gpui::size(gpui::px(942.), gpui::px(190.))
    );
    popup.simulate_keystrokes("ctrl-tab ctrl-tab ctrl-tab ctrl-tab");
    assert_eq!(selected(&view, cx), Some(ids[5]));
    popup.simulate_modifiers_change(Modifiers::none());
    popup.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id),
        ids[5]
    );
}

#[gpui::test]
fn mru_hover_and_release_commit_the_highlight_without_reordering_the_sidebar(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let ids = visits(&view, cx, 3);
    let order = view.read_with(cx, |v, _| {
        v.browser
            .space()
            .today
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>()
    });
    fake.take_calls();
    hold_control_tab(cx);
    let mut popup = panel(&view, cx);
    let target = popup.debug_bounds("recent-item-2").unwrap().center();
    popup.simulate_mouse_move(
        target,
        None,
        Modifiers {
            control: true,
            ..Modifiers::none()
        },
    );
    popup.run_until_parked();
    assert_eq!(selected(&view, cx), Some(ids[0]));
    popup.simulate_modifiers_change(Modifiers::none());
    popup.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id),
        ids[0]
    );
    assert_eq!(
        view.read_with(cx, |v, _| v
            .browser
            .space()
            .today
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>()),
        order
    );
    assert!(view.read_with(cx, |v, _| v.tab_switcher.is_none()));
    no_document_replacement(&fake.take_calls());
}

#[gpui::test]
fn mru_page_shortcut_uses_the_same_overlay_and_a_released_quick_tap_cannot_leave_a_child(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let ids = visits(&view, cx, 3);
    fake.take_calls();
    cx.simulate_modifiers_change(Modifiers {
        control: true,
        ..Modifiers::none()
    });
    fake.send(
        cx,
        fake.shortcut(Shortcut {
            ctrl: true,
            key: "tab".into(),
            ..Default::default()
        }),
    );
    let mut popup = panel(&view, cx);
    assert_eq!(selected(&view, cx), Some(ids[1]));
    popup.simulate_modifiers_change(Modifiers::none());
    popup.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id),
        ids[1]
    );
    cx.simulate_modifiers_change(Modifiers::none());
    fake.send(
        cx,
        fake.shortcut(Shortcut {
            ctrl: true,
            key: "tab".into(),
            ..Default::default()
        }),
    );
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.tab_switcher.is_none()));
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id),
        ids[2]
    );
    assert_eq!(cx.cx.read(|cx| cx.windows().len()), 1);
    no_document_replacement(&fake.take_calls());
}

#[gpui::test]
fn mru_excludes_a_closed_pin_and_does_not_inherit_another_windows_visits(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let ids = visits(&view, cx, 3);
    view.update(cx, |view, cx| {
        view.edit_browser(|b| b.toggle_pin(ids[1]));
        view.close_tab(ids[1], cx);
    });
    cx.run_until_parked();
    hold_control_tab(cx);
    let mut popup = panel(&view, cx);
    assert!(!view.read_with(cx, |v, _| v
        .tab_switcher
        .as_ref()
        .unwrap()
        .selection
        .items()
        .contains(&ids[1])));
    popup.simulate_keystrokes("ctrl-escape");
    popup.run_until_parked();
    let (other, mut other_cx) = another_profile_window(&view, cx);
    hold_control_tab(&mut other_cx);
    assert!(other.read_with(&other_cx, |v, _| v.tab_switcher.is_none()));
}

#[gpui::test]
fn mru_owner_window_close_removes_the_overlay_without_archiving_its_tabs(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    visits(&view, cx, 3);
    let count = view.read_with(cx, |v, _| v.browser.all_tabs().count());
    hold_control_tab(cx);
    let mut popup = panel(&view, cx);
    dispatch_menu(&mut popup, "File", "Close Window");
    cx.run_until_parked();
    assert_eq!(cx.cx.read(|cx| cx.windows().len()), 0);
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.all_tabs().count()),
        count
    );
}

#[gpui::test]
fn mru_capture_timeout_keeps_the_keyboard_usable_and_does_not_select_a_preview(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let ids = visits(&view, cx, 3);
    fake.0.borrow_mut().capture_supported = true;
    fake.take_calls();
    hold_control_tab(cx);
    let mut popup = panel(&view, cx);
    assert!(popup.debug_bounds("recent-items-overlay").is_some());
    assert!(popup.debug_bounds("recent-items-card").is_none());
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    popup.run_until_parked();
    assert!(popup.debug_bounds("recent-items-card").is_some());
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id),
        ids[2]
    );
    popup.simulate_modifiers_change(Modifiers::none());
    popup.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id),
        ids[1]
    );
    assert!(view.read_with(cx, |view, _| view.tab_switcher.is_none()));
    no_document_replacement(&fake.take_calls());
}

#[gpui::test]
fn mru_long_page_titles_use_the_measured_label_width_and_a_visible_ellipsis(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    visits(&view, cx, 3);
    let title = "Echo Recent Items title with a long 👩🏽‍💻 combining sequence and trailing content";
    fake.send(
        cx,
        Event::TitleChanged {
            view: page(1),
            title: title.into(),
        },
    );
    crate::sidebar_title::take_painted_titles();
    hold_control_tab(cx);
    let mut popup = panel(&view, cx);
    let records = crate::sidebar_title::take_painted_titles();
    let label = records
        .iter()
        .find(|record| record.original == title && record.font_size == gpui::px(14.))
        .expect("Recent Items label painted");
    assert_eq!(label.width, gpui::px(134.));
    assert!(label.shaped_width <= label.width);
    assert!(label.text.ends_with('…'), "{}", label.text);
    popup.simulate_keystrokes("ctrl-escape");
}
