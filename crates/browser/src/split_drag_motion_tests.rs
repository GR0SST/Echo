use super::*;

#[gpui::test]
fn page_center_keeps_the_drag_card_and_an_armed_edge_survives_return_to_center(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_split_pages(cx, 1);
    let anchor = view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap());
    let source = view.update(cx, |w, cx| {
        let source =
            w.edit_browser(|b| b.new_tab("https://dragged.example", std::time::SystemTime::now()));
        w.edit_browser(|b| b.activate_tab(anchor, std::time::SystemTime::now()));
        cx.notify();
        source
    });
    cx.run_until_parked();
    let start = cx
        .debug_bounds(test_selector(format!("tab-{}", source.get())))
        .unwrap()
        .center();
    let frame = cx.debug_bounds("page-surface").unwrap();
    let before = view.read_with(cx, |w, _| {
        w.pages.as_ref().unwrap().borrow().surface_layout()
    });
    fake.take_calls();
    for armed in [false, true] {
        cx.simulate_mouse_move(start, None, Modifiers::none());
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            start + gpui::point(gpui::px(8.), gpui::px(0.)),
            gpui::MouseButton::Left,
            Modifiers::none(),
        );
        cx.run_until_parked();
        if armed {
            cx.simulate_mouse_move(
                gpui::point(frame.right() - gpui::px(12.), frame.center().y),
                gpui::MouseButton::Left,
                Modifiers::none(),
            );
            cx.run_until_parked();
        }
        cx.simulate_mouse_move(frame.center(), gpui::MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        let handle = view.read_with(cx, |w, _| {
            let preview = w.split_drop.as_ref().unwrap();
            assert_eq!(
                preview.placement,
                armed.then_some(browser_core::splits::Placement::Right)
            );
            assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout(), before);
            assert!(w.browser.active_split().is_none());
            assert_eq!(w.browser.space().active_tab, Some(anchor));
            preview.overlay.unwrap()
        });
        assert!(
            cx.debug_bounds("tab-drag-hidden").is_some(),
            "the root must render the explicit hidden drag view over the document"
        );
        let mut popup = VisualTestContext::from_window(handle.into(), &cx.cx);
        popup.run_until_parked();
        assert!(
            popup.debug_bounds("tab-drag-row").is_none(),
            "the passive card must not have a second row ghost"
        );
        let card = popup.debug_bounds("split-drag-card").unwrap();
        assert_eq!(card.size, gpui::size(gpui::px(128.), gpui::px(250.)));
        assert_eq!(
            card.origin,
            frame.center() + gpui::point(gpui::px(136.), gpui::px(80.))
        );
        finish_sidebar_drag(cx, frame.center());
        view.read_with(cx, |w, _| {
            assert!(w.split_drop.is_none());
            if armed {
                let split = w.browser.active_split().unwrap();
                assert_eq!(split.panes, vec![anchor, source]);
                assert_eq!(split.active, source);
            } else {
                assert!(w.browser.active_split().is_none());
                assert_eq!(w.browser.space().active_tab, Some(anchor));
            }
        });
    }
    assert!(!fake.take_calls().iter().any(|call| matches!(
        call,
        Call::Close(..) | Call::Navigate(..) | Call::Reload(..)
    )));
}

#[gpui::test]
fn leaving_the_page_disarms_the_split_target_before_reentering_its_center(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 1);
    let anchor = view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap());
    let source = view.update(cx, |w, cx| {
        let source =
            w.edit_browser(|b| b.new_tab("https://dragged.example", std::time::SystemTime::now()));
        w.edit_browser(|b| b.activate_tab(anchor, std::time::SystemTime::now()));
        cx.notify();
        source
    });
    cx.run_until_parked();
    let start = cx
        .debug_bounds(test_selector(format!("tab-{}", source.get())))
        .unwrap()
        .center();
    let frame = cx.debug_bounds("page-surface").unwrap();
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        start + gpui::point(gpui::px(8.), gpui::px(0.)),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_move(
        gpui::point(frame.right() - gpui::px(12.), frame.center().y),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w
        .split_drop
        .as_ref()
        .unwrap()
        .placement
        .is_some()));
    cx.simulate_mouse_move(
        gpui::point(gpui::px(100.), frame.center().y),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.split_drop.is_none()));
    cx.simulate_mouse_move(frame.center(), gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w
        .split_drop
        .as_ref()
        .unwrap()
        .placement
        .is_none()));
    finish_sidebar_drag(cx, frame.center());
    assert!(view.read_with(cx, |w, _| w.split_drop.is_none()
        && w.browser.active_split().is_none()));
}

#[gpui::test]
fn sidebar_drag_ghost_is_a_row_and_tab_on_tab_joins_a_split_in_place(cx: &mut TestAppContext) {
    let (view, cx, _fake) = open_split_pages(cx, 1);
    let target = view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap());
    let source = view.update(cx, |w, cx| {
        let source = w.edit_browser(|b| b.new_tab("https://dragged.example", std::time::SystemTime::now()));
        // Keep the target in Pinned: a sidebar split stays where the target row is.
        w.edit_browser(|b| b.move_tab(target, b.space().id, browser_core::Section::Pinned));
        cx.notify();
        source
    });
    cx.run_until_parked();
    let row = cx.debug_bounds(test_selector(format!("tab-{}", target.get()))).unwrap();
    begin_sidebar_drag(cx, test_selector(format!("tab-{}", source.get())));
    let held = cx.debug_bounds(test_selector(format!("tab-{}", source.get()))).unwrap().center() + gpui::point(gpui::px(12.), gpui::px(0.));
    cx.simulate_mouse_move(held, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    // The row is painted in a passive window over pages, not in the root.
    assert!(cx.debug_bounds("tab-drag-hidden").is_some());
    let overlay = view.read_with(cx, |w, _| w.drag_ghost.as_ref().unwrap().overlay_handle()).expect("ghost window");
    let mut popup = VisualTestContext::from_window(overlay.into(), &cx.cx);
    popup.run_until_parked();
    let ghost = popup.debug_bounds("tab-drag-row").unwrap();
    assert_eq!(ghost.size.height, gpui::px(crate::window::SIDEBAR_ROW_HEIGHT));
    // Upper band reorders, middle band offers the right half for a split.
    cx.simulate_mouse_move(gpui::point(row.right() - gpui::px(20.), row.top() + gpui::px(5.)), gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds(test_selector(format!("tab-split-half-{}", target.get()))).is_none());
    let middle = gpui::point(row.right() - gpui::px(20.), row.center().y);
    cx.simulate_mouse_move(middle, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let half = cx.debug_bounds(test_selector(format!("tab-split-half-{}", target.get()))).unwrap();
    assert!(half.left() >= row.center().x - gpui::px(1.));
    finish_sidebar_drag(cx, middle);
    view.read_with(cx, |w, _| {
        let split = w.browser.split_for(target).expect("joined in the sidebar");
        assert_eq!(split.panes, vec![target, source]);
        assert!(w.browser.space().pinned.iter().any(|tab| tab.id == source), "the group stays in Pinned");
    });
}

#[gpui::test]
fn the_split_preview_slides_the_new_pane_in_from_the_edge(cx: &mut TestAppContext) {
    let (view, cx, _fake) = open_split_pages(cx, 1);
    let anchor = view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap());
    let source = view.update(cx, |w, cx| {
        let source = w.edit_browser(|b| b.new_tab("https://dragged.example", std::time::SystemTime::now()));
        w.edit_browser(|b| b.activate_tab(anchor, std::time::SystemTime::now()));
        cx.notify();
        source
    });
    cx.run_until_parked();
    let frame = cx.debug_bounds("page-surface").unwrap();
    begin_sidebar_drag(cx, test_selector(format!("tab-{}", source.get())));
    cx.simulate_mouse_move(frame.center(), gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let center = view.read_with(cx, |w, _| w.split_drop.as_ref().unwrap().outline);
    cx.simulate_mouse_move(gpui::point(frame.right() - gpui::px(12.), frame.center().y), gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        let preview = w.split_drop.as_ref().unwrap();
        assert_eq!(preview.placement, Some(browser_core::splits::Placement::Right));
        // The pane grows from the right edge and the contour starts from the whole page.
        assert_eq!(preview.from_cover.size.width, gpui::px(0.));
        assert_eq!(preview.from_cover.right(), preview.cover.right());
        assert_eq!(preview.from_outline, center);
    });
    finish_sidebar_drag(cx, gpui::point(frame.right() - gpui::px(12.), frame.center().y));
}
