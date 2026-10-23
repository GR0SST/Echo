//! Empty Favorites drops use GPUI's real drag dispatch, without native input.
use std::{cell::RefCell, rc::Rc, time::SystemTime};

use browser_core::{Browser, TabId};
use engine::{Bounds, Event, ViewId};
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

use crate::{pages::WebEngine, window::BrowserWindow};

#[derive(Default)]
struct EngineState {
    next: u64,
    live: Vec<ViewId>,
    focused: Option<ViewId>,
    document_changes: usize,
}

#[derive(Clone, Default)]
struct RecordingEngine(Rc<RefCell<EngineState>>);

impl WebEngine for RecordingEngine {
    fn create(&mut self, _: &str, _: Bounds) -> Option<ViewId> {
        let mut state = self.0.borrow_mut();
        state.next += 1;
        let view = ViewId::from_raw(state.next);
        state.live.push(view);
        state.document_changes += 1;
        Some(view)
    }
    fn set_bounds(&mut self, _: ViewId, _: Bounds) {}
    fn set_top_corners_rounded(&mut self, _: ViewId, _: bool) {}
    fn set_visible(&mut self, _: ViewId, _: bool) {}
    fn focus_page(&mut self, view: ViewId) {
        self.0.borrow_mut().focused = Some(view);
    }
    fn focus_ui(&mut self) {
        self.0.borrow_mut().focused = None;
    }
    fn navigate(&mut self, _: ViewId, _: &str) {
        self.0.borrow_mut().document_changes += 1;
    }
    fn go_back(&mut self, _: ViewId) {}
    fn go_forward(&mut self, _: ViewId) {}
    fn undo(&mut self, _: ViewId) {}
    fn redo(&mut self, _: ViewId) {}
    fn reload(&mut self, _: ViewId) {
        self.0.borrow_mut().document_changes += 1;
    }
    fn force_reload(&mut self, _: ViewId) {
        self.0.borrow_mut().document_changes += 1;
    }
    fn stop_loading(&mut self, _: ViewId) {}
    fn set_audio_muted(&mut self, _: ViewId, _: bool) {}
    fn audio_muted(&self, _: ViewId) -> Option<bool> {
        Some(false)
    }
    fn zoom(&mut self, _: ViewId, _: i32) {}
    fn find(&mut self, _: ViewId, _: &str, _: bool, _: bool) {}
    fn stop_finding(&mut self, _: ViewId) {}
    fn close(&mut self, view: ViewId) {
        let mut state = self.0.borrow_mut();
        state.live.retain(|id| *id != view);
        state.document_changes += 1;
    }
    fn pump_fn(&self) -> fn() {
        || {}
    }
    fn take_events(&mut self) -> Vec<Event> {
        Vec::new()
    }
}

impl RecordingEngine {
    fn snapshot(&self) -> (Vec<ViewId>, Option<ViewId>, usize) {
        let state = self.0.borrow();
        (state.live.clone(), state.focused, state.document_changes)
    }
}

fn open(
    cx: &mut TestAppContext,
    browser: Browser,
) -> (
    Entity<BrowserWindow>,
    &mut VisualTestContext,
    RecordingEngine,
) {
    cx.update(crate::init);
    let engine = RecordingEngine::default();
    let (view, visual) = cx.add_window_view(|window, cx| {
        let mut view = BrowserWindow::new(browser, Some(Box::new(engine.clone())), window, cx);
        view.save_session_to(
            std::env::temp_dir().join(format!(
                "echo-empty-favorites-{}-{:?}.json",
                std::process::id(),
                cx.entity_id()
            )),
            cx,
        );
        view.focus(window);
        view
    });
    visual.run_until_parked();
    (view, visual, engine)
}

fn selector(prefix: &str, id: TabId) -> &'static str {
    Box::leak(format!("{prefix}-{}", id.get()).into_boxed_str())
}

fn begin_drag(cx: &mut VisualTestContext, source: &'static str) {
    cx.run_until_parked();
    let start = cx
        .debug_bounds(source)
        .expect("drag source is visible")
        .center();
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        start + gpui::point(gpui::px(5.), gpui::px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
}

fn finish_drag(cx: &mut VisualTestContext, destination: &'static str) {
    // Resolve after starting: a dismissed card is restored for a tab drag.
    let end = cx
        .debug_bounds(destination)
        .expect("drop target is visible")
        .center();
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn move_first_favorite(cx: &mut TestAppContext, pinned: bool, dismissed: bool) {
    let now = SystemTime::now();
    let mut browser = Browser::new();
    browser.active_user_profile_mut().empty_favorites_dismissed = dismissed;
    let tab = browser.new_tab("https://original.example/path?keep=1", now);
    if pinned {
        browser.toggle_pin(tab);
        // Favoriting a Pin must keep its original pinned URL after navigation.
        browser.navigate(tab, "https://current.example/changed");
    }
    let original = browser.active_tab().unwrap().clone();
    let (view, visual, engine) = open(cx, browser);
    let pages = engine.snapshot();
    assert_eq!(visual.debug_bounds("empty-favorites").is_none(), dismissed);
    let header = visual.debug_bounds("space-header-0").unwrap();
    begin_drag(visual, selector("tab", tab));
    let dock = visual.debug_bounds("favorites").unwrap();
    assert_eq!(dock.size.height, gpui::px(95.));
    assert!(dock.bottom() <= visual.debug_bounds("space-header-0").unwrap().top());
    finish_drag(visual, "favorites");
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_user_profile().favorites.len(), 1);
        let favorite = &view.browser.active_user_profile().favorites[0];
        assert_eq!(favorite.id, tab);
        assert_eq!(favorite.url, original.url);
        assert_eq!(
            favorite.pinned_url.as_deref(),
            Some("https://original.example/path?keep=1")
        );
        assert_eq!(view.browser.space().active_tab, Some(tab));
        assert!(view.browser.space().today.is_empty());
        assert!(view.browser.space().pinned.is_empty());
        assert!(view.command_bar.is_none());
    });
    assert_eq!(
        engine.snapshot(),
        pages,
        "retain the same live document and focus"
    );
    assert_eq!(
        visual
            .debug_bounds(selector("favorite", tab))
            .unwrap()
            .size
            .height,
        gpui::px(46.)
    );
    assert_eq!(
        visual.debug_bounds("space-header-0").unwrap().top(),
        header.top() + if dismissed { gpui::px(54.) } else { gpui::px(-49.) }
    );
}

#[gpui::test]
fn today_drag_creates_the_first_favorite_without_reloading(cx: &mut TestAppContext) {
    move_first_favorite(cx, false, false);
}

#[gpui::test]
fn pinned_drag_creates_the_first_favorite_with_its_original_url(cx: &mut TestAppContext) {
    move_first_favorite(cx, true, false);
}

fn move_first_split(cx: &mut TestAppContext, pinned: bool) {
    let now = SystemTime::now();
    let mut browser = Browser::new();
    let left = browser.new_tab("https://left.example", now);
    let right = browser.new_tab("https://right.example", now);
    browser
        .add_split(left, right, browser_core::splits::Placement::Right, now)
        .unwrap();
    if pinned {
        browser.toggle_pin(left);
    }
    browser.activate_tab(right, now);
    let split = browser.active_split().unwrap().clone();
    let (view, visual, engine) = open(cx, browser);
    let documents = engine.snapshot();
    assert_eq!(documents.0.len(), 2);
    begin_drag(visual, selector("tab", left));
    finish_drag(visual, "favorites");
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.favorite_count(), 1);
        assert_eq!(
            view.browser.active_user_profile().favorites
                .iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            vec![left, right]
        );
        assert!(view.browser.space().today.is_empty());
        assert!(view.browser.space().pinned.is_empty());
        assert!(view.browser.space().splits.is_empty());
        assert_eq!(view.browser.active_user_profile().favorite_splits.len(), 1);
        let moved = view.browser.active_split().unwrap();
        assert_eq!(moved.id, split.id);
        assert_eq!(moved.anchor, split.anchor);
        assert_eq!(moved.panes, split.panes);
        assert_eq!(moved.active, right);
        assert_eq!(moved.axis, split.axis);
        assert_eq!(moved.weights, split.weights);
        assert_eq!(view.browser.space().active_tab, Some(right));
        assert_eq!(
            view.pages
                .as_ref()
                .unwrap()
                .borrow()
                .surface_layout()
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            split.panes
        );
        let restored = browser_core::session::from_json(&view.session_json()).unwrap();
        assert_eq!(restored.active_split().unwrap().id, split.id);
        assert_eq!(restored.space().active_tab, Some(right));
    });
    assert!(visual.debug_bounds(selector("favorite", left)).is_some());
    assert!(visual.debug_bounds(selector("favorite", right)).is_none());
    assert_eq!(
        engine.snapshot(),
        documents,
        "move both live views atomically and preserve pane focus"
    );
}

#[gpui::test]
fn today_split_becomes_one_first_favorite_with_both_live_panes(cx: &mut TestAppContext) {
    move_first_split(cx, false);
}

#[gpui::test]
fn pinned_split_becomes_one_first_favorite_with_both_live_panes(cx: &mut TestAppContext) {
    move_first_split(cx, true);
}

#[gpui::test]
fn cancelling_outside_the_empty_card_preserves_it_without_loading_the_source(
    cx: &mut TestAppContext,
) {
    let now = SystemTime::now();
    let mut browser = Browser::new();
    let inactive = browser.new_tab("https://inactive.example", now);
    browser.new_tab("https://active.example", now);
    let (view, visual, engine) = open(cx, browser);
    let before = view.read_with(visual, |view, _| view.session_json());
    let documents = engine.snapshot();
    let header = visual.debug_bounds("space-header-0").unwrap();
    begin_drag(visual, selector("tab", inactive));
    let dock = visual.debug_bounds("favorites").unwrap().center();
    visual.simulate_mouse_move(dock, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    finish_drag(visual, "address");
    // GPUI keeps historical debug bounds after an element disappears.
    visual.update(|_, cx| assert!(!cx.has_active_drag()));
    assert!(!view.read_with(visual, |view, _| view.tab_drag_active));
    assert_eq!(visual.debug_bounds("space-header-0").unwrap(), header);
    assert_eq!(
        view.read_with(visual, |view, _| view.session_json()),
        before
    );
    assert_eq!(engine.snapshot(), documents);
}

#[gpui::test]
fn a_full_dock_rejects_a_new_item_but_still_reorders_existing_favorites(cx: &mut TestAppContext) {
    let now = SystemTime::now();
    let mut browser = Browser::new();
    let mut favorites = Vec::new();
    for index in 0..browser_core::favorites::MAX_FAVORITES {
        let tab = browser.new_tab(format!("https://favorite{index}.example"), now);
        assert!(browser.favorite_tab(tab));
        favorites.push(tab);
    }
    let today = browser.new_tab("https://today.example", now);
    let (view, visual, engine) = open(cx, browser);
    let before = view.read_with(visual, |view, _| view.session_json());
    let documents = engine.snapshot();
    begin_drag(visual, selector("tab", today));
    finish_drag(visual, "favorites");
    assert_eq!(
        view.read_with(visual, |view, _| view.session_json()),
        before
    );
    assert_eq!(engine.snapshot(), documents);

    begin_drag(visual, selector("favorite", favorites[0]));
    finish_drag(visual, selector("favorite", favorites[1]));
    favorites.swap(0, 1);
    view.read_with(visual, |view, _| {
        assert_eq!(
            view.browser.favorite_count(),
            browser_core::favorites::MAX_FAVORITES
        );
        assert_eq!(
            view.browser.active_user_profile().favorites
                .iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            favorites
        );
        assert_eq!(view.browser.space().active_tab, Some(today));
        assert_eq!(view.browser.space().today[0].id, today);
    });
    assert_eq!(engine.snapshot(), documents);
}

fn unrelated_drag_keeps_the_empty_card_unchanged(cx: &mut TestAppContext, folder_drag: bool, dismissed: bool) {
    // Test both the visible onboarding card and the persisted collapsed state.
    let mut browser = Browser::new();
    browser.new_tab("https://active.example", SystemTime::now());
    browser.active_user_profile_mut().empty_favorites_dismissed = dismissed;
    let folder = browser.create_folder(browser.space().id, None, "Projects").unwrap();
    let (view, visual, engine) = open(cx, browser);
    let header = visual.debug_bounds("space-header-0").unwrap();
    let documents = engine.snapshot();
    let source = if folder_drag {
        Box::leak(format!("folder-{}", folder.get()).into_boxed_str())
    } else {
        "space-0"
    };
    begin_drag(visual, source);
    visual.update(|_, cx| assert!(cx.has_active_drag()));
    assert!(!view.read_with(visual, |view, _| view.tab_drag_active));
    assert_eq!(visual.debug_bounds("empty-favorites").is_none(), dismissed);
    assert_eq!(visual.debug_bounds("space-header-0").unwrap(), header);
    finish_drag(visual, "address");
    assert_eq!(engine.snapshot(), documents);
}

#[gpui::test]
fn folder_drag_does_not_create_an_empty_favorites_target_or_shift_the_sidebar(cx: &mut TestAppContext) {
    unrelated_drag_keeps_the_empty_card_unchanged(cx, true, false);
    unrelated_drag_keeps_the_empty_card_unchanged(cx, true, true);
}

#[gpui::test]
fn space_drag_does_not_create_an_empty_favorites_target_or_shift_the_sidebar(cx: &mut TestAppContext) {
    unrelated_drag_keeps_the_empty_card_unchanged(cx, false, false);
    unrelated_drag_keeps_the_empty_card_unchanged(cx, false, true);
}

#[gpui::test]
fn dismissing_the_card_collapses_its_full_height_and_survives_session_restore(cx: &mut TestAppContext) {
    let mut browser = Browser::new();
    browser.new_tab("https://active.example", SystemTime::now());
    let (view, visual, engine) = open(cx, browser);
    let header = visual.debug_bounds("space-header-0").unwrap();
    let documents = engine.snapshot();
    let close = visual.debug_bounds("dismiss-empty-favorites").unwrap().center();
    visual.simulate_mouse_move(close, None, Modifiers::none());
    visual.simulate_click(close, Modifiers::none());
    visual.run_until_parked();
    assert_eq!(visual.debug_bounds("space-header-0").unwrap().top(), header.top() - gpui::px(103.));
    let json = view.read_with(visual, |view, _| {
        assert!(view.browser.active_user_profile().empty_favorites_dismissed);
        assert!(view.command_bar.is_none());
        view.session_json()
    });
    assert_eq!(engine.snapshot(), documents);
    let restored = browser_core::session::from_json(&json).unwrap();
    let (_, restored_visual, _) = open(cx, restored);
    assert!(restored_visual.debug_bounds("empty-favorites").is_none());
    assert_eq!(restored_visual.debug_bounds("space-header-0").unwrap().top(), header.top() - gpui::px(103.));
}

#[gpui::test]
fn dismissed_card_returns_only_for_tab_drag_and_cancelling_restores_collapsed_layout(cx: &mut TestAppContext) {
    let mut browser = Browser::new();
    browser.active_user_profile_mut().empty_favorites_dismissed = true;
    let tab = browser.new_tab("https://active.example", SystemTime::now());
    let (view, visual, engine) = open(cx, browser);
    let before = view.read_with(visual, |view, _| view.session_json());
    let documents = engine.snapshot();
    let header = visual.debug_bounds("space-header-0").unwrap();
    assert!(visual.debug_bounds("empty-favorites").is_none());
    begin_drag(visual, selector("tab", tab));
    assert_eq!(visual.debug_bounds("empty-favorites").unwrap().size.height, gpui::px(95.));
    assert_eq!(visual.debug_bounds("space-header-0").unwrap().top(), header.top() + gpui::px(103.));
    finish_drag(visual, "address");
    assert_eq!(visual.debug_bounds("space-header-0").unwrap(), header);
    assert_eq!(view.read_with(visual, |view, _| view.session_json()), before);
    assert_eq!(engine.snapshot(), documents);
}

#[gpui::test]
fn private_window_has_no_empty_favorites_card(cx: &mut TestAppContext) {
    let (_, visual, _) = open(cx, Browser::new_private());
    assert!(visual.debug_bounds("empty-favorites").is_none());
    assert!(visual.debug_bounds("favorites").is_none());
}

#[gpui::test]
fn a_dismissed_card_accepts_the_first_favorite_without_reloading(cx: &mut TestAppContext) {
    move_first_favorite(cx, false, true);
}

#[gpui::test]
fn dismissal_updates_other_windows_in_the_same_profile(cx: &mut TestAppContext) {
    let mut other_cx = cx.clone();
    let (view, visual, _) = open(cx, Browser::new());
    let profile = view.read_with(visual, |view, _| view.profile.handle.clone());
    let (other, other_visual) = other_cx.add_window_view(|window, cx| {
        BrowserWindow::with_profile(profile, None, None, window, cx)
    });
    other_visual.run_until_parked();
    let header = other_visual.debug_bounds("space-header-0").unwrap();
    let close = visual.debug_bounds("dismiss-empty-favorites").unwrap().center();
    visual.simulate_mouse_move(close, None, Modifiers::none());
    visual.simulate_click(close, Modifiers::none());
    visual.run_until_parked();
    other_visual.run_until_parked();
    assert!(other.read_with(other_visual, |view, _| view.browser.active_user_profile().empty_favorites_dismissed));
    assert_eq!(other_visual.debug_bounds("space-header-0").unwrap().top(), header.top() - gpui::px(103.));
}
