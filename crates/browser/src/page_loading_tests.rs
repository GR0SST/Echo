use super::*;

fn frame(view: &Entity<BrowserWindow>, visual: &mut VisualTestContext) {
    // The test platform has no display-link callback; invalidate the animated view.
    view.update(visual, |_, cx| cx.notify());
    visual.cx.refresh().unwrap();
    visual.run_until_parked();
}

fn loading(
    root: &Entity<BrowserWindow>,
    fake: &FakeEngine,
    visual: &mut VisualTestContext,
    view: u64,
    active: bool,
) {
    fake.send(
        visual,
        Event::LoadingChanged {
            view: page(view),
            loading: active,
            can_go_back: false,
            can_go_forward: false,
        },
    );
    frame(root, visual);
}

#[gpui::test]
fn loading_chrome_pulses_expands_when_stalled_and_finishes_on_engine_progress(
    cx: &mut TestAppContext,
) {
    let (view, visual, fake) = open_pages(cx);
    loading(&view, &fake, visual, 1, true);
    visual.executor().advance_clock(Duration::from_millis(800));
    frame(&view, visual);
    let compact = visual.debug_bounds("page-loading").unwrap();
    assert_eq!(f32::from(compact.size.height), 4.);
    assert!(
        (f32::from(compact.size.width) - 48.).abs() < 1.,
        "compact width: {}",
        f32::from(compact.size.width)
    );
    assert_eq!(compact.origin.y, gpui::px(3.));
    visual.executor().advance_clock(Duration::from_secs(4));
    frame(&view, visual);
    assert_eq!(
        f32::from(visual.debug_bounds("page-loading").unwrap().size.width),
        128.
    );
    fake.send(
        visual,
        Event::LoadingProgress {
            view: page(1),
            progress: 1000,
        },
    );
    frame(&view, visual);
    visual.executor().advance_clock(Duration::from_millis(300));
    frame(&view, visual);
    assert!(view.read_with(visual, |view, _| view.loading_indicators.is_empty()));
}

#[gpui::test]
fn hidden_page_loads_do_not_draw_in_the_selected_sites_chrome(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    loading(&view, &fake, visual, 1, true);
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("https://not-loading.example/");
    bar(&view, visual).simulate_keystrokes("enter");
    frame(&view, visual);
    assert!(view.read_with(visual, |view, _| view.loading_indicators.is_empty()));
    fake.send(
        visual,
        Event::LoadingProgress {
            view: page(1),
            progress: 250,
        },
    );
    assert!(view.read_with(visual, |view, _| view.loading_indicators.is_empty()));
}

#[gpui::test]
fn showing_the_toolbar_keeps_loading_progress_above_the_whole_card(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    visual.simulate_keystrokes("cmd-shift-d");
    frame(&view, visual);
    loading(&view, &fake, visual, 1, true);
    visual.executor().advance_clock(Duration::from_millis(100));
    frame(&view, visual);
    assert_eq!(
        visual.debug_bounds("page-loading").unwrap().origin.y,
        gpui::px(3.)
    );
    loading(&view, &fake, visual, 1, false);
    visual.executor().advance_clock(Duration::from_millis(300));
    frame(&view, visual);
    assert!(view.read_with(visual, |view, _| view.loading_indicators.is_empty()));
}
