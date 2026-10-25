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
