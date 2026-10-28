use super::*;
use browser_core::folders::FolderId;

fn fixture(
    cx: &mut TestAppContext,
) -> (
    Entity<BrowserWindow>,
    &mut VisualTestContext,
    FakeEngine,
    FolderId,
) {
    let (view, cx, fake) = open_long_sidebar(cx, false);
    let folder = view.update(cx, |view, cx| {
        let folder = view.edit_browser(|browser| {
            browser
                .create_folder(browser.space().id, None, "Held folder")
                .unwrap()
        });
        cx.notify();
        folder
    });
    cx.run_until_parked();
    (view, cx, fake, folder)
}

fn begin_folder(cx: &mut VisualTestContext, folder: FolderId, edge: gpui::Point<gpui::Pixels>) {
    let start = cx
        .debug_bounds(Box::leak(format!("folder-{}", folder.get()).into_boxed_str()))
        .unwrap()
        .center();
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        start + gpui::point(gpui::px(5.), gpui::px(0.)),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_move(edge, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn advance(cx: &mut VisualTestContext, ticks: usize) {
    for _ in 0..ticks {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
}

#[gpui::test]
fn held_folder_scrolls_with_tab_physics_and_renderer_escape_cancels_without_a_drop(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake, folder) = fixture(cx);
    let viewport = cx.debug_bounds("sidebar-content").unwrap();
    let header = cx.debug_bounds("space-header-0").unwrap();
    let before = cx.debug_bounds("today-list").unwrap().top();
    let selection = active_url(&view, cx);
    let inventory = view.read_with(cx, |view, _| browser_core::session::to_json(&view.browser));
    let edge = gpui::point(viewport.center().x, viewport.bottom() - gpui::px(11.));
    fake.take_calls();
    begin_folder(cx, folder, edge);
    advance(cx, 12); // Dwell has not elapsed.
    assert_eq!(cx.debug_bounds("today-list").unwrap().top(), before);
    advance(cx, 48);
    let shifted = cx.debug_bounds("today-list").unwrap().top();
    assert!(
        before - shifted > gpui::px(40.) && before - shifted < gpui::px(65.),
        "distance={:?}",
        before - shifted
    );
    assert_eq!(cx.debug_bounds("space-header-0").unwrap(), header);
    fake.send(
        cx,
        fake.shortcut(Shortcut {
            key: "escape".into(),
            ..Default::default()
        }),
    );
    cx.run_until_parked();
    // A step already computed before Escape may still paint; after that the
    // list must not move again.
    let stopped = cx.debug_bounds("today-list").unwrap().top();
    assert!((stopped - shifted).abs() <= gpui::px(2.), "after escape {stopped:?} vs {shifted:?}");
    advance(cx, 60);
    assert_eq!(cx.debug_bounds("today-list").unwrap().top(), stopped);
    finish_sidebar_drag(cx, edge);
    assert_eq!(active_url(&view, cx), selection);
    assert_eq!(
        view.read_with(cx, |view, _| browser_core::session::to_json(&view.browser)),
        inventory
    );
    assert!(!fake.take_calls().iter().any(|call| matches!(
        call,
        Call::Create(..) | Call::Close(..) | Call::Navigate(..)
    )));
}

#[gpui::test]
fn held_folder_scroll_clamps_and_stops_outside_the_viewport_or_on_release(cx: &mut TestAppContext) {
    let (view, cx, _, folder) = fixture(cx);
    let viewport = cx.debug_bounds("sidebar-content").unwrap();
    begin_folder(
        cx,
        folder,
        gpui::point(viewport.center().x, viewport.bottom() - gpui::px(1.)),
    );
    advance(cx, 400);
    let end = cx.debug_bounds("today-list").unwrap();
    assert!((end.bottom() - viewport.bottom()).abs() < gpui::px(1.));
    // Move to the upper edge to prove reversal without a fresh drag.
    let top = gpui::point(viewport.center().x, viewport.top() + gpui::px(1.));
    cx.simulate_mouse_move(top, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    advance(cx, 30);
    let reversed = cx.debug_bounds("today-list").unwrap();
    assert!(reversed.top() > end.top());
    let outside = gpui::point(viewport.right() + gpui::px(30.), viewport.center().y);
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    advance(cx, 60);
    assert_eq!(cx.debug_bounds("today-list").unwrap(), reversed);
    finish_sidebar_drag(cx, outside);
    advance(cx, 60);
    assert_eq!(cx.debug_bounds("today-list").unwrap(), reversed);
    assert!(view.read_with(cx, |view, _| view.browser.folder(folder).is_some()));
}

#[gpui::test]
fn folder_scroll_revalidates_live_identity_profile_and_workspace(cx: &mut TestAppContext) {
    let (view, cx, _, folder) = fixture(cx);
    let (drag, space) = view.read_with(cx, |view, _| {
        (
            crate::folder_drag::FolderDrag::new(
                view.browser.folder(folder).unwrap(),
                None,
                view.profile.handle.clone(),
                view.browser.active_user_profile_id(),
            ),
            view.browser.space().id,
        )
    });
    assert!(view.read_with(cx, |view, _| view.accepts_folder_drag(&drag, space)));
    let foreign =
        cx.update(|_, cx| crate::profile::ProfileHandle::new(browser_core::Browser::new(), cx));
    let foreign_drag = view.read_with(cx, |view, _| {
        crate::folder_drag::FolderDrag::new(
            view.browser.folder(folder).unwrap(),
            None,
            foreign,
            view.browser.active_user_profile_id(),
        )
    });
    assert!(!view.read_with(cx, |view, _| view.accepts_folder_drag(&foreign_drag, space)));
    let other = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let profile = browser.create_user_profile("Other").unwrap();
            let other = browser.add_space("Other", 40.);
            browser
                .spaces
                .iter_mut()
                .find(|entry| entry.id == other)
                .unwrap()
                .user_profile_id = profile;
            other
        })
    });
    assert!(!view.read_with(cx, |view, _| view.accepts_folder_drag(&drag, other)));
    let viewport = cx.debug_bounds("sidebar-content").unwrap();
    begin_folder(
        cx,
        folder,
        gpui::point(viewport.center().x, viewport.bottom() - gpui::px(11.)),
    );
    advance(cx, 30);
    view.update(cx, |view, cx| {
        view.edit_browser(|browser| {
            browser.delete_empty_folder(folder);
        });
        cx.notify();
    });
    cx.run_until_parked();
    let retired = cx.debug_bounds("today-list").unwrap();
    advance(cx, 60);
    assert_eq!(cx.debug_bounds("today-list").unwrap(), retired);
    assert!(!view.read_with(cx, |view, _| view.accepts_folder_drag(&drag, space)));
    cx.simulate_keystrokes("escape");
}
