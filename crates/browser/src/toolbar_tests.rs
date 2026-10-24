use super::*;

#[gpui::test]
fn toolbar_moves_address_out_of_sidebar_and_keeps_url_editing(cx: &mut TestAppContext) {
    let (view, visual) = open(cx);
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("https://toolbar.example/path");
    bar(&view, visual).simulate_keystrokes("enter");
    visual.run_until_parked();
    let favorites = visual.debug_bounds("favorites").unwrap();
    assert!(visual.debug_bounds("address").is_some());
    visual.simulate_keystrokes("cmd-shift-d");
    visual.run_until_parked();
    assert!(
        view.read_with(visual, |view, _| view.toolbar_shown()),
        "toolbar shortcut did not execute"
    );
    visual.cx.refresh().unwrap();
    visual.executor().advance_clock(Duration::from_millis(100));
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("top-toolbar").is_some(),
        "toolbar did not paint"
    );
    // GPUI retains debug-selector bounds from earlier frames; verify the moved layout.
    assert_eq!(
        f32::from(visual.debug_bounds("top-toolbar").unwrap().size.height),
        33.
    );
    assert_eq!(
        f32::from(favorites.origin.y - visual.debug_bounds("favorites").unwrap().origin.y),
        46.
    );
    click(visual, "toolbar-address");
    assert_eq!(
        view.read_with(visual, |view, cx| view
            .command_bar
            .as_ref()
            .unwrap()
            .input
            .read(cx)
            .text()
            .to_string()),
        "https://toolbar.example/path"
    );
    bar(&view, visual).simulate_keystrokes("escape");
    visual.simulate_keystrokes("cmd-shift-d");
    visual.run_until_parked();
    assert!(!view.read_with(visual, |view, _| view.toolbar_shown()));
    assert!(visual.debug_bounds("address").is_some());
}

#[gpui::test]
fn toolbar_offsets_the_native_document_and_squares_its_header_seam_without_reload(
    cx: &mut TestAppContext,
) {
    let (view, visual, fake) = open_pages(cx);
    let url = active_url(&view, visual);
    fake.take_calls();
    visual.simulate_keystrokes("cmd-shift-d");
    visual.cx.refresh().unwrap();
    visual.run_until_parked();
    assert_eq!(
        f32::from(
            visual
                .debug_bounds("page-document-surface")
                .unwrap()
                .origin
                .y
        ),
        43.
    );
    view.read_with(visual, |view, _| {
        let bounds = view.pages.as_ref().unwrap().borrow().surface_bounds();
        assert_eq!(bounds[0].y, 43.);
    });
    let calls = fake.take_calls();
    assert!(calls
        .iter()
        .any(|call| matches!(call, Call::TopCorners(_, false))));
    assert!(!calls.iter().any(|call| matches!(
        call,
        Call::Create(..) | Call::Navigate(..) | Call::Close(..)
    )));
    visual.simulate_keystrokes("cmd-shift-d");
    visual.cx.refresh().unwrap();
    visual.run_until_parked();
    assert_eq!(
        f32::from(
            visual
                .debug_bounds("page-document-surface")
                .unwrap()
                .origin
                .y
        ),
        10.
    );
    assert!(fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::TopCorners(_, true))));
    assert_eq!(active_url(&view, visual), url);
}

#[gpui::test]
fn toolbar_mode_preserves_split_pane_headers_and_native_geometry(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_split_pages(cx, 2);
    let before = view.read_with(visual, |view, _| {
        view.pages.as_ref().unwrap().borrow().surface_layout()
    });
    let first = view.read_with(visual, |view, _| {
        view.browser.active_split().unwrap().panes[0]
    });
    let header = test_selector(format!("pane-header-{}", first.get()));
    let bounds = visual.debug_bounds(header).unwrap();
    fake.take_calls();
    visual.simulate_keystrokes("cmd-shift-d");
    visual.cx.refresh().unwrap();
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.toolbar_shown()));
    assert!(visual.debug_bounds("top-toolbar").is_none());
    assert_eq!(visual.debug_bounds(header).unwrap(), bounds);
    assert_eq!(
        view.read_with(visual, |view, _| view
            .pages
            .as_ref()
            .unwrap()
            .borrow()
            .surface_layout()),
        before
    );
    assert!(!fake.take_calls().iter().any(|call| matches!(
        call,
        Call::Create(..) | Call::Navigate(..) | Call::Close(..)
    )));
    click(visual, "go-back");
    assert!(fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::Back(_))));
}
