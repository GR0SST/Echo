//! Real GPUI layout/input tests with fictional parent-owned completed downloads.
use super::*;
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use std::cell::RefCell;

type Log = Rc<RefCell<Vec<DownloadsIntent<u64>>>>;
struct Host {
    view: Entity<DownloadsView<u64>>,
    focus: gpui::FocusHandle,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .child(div().absolute().left_0().top_0().bottom_0().w(px(112.)))
            .child(
                div()
                    .absolute()
                    .left(px(112.))
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .child(self.view.clone()),
            )
    }
}
fn colors() -> DownloadsColors {
    DownloadsColors {
        column: gpui::rgb(0xeeeeee).into(),
        preview: gpui::rgb(0xdddddd).into(),
        text: gpui::rgb(0x111111).into(),
        muted: gpui::rgb(0x666666).into(),
        control: gpui::rgb(0xcccccc).into(),
        selected: gpui::rgb(0xbbbbbb).into(),
        hover: gpui::rgb(0xaaaaaa).into(),
    }
}
fn fixture() -> DownloadsProjection<u64> {
    let mut browser = browser_core::Browser::new();
    let default = browser.default_user_profile_id();
    let work = browser.create_user_profile("Work").unwrap();
    DownloadsProjection {
        profiles: vec![
            DownloadsProfile {
                id: default,
                name: "Default".into(),
                selected: false,
            },
            DownloadsProfile {
                id: work,
                name: "Work".into(),
                selected: false,
            },
        ],
        list: DownloadsList::Completed(vec![DownloadsGroup {
            heading: "Today".into(),
            rows: vec![CompletedDownload {
                token: 101,
                profile: work,
                title: "Example report.pdf".into(),
                subtitle: "PDF document · example.test".into(),
            }],
        }]),
    }
}
fn open(
    cx: &mut TestAppContext,
    projection: DownloadsProjection<u64>,
) -> (
    Entity<DownloadsView<u64>>,
    &mut VisualTestContext,
    Log,
    Subscription,
) {
    cx.update(crate::text_input::bind_keys);
    cx.update(super::bind_keys);
    let (host, visual) = cx.add_window_view(|_, cx| Host {
        view: cx.new(|cx| {
            DownloadsView::new(
                projection,
                colors(),
                Rc::new(|_, _| {
                    div()
                        .debug_selector(|| "parent-download-preview".into())
                        .size_full()
                        .into_any_element()
                }),
                cx,
            )
        }),
        focus: cx.focus_handle(),
    });
    let view = host.read_with(visual, |host, _| host.view.clone());
    let log: Log = Rc::default();
    let captured = log.clone();
    let subscription = visual.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &DownloadsIntent<u64>, _| {
            captured.borrow_mut().push(event.clone())
        })
    });
    visual.simulate_resize(gpui::size(px(1320.), px(800.)));
    visual.run_until_parked();
    (view, visual, log, subscription)
}
fn click(visual: &mut VisualTestContext, selector: &'static str) {
    let bounds = visual
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    visual.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_click(bounds.center(), Modifiers::none());
    visual.run_until_parked();
}

#[gpui::test]
fn list_starts_after_navigation_and_preview_is_parent_owned(cx: &mut TestAppContext) {
    let (_, visual, log, _subscription) = open(cx, fixture());
    let root = visual.debug_bounds("downloads-view").unwrap();
    let column = visual.debug_bounds("downloads-column").unwrap();
    let preview = visual.debug_bounds("downloads-preview").unwrap();
    let parent = visual.debug_bounds("parent-download-preview").unwrap();
    assert_eq!(root.origin.x, px(112.));
    assert_eq!(column.origin.x, px(112.));
    assert_eq!(column.size.width, px(305.));
    assert_eq!(preview.origin.x, px(417.));
    assert_eq!(preview.right(), root.right());
    assert_eq!(parent.origin, preview.origin);
    assert!(
        log.borrow().is_empty(),
        "presentation must not start a transfer or file action"
    );
}

#[gpui::test]
fn inline_filter_done_and_profile_intent_keep_identity_after_reordering(cx: &mut TestAppContext) {
    let projection = fixture();
    let expected = projection.profiles[1].id;
    let (view, visual, log, _subscription) = open(cx, projection.clone());
    click(visual, "downloads-filter");
    assert!(visual.debug_bounds("downloads-filter-inline").is_some());
    assert!(view.read_with(visual, |view, _| view.filter_visible));
    let mut replacement = projection;
    replacement.profiles.reverse();
    replacement.profiles[0].selected = true;
    visual.update(|window, cx| {
        view.update(cx, |view, cx| view.set_projection(replacement, window, cx));
    });
    visual.run_until_parked();
    click(visual, "downloads-profile-0");
    assert!(
        view.read_with(visual, |view, _| view.projection.profiles[0].selected),
        "the presenter must not invent Profile multi-select/filter semantics"
    );
    click(visual, "downloads-filter-done");
    assert!(visual.debug_bounds("downloads-search-toolbar").is_some());
    assert!(!view.read_with(visual, |view, _| view.filter_visible));
    // A removed selector can remain cached by GPUI. Prove that the restored
    // search toolbar's real Filter control is clickable again instead.
    click(visual, "downloads-filter");
    assert!(view.read_with(visual, |view, _| view.filter_visible));
    assert_eq!(
        *log.borrow(),
        vec![
            DownloadsIntent::FilterVisibilityChanged(true),
            DownloadsIntent::ToggleProfile(expected),
            DownloadsIntent::FilterVisibilityChanged(false),
            DownloadsIntent::FilterVisibilityChanged(true)
        ]
    );
}

#[gpui::test]
fn typing_emits_query_without_locally_hiding_rows_and_done_retains_it(cx: &mut TestAppContext) {
    let (view, visual, log, _subscription) = open(cx, fixture());
    click(visual, "downloads-search");
    visual.simulate_input("fictional query");
    visual.run_until_parked();
    assert!(log
        .borrow()
        .contains(&DownloadsIntent::SearchChanged("fictional query".into())));
    assert!(visual.debug_bounds("downloads-row-0-0").is_some());
    click(visual, "downloads-filter");
    click(visual, "downloads-filter-done");
    assert_eq!(
        view.read_with(visual, |view, cx| view.input.read(cx).text().to_owned()),
        "fictional query"
    );
}

#[gpui::test]
fn filtered_empty_is_centered_in_list_body_not_preview(cx: &mut TestAppContext) {
    let mut projection = fixture();
    projection.list = DownloadsList::FilteredEmpty;
    let (_, visual, _, _subscription) = open(cx, projection);
    click(visual, "downloads-filter");
    for viewport in [
        gpui::size(px(1320.), px(800.)),
        gpui::size(px(640.), px(480.)),
    ] {
        visual.simulate_resize(viewport);
        visual.run_until_parked();
        let body = visual.debug_bounds("downloads-list-body").unwrap();
        let icon = visual.debug_bounds("downloads-empty-icon").unwrap();
        let title = visual.debug_bounds("downloads-empty-title").unwrap();
        let subtitle = visual.debug_bounds("downloads-empty-body").unwrap();
        let preview = visual.debug_bounds("downloads-preview").unwrap();
        assert!((icon.center().x - body.center().x).abs() <= px(1.));
        assert!((title.center().x - body.center().x).abs() <= px(1.));
        assert!((subtitle.center().x - body.center().x).abs() <= px(1.));
        assert!(((icon.origin.y + subtitle.bottom()) / 2. - body.center().y).abs() <= px(1.));
        assert!(subtitle.right() <= body.right());
        assert!(icon.center().x < preview.origin.x);
        assert!(visual.debug_bounds("downloads-filter-done").is_some());
        assert!(visual.debug_bounds("downloads-row-0-0").is_none());
    }
}

#[gpui::test]
fn search_miss_keeps_query_and_filter_controls_in_the_list_column(cx: &mut TestAppContext) {
    let query = " SOURCE-TAB ";
    let mut projection = fixture();
    projection.list = DownloadsList::SearchEmpty {
        query: query.into(),
    };
    let (view, visual, log, _subscription) = open(cx, projection);
    click(visual, "downloads-search");
    visual.simulate_input(query);
    click(visual, "downloads-filter");
    click(visual, "downloads-filter-done");
    assert_eq!(
        view.read_with(visual, |view, cx| view.input.read(cx).text().to_owned()),
        query
    );
    for viewport in [
        gpui::size(px(1320.), px(800.)),
        gpui::size(px(640.), px(480.)),
    ] {
        visual.simulate_resize(viewport);
        visual.run_until_parked();
        let body = visual.debug_bounds("downloads-list-body").unwrap();
        let icon = visual.debug_bounds("downloads-search-empty-icon").unwrap();
        let title = visual.debug_bounds("downloads-search-empty-title").unwrap();
        let preview = visual.debug_bounds("downloads-preview").unwrap();
        assert!((icon.center().x - body.center().x).abs() <= px(1.));
        assert!(title.origin.x >= body.origin.x && title.right() <= body.right());
        assert!(((icon.origin.y + title.bottom()) / 2. - body.center().y).abs() <= px(1.));
        assert!(title.right() <= preview.origin.x);
    }
    assert!(log
        .borrow()
        .iter()
        .all(|intent| !matches!(intent, DownloadsIntent::RowPressed { .. })));
}

#[gpui::test]
fn long_completed_names_and_wrapping_chips_preserve_small_window_geometry(cx: &mut TestAppContext) {
    let mut projection = fixture();
    let DownloadsList::Completed(groups) = &mut projection.list else {
        unreachable!()
    };
    groups[0].rows[0].title = "A fictional unusually long completed report name ".repeat(12);
    groups[0].rows[0].subtitle =
        "PDF document · very-long-fictional-domain.example.test".repeat(10);
    projection.profiles[0].name = "A fictional very long local Profile name ".repeat(8);
    let long_profile = projection.profiles[0].name.clone();
    let (view, visual, _, _subscription) = open(cx, projection);
    view.update(visual, |view, cx| {
        view.set_row_artwork(
            Rc::new(|_, _, _| div().size(px(600.)).into_any_element()),
            cx,
        )
    });
    visual.simulate_resize(gpui::size(px(640.), px(480.)));
    visual.run_until_parked();
    let row = visual.debug_bounds("downloads-row-0-0").unwrap();
    let title = visual.debug_bounds("downloads-title-0-0").unwrap();
    let subtitle = visual.debug_bounds("downloads-subtitle-0-0").unwrap();
    let artwork = visual.debug_bounds("downloads-artwork-0-0").unwrap();
    assert_eq!(row.size, gpui::size(px(289.), px(56.)));
    assert_eq!(artwork.size, gpui::size(px(36.), px(36.)));
    assert_eq!(title.size.height, px(18.));
    assert_eq!(subtitle.size.height, px(16.));
    assert!(title.right() <= row.right() - px(4.));
    assert!(subtitle.right() <= row.right() - px(4.));
    assert!(title.origin.x >= artwork.right() + px(4.));
    click(visual, "downloads-filter");
    let chips = visual.debug_bounds("downloads-profile-chips").unwrap();
    let first = visual.debug_bounds("downloads-profile-0").unwrap();
    let second = visual.debug_bounds("downloads-profile-1").unwrap();
    assert!(first.right() <= chips.right());
    assert!(
        second.origin.y >= first.bottom(),
        "the second chip must wrap below the full-width name"
    );
    crate::sidebar_title::take_painted_titles();
    visual.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
    let records = crate::sidebar_title::take_painted_titles();
    let label = records
        .iter()
        .rev()
        .find(|record| record.original == long_profile)
        .expect("the actual long Profile chip label must paint");
    assert_eq!(label.len, label.text.len());
    assert!(label.shaped_width <= label.width);
    assert!(label.text.ends_with('…'));
    let short = records
        .iter()
        .rev()
        .find(|record| record.original == "Work")
        .expect("the short Profile chip label must paint too");
    assert_eq!(short.text, "Work");
    assert_eq!(short.glyph_indices, (0..4).collect::<Vec<_>>());
    assert!(
        visual
            .debug_bounds("downloads-filter-done")
            .unwrap()
            .right()
            <= px(417.)
    );
}

#[gpui::test]
fn completed_rows_scroll_but_toolbar_stays_and_row_effects_require_opt_in(cx: &mut TestAppContext) {
    let mut projection = fixture();
    let profile = projection.profiles[1].id;
    let DownloadsList::Completed(groups) = &mut projection.list else {
        unreachable!()
    };
    groups[0].rows = (0..30)
        .map(|index| CompletedDownload {
            token: 1000 + index,
            profile,
            title: format!("Example {index}.pdf"),
            subtitle: "PDF document · example.test".into(),
        })
        .collect();
    let (view, visual, log, _subscription) = open(cx, projection);
    visual.simulate_resize(gpui::size(px(640.), px(480.)));
    visual.run_until_parked();
    click(visual, "downloads-row-0-0");
    assert!(log.borrow().is_empty());
    view.update(visual, |view, cx| view.set_row_selection_enabled(true, cx));
    visual.run_until_parked();
    let toolbar = visual.debug_bounds("downloads-search-toolbar").unwrap();
    let column = visual.debug_bounds("downloads-column").unwrap();
    let body = visual.debug_bounds("downloads-list-body").unwrap();
    visual.simulate_mouse_move(body.center(), None, Modifiers::none());
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: body.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-3000.))),
        modifiers: Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    visual.run_until_parked();
    assert_eq!(
        visual.debug_bounds("downloads-search-toolbar").unwrap(),
        toolbar
    );
    assert_eq!(visual.debug_bounds("downloads-column").unwrap(), column);
    let last = visual.debug_bounds("downloads-row-0-29").unwrap();
    assert!(last.origin.y >= body.origin.y);
    assert!(last.bottom() <= body.bottom());
    click(visual, "downloads-row-0-29");
    assert_eq!(
        *log.borrow(),
        vec![DownloadsIntent::RowPressed {
            token: 1029,
            profile
        }]
    );
}

fn click_at(visual: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    visual.simulate_mouse_move(position, None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_click(position, Modifiers::none());
    visual.run_until_parked();
}

#[gpui::test]
fn search_icon_padding_and_vertical_gutters_focus_input_under_focusable_host(
    cx: &mut TestAppContext,
) {
    let (view, visual, log, _subscription) = open(cx, fixture());
    let input = view.read_with(visual, |view, _| view.input.clone());
    let field = visual.debug_bounds("downloads-search").unwrap();
    for position in [
        gpui::point(field.origin.x + px(2.), field.center().y),
        gpui::point(field.right() - px(2.), field.center().y),
        gpui::point(field.center().x, field.origin.y + px(2.)),
        gpui::point(field.center().x, field.bottom() - px(2.)),
        gpui::point(field.origin.x + px(17.), field.center().y),
    ] {
        input.update(visual, |input, cx| input.set_text("", cx));
        visual.update(|window, _| window.blur());
        visual.run_until_parked();
        log.borrow_mut().clear();
        click_at(visual, position);
        assert!(
            visual.update(|window, cx| input.read(cx).focus_handle(cx).is_focused(window)),
            "the visible search field must focus input at {position:?}"
        );
        visual.simulate_input("x");
        visual.run_until_parked();
        assert_eq!(
            view.read_with(visual, |view, cx| view.input.read(cx).text().to_owned()),
            "x"
        );
        assert_eq!(
            *log.borrow(),
            vec![DownloadsIntent::SearchChanged("x".into())]
        );
    }
}

#[gpui::test]
fn inner_search_clicks_preserve_caret_placement(cx: &mut TestAppContext) {
    let (view, visual, _, _subscription) = open(cx, fixture());
    let input = view.read_with(visual, |view, _| view.input.clone());
    input.update(visual, |input, cx| input.set_text("abcd", cx));
    visual.update(|window, _| window.blur());
    visual.run_until_parked();
    let inner = visual.debug_bounds("downloads-search-input").unwrap();
    click_at(
        visual,
        gpui::point(inner.origin.x + px(1.), inner.center().y),
    );
    visual.simulate_input("X");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |view, cx| view.input.read(cx).text().to_owned()),
        "Xabcd"
    );
    click_at(
        visual,
        gpui::point(inner.right() - px(1.), inner.center().y),
    );
    visual.simulate_input("Y");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |view, cx| view.input.read(cx).text().to_owned()),
        "XabcdY"
    );
}

#[gpui::test]
fn completed_labels_paint_fresh_full_runs_at_final_row_width(cx: &mut TestAppContext) {
    let mut projection = fixture();
    let profile = projection.profiles[1].id;
    let cases = [
        ("sample.pdf".to_owned(), "PDF document".to_owned()),
        (
            "A deliberately oversized fictional completed download title ".repeat(8),
            "A deliberately oversized fictional source domain ".repeat(8),
        ),
    ];
    projection.list = DownloadsList::Completed(vec![DownloadsGroup {
        heading: "Today".into(),
        rows: cases
            .iter()
            .enumerate()
            .map(|(index, (title, subtitle))| CompletedDownload {
                token: 2000 + index as u64,
                profile,
                title: title.clone(),
                subtitle: subtitle.clone(),
            })
            .collect(),
    }]);
    let (_, visual, _, _subscription) = open(cx, projection);
    for viewport in [
        gpui::size(px(640.), px(480.)),
        gpui::size(px(1280.), px(800.)),
    ] {
        visual.simulate_resize(viewport);
        visual.run_until_parked();
        crate::sidebar_title::take_painted_titles();
        visual.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        let records = crate::sidebar_title::take_painted_titles();
        for (index, (title, subtitle)) in cases.iter().enumerate() {
            for original in [title, subtitle] {
                let painted = records
                    .iter()
                    .rev()
                    .find(|record| &record.original == original)
                    .expect("the real completed row label must reach paint");
                assert_eq!(painted.width, px(217.));
                assert_eq!(painted.len, painted.text.len());
                assert!(painted.shaped_width <= painted.width);
                if index == 0 {
                    assert_eq!(painted.text, *original);
                    assert_eq!(
                        painted.glyph_indices,
                        (0..original.len()).collect::<Vec<_>>()
                    );
                } else {
                    assert!(painted.text.ends_with('…'));
                    assert_ne!(painted.text, *original);
                }
            }
        }
    }
}

fn many_profiles() -> DownloadsProjection<u64> {
    let mut projection = fixture();
    let mut browser = browser_core::Browser::new();
    projection.profiles = (0..20)
        .map(|index| {
            let name = format!("Fictional long local Profile {index} ").repeat(8);
            DownloadsProfile {
                id: browser.create_user_profile(&name).unwrap(),
                name,
                selected: false,
            }
        })
        .collect();
    projection
}

#[gpui::test]
fn many_profile_chips_scroll_to_click_without_displacing_done_or_results(cx: &mut TestAppContext) {
    let projection = many_profiles();
    let expected = projection.profiles[19].id;
    let (_, visual, log, _subscription) = open(cx, projection);
    visual.simulate_resize(gpui::size(px(640.), px(480.)));
    visual.run_until_parked();
    click(visual, "downloads-filter");
    let header = visual.debug_bounds("downloads-filter-inline").unwrap();
    let done = visual.debug_bounds("downloads-filter-done").unwrap();
    let body = visual.debug_bounds("downloads-list-body").unwrap();
    let chips = visual.debug_bounds("downloads-profile-chips").unwrap();
    assert!(chips.size.height <= px(130.));
    assert!(body.size.height >= px(200.));
    assert!(visual.debug_bounds("downloads-row-0-0").unwrap().bottom() <= body.bottom());
    assert!(
        visual
            .debug_bounds("downloads-profile-19")
            .unwrap()
            .origin
            .y
            >= chips.bottom()
    );
    visual.simulate_mouse_move(chips.center(), None, Modifiers::none());
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: chips.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-3000.))),
        modifiers: Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    visual.run_until_parked();
    let last = visual.debug_bounds("downloads-profile-19").unwrap();
    assert!(last.origin.y >= chips.origin.y);
    assert!(last.bottom() <= chips.bottom());
    assert_eq!(
        visual.debug_bounds("downloads-filter-inline").unwrap(),
        header
    );
    assert_eq!(visual.debug_bounds("downloads-filter-done").unwrap(), done);
    assert_eq!(visual.debug_bounds("downloads-list-body").unwrap(), body);
    click(visual, "downloads-profile-19");
    click(visual, "downloads-filter-done");
    assert_eq!(
        *log.borrow(),
        vec![
            DownloadsIntent::FilterVisibilityChanged(true),
            DownloadsIntent::ToggleProfile(expected),
            DownloadsIntent::FilterVisibilityChanged(false),
        ]
    );
}

#[gpui::test]
fn keyboard_filter_profiles_and_done_traverse_scroll_and_restore_focus(cx: &mut TestAppContext) {
    let projection = many_profiles();
    let expected = projection.profiles[19].id;
    let (view, visual, log, _subscription) = open(cx, projection.clone());
    visual.simulate_resize(gpui::size(px(640.), px(480.)));
    visual.run_until_parked();
    // Enter through the host-facing Focusable entry; all controls below are
    // reached and activated by real keystrokes rather than dispatched actions.
    visual.update(|window, cx| view.read(cx).focus_handle(cx).focus(window));
    press_key(visual, "tab");
    press_key(visual, "enter");
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.filter_visible));
    assert!(visual.update(|window, cx| view.read(cx).done_focus.is_focused(window)));
    for _ in 0..20 {
        press_key(visual, "tab");
        visual.run_until_parked();
    }
    let chips = visual.debug_bounds("downloads-profile-chips").unwrap();
    let last = visual.debug_bounds("downloads-profile-19").unwrap();
    assert!(last.origin.y >= chips.origin.y && last.bottom() <= chips.bottom());
    assert!(visual.update(|window, cx| view.read(cx).profile_focus[19].1.is_focused(window)));
    assert!(view.read_with(visual, |view, _| view.chips_scroll.offset().y < px(0.)));
    // Reverse all 20 long chips while focused at the bottom. The retained ID
    // moves to index zero and must be revealed there without another Tab.
    let mut replacement = projection;
    replacement.profiles.reverse();
    visual.update(|window, cx| {
        view.update(cx, |view, cx| view.set_projection(replacement, window, cx));
    });
    visual.run_until_parked();
    assert!(visual.update(|window, cx| view.read(cx).profile_focus[0].1.is_focused(window)));
    assert_eq!(
        view.read_with(visual, |view, _| view.profile_focus[0].0),
        expected
    );
    let retained = visual.debug_bounds("downloads-profile-0").unwrap();
    assert!(retained.origin.y >= chips.origin.y && retained.bottom() <= chips.bottom());
    press_key(visual, "space");
    visual.run_until_parked();
    press_key(visual, "shift-tab");
    press_key(visual, "enter");
    visual.run_until_parked();
    assert!(!view.read_with(visual, |view, _| view.filter_visible));
    assert!(visual.update(|window, cx| view.read(cx).filter_focus.is_focused(window)));
    press_key(visual, "shift-tab");
    let query = "retained query";
    visual.simulate_input(query);
    visual.run_until_parked();
    assert!(visual.update(|window, cx| view
        .read(cx)
        .input
        .read(cx)
        .focus_handle(cx)
        .is_focused(window)));
    // Reopen with Space, wrap backwards to the last chip, forwards to Done,
    // and close with Space as well. Search typing must never activate a chip.
    for key in ["tab", "space", "shift-tab", "tab", "space"] {
        press_key(visual, key);
        visual.run_until_parked();
    }
    assert!(visual.update(|window, cx| view.read(cx).filter_focus.is_focused(window)));
    assert_eq!(
        view.read_with(visual, |view, cx| view.input.read(cx).text().to_owned()),
        query
    );
    let mut expected_intents = vec![
        DownloadsIntent::FilterVisibilityChanged(true),
        DownloadsIntent::ToggleProfile(expected),
        DownloadsIntent::FilterVisibilityChanged(false),
    ];
    // Real input emits Changed for every character. Keep every query prefix
    // and control event in order rather than imposing host debounce policy.
    expected_intents.extend(query.chars().scan(String::new(), |prefix, character| {
        prefix.push(character);
        Some(DownloadsIntent::SearchChanged(prefix.clone()))
    }));
    expected_intents.extend([
        DownloadsIntent::FilterVisibilityChanged(true),
        DownloadsIntent::FilterVisibilityChanged(false),
    ]);
    assert_eq!(*log.borrow(), expected_intents);
}

/// Unlike simulate_keystrokes (keydown only), this includes GPUI's focused
/// on_click keyup path, with a frame between down and up to catch focus changes.
fn key_down(visual: &mut VisualTestContext, key: &str) {
    visual.simulate_event(gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse(key).unwrap(),
        is_held: false,
    });
    visual.run_until_parked();
}

fn key_up(visual: &mut VisualTestContext, key: &str) {
    visual.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse(key).unwrap(),
    });
    visual.run_until_parked();
}

fn press_key(visual: &mut VisualTestContext, key: &str) {
    key_down(visual, key);
    key_up(visual, key);
}

#[gpui::test]
fn control_key_down_up_pairs_activate_once_without_cross_toolbar_activation(
    cx: &mut TestAppContext,
) {
    let projection = fixture();
    let profile = projection.profiles[0].id;
    let (view, visual, log, _subscription) = open(cx, projection);
    visual.update(|window, cx| view.read(cx).focus_handle(cx).focus(window));
    press_key(visual, "tab");
    let mut expected = Vec::new();
    for key in ["enter", "space"] {
        key_down(visual, key);
        assert_eq!(*log.borrow(), expected, "Filter must wait for keyup");
        assert!(!view.read_with(visual, |view, _| view.filter_visible));
        key_up(visual, key);
        expected.push(DownloadsIntent::FilterVisibilityChanged(true));
        assert_eq!(*log.borrow(), expected);
        assert!(view.read_with(visual, |view, _| view.filter_visible));
        assert!(visual.update(|window, cx| view.read(cx).done_focus.is_focused(window)));
        press_key(visual, "tab");
        key_down(visual, key);
        assert_eq!(*log.borrow(), expected, "Profile must wait for keyup");
        key_up(visual, key);
        expected.push(DownloadsIntent::ToggleProfile(profile));
        assert_eq!(*log.borrow(), expected, "Profile toggles exactly once");
        press_key(visual, "shift-tab");
        key_down(visual, key);
        assert_eq!(*log.borrow(), expected, "Done must wait for keyup");
        key_up(visual, key);
        expected.push(DownloadsIntent::FilterVisibilityChanged(false));
        assert_eq!(*log.borrow(), expected);
        assert!(!view.read_with(visual, |view, _| view.filter_visible));
        assert!(visual.update(|window, cx| view.read(cx).filter_focus.is_focused(window)));
    }
}

#[gpui::test]
fn removing_focused_profile_restores_done_and_scoped_keyboard(cx: &mut TestAppContext) {
    let projection = fixture();
    let survivor = projection.profiles[1].id;
    let (view, visual, log, _subscription) = open(cx, projection.clone());
    visual.update(|window, cx| view.read(cx).focus_handle(cx).focus(window));
    press_key(visual, "tab");
    press_key(visual, "enter");
    press_key(visual, "tab");
    assert!(visual.update(|window, cx| view.read(cx).profile_focus[0].1.is_focused(window)));
    let mut replacement = projection;
    replacement.profiles.remove(0);
    visual.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_projection(replacement.clone(), window, cx)
        });
    });
    visual.run_until_parked();
    assert!(visual.update(|window, cx| view.read(cx).done_focus.is_focused(window)));
    press_key(visual, "tab");
    press_key(visual, "space");
    // Also cover removing every Profile while the surviving chip is focused.
    replacement.profiles.clear();
    visual.update(|window, cx| {
        view.update(cx, |view, cx| view.set_projection(replacement, window, cx));
    });
    visual.run_until_parked();
    assert!(visual.update(|window, cx| view.read(cx).done_focus.is_focused(window)));
    press_key(visual, "tab");
    assert!(visual.update(|window, cx| view.read(cx).done_focus.is_focused(window)));
    press_key(visual, "enter");
    assert!(visual.update(|window, cx| view.read(cx).filter_focus.is_focused(window)));
    assert_eq!(
        *log.borrow(),
        vec![
            DownloadsIntent::FilterVisibilityChanged(true),
            DownloadsIntent::ToggleProfile(survivor),
            DownloadsIntent::FilterVisibilityChanged(false),
        ]
    );
}

fn mouse_down_at(visual: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    visual.simulate_mouse_move(position, None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_event(gpui::MouseDownEvent {
        position,
        button: gpui::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    visual.run_until_parked();
}

fn mouse_up_at(visual: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    visual.simulate_event(gpui::MouseUpEvent {
        position,
        button: gpui::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    visual.run_until_parked();
}

#[gpui::test]
fn held_profile_press_cannot_activate_position_replacement_after_projection_update(
    cx: &mut TestAppContext,
) {
    for reorder in [true, false] {
        let projection = fixture();
        let original = projection.profiles[0].id;
        let (view, visual, log, _subscription) = open(cx, projection.clone());
        click(visual, "downloads-filter");
        let pressed = visual.debug_bounds("downloads-profile-0").unwrap();
        let position = gpui::point(pressed.origin.x + px(12.), pressed.center().y);
        mouse_down_at(visual, position);
        assert_eq!(
            *log.borrow(),
            vec![DownloadsIntent::FilterVisibilityChanged(true)]
        );
        let mut replacement = projection;
        if reorder {
            replacement.profiles.reverse();
        } else {
            // A newly projected stable ID, not either existing Profile.
            replacement.profiles = vec![many_profiles().profiles[19].clone()];
        }
        let target = replacement.profiles[0].id;
        assert_ne!(target, original);
        visual.update(|window, cx| {
            view.update(cx, |view, cx| view.set_projection(replacement, window, cx));
        });
        visual.run_until_parked();
        assert!(visual
            .debug_bounds("downloads-profile-0")
            .unwrap()
            .contains(&position));
        mouse_up_at(visual, position);
        let mut expected = vec![DownloadsIntent::FilterVisibilityChanged(true)];
        assert_eq!(
            *log.borrow(),
            expected,
            "release must not toggle the unpressed Profile"
        );
        click(visual, "downloads-profile-0");
        expected.push(DownloadsIntent::ToggleProfile(target));
        if reorder {
            click(visual, "downloads-profile-1");
            expected.push(DownloadsIntent::ToggleProfile(original));
        }
        assert_eq!(*log.borrow(), expected);
    }
}

#[gpui::test]
fn held_row_press_cannot_capture_reordered_or_replaced_token_profile(cx: &mut TestAppContext) {
    for reorder in [true, false] {
        let mut projection = fixture();
        let other_profile = projection.profiles[0].id;
        let DownloadsList::Completed(groups) = &mut projection.list else {
            unreachable!()
        };
        let original = groups[0].rows[0].clone();
        let mut target = original.clone();
        if reorder {
            // Same opaque token, different Profile: both identity parts matter.
            target.profile = other_profile;
            target.title = "Fictional other Profile report.pdf".into();
            groups[0].rows.push(target.clone());
        } else {
            // Same Profile, different opaque token; never reconstruct a token.
            target.token = 102;
            target.title = "Fictional replacement report.pdf".into();
        }
        let (view, visual, log, _subscription) = open(cx, projection.clone());
        view.update(visual, |view, cx| view.set_row_selection_enabled(true, cx));
        visual.run_until_parked();
        let position = visual.debug_bounds("downloads-row-0-0").unwrap().center();
        mouse_down_at(visual, position);
        assert!(log.borrow().is_empty());
        let mut replacement = projection;
        let DownloadsList::Completed(groups) = &mut replacement.list else {
            unreachable!()
        };
        if reorder {
            groups[0].rows.reverse();
        } else {
            groups[0].rows = vec![target.clone()];
        }
        visual.update(|window, cx| {
            view.update(cx, |view, cx| view.set_projection(replacement, window, cx));
        });
        visual.run_until_parked();
        assert!(visual
            .debug_bounds("downloads-row-0-0")
            .unwrap()
            .contains(&position));
        mouse_up_at(visual, position);
        assert!(
            log.borrow().is_empty(),
            "release must not capture the unpressed token/Profile"
        );
        click(visual, "downloads-row-0-0");
        let mut expected = vec![DownloadsIntent::RowPressed {
            token: target.token,
            profile: target.profile,
        }];
        if reorder {
            click(visual, "downloads-row-0-1");
            expected.push(DownloadsIntent::RowPressed {
                token: original.token,
                profile: original.profile,
            });
        }
        assert_eq!(*log.borrow(), expected);
    }
}

#[gpui::test]
fn row_overflow_emits_the_captured_identity_without_opening_or_selecting(cx: &mut TestAppContext) {
    let projection = fixture();
    let DownloadsList::Completed(groups) = &projection.list else { panic!() };
    let profile = groups[0].rows[0].profile;
    let (view, visual, log, _subscription) = open(cx,projection);
    view.update(visual,|view,cx| view.set_row_selection_enabled(true,cx));
    visual.run_until_parked();
    let row = visual.debug_bounds("downloads-row-0-0").unwrap();
    visual.simulate_mouse_move(row.center(),None,Modifiers::none());
    visual.run_until_parked();
    click(visual,"downloads-overflow-0-0");
    assert_eq!(log.borrow().len(),1);
    assert!(matches!(log.borrow()[0],DownloadsIntent::RowMenuRequested {token:101,profile:owner,..} if owner==profile));
    assert!(view.read_with(visual,|view,_| view.selected_row.is_none()));
}

#[gpui::test]
fn populated_rows_match_reference_offsets_and_reserve_hover_action_width(cx: &mut TestAppContext) {
    let (view,visual,_,_subscription) = open(cx,fixture());
    let search = visual.debug_bounds("downloads-search").unwrap();
    let row = visual.debug_bounds("downloads-row-0-0").unwrap();
    let title = visual.debug_bounds("downloads-title-0-0").unwrap();
    assert_eq!(search.origin,gpui::point(px(120.),px(10.)));
    assert_eq!(row.origin,gpui::point(px(120.),px(82.)));
    assert_eq!(title.origin.x,px(164.));
    assert_eq!(title.size.width,px(217.));
    view.update(visual,|view,cx|view.set_row_selection_enabled(true,cx));
    visual.run_until_parked();
    visual.simulate_mouse_move(row.center(),None,Modifiers::none());visual.run_until_parked();
    assert_eq!(visual.debug_bounds("downloads-row-0-0").unwrap(),row);
    assert_eq!(visual.debug_bounds("downloads-title-0-0").unwrap().size.width,px(181.));
}

#[gpui::test]
fn open_file_menu_retains_row_layout_after_pointer_enters_overlay(cx: &mut TestAppContext) {
    let projection = fixture();
    let DownloadsList::Completed(groups) = &projection.list else { panic!() };
    let profile = groups[0].rows[0].profile;
    let (view,visual,log,_subscription) = open(cx,projection);
    view.update(visual,|view,cx| { view.set_row_selection_enabled(true,cx); view.set_menu_row(Some((101,profile)),cx); });
    visual.run_until_parked();
    visual.simulate_mouse_move(gpui::point(px(700.),px(200.)),None,Modifiers::none());visual.run_until_parked();
    assert_eq!(visual.debug_bounds("downloads-title-0-0").unwrap().size.width,px(181.));
    view.update(visual,|view,cx|view.set_menu_row(None,cx));visual.run_until_parked();
    assert_eq!(visual.debug_bounds("downloads-title-0-0").unwrap().size.width,px(217.));
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn verified_context_actions_do_not_enable_unverified_file_activation(cx: &mut TestAppContext) {
    let (view,visual,log,_subscription) = open(cx,fixture());
    view.update(visual,|view,cx|view.set_row_actions_enabled(true,cx));visual.run_until_parked();
    click(visual,"downloads-row-0-0");
    assert!(log.borrow().is_empty());
    click(visual,"downloads-overflow-0-0");
    assert_eq!(log.borrow().len(),1);
    assert!(matches!(log.borrow()[0],DownloadsIntent::RowMenuRequested {token:101,..}));
}

#[gpui::test]
fn transfer_overflow_keeps_the_token_after_duplicate_names_reorder(cx:&mut TestAppContext) {
    let projection=fixture();let first=projection.profiles[0].id;let second=projection.profiles[1].id;
    let (view,visual,log,_subscription)=open(cx,projection);
    let row=|id,profile|TransferRow{id:TransferId(id),profile,title:"same.bin".into(),subtitle:"1 KB/8 MB".into(),progress:Some(0.2),canceled:false};
    view.update(visual,|view,cx|view.set_transfers(vec![row(7,first),row(8,second)],cx));visual.run_until_parked();
    let bounds=visual.debug_bounds("downloads-transfer-8").unwrap();
    visual.simulate_mouse_move(bounds.center(),None,Modifiers::none());visual.run_until_parked();
    view.update(visual,|view,cx|view.set_transfers(vec![row(8,second),row(7,first)],cx));visual.run_until_parked();
    let title=visual.debug_bounds("downloads-transfer-title-8").unwrap();
    assert_eq!(title.origin.x,px(172.));
    assert_eq!(visual.debug_bounds("downloads-transfer-8").unwrap().origin.y,px(83.));
    click(visual,"downloads-transfer-overflow-8");
    assert_eq!(log.borrow().len(),1);
    assert!(matches!(log.borrow()[0],DownloadsIntent::TransferMenuRequested{id:TransferId(8),profile,..} if profile==second));
}
#[gpui::test]
fn canceled_transfer_retains_its_row_and_cannot_queue_an_open(cx:&mut TestAppContext) {
    let projection=fixture();let profile=projection.profiles[0].id;
    let (view,visual,log,_subscription)=open(cx,projection);
    let mut row=TransferRow{id:TransferId(9),profile,title:"same.bin".into(),subtitle:"1 KB/8 MB".into(),progress:Some(0.2),canceled:false};
    view.update(visual,|view,cx|view.set_transfers(vec![row.clone()],cx));visual.run_until_parked();
    click(visual,"downloads-transfer-9");
    assert!(matches!(log.borrow()[0],DownloadsIntent::TransferOpened{id:TransferId(9),..}));log.borrow_mut().clear();
    row.canceled=true;row.subtitle="Cancelled".into();
    view.update(visual,|view,cx|view.set_transfers(vec![row],cx));visual.run_until_parked();
    click(visual,"downloads-transfer-9");assert!(log.borrow().is_empty());
    click(visual,"downloads-transfer-overflow-9");
    assert!(matches!(log.borrow()[0],DownloadsIntent::TransferMenuRequested{id:TransferId(9),..}));
}
