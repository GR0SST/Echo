use super::*;

fn set_zoom(fake: &FakeEngine, percent: u16) {
    fake.0.borrow_mut().site_infos.insert(
        1,
        engine::SiteInfo {
            origin: "https://gpui.rs".into(),
            host: "gpui.rs".into(),
            secure: true,
            certificate: None,
            zoom_percent: percent,
            permissions: Vec::new(),
        },
    );
}

#[gpui::test]
fn zoom_shortcuts_show_the_live_percentage_without_taking_page_focus_and_reset_removes_it(
    cx: &mut TestAppContext,
) {
    let (view, visual, fake) = open_pages(cx);
    assert!(view.read_with(visual, |view, _| view.page_zoom.is_none()));
    set_zoom(&fake, 110);
    fake.take_calls();
    let focused = visual.update(|window, cx| view.read(cx).focus_handle.is_focused(window));
    visual.simulate_keystrokes("cmd-=");
    visual.run_until_parked();
    assert!(fake.take_calls().contains(&Call::Zoom(1, 1)));
    let overlay = view.read_with(visual, |view, _| {
        view.page_zoom.as_ref().unwrap().overlay.unwrap()
    });
    let mut badge = VisualTestContext::from_window(overlay.into(), &visual.cx);
    badge.run_until_parked();
    let rect = badge.debug_bounds("page-zoom").unwrap();
    assert_eq!(rect.size, gpui::size(gpui::px(60.), gpui::px(42.)));
    let frame = view.read_with(visual, |view, _| {
        view.pages.as_ref().unwrap().borrow().surface_layout()[0].1
    });
    assert_eq!(
        rect.origin.x,
        gpui::px((frame.x + frame.width - 68.) as f32)
    );
    assert_eq!(rect.origin.y, gpui::px(frame.y as f32 + 8.));
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
    assert_eq!(
        visual.update(|window, cx| view.read(cx).focus_handle.is_focused(window)),
        focused
    );
    set_zoom(&fake, 100);
    visual.simulate_keystrokes("cmd-0");
    visual.run_until_parked();
    assert!(fake.take_calls().contains(&Call::Zoom(1, 0)));
    assert!(view.read_with(visual, |view, _| view.page_zoom.is_none()));
}

#[gpui::test]
fn a_new_site_does_not_inherit_the_previous_pages_zoom_badge(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    set_zoom(&fake, 150);
    visual.simulate_keystrokes("cmd-=");
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.page_zoom.is_some()));
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("https://unzoomed.example/");
    bar(&view, visual).simulate_keystrokes("enter");
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.page_zoom.is_none()));
    assert!(fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::Create(_, url) if url=="https://unzoomed.example/")));
}

#[gpui::test]
fn zoom_badges_expire_and_an_older_timer_cannot_dismiss_a_new_percentage(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    set_zoom(&fake, 110);
    visual.simulate_keystrokes("cmd-=");
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_secs(4));
    visual.run_until_parked();
    set_zoom(&fake, 125);
    visual.simulate_keystrokes("cmd-=");
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_secs(2));
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.page_zoom.is_some()));
    visual.executor().advance_clock(Duration::from_secs(4));
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.page_zoom.is_none()));
    view.update(visual, |_, cx| cx.notify());
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.page_zoom.is_none()));
}
