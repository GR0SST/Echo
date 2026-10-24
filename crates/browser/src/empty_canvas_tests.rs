//! Arc content-frame and empty-canvas regressions on GPUI's test platform.
use super::*;

#[gpui::test]
fn empty_space_canvas_replaces_page_card_and_new_page_restores_it(cx: &mut TestAppContext) {
    cx.update(init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser_core::Browser::new(), None, window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("empty-space-canvas").is_some());
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://example.com");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.browser.active_tab().is_some()));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.debug_bounds("page-placeholder").is_some());
    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    assert!(cx.debug_bounds("empty-space-canvas").is_some());
    assert!(view.read_with(cx, |view, _| view.browser.active_tab().is_none()));
}

#[gpui::test]
fn page_frame_uses_measured_arc_insets_with_sidebar_shown_and_hidden(cx: &mut TestAppContext) {
    let (_, cx, fake) = open_pages(cx);
    cx.simulate_resize(gpui::size(gpui::px(1000.), gpui::px(760.)));
    cx.run_until_parked();
    let surface = cx.debug_bounds("page-surface").unwrap();
    assert_eq!(surface.top(), gpui::px(10.));
    assert_eq!(surface.right(), gpui::px(990.));
    assert_eq!(surface.bottom(), gpui::px(750.));
    assert_eq!(crate::pages::CONTENT_CORNER_RADIUS, 6.);
    cx.simulate_keystrokes("cmd-s");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    let surface = cx.debug_bounds("page-surface").unwrap();
    assert_eq!(surface.origin, gpui::point(gpui::px(10.), gpui::px(10.)));
    assert_eq!(surface.size, gpui::size(gpui::px(980.), gpui::px(740.)));
    let document = fake.0.borrow().focused_page.unwrap();
    assert_eq!(fake.0.borrow().bounds[&raw(document)], Bounds { x: 10., y: 10., width: 980., height: 740. });
}
