//! Tests use only the empty presentation; no files, permissions or enumeration.
use super::*;
use crate::library::{self, LibraryActions, LibraryColors, LibraryProjection, LibrarySection};
use gpui::{AnyElement, Modifiers, TestAppContext, VisualTestContext};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Log = Rc<RefCell<Vec<MediaIntent>>>;
struct Host {
    projection: LibraryProjection,
    media: Entity<MediaView>,
    section: LibrarySection,
    focus: gpui::FocusHandle,
    media_paints: Rc<Cell<usize>>,
}
fn colors() -> MediaColors {
    MediaColors {
        column: gpui::rgb(0x333333).into(),
        preview: gpui::rgb(0x444444).into(),
        text: gpui::rgb(0xeeeeee).into(),
        muted: gpui::rgb(0xaaaaaa).into(),
        control: gpui::rgb(0x555555).into(),
        hover: gpui::rgb(0x666666).into(),
    }
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let colors = colors();
        let media = (self.section == LibrarySection::Media).then(|| {
            let paints = self.media_paints.clone();
            div()
                .relative()
                .size_full()
                .child(self.media.clone())
                .child(
                    gpui::canvas(
                        |_, _, _| (),
                        move |_, _, _, _| {
                            paints.set(paints.get() + 1);
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
                .into_any_element()
        });
        let content = library::render_library(
            &self.projection,
            self.section,
            LibraryColors {
                background: colors.column,
                card_surface: |_| gpui::rgb(0xcccccc).into(),
                card_overlay: gpui::transparent_black(),
                text: colors.text,
                muted: colors.muted,
                navigation_icon: gpui::rgb(0x6258ff).into(),
                hover: colors.hover,
                selected: colors.control,
            },
            LibraryActions {
                media: true,
                ..Default::default()
            },
            Rc::new(|_, _, _| {}),
            |_, _| div().into_any_element(),
            media,
        );
        div().size_full().track_focus(&self.focus).child(content)
    }
}
fn open(
    cx: &mut TestAppContext,
) -> (
    Entity<Host>,
    Entity<MediaView>,
    &mut VisualTestContext,
    Log,
    Subscription,
) {
    cx.update(crate::text_input::bind_keys);
    let browser = browser_core::Browser::new();
    let projection = LibraryProjection::from_browser(&browser, browser.spaces[0].id).unwrap();
    let (host, visual) = cx.add_window_view(|_, cx| Host {
        projection,
        section: LibrarySection::Media,
        media: cx.new(|cx| MediaView::new(colors(), cx)),
        focus: cx.focus_handle(),
        media_paints: Rc::default(),
    });
    let media = host.read_with(visual, |host, _| host.media.clone());
    let log: Log = Rc::default();
    let captured = log.clone();
    let subscription = visual.update(|_, cx| {
        cx.subscribe(&media, move |_, event: &MediaIntent, _| {
            if !matches!(event, MediaIntent::SearchFocused) { captured.borrow_mut().push(event.clone()); }
        })
    });
    visual.simulate_resize(gpui::size(px(707.), px(2000.)));
    visual.run_until_parked();
    (host, media, visual, log, subscription)
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
fn media_has_reference_width_list_after_shared_rail_and_separate_preview(cx: &mut TestAppContext) {
    let (_, _, visual, log, _subscription) = open(cx);
    let list = visual.debug_bounds("library-media-column").unwrap();
    let search = visual.debug_bounds("library-media-search").unwrap();
    let preview = visual.debug_bounds("library-media-preview").unwrap();
    let icon = visual.debug_bounds("library-media-empty-icon").unwrap();
    let title = visual.debug_bounds("library-media-empty-title").unwrap();
    let body = visual.debug_bounds("library-media-empty-body").unwrap();
    let learn_more = visual.debug_bounds("library-media-learn-more").unwrap();
    assert_eq!(list.origin.x, px(112.));
    assert_eq!(list.size.width, px(305.));
    assert_eq!(preview.origin.x, list.right());
    assert_eq!(preview.origin.y, px(10.));
    assert_eq!(preview.right(), px(697.));
    assert_eq!(search.origin.x - list.origin.x, px(8.));
    assert_eq!(search.origin.y, px(10.));
    assert_eq!(search.size, gpui::size(px(289.), px(36.)));
    assert_eq!(icon.size, gpui::size(px(56.), px(56.)));
    assert_eq!(title.origin.y - icon.bottom(), px(12.));
    assert_eq!(body.origin.y - title.bottom(), px(6.));
    assert_eq!(body.size, gpui::size(px(272.), px(48.)));
    assert_eq!(learn_more.origin.y - body.bottom(), px(20.));
    assert_eq!(learn_more.size, gpui::size(px(100.), px(28.)));
    let list_body = visual.debug_bounds("library-media-list-body").unwrap();
    assert!(((icon.origin.y + learn_more.bottom()) / 2. - list_body.center().y).abs() <= px(1.));
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn empty_stays_centered_in_list_at_ordinary_window_sizes(cx: &mut TestAppContext) {
    let (_, _, visual, _, _subscription) = open(cx);
    for viewport in [
        gpui::size(px(1280.), px(800.)),
        gpui::size(px(640.), px(480.)),
    ] {
        visual.simulate_resize(viewport);
        visual.run_until_parked();
        let body = visual.debug_bounds("library-media-list-body").unwrap();
        let empty = visual.debug_bounds("library-media-empty").unwrap();
        let icon = visual.debug_bounds("library-media-empty-icon").unwrap();
        let preview = visual.debug_bounds("library-media-preview").unwrap();
        assert_eq!(body.size.width, px(305.));
        assert!((empty.center().y - body.center().y).abs() <= px(1.));
        assert_eq!(icon.center().x, body.center().x);
        assert!(empty.right() <= preview.origin.x);
        assert_eq!(preview.origin.x, body.right());
    }
}

#[gpui::test]
fn real_query_survives_route_and_semantic_color_updates_without_global_appearance_change(
    cx: &mut TestAppContext,
) {
    let (host, media, visual, log, _subscription) = open(cx);
    click(visual, "library-media-search");
    visual.simulate_input("fictional media query");
    visual.run_until_parked();
    assert_eq!(
        media.read_with(visual, |view, cx| view.query(cx)),
        "fictional media query"
    );
    assert!(log
        .borrow()
        .contains(&MediaIntent::SearchChanged("fictional media query".into())));
    let paints = host.read_with(visual, |host, _| host.media_paints.clone());
    let painted_media = paints.get();
    assert!(
        painted_media > 0,
        "Media must actually paint before leaving it"
    );
    host.update(visual, |host, cx| {
        host.section = LibrarySection::Spaces;
        cx.notify();
    });
    visual.run_until_parked();
    let card = visual.debug_bounds("library-space-0").unwrap();
    assert_eq!(card.origin.x, px(136.));
    assert_eq!(card.size.width, px(240.));
    // GPUI can retain debug bounds from an earlier frame. Verify the mounted
    // paint path instead of treating a stale selector as visible Media.
    assert_eq!(paints.get(), painted_media, "Spaces must not paint Media");
    let mut inherited = colors();
    inherited.column = gpui::rgb(0xfafafa).into();
    media.update(visual, |view, cx| view.set_colors(inherited, cx));
    host.update(visual, |_, cx| cx.notify());
    visual.run_until_parked();
    assert_eq!(
        paints.get(),
        painted_media,
        "invalidating the retained Media entity must not paint it on Spaces"
    );
    host.update(visual, |host, cx| {
        host.section = LibrarySection::Media;
        cx.notify();
    });
    visual.run_until_parked();
    assert_eq!(
        media.read_with(visual, |view, cx| view.query(cx)),
        "fictional media query"
    );
    assert!(
        paints.get() > painted_media,
        "returning to Media must paint it again"
    );
    assert_eq!(
        media.read_with(visual, |view, _| view.colors.column),
        inherited.column
    );
    assert!(
        visual.debug_bounds("library-media-empty").is_some(),
        "query input must not invent media results"
    );
}

#[gpui::test]
fn learn_more_is_only_an_intent_without_fabricated_target_or_help_copy(cx: &mut TestAppContext) {
    let (_, _, visual, log, _subscription) = open(cx);
    click(visual, "library-media-learn-more");
    assert_eq!(*log.borrow(), vec![MediaIntent::LearnMoreRequested]);
    assert!(visual.debug_bounds("library-media-preview").is_some());
}

#[gpui::test]
fn all_library_sections_keep_the_same_rail_and_tile_geometry(cx: &mut TestAppContext) {
    let (host, _, visual, _, _subscription) = open(cx);
    visual.simulate_resize(gpui::size(px(1280.), px(800.)));
    visual.run_until_parked();
    let rail = visual.debug_bounds("library-nav-scroll").unwrap();
    let group = visual.debug_bounds("library-nav-group").unwrap();
    assert_eq!(rail.size.width, px(112.));
    assert_eq!(group.size, gpui::size(px(96.), px(580.)));
    assert_eq!(group.origin, gpui::point(px(8.), px(105.)));
    let selectors = [
        "library-nav-media",
        "library-nav-downloads",
        "library-nav-easels",
        "library-nav-spaces",
        "library-nav-boosts",
        "library-nav-archive",
    ];
    let tiles = selectors.map(|selector| visual.debug_bounds(selector).unwrap());
    let media_icon = visual.debug_bounds("library-nav-media-icon").unwrap();
    for section in LibrarySection::ALL {
        host.update(visual, |host, cx| {
            host.section = section;
            cx.notify();
        });
        visual.run_until_parked();
        assert_eq!(visual.debug_bounds("library-nav-scroll").unwrap(), rail);
        assert_eq!(visual.debug_bounds("library-nav-group").unwrap(), group);
        assert_eq!(
            visual.debug_bounds("library-nav-media-icon").unwrap(),
            media_icon
        );
        for (selector, expected) in selectors.into_iter().zip(tiles) {
            assert_eq!(visual.debug_bounds(selector).unwrap(), expected);
        }
    }
}

fn click_at(visual: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    visual.simulate_mouse_move(position, None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_click(position, Modifiers::none());
    visual.run_until_parked();
}

#[gpui::test]
fn search_icon_padding_and_vertical_gutters_focus_input_inside_focusable_host(
    cx: &mut TestAppContext,
) {
    let (host, media, visual, log, _subscription) = open(cx);
    let input = media.read_with(visual, |view, _| view.input.clone());
    let parent_focus = host.read_with(visual, |host, _| host.focus.clone());
    let field = visual.debug_bounds("library-media-search").unwrap();
    for position in [
        gpui::point(field.origin.x + px(2.), field.center().y),
        gpui::point(field.right() - px(2.), field.center().y),
        gpui::point(field.center().x, field.origin.y + px(2.)),
        gpui::point(field.center().x, field.bottom() - px(2.)),
        gpui::point(field.origin.x + px(19.), field.center().y),
    ] {
        input.update(visual, |input, cx| input.set_text("", cx));
        visual.update(|window, _| parent_focus.focus(window));
        visual.run_until_parked();
        log.borrow_mut().clear();
        click_at(visual, position);
        assert!(
            visual.update(|window, cx| input.read(cx).focus_handle(cx).is_focused(window)),
            "the complete visible field must focus its input, including {position:?}"
        );
        visual.simulate_input("x");
        visual.run_until_parked();
        assert_eq!(media.read_with(visual, |view, cx| view.query(cx)), "x");
        assert_eq!(*log.borrow(), vec![MediaIntent::SearchChanged("x".into())]);
    }
}

#[gpui::test]
fn inner_search_clicks_keep_text_input_caret_placement(cx: &mut TestAppContext) {
    let (host, media, visual, _, _subscription) = open(cx);
    let input = media.read_with(visual, |view, _| view.input.clone());
    input.update(visual, |input, cx| input.set_text("abcd", cx));
    let parent_focus = host.read_with(visual, |host, _| host.focus.clone());
    visual.update(|window, _| parent_focus.focus(window));
    visual.run_until_parked();
    let inner = visual.debug_bounds("library-media-search-input").unwrap();
    click_at(
        visual,
        gpui::point(inner.origin.x + px(1.), inner.center().y),
    );
    visual.simulate_input("X");
    visual.run_until_parked();
    assert_eq!(media.read_with(visual, |view, cx| view.query(cx)), "Xabcd");
    // Already focused: the container must not reset the caret chosen by the
    // actual editor click at the far end of its text area.
    click_at(
        visual,
        gpui::point(inner.right() - px(1.), inner.center().y),
    );
    visual.simulate_input("Y");
    visual.run_until_parked();
    assert_eq!(media.read_with(visual, |view, cx| view.query(cx)), "XabcdY");
}

#[gpui::test]
fn media_search_replaces_idle_help_and_clearing_restores_it(cx: &mut TestAppContext) {
    let (_, _, visual, _, _subscription) = open(cx);
    let idle = visual.debug_bounds("library-media-empty").unwrap();
    assert!(visual.debug_bounds("library-media-no-results").is_none());
    click(visual, "library-media-search");
    visual.simulate_input("Echo Media Round Two 31724a8b.png");
    visual.run_until_parked();
    let message = visual.debug_bounds("library-media-no-results").unwrap();
    let group = visual.debug_bounds("library-media-empty").unwrap();
    let icon = visual.debug_bounds("library-media-empty-icon").unwrap();
    assert_eq!(message.size.width, px(272.));
    assert_eq!(message.top() - icon.bottom(), px(12.));
    assert!(group.size.height < idle.size.height, "idle help must leave layout during search");
    assert!((group.center().y - idle.center().y).abs() <= px(1.));
    visual.simulate_keystrokes("cmd-a backspace");
    visual.run_until_parked();
    assert_eq!(visual.debug_bounds("library-media-empty").unwrap(), idle);
    click(visual, "library-media-learn-more");
}
