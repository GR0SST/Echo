//! Geometry and input contract tests on GPUI's real test platform.
use super::*;
use gpui::{Context, Modifiers, Render, TestAppContext, VisualTestContext};
use std::cell::RefCell;

type Log = Rc<RefCell<Vec<LibraryIntent>>>;
struct Harness {
    projection: LibraryProjection,
    log: Log,
    actions: LibraryActions,
    section: LibrarySection,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        let events: LibraryEvents = Rc::new(move |event, _, _| log.borrow_mut().push(event));
        let mut card_index = 0usize;
        div().size_full().child(render_library(
            &self.projection,
            self.section,
            LibraryColors {
                background: gpui::rgb(0xffffff).into(),
                card_surface: |_| gpui::rgb(0xdddddd).into(),
                card_overlay: gpui::transparent_black(),
                text: gpui::rgb(0x111111).into(),
                muted: gpui::rgb(0x666666).into(),
                navigation_icon: gpui::rgb(0x6258ff).into(),
                hover: gpui::rgb(0xeeeeee).into(),
                selected: gpui::rgb(0xcccccc).into(),
            },
            self.actions,
            events,
            |space, events| {
                let index = card_index;
                card_index += 1;
                div()
                    .id(("injected-sidebar", index))
                    .debug_selector({
                        let id = space.name.clone();
                        move || format!("injected-{id}")
                    })
                    .h_full()
                    .min_w_0()
                    .overflow_y_scroll()
                    .children((0..50).map(|row| {
                        let events = events.clone();
                        div()
                            .id(("injected-row", index * 50 + row))
                            .debug_selector(move || format!("injected-row-{index}-{row}"))
                            .h(px(24.))
                            .w_full()
                            // Deliberately do not stop propagation: the card
                            // must not turn a sidebar click into SelectSpace.
                            .on_click(move |_, window, cx| {
                                events(LibraryIntent::Navigate(LibrarySection::Media), window, cx);
                            })
                            .child(format!("Sidebar item {row}"))
                    }))
                    .into_any_element()
            },
            None,
        ))
    }
}
fn fixture() -> Browser {
    let mut browser = Browser::new();
    for i in 1..5 {
        browser.add_space(format!("Space {i}"), i as f32 * 30.);
    }
    browser
}
fn open<'a>(
    cx: &'a mut TestAppContext,
    browser: &Browser,
    actions: LibraryActions,
) -> (gpui::Entity<Harness>, &'a mut VisualTestContext, Log) {
    let log: Log = Rc::default();
    let projection = LibraryProjection::from_browser(browser, browser.spaces[0].id).unwrap();
    let (view, visual) = cx.add_window_view(|_, _| Harness {
        projection,
        log: log.clone(),
        actions,
        section: LibrarySection::Spaces,
    });
    visual.simulate_resize(gpui::size(px(1920.), px(1050.)));
    visual.run_until_parked();
    (view, visual, log)
}
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn five_cards_match_observed_library_geometry(cx: &mut TestAppContext) {
    let browser = fixture();
    let (_, visual, log) = open(cx, &browser, LibraryActions::default());
    let root = visual.debug_bounds("library").unwrap();
    for (i, selector) in [
        "library-space-0",
        "library-space-1",
        "library-space-2",
        "library-space-3",
        "library-space-4",
    ]
    .into_iter()
    .enumerate()
    {
        let card = visual.debug_bounds(selector).unwrap();
        assert_eq!(card.origin.x - root.origin.x, px(136. + i as f32 * 264.));
        assert_eq!(card.origin.y - root.origin.y, px(48.));
        assert_eq!(card.size.width, px(240.));
        assert_eq!(root.bottom() - card.bottom(), px(48.));
    }
    let plus = visual.debug_bounds("library-new-space").unwrap();
    assert_eq!(
        plus.center() - root.origin,
        gpui::point(px(1480.), px(525.))
    );
    assert_eq!(plus.size, gpui::size(px(48.), px(48.)));
    assert!(visual.debug_bounds("injected-Space 1").is_some());
    assert!(
        log.borrow().is_empty(),
        "opening Library must never create a Space"
    );
}

#[gpui::test]
fn clicks_retain_origin_and_inactive_card_profile_identity(cx: &mut TestAppContext) {
    let mut browser = fixture();
    let profile = browser.create_user_profile("Work").unwrap();
    let target = browser.spaces[1].id;
    browser.assign_space_user_profile(target, profile).unwrap();
    let origin = browser.spaces[0].id;
    let (_, visual, log) = open(
        cx,
        &browser,
        LibraryActions {
            profile: true,
            theme: true,
            overflow: true,
            ..Default::default()
        },
    );
    click(visual, "library-space-profile-1");
    click(visual, "library-space-theme-1");
    click(visual, "library-space-overflow-1");
    click(visual, "library-space-select-1");
    click(visual, "library-new-space");
    assert_eq!(
        *log.borrow(),
        vec![
            LibraryIntent::OpenProfile {
                space: target,
                profile
            },
            LibraryIntent::EditTheme(target),
            LibraryIntent::Overflow(target),
            LibraryIntent::SelectSpace(target),
            LibraryIntent::BeginSpaceCreation { origin },
        ]
    );
    assert_eq!(
        browser.spaces.len(),
        5,
        "presentation must not mutate the model"
    );
}

#[gpui::test]
fn unsupported_controls_and_easels_emit_no_actions(cx: &mut TestAppContext) {
    let browser = fixture();
    let (_, visual, log) = open(cx, &browser, LibraryActions::default());
    for selector in [
        "library-space-theme-0",
        "library-space-profile-0",
        "library-space-overflow-0",
        "library-nav-easels",
        "library-nav-media",
    ] {
        click(visual, selector);
    }
    assert!(log.borrow().is_empty());
    click(visual, "library-nav-spaces");
    assert_eq!(
        *log.borrow(),
        vec![LibraryIntent::Navigate(LibrarySection::Spaces)]
    );
}

#[test]
fn projection_keeps_order_and_rejects_non_workspace_windows() {
    let browser = fixture();
    let projection = LibraryProjection::from_browser(&browser, browser.spaces[3].id).unwrap();
    assert_eq!(projection.origin, browser.spaces[3].id);
    assert_eq!(
        projection
            .spaces
            .iter()
            .map(|space| space.id)
            .collect::<Vec<_>>(),
        browser
            .spaces
            .iter()
            .map(|space| space.id)
            .collect::<Vec<_>>()
    );
    let blank = Browser::new_blank();
    assert!(LibraryProjection::from_browser(&blank, blank.spaces[0].id).is_none());
    let private = Browser::new_private();
    assert!(LibraryProjection::from_browser(&private, private.spaces[0].id).is_none());
}

fn wheel(visual: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>, dx: f32, dy: f32) {
    visual.simulate_mouse_move(position, None, Modifiers::none());
    visual.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(dx), px(dy))),
        modifiers: Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    visual.run_until_parked();
}

#[gpui::test]
fn long_names_keep_controls_inside_cards_in_a_small_window(cx: &mut TestAppContext) {
    let mut browser = fixture();
    browser.spaces[0].name = "A very long Space name without a useful early ending ".repeat(8);
    let profile = browser
        .create_user_profile(&"A very long Profile name ".repeat(12))
        .unwrap();
    browser
        .assign_space_user_profile(browser.spaces[0].id, profile)
        .unwrap();
    let (_, visual, log) = open(
        cx,
        &browser,
        LibraryActions {
            profile: true,
            theme: true,
            overflow: true,
            ..Default::default()
        },
    );
    visual.simulate_resize(gpui::size(px(800.), px(480.)));
    visual.run_until_parked();
    let card = visual.debug_bounds("library-space-0").unwrap();
    let header = visual.debug_bounds("library-space-select-0").unwrap();
    let name = visual.debug_bounds("library-space-name-0").unwrap();
    let pill = visual.debug_bounds("library-space-profile-0").unwrap();
    let profile_name = visual.debug_bounds("library-space-profile-name-0").unwrap();
    let theme = visual.debug_bounds("library-space-theme-0").unwrap();
    assert_eq!(card.size, gpui::size(px(240.), px(384.)));
    assert_eq!(header.size.height, px(28.));
    assert!(name.right() <= card.right() - px(12.));
    assert!(name.origin.x >= header.origin.x);
    assert!(pill.size.width <= px(90.));
    assert!(profile_name.right() <= pill.right() - px(6.));
    assert!(pill.right() + px(6.) <= theme.origin.x);
    assert_eq!(theme.size, gpui::size(px(20.), px(20.)));
    assert_eq!(theme.right(), card.right() - px(16.));
    click(visual, "library-space-theme-0");
    assert_eq!(
        *log.borrow(),
        vec![LibraryIntent::EditTheme(browser.spaces[0].id)]
    );
}

#[gpui::test]
fn small_window_scroll_reaches_creation_and_sidebar_body_without_selecting_card(
    cx: &mut TestAppContext,
) {
    let browser = fixture();
    let (_, visual, log) = open(cx, &browser, LibraryActions::default());
    visual.simulate_resize(gpui::size(px(800.), px(480.)));
    visual.run_until_parked();
    let body = visual.debug_bounds("library-space-body-0").unwrap();
    let header = visual.debug_bounds("library-space-select-0").unwrap();
    let footer = visual.debug_bounds("library-space-overflow-0").unwrap();
    let last_before = visual.debug_bounds("injected-row-0-49").unwrap();
    assert!(last_before.origin.y > body.bottom());
    assert!(body.origin.y >= header.bottom());
    assert!(body.bottom() <= footer.origin.y);
    click(visual, "injected-row-0-0");
    assert_eq!(
        *log.borrow(),
        vec![LibraryIntent::Navigate(LibrarySection::Media)]
    );
    log.borrow_mut().clear();
    let card_before_body_scroll = visual.debug_bounds("library-space-0").unwrap();
    let plus_before_body_scroll = visual.debug_bounds("library-new-space").unwrap();
    // Only a vertical body gesture occurs between these samples. GPUI's
    // default axis fallback must not also move the outer horizontal strip.
    wheel(visual, body.center(), 0., -2000.);
    assert_eq!(
        visual.debug_bounds("library-space-0").unwrap(),
        card_before_body_scroll
    );
    assert_eq!(
        visual.debug_bounds("library-new-space").unwrap(),
        plus_before_body_scroll
    );
    let last_after = visual.debug_bounds("injected-row-0-49").unwrap();
    assert!(last_after.origin.y >= body.origin.y);
    assert!(last_after.bottom() <= body.bottom());
    assert_eq!(
        visual.debug_bounds("library-space-select-0").unwrap(),
        header
    );
    assert_eq!(
        visual.debug_bounds("library-space-overflow-0").unwrap(),
        footer
    );
    click(visual, "injected-row-0-49");
    assert_eq!(
        *log.borrow(),
        vec![LibraryIntent::Navigate(LibrarySection::Media)]
    );
    log.borrow_mut().clear();
    let root = visual.debug_bounds("library").unwrap();
    let viewport = visual.debug_bounds("library-spaces-scroll").unwrap();
    assert!(visual.debug_bounds("library-new-space").unwrap().origin.x > root.right());
    wheel(
        visual,
        gpui::point(viewport.right() - px(8.), viewport.bottom() - px(8.)),
        -3000.,
        0.,
    );
    let plus = visual.debug_bounds("library-new-space").unwrap();
    // The explicit horizontal gesture should move cards and their chrome
    // together, independently of the earlier vertical-body assertions.
    let card_after_horizontal = visual.debug_bounds("library-space-0").unwrap();
    let header_after_horizontal = visual.debug_bounds("library-space-select-0").unwrap();
    let footer_after_horizontal = visual.debug_bounds("library-space-overflow-0").unwrap();
    assert!(card_after_horizontal.origin.x < card_before_body_scroll.origin.x);
    assert_eq!(
        header_after_horizontal.origin - card_after_horizontal.origin,
        header.origin - card_before_body_scroll.origin
    );
    assert_eq!(
        footer_after_horizontal.origin - card_after_horizontal.origin,
        footer.origin - card_before_body_scroll.origin
    );
    assert!(plus.origin.x >= viewport.origin.x);
    assert!(plus.right() <= viewport.right());
    click(visual, "library-new-space");
    assert_eq!(
        *log.borrow(),
        vec![LibraryIntent::BeginSpaceCreation {
            origin: browser.spaces[0].id
        }]
    );
}

fn assert_reference_navigation(visual: &mut VisualTestContext, first_y: f32) {
    let root = visual.debug_bounds("library").unwrap();
    let group = visual.debug_bounds("library-nav-group").unwrap();
    assert_eq!(group.origin - root.origin, gpui::point(px(8.), px(first_y)));
    assert_eq!(group.size, gpui::size(px(96.), px(580.)));
    for (index, (tile, icon, label)) in [
        (
            "library-nav-media",
            "library-nav-media-icon",
            "library-nav-media-label",
        ),
        (
            "library-nav-downloads",
            "library-nav-downloads-icon",
            "library-nav-downloads-label",
        ),
        (
            "library-nav-easels",
            "library-nav-easels-icon",
            "library-nav-easels-label",
        ),
        (
            "library-nav-spaces",
            "library-nav-spaces-icon",
            "library-nav-spaces-label",
        ),
        (
            "library-nav-boosts",
            "library-nav-boosts-icon",
            "library-nav-boosts-label",
        ),
        (
            "library-nav-archive",
            "library-nav-archive-icon",
            "library-nav-archive-label",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let tile = visual.debug_bounds(tile).unwrap();
        let icon = visual.debug_bounds(icon).unwrap();
        let label = visual.debug_bounds(label).unwrap();
        assert_eq!(tile.size, gpui::size(px(96.), px(80.)));
        assert_eq!(
            tile.origin - root.origin,
            gpui::point(px(8.), px(first_y + index as f32 * 100.))
        );
        assert_eq!(icon.size, gpui::size(px(24.), px(24.)));
        assert_eq!(icon.origin - tile.origin, gpui::point(px(36.), px(16.)));
        assert_eq!(label.origin.y - tile.origin.y, px(50.));
        assert_eq!(label.size.height, px(12.));
        assert_eq!(label.center().x, tile.center().x);
    }
}

#[gpui::test]
fn navigation_matches_captured_icon_tiles_at_reference_and_ordinary_heights(
    cx: &mut TestAppContext,
) {
    let (view, visual, log) = open(
        cx,
        &fixture(),
        LibraryActions {
            downloads: true,
            ..Default::default()
        },
    );
    view.update(visual, |view, cx| {
        view.section = LibrarySection::Downloads;
        cx.notify();
    });
    for (height, first_y) in [(2130., 770.), (800., 105.), (760., 85.)] {
        visual.simulate_resize(gpui::size(px(1920.), px(height)));
        visual.run_until_parked();
        assert_reference_navigation(visual, first_y);
        let selected = visual.debug_bounds("library-nav-selected-fill").unwrap();
        assert_eq!(
            selected,
            visual.debug_bounds("library-nav-downloads").unwrap()
        );
    }
    // The section is clickable through its icon/label child, not a text-only row.
    click(visual, "library-nav-downloads-icon");
    click(visual, "library-nav-downloads-label");
    assert_eq!(
        *log.borrow(),
        vec![
            LibraryIntent::Navigate(LibrarySection::Downloads),
            LibraryIntent::Navigate(LibrarySection::Downloads)
        ]
    );
}

#[gpui::test]
fn short_navigation_keeps_tile_geometry_and_scrolls_to_last_route(cx: &mut TestAppContext) {
    let (_, visual, log) = open(
        cx,
        &fixture(),
        LibraryActions {
            archive: true,
            ..Default::default()
        },
    );
    visual.simulate_resize(gpui::size(px(800.), px(480.)));
    visual.run_until_parked();
    assert_reference_navigation(visual, 0.);
    let viewport = visual.debug_bounds("library-nav-scroll").unwrap();
    assert_eq!(viewport.size, gpui::size(px(112.), px(470.)));
    let card = visual.debug_bounds("library-space-0").unwrap();
    let last_before = visual.debug_bounds("library-nav-archive").unwrap();
    assert!(last_before.origin.y >= viewport.bottom());
    wheel(visual, viewport.center(), 0., -1000.);
    let last = visual.debug_bounds("library-nav-archive").unwrap();
    assert_eq!(last.size, gpui::size(px(96.), px(80.)));
    assert!(last.origin.y >= viewport.origin.y);
    assert!(last.bottom() <= viewport.bottom());
    assert_eq!(visual.debug_bounds("library-space-0").unwrap(), card);
    click(visual, "library-nav-archive-label");
    assert_eq!(
        *log.borrow(),
        vec![LibraryIntent::Navigate(LibrarySection::ArchivedTabs)]
    );
}

#[gpui::test]
fn hovering_another_rail_tile_preserves_selection_until_activation(cx: &mut TestAppContext) {
    let (view, visual, log) = open(cx, &fixture(), LibraryActions {
        downloads: true, media: true, ..Default::default()
    });
    view.update(visual, |view, cx| { view.section = LibrarySection::Downloads; cx.notify(); });
    visual.run_until_parked();
    let selected = visual.debug_bounds("library-nav-downloads").unwrap();
    let media = visual.debug_bounds("library-nav-media").unwrap();
    visual.simulate_mouse_move(media.center(), None, Modifiers::none());
    visual.run_until_parked();
    assert_eq!(visual.debug_bounds("library-nav-selected-fill"), Some(selected));
    assert!(log.borrow().is_empty(), "hover must not activate another section");
    click(visual, "library-nav-media-icon");
    assert_eq!(*log.borrow(), vec![LibraryIntent::Navigate(LibrarySection::Media)]);
}
