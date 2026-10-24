//! Arc content-frame and empty-canvas regressions on GPUI's test platform.
use super::*;

#[gpui::test]
fn empty_space_panel_survives_theme_changes_and_returns_when_the_last_page_closes(cx: &mut TestAppContext) {
    cx.update(init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser_core::Browser::new(), None, window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    cx.simulate_resize(gpui::size(gpui::px(1000.), gpui::px(760.)));
    cx.run_until_parked();
    let frame = cx.debug_bounds("empty-space-canvas").unwrap();
    assert_eq!(cx.debug_bounds("empty-space-panel-shadow"), Some(frame));
    assert_eq!(frame.top(), gpui::px(10.));
    assert_eq!(frame.right(), gpui::px(990.));
    assert_eq!(frame.bottom(), gpui::px(750.));
    for appearance in [browser_core::themes::ThemeAppearance::Light, browser_core::themes::ThemeAppearance::Dark] {
        view.update(cx, |w, cx| {
            let mut theme = w.browser.space().theme();
            theme.appearance = appearance;
            let space = w.browser.space().id;
            w.edit_browser(|browser| assert!(browser.update_space_theme(space, theme)));
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(cx.debug_bounds("empty-space-canvas"), Some(frame));
    }
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

#[test]
fn rounded_theme_mask_keeps_the_window_origin_and_measured_corner_area() {
    let bounds = gpui::Bounds::new(gpui::point(gpui::px(228.), gpui::px(10.)), gpui::size(gpui::px(762.), gpui::px(740.)));
    for scale in [1., 2.] {
        let mask = crate::space_theme::rounded_theme_mask(bounds, 6., scale);
        assert!(mask.iter().all(|row| bounds.contains(&row.origin) && row.right() <= bounds.right() && row.bottom() <= bounds.bottom()));
        let area: f32 = mask.iter().map(|row| f32::from(row.size.width) * f32::from(row.size.height)).sum();
        let expected = 762. * 740. - (4. - std::f32::consts::PI) * 36.;
        assert!((area - expected).abs() < 1.);
        assert_eq!(mask[0].origin, gpui::point(gpui::px(228.), gpui::px(16.)));
    }
}
