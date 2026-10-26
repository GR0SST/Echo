//! Fresh-window width is captured from committed chrome, then checkpointed.
use super::*;

fn roots(cx: &mut VisualTestContext) -> Vec<Entity<BrowserWindow>> {
    cx.cx.update(|cx| cx.windows().into_iter()
        .filter_map(|window| window.downcast::<BrowserWindow>())
        .filter_map(|window| window.root(cx).ok()).collect())
}

#[gpui::test]
fn new_window_inherits_committed_source_width_without_moving_its_document(cx: &mut TestAppContext) {
    let (source, cx, fake) = open_pages(cx);
    let selected = source.read_with(cx, |view, _| view.browser.active_tab().unwrap().id);
    source.update(cx, |view, _| {
        view.sidebar_width = 312.;
        // Presentation overrides used during layout/drag are not committed.
        view.sidebar_render_width = Some(480.);
        view.sidebar_visible = false;
    });
    fake.take_calls();
    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    let other = roots(cx).into_iter().find(|view| view != &source).unwrap();
    other.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_width, 312.);
        assert!(view.sidebar_visible);
        assert!(view.browser.active_tab().is_none());
    });
    source.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_width, 312.);
        assert_eq!(view.browser.active_tab().unwrap().id, selected);
        assert!(!view.sidebar_visible);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn new_window_overlay_keeps_its_source_width_when_another_window_has_a_newer_checkpoint(cx: &mut TestAppContext) {
    let (source, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    let other = roots(cx).into_iter().find(|view| view != &source).unwrap();
    other.update(cx, |view, _| { view.sidebar_width = 190.; view.checkpoint_window(true); });
    source.update(cx, |view, _| view.sidebar_width = 337.);
    cx.update(|window, _| window.activate_window());
    cx.simulate_keystrokes("cmd-t");
    bar(&source, cx).simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    let third = roots(cx).into_iter().find(|view| view != &source && view != &other).unwrap();
    assert_eq!(third.read_with(cx, |view, _| view.sidebar_width), 337.);
    assert_eq!(other.read_with(cx, |view, _| view.sidebar_width), 190.);
    assert!(bar_input_focused(&third, cx));
}

#[gpui::test]
fn renderer_new_window_captures_source_width_and_checkpoints_it_for_restore(cx: &mut TestAppContext) {
    let (source, cx, fake) = open_pages(cx);
    source.update(cx, |view, _| view.sidebar_width = 365.);
    let shortcut = crate::window::page_shortcuts().into_iter()
        .find(|s| s.cmd && !s.shift && !s.alt && !s.ctrl && s.key == "n").unwrap();
    fake.take_calls();
    fake.send(cx, fake.shortcut(shortcut));
    let other = roots(cx).into_iter().find(|view| view != &source).unwrap();
    assert_eq!(other.read_with(cx, |view, _| view.sidebar_width), 365.);
    let json = other.read_with(cx, |view, _| view.profile.handle.read_settings(browser_core::session::to_json));
    let restored = browser_core::session::from_json(&json).unwrap();
    assert_eq!(restored.windows.len(), 1);
    assert_eq!(restored.windows[0].sidebar_width, 365.);
    // An explicit restored checkpoint wins over a different fresh-window width.
    let saved = restored.windows[0].clone();
    let mut other_cx = VisualTestContext::from_window(other.read_with(cx, |view, _| view.main_window), &cx.cx);
    other_cx.update(|window, cx| other.update(cx, |view, cx| {
        view.sidebar_width = 228.;
        view.attach_restoration(Some(saved), false, window, cx);
        assert_eq!(view.sidebar_width, 365.);
    }));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn app_level_new_window_uses_the_latest_normal_checkpoint_width(cx: &mut TestAppContext) {
    let (source, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    let other = roots(cx).into_iter().find(|view| view != &source).unwrap();
    other.update(cx, |view, _| { view.sidebar_width = 294.; view.checkpoint_window(true); });
    cx.cx.update(|cx| crate::new_window::open(true, cx));
    cx.run_until_parked();
    let third = roots(cx).into_iter().find(|view| view != &source && view != &other).unwrap();
    assert_eq!(third.read_with(cx, |view, _| view.sidebar_width), 294.);
}
