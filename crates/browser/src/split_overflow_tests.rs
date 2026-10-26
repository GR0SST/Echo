use super::*;

fn exercise_overflow(cx: &mut TestAppContext, vertical: bool) {
    let (view, cx, fake) = open_split_pages(cx, 4);
    view.update(cx, |view, cx| {
        let anchor = view.browser.active_split().unwrap().anchor;
        view.edit_browser(|browser| browser.set_split_weights(anchor, &[1.; 4]));
        cx.notify();
    });
    if vertical {
        run_named_command(&view, cx, "convert to vertical split");
    }
    cx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(600.)));
    cx.run_until_parked();
    let before = view.read_with(cx, |view, _| view.browser.active_split().unwrap().clone());
    let active = active_url(&view, cx);
    let surfaces = view.read_with(cx, |view, _| {
        view.pages.as_ref().unwrap().borrow().surface_layout()
    });
    assert_eq!(surfaces.len(), 4);
    for (id, surface) in &surfaces {
        let pane = cx
            .debug_bounds(test_selector(format!("split-pane-{}", id.get())))
            .unwrap();
        let extent = if vertical {
            pane.size.height
        } else {
            pane.size.width
        };
        assert!((f32::from(extent) - 220.).abs() < 0.6);
        assert!((surface.x - f64::from(f32::from(pane.left())) - 2.).abs() < 0.6);
        assert!((surface.y - f64::from(f32::from(pane.top())) - 32.).abs() < 0.6);
        assert!((surface.width - f64::from(f32::from(pane.size.width)) + 4.).abs() < 0.6);
        assert!((surface.height - f64::from(f32::from(pane.size.height)) + 34.).abs() < 0.6);
    }
    let first = cx
        .debug_bounds(test_selector(format!(
            "split-pane-{}",
            before.panes[0].get()
        )))
        .unwrap();
    let last = cx
        .debug_bounds(test_selector(format!(
            "split-pane-{}",
            before.panes[3].get()
        )))
        .unwrap();
    if vertical {
        assert!(first.top() < gpui::px(0.));
    } else {
        assert!(last.right() > gpui::px(800.));
    }
    // One divider is visible, while another lies beyond the clipped window.
    let visible = if vertical {
        "split-divider-1"
    } else {
        "split-divider-0"
    };
    let clipped = if vertical {
        "split-divider-0"
    } else {
        "split-divider-2"
    };
    let start = cx.debug_bounds(visible).unwrap().center();
    let hidden = cx.debug_bounds(clipped).unwrap().center();
    assert!(
        start.x >= gpui::px(0.)
            && start.x < gpui::px(800.)
            && start.y >= gpui::px(0.)
            && start.y < gpui::px(600.)
    );
    assert!(hidden.x > gpui::px(800.) || hidden.y < gpui::px(0.));
    let motion = if vertical {
        gpui::point(gpui::px(0.), gpui::px(12.))
    } else {
        gpui::point(gpui::px(12.), gpui::px(0.))
    };
    fake.take_calls();
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(start + motion, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.update(|_, cx| cx.has_active_drag()),
        "visible divider must capture a typed drag even at minimum size"
    );
    // No pane can shrink below its minimum; the drag remains a valid no-op.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| cx.has_active_drag()));
    finish_sidebar_drag(cx, start + motion);
    cx.simulate_mouse_move(hidden, None, Modifiers::none());
    cx.simulate_mouse_down(hidden, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(hidden + motion, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(
        !cx.update(|_, cx| cx.has_active_drag()),
        "clipped divider must not capture off-window input"
    );
    finish_sidebar_drag(cx, hidden + motion);
    view.read_with(cx, |view, _| {
        let split = view.browser.active_split().unwrap();
        assert_eq!(split.panes, before.panes);
        assert_eq!(split.active, before.active);
        for (after, before) in split.weights.iter().zip(&before.weights) {
            assert!((after - before).abs() < 0.001);
        }
    });
    assert_eq!(active_url(&view, cx), active);
    assert_eq!(
        view.read_with(cx, |view, _| view
            .pages
            .as_ref()
            .unwrap()
            .borrow()
            .surface_layout()),
        surfaces
    );
    assert!(!fake.take_calls().iter().any(|call| matches!(
        call,
        Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..)
    )));
}

#[gpui::test]
fn four_horizontal_overflow_panes_keep_visible_divider_hit_testing_and_clip_off_window_targets(
    cx: &mut TestAppContext,
) {
    exercise_overflow(cx, false);
}

#[gpui::test]
fn four_vertical_overflow_panes_keep_visible_divider_hit_testing_and_clip_off_window_targets(
    cx: &mut TestAppContext,
) {
    exercise_overflow(cx, true);
}
