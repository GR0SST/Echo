use super::*;

#[gpui::test]
fn reorder_feedback_is_per_drag_boundary_and_the_advanced_toggle_disables_it(cx:&mut TestAppContext){
    let(view,cx,fake)=open_pages(cx);
    crate::native::HAPTIC_PULSES.with(|n|n.set(0));
    let before=active_url(&view,cx);fake.take_calls();
    begin_sidebar_drag(cx,"tab-7");
    let target=cx.debug_bounds("tab-6").unwrap();
    let above=gpui::point(target.center().x,target.top()+gpui::px(3.));
    let below=gpui::point(target.center().x,target.bottom()-gpui::px(3.));
    cx.simulate_mouse_move(above,gpui::MouseButton::Left,Modifiers::none());cx.run_until_parked();
    let first=crate::native::HAPTIC_PULSES.with(|n|n.get());assert!(first>0);
    cx.simulate_mouse_move(above,gpui::MouseButton::Left,Modifiers::none());cx.run_until_parked();
    assert_eq!(crate::native::HAPTIC_PULSES.with(|n|n.get()),first);
    cx.simulate_mouse_move(below,gpui::MouseButton::Left,Modifiers::none());cx.run_until_parked();
    assert_eq!(crate::native::HAPTIC_PULSES.with(|n|n.get()),first+1);
    assert_eq!(active_url(&view,cx),before);
    finish_sidebar_drag(cx,below);
    assert!(!fake.take_calls().iter().any(|call|matches!(call,Call::Navigate(..))));
    cx.simulate_keystrokes("cmd-,");let(_,mut settings)=settings_window(cx);
    click(&mut settings,"settings-tab-Advanced");click(&mut settings,"settings-toggle-HapticFeedback");
    settings.simulate_keystrokes("cmd-w");cx.run_until_parked();
    let count=crate::native::HAPTIC_PULSES.with(|n|n.get());
    drag_to_row_edge(cx,"tab-7","tab-8",false);
    assert_eq!(crate::native::HAPTIC_PULSES.with(|n|n.get()),count);
}

#[gpui::test]
fn split_reorder_does_not_pulse_over_another_pane_of_itself(cx:&mut TestAppContext){
    let(view,cx,_)=open_split_pages(cx,2);
    let panes=view.read_with(cx,|view,_|view.browser.active_split().unwrap().panes.clone());
    let source=test_selector(format!("tab-{}",panes[0].get()));
    let target=cx.debug_bounds(test_selector(format!("sidebar-pane-{}",panes[1].get()))).unwrap().center();
    begin_sidebar_drag(cx,source);
    crate::native::HAPTIC_PULSES.with(|count|count.set(0));
    cx.simulate_mouse_move(target,gpui::MouseButton::Left,Modifiers::none());cx.run_until_parked();
    assert_eq!(crate::native::HAPTIC_PULSES.with(|count|count.get()),0);
    finish_sidebar_drag(cx,target);
}

#[gpui::test]
fn adjacent_row_halves_share_one_insertion_line_and_one_haptic(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = active_url(&view, cx);
    fake.take_calls();
    begin_sidebar_drag(cx, "tab-8");
    let upper = cx.debug_bounds("tab-7").unwrap();
    let lower = cx.debug_bounds("tab-6").unwrap();
    let boundary = (upper.bottom() + lower.top()) / 2.;
    let above = gpui::point(upper.center().x, upper.bottom() - gpui::px(3.));
    let below = gpui::point(lower.center().x, lower.top() + gpui::px(3.));
    crate::native::HAPTIC_PULSES.with(|n| n.set(0));
    cx.simulate_mouse_move(above, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let line = cx.debug_bounds("tab-insertion-line-7").unwrap();
    assert_eq!(line.center().y, boundary);
    assert_eq!(crate::native::HAPTIC_PULSES.with(|n| n.get()), 1);
    let middle = gpui::point(upper.center().x, boundary);
    for point in [middle, below, above, middle, below] {
        cx.simulate_mouse_move(point, gpui::MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(crate::native::HAPTIC_PULSES.with(|n| n.get()), 1);
        let selector = if point.y < boundary { "tab-insertion-line-7" } else { "tab-insertion-line-6" };
        assert_eq!(cx.debug_bounds(selector).unwrap(), line);
    }
    assert_eq!(cx.debug_bounds("tab-insertion-line-6").unwrap(), line);
    finish_sidebar_drag(cx, middle);
    view.read_with(cx, |view, _| {
        let ids: Vec<_> = view.browser.space().today.iter().map(|tab| tab.id.get()).collect();
        let upper = ids.iter().position(|id| *id == 7).unwrap();
        assert_eq!(&ids[upper..upper + 3], &[7, 8, 6]);
    });
    assert_eq!(active_url(&view, cx), before);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Navigate(..))));
}

#[gpui::test]
fn pinned_siblings_share_the_gap_even_with_haptics_disabled(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    begin_sidebar_drag(cx, "tab-8");
    let upper = cx.debug_bounds("tab-4").unwrap();
    let lower = cx.debug_bounds("tab-5").unwrap();
    let above = gpui::point(upper.center().x, upper.bottom() - gpui::px(3.));
    let below = gpui::point(lower.center().x, lower.top() + gpui::px(3.));
    crate::native::HAPTIC_PULSES.with(|n| n.set(0));
    cx.simulate_mouse_move(above, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let line = cx.debug_bounds("tab-insertion-line-4").unwrap();
    assert_eq!(line.center().y, (upper.bottom() + lower.top()) / 2.);
    cx.simulate_mouse_move(below, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(crate::native::HAPTIC_PULSES.with(|n| n.get()), 1);
    assert_eq!(cx.debug_bounds("tab-insertion-line-5").unwrap(), line);
    view.update(cx, |view, _| view.edit_profile(|browser| browser.settings.set_enabled(browser_core::settings::Toggle::HapticFeedback, false)));
    cx.simulate_mouse_move(above, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("tab-insertion-line-4").unwrap(), line);
    assert_eq!(crate::native::HAPTIC_PULSES.with(|n| n.get()), 1);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
}

#[gpui::test]
fn dropping_far_below_today_appends_and_keeps_one_end_boundary(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = active_url(&view, cx);
    fake.take_calls();
    begin_sidebar_drag(cx, "tab-8");
    let last = cx.debug_bounds("tab-6").unwrap();
    let end = cx.debug_bounds("tab-list-end").unwrap();
    assert!(end.size.height > gpui::px(100.));
    let near = gpui::point(last.center().x, last.bottom() - gpui::px(3.));
    let far = gpui::point(end.center().x, end.bottom() - gpui::px(20.));
    crate::native::HAPTIC_PULSES.with(|n| n.set(0));
    for point in [near, end.center(), far] {
        cx.simulate_mouse_move(point, gpui::MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(crate::native::HAPTIC_PULSES.with(|n| n.get()), 1);
        assert_eq!(cx.debug_bounds("tab-insertion-line-6").unwrap().center().y, last.bottom() + gpui::px(1.5));
    }
    finish_sidebar_drag(cx, far);
    view.read_with(cx, |view, _| assert_eq!(view.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![7, 6, 8]));
    assert_eq!(active_url(&view, cx), before);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn private_tabs_also_append_when_dropped_in_the_empty_tail(cx: &mut TestAppContext) {
    cx.update(init);
    let (view, visual) = cx.add_window_view(|window, cx| {
        let mut browser = browser_core::Browser::new_private();
        for _ in 0..3 { browser.new_tab("about:blank", std::time::SystemTime::now()); }
        let view = BrowserWindow::new(browser, None, window, cx);
        view.focus(window);
        view
    });
    visual.run_until_parked();
    let ids = view.read_with(visual, |view, _| view.browser.space().today.iter().map(|tab| tab.id).collect::<Vec<_>>());
    begin_sidebar_drag(visual, test_selector(format!("tab-{}", ids[0].get())));
    let tail = visual.debug_bounds("tab-list-end").unwrap();
    let far = gpui::point(tail.center().x, tail.bottom() - gpui::px(20.));
    visual.simulate_mouse_move(far, gpui::MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds(test_selector(format!("tab-insertion-line-{}", ids[2].get()))).is_some());
    finish_sidebar_drag(visual, far);
    view.read_with(visual, |view, _| assert_eq!(view.browser.space().today.last().unwrap().id, ids[0]));
}
