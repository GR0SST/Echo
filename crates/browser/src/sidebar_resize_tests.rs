//! Pointer-driven host coverage, included as a child of window.rs.
//! Cargo and native validation belong to the integrating parent.
use super::BrowserWindow;
use crate::{
    pages::{StorageSelection, WebEngine},
    sidebar_pages::SidebarPage,
};
use engine::{Bounds, Event, ViewId};
use gpui::{
    Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

#[derive(Default)]
struct EngineState {
    next: u64,
    documents: usize,
    bounds: HashMap<ViewId, Bounds>,
    visible: HashMap<ViewId, bool>,
    focus: Vec<NativeFocus>,
    inspector: Option<engine::InspectorStatus>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeFocus {
    Page(ViewId),
    Inspector(engine::InspectorId),
    Ui,
}

#[derive(Clone, Default)]
struct RecordingEngine(Rc<RefCell<EngineState>>);

impl WebEngine for RecordingEngine {
    fn create_in_storage(
        &mut self,
        _: &StorageSelection,
        _: *mut std::ffi::c_void,
        url: &str,
        bounds: Bounds,
    ) -> Option<ViewId> {
        self.create(url, bounds)
    }
    fn create(&mut self, _: &str, bounds: Bounds) -> Option<ViewId> {
        let mut state = self.0.borrow_mut();
        state.next += 1;
        state.documents += 1;
        let id = ViewId::from_raw(state.next);
        state.bounds.insert(id, bounds);
        state.visible.insert(id, false);
        Some(id)
    }
    fn set_bounds(&mut self, id: ViewId, bounds: Bounds) {
        self.0.borrow_mut().bounds.insert(id, bounds);
    }
    fn set_top_corners_rounded(&mut self, _: ViewId, _: bool) {}
    fn set_visible(&mut self, id: ViewId, visible: bool) {
        self.0.borrow_mut().visible.insert(id, visible);
    }
    fn focus_page(&mut self, id: ViewId) {
        self.0.borrow_mut().focus.push(NativeFocus::Page(id));
    }
    fn focus_ui(&mut self) {
        self.0.borrow_mut().focus.push(NativeFocus::Ui);
    }
    fn supports_inspector(&self) -> bool {
        true
    }
    fn open_inspector(
        &mut self,
        inspected: ViewId,
        _: *mut std::ffi::c_void,
        _: Bounds,
        panel: engine::InspectorPanel,
        _: bool,
    ) -> Result<engine::InspectorId, String> {
        let id = engine::InspectorId::from_raw(11);
        self.0.borrow_mut().inspector = Some(engine::InspectorStatus {
            id,
            inspected,
            generation: 13,
            panel,
            selected_panel: Some(panel),
            phase: engine::InspectorPhase::FrontendReady,
            url: "https://example.com/frontend".into(),
        });
        Ok(id)
    }
    fn inspector_status(&self, id: engine::InspectorId) -> Option<engine::InspectorStatus> {
        self.0
            .borrow()
            .inspector
            .clone()
            .filter(|status| status.id == id)
    }
    fn focus_inspector(&mut self, id: engine::InspectorId) {
        self.0.borrow_mut().focus.push(NativeFocus::Inspector(id));
    }

    fn navigate(&mut self, _: ViewId, _: &str) {
        self.0.borrow_mut().documents += 1;
    }
    fn go_back(&mut self, _: ViewId) {}
    fn go_forward(&mut self, _: ViewId) {}
    fn undo(&mut self, _: ViewId) {}
    fn redo(&mut self, _: ViewId) {}
    fn reload(&mut self, _: ViewId) {
        self.0.borrow_mut().documents += 1;
    }
    fn force_reload(&mut self, _: ViewId) {
        self.0.borrow_mut().documents += 1;
    }
    fn stop_loading(&mut self, _: ViewId) {}
    fn set_audio_muted(&mut self, _: ViewId, _: bool) {}
    fn audio_muted(&self, _: ViewId) -> Option<bool> {
        Some(false)
    }
    fn zoom(&mut self, _: ViewId, _: i32) {}
    fn find(&mut self, _: ViewId, _: &str, _: bool, _: bool) {}
    fn stop_finding(&mut self, _: ViewId) {}
    fn close(&mut self, id: ViewId) {
        let mut state = self.0.borrow_mut();
        state.documents += 1;
        state.bounds.remove(&id);
        state.visible.remove(&id);
    }
    fn pump_fn(&self) -> fn() {
        || {}
    }
    fn take_events(&mut self) -> Vec<Event> {
        Vec::new()
    }
}

fn settle(visual: &mut VisualTestContext) {
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_secs(1));
    visual.run_until_parked();
}

fn open(
    cx: &mut TestAppContext,
) -> (
    Entity<BrowserWindow>,
    &mut VisualTestContext,
    RecordingEngine,
) {
    cx.update(crate::init);
    let engine = RecordingEngine::default();
    let (view, visual) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(
            crate::demo_browser(),
            Some(Box::new(engine.clone())),
            window,
            cx,
        );
        view.focus(window);
        view
    });
    visual.simulate_resize(gpui::size(px(1440.), px(1000.)));
    settle(visual);
    (view, visual, engine)
}

fn width(view: &Entity<BrowserWindow>, visual: &VisualTestContext) -> f32 {
    view.read_with(visual, |view, _| view.sidebar_width())
}

fn session(view: &Entity<BrowserWindow>, visual: &VisualTestContext) -> String {
    view.read_with(visual, |view, _| view.session_json())
}

fn begin(visual: &mut VisualTestContext) -> Point<Pixels> {
    visual.run_until_parked();
    let handle = visual
        .debug_bounds("sidebar-resize-handle")
        .expect("real resize handle");
    let sidebar = visual.debug_bounds("sidebar").expect("docked sidebar");
    assert!(handle.left() >= sidebar.left() && handle.right() <= sidebar.right());
    assert!(
        (f32::from(handle.right() - sidebar.right())).abs() <= 1.,
        "handle belongs at the sidebar's right edge"
    );
    let start = handle.center();
    visual.simulate_mouse_move(start, None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    start
}

fn move_to(
    visual: &mut VisualTestContext,
    start: Point<Pixels>,
    begin_width: f32,
    raw_width: f32,
) -> Point<Pixels> {
    let end = start + point(px(raw_width - begin_width), px(0.));
    visual.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    end
}

fn end(visual: &mut VisualTestContext, position: Point<Pixels>) {
    visual.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
    settle(visual);
}

fn resize(view: &Entity<BrowserWindow>, visual: &mut VisualTestContext, raw_width: f32) {
    let old = width(view, visual);
    let start = begin(visual);
    let at = move_to(visual, start, old, raw_width);
    end(visual, at);
}

fn assert_layout(visual: &mut VisualTestContext, engine: &RecordingEngine, expected: f32) {
    let sidebar = visual.debug_bounds("sidebar").expect("sidebar");
    let surface = visual.debug_bounds("page-surface").expect("page surface");
    assert_eq!(sidebar.size.width, px(expected));
    assert_eq!(surface.left(), sidebar.right());
    let state = engine.0.borrow();
    let (_, bounds) = state
        .bounds
        .iter()
        .find(|(id, _)| state.visible.get(id) == Some(&true))
        .expect("visible native document");
    for (actual, expected) in [
        (bounds.x, f64::from(f32::from(surface.left()))),
        (bounds.y, f64::from(f32::from(surface.top()))),
        (bounds.width, f64::from(f32::from(surface.size.width))),
        (bounds.height, f64::from(f32::from(surface.size.height))),
    ] {
        assert!(
            (actual - expected).abs() <= 1.,
            "native bounds must follow live GPUI layout: {bounds:?} / {surface:?}"
        );
    }
}

#[gpui::test]
fn pointer_resize_updates_live_bounds_clamps_and_keeps_the_document(cx: &mut TestAppContext) {
    let (view, visual, engine) = open(cx);
    assert_eq!(width(&view, visual), 240.);
    assert_layout(visual, &engine, 240.);
    let before = session(&view, visual);
    let pages = view.read_with(visual, |view, _| view.pages.clone().unwrap());
    let identities = engine.0.borrow().bounds.keys().copied().collect::<Vec<_>>();
    let documents = engine.0.borrow().documents;
    let start = begin(visual);
    // Every assertion precedes MouseUp; a release-only implementation fails.
    let mut at = start;
    for (raw, clamped) in [
        (360., 360.),
        (900., 500.),
        (120., 164.),
        (82., 164.),
        (300., 300.),
    ] {
        at = move_to(visual, start, 240., raw);
        assert_eq!(width(&view, visual), clamped);
        assert_layout(visual, &engine, clamped);
        assert_eq!(session(&view, visual), before);
        assert_eq!(
            engine.0.borrow().documents,
            documents,
            "geometry cannot navigate, reload, create or close"
        );
    }
    end(visual, at);
    assert_eq!(width(&view, visual), 300.);
    assert_layout(visual, &engine, 300.);
    assert!(view.read_with(visual, |view, _| Rc::ptr_eq(
        view.pages.as_ref().unwrap(),
        &pages
    )));
    assert_eq!(engine.0.borrow().bounds.len(), identities.len());
    assert!(
        identities
            .iter()
            .all(|id| engine.0.borrow().bounds.contains_key(id))
    );
    assert_eq!(engine.0.borrow().documents, documents);
    assert_eq!(session(&view, visual), before);
    // Released motion must no longer resize.
    visual.simulate_mouse_move(at + point(px(100.), px(0.)), None, Modifiers::none());
    visual.run_until_parked();
    assert_eq!(width(&view, visual), 300.);
}

#[gpui::test]
fn escape_cancels_a_live_resize_and_mouseup_cannot_commit_it(cx: &mut TestAppContext) {
    let (view, visual, engine) = open(cx);
    resize(&view, visual, 320.);
    let before = session(&view, visual);
    let start = begin(visual);
    let at = move_to(visual, start, 320., 470.);
    assert_layout(visual, &engine, 470.);
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert_eq!(width(&view, visual), 320.);
    assert_layout(visual, &engine, 320.);
    let at = move_to(visual, at, 470., 490.);
    end(visual, at);
    assert_eq!(width(&view, visual), 320.);
    assert!(view.read_with(visual, |view, _| view.sidebar_visible));
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn collapse_is_live_reversible_and_release_retains_the_begin_width(cx: &mut TestAppContext) {
    let (view, visual, engine) = open(cx);
    resize(&view, visual, 340.);
    let before = session(&view, visual);
    let documents = engine.0.borrow().documents;
    let start = begin(visual);
    let at = move_to(visual, start, 340., 81.);
    let collapsed = visual.debug_bounds("page-surface").unwrap();
    assert!(
        collapsed.left() <= px(10.),
        "raw < 82 must collapse the content inset before release"
    );
    // Dragging back out before release recovers the same gesture.
    let at = move_to(visual, at, 81., 280.);
    assert_layout(visual, &engine, 280.);
    end(visual, at);
    assert!(view.read_with(visual, |view, _| view.sidebar_visible));
    resize(&view, visual, 340.);
    // Exact threshold remains visually clamped while dragging but hides on release.
    let start = begin(visual);
    let at = move_to(visual, start, 340., 82.);
    assert_layout(visual, &engine, 164.);
    end(visual, at);
    assert!(!view.read_with(visual, |view, _| view.sidebar_visible));
    assert_eq!(
        width(&view, visual),
        340.,
        "reopen retains BEGIN width, not the clamped drag width"
    );
    visual.simulate_keystrokes("cmd-s");
    settle(visual);
    assert!(view.read_with(visual, |view, _| view.sidebar_visible));
    assert_layout(visual, &engine, 340.);
    // A release strictly below the threshold follows the same retained-width rule.
    resize(&view, visual, 60.);
    assert!(!view.read_with(visual, |view, _| view.sidebar_visible));
    assert_eq!(width(&view, visual), 340.);
    visual.simulate_keystrokes("cmd-s");
    settle(visual);
    assert_layout(visual, &engine, 340.);
    assert_eq!(engine.0.borrow().documents, documents);
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn double_click_resets_to_arc_width_and_escape_restores_from_live_collapse(
    cx: &mut TestAppContext,
) {
    let (view, visual, engine) = open(cx);
    resize(&view, visual, 410.);
    let position = visual
        .debug_bounds("sidebar-resize-handle")
        .unwrap()
        .center();
    visual.simulate_mouse_move(position, None, Modifiers::none());
    visual.simulate_click(position, Modifiers::none());
    visual.simulate_event(gpui::MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    visual.simulate_event(gpui::MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    settle(visual);
    assert_eq!(width(&view, visual), 228.);
    assert_layout(visual, &engine, 228.);
    let start = begin(visual);
    let at = move_to(visual, start, 228., 60.);
    visual.simulate_keystrokes("escape");
    end(visual, at);
    assert!(view.read_with(visual, |view, _| view.sidebar_visible));
    assert_layout(visual, &engine, 228.);
}

#[gpui::test]
fn shared_profile_hosts_keep_independent_widths(cx: &mut TestAppContext) {
    let (a, visual_a, _) = open(cx);
    resize(&a, visual_a, 360.);
    let profile = a.read_with(visual_a, |view, _| view.profile.handle.clone());
    let handle = visual_a.cx.update(|cx| {
        use gpui::AppContext;
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| {
                let view = BrowserWindow::with_profile(profile, None, None, window, cx);
                view.focus(window);
                view
            })
        })
        .unwrap()
    });
    let b = handle.root(&mut visual_a.cx).unwrap();
    let mut visual_b = VisualTestContext::from_window(handle.into(), &visual_a.cx);
    visual_b.simulate_resize(gpui::size(px(1440.), px(1000.)));
    settle(&mut visual_b);
    assert_eq!(
        width(&b, &visual_b),
        240.,
        "new host does not inherit another host's width"
    );
    resize(&b, &mut visual_b, 190.);
    assert_eq!(width(&a, visual_a), 360.);
    resize(&a, visual_a, 60.);
    assert_eq!(width(&b, &visual_b), 190.);
    assert!(b.read_with(&visual_b, |view, _| view.sidebar_visible));
    visual_a.simulate_keystrokes("cmd-s");
    settle(visual_a);
    assert_eq!(width(&a, visual_a), 360.);
    assert_eq!(width(&b, &visual_b), 190.);
}

fn select(view: &Entity<BrowserWindow>, visual: &mut VisualTestContext, page: SidebarPage) {
    view.update(visual, |view, cx| view.select_sidebar_page(page, cx));
    settle(visual);
}

#[gpui::test]
fn virtual_form_tracks_host_width_keeps_input_focus_and_library_cards_stay_fixed(
    cx: &mut TestAppContext,
) {
    let (view, visual, engine) = open(cx);
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    let before = session(&view, visual);
    let documents = engine.0.borrow().documents;
    view.update(visual, |view, cx| view.begin_space_creation(origin, cx));
    settle(visual);
    let form = view.read_with(visual, |view, _| {
        view.sidebar_navigation.form.clone().unwrap()
    });
    let name = visual.debug_bounds("space-creation-name").unwrap().center();
    visual.simulate_mouse_move(name, None, Modifiers::none());
    visual.simulate_click(name, Modifiers::none());
    visual.simulate_input("Keep");
    for expected in [400., 180.] {
        resize(&view, visual, expected);
        assert_eq!(
            visual
                .debug_bounds("space-creation-form")
                .unwrap()
                .size
                .width,
            px(expected)
        );
        assert_eq!(
            view.read_with(visual, |view, _| view
                .sidebar_navigation
                .form
                .clone()
                .unwrap()),
            form
        );
    }
    // No refocus click: the drag must preserve a focused TextInput.
    visual.simulate_input(" typing");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |view, _| view
            .sidebar_navigation
            .controller
            .draft()
            .unwrap()
            .name
            .clone()),
        "Keep typing"
    );
    select(&view, visual, SidebarPage::Space(origin));
    for expected in [180., 400.] {
        resize(&view, visual, expected);
        select(&view, visual, SidebarPage::Library);
        let nav = visual.debug_bounds("library-nav-spaces").unwrap().center();
        visual.simulate_mouse_move(nav, None, Modifiers::none());
        visual.simulate_click(nav, Modifiers::none());
        settle(visual);
        assert_eq!(
            visual.debug_bounds("library-space-0").unwrap().size.width,
            px(240.)
        );
        assert_eq!(
            visual
                .debug_bounds("library-space-body-0")
                .unwrap()
                .size
                .width,
            px(224.)
        );
        assert_eq!(width(&view, visual), expected);
        select(&view, visual, SidebarPage::Space(origin));
    }
    assert_eq!(session(&view, visual), before);
    assert_eq!(engine.0.borrow().documents, documents);
}

/// Keep the root active: the floating child is opened with `focus: false`.
/// Its real handle still receives pointer events through its own test window.
fn floating(view: &Entity<BrowserWindow>, visual: &mut VisualTestContext) -> VisualTestContext {
    visual.simulate_keystrokes("cmd-s");
    settle(visual);
    assert!(!view.read_with(visual, |view, _| view.sidebar_visible));
    visual.update(|window, cx| {
        view.update(cx, |view, cx| view.reveal_sidebar(window, cx));
    });
    visual.run_until_parked();
    let overlay = view.update(visual, |view, _| {
        let floating = view.floating_sidebar.as_mut().expect("floating sidebar");
        // Prevent the grace timer from dismissing the panel before the pointer
        // reaches its handle; no resize state is manufactured by this fixture.
        floating.entered = true;
        floating.overlay.expect("floating native child window")
    });
    let mut child = VisualTestContext::from_window(overlay.into(), &visual.cx);
    settle(&mut child);
    // GPUI's AnimationElement uses wall-clock Instant, unlike executor timers.
    // Match the existing floating-sidebar fixtures before clicking its handle.
    std::thread::sleep(Duration::from_millis(200));
    child.update(|_, cx| cx.refresh_windows());
    child.run_until_parked();
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    child
}

fn viewport(visual: &mut VisualTestContext) -> gpui::Size<Pixels> {
    visual.update(|window, _| window.viewport_size())
}

/// TestWindow::resize changes native bounds without sending its on_resize
/// callback. Assert the actual requested bounds, then deliver that OS callback.
fn deliver_requested_resize(visual: &mut VisualTestContext, expected_width: Pixels) {
    let requested = visual.update(|window, _| window.bounds().size);
    assert_eq!(
        requested.width, expected_width,
        "real deferred native resize request"
    );
    visual.simulate_resize(requested);
    visual.run_until_parked();
}

fn begin_floating(visual: &mut VisualTestContext) -> Point<Pixels> {
    let panel = visual
        .debug_bounds("floating-sidebar")
        .expect("floating panel");
    let handle = visual
        .debug_bounds("sidebar-resize-handle")
        .expect("floating handle");
    assert!((f32::from(handle.right() - panel.right())).abs() <= 1.);
    let start = handle.center();
    visual.simulate_mouse_move(start, None, Modifiers::none());
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    start
}

#[gpui::test]
fn floating_deactivation_restores_live_panel_and_child_viewport(cx: &mut TestAppContext) {
    let (view, visual, engine) = open(cx);
    resize(&view, visual, 320.);
    let mut child = floating(&view, visual);
    let initial_panel = child.debug_bounds("floating-sidebar").unwrap();
    let initial_viewport = viewport(&mut child);
    let initial_native = engine.0.borrow().bounds.clone();
    let documents = engine.0.borrow().documents;
    let pages = view.read_with(visual, |view, _| view.pages.clone().unwrap());
    let before = session(&view, visual);
    let start = begin_floating(&mut child);
    assert!(
        view.read_with(visual, |view, _| view.sidebar_resize.is_some()),
        "the visible floating handle starts the actual gesture"
    );
    let at = move_to(&mut child, start, 320., 460.);
    assert_eq!(width(&view, visual), 460.);
    deliver_requested_resize(&mut child, initial_viewport.width + px(140.));
    assert_eq!(
        child.debug_bounds("floating-sidebar").unwrap().size.width,
        px(460.)
    );
    assert_eq!(
        viewport(&mut child).width,
        initial_viewport.width + px(140.)
    );
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_some()));

    // Exercise the actual GPUI activation observer, including its queued
    // native child resize. Merely abandoning the gesture leaves a stale child.
    assert!(visual.update(|window, _| window.is_window_active()));
    visual.deactivate_window();
    settle(&mut child);
    assert!(!visual.update(|window, _| window.is_window_active()));
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_none()));
    assert_eq!(width(&view, visual), 320.);
    deliver_requested_resize(&mut child, initial_viewport.width);
    assert_eq!(
        child.debug_bounds("floating-sidebar").unwrap(),
        initial_panel
    );
    assert_eq!(viewport(&mut child), initial_viewport);
    assert_eq!(engine.0.borrow().bounds, initial_native);
    end(&mut child, at);
    assert_eq!(
        width(&view, visual),
        320.,
        "late child MouseUp cannot commit"
    );
    assert_eq!(viewport(&mut child), initial_viewport);
    assert_eq!(engine.0.borrow().documents, documents);
    assert!(view.read_with(visual, |view, _| Rc::ptr_eq(
        view.pages.as_ref().unwrap(),
        &pages
    )));
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn inactive_cancel_defers_original_page_and_inspector_focus_until_activation(
    cx: &mut TestAppContext,
) {
    let (view, visual, engine) = open(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert!(visual.update(|window, _| window.is_window_active()));
    let pages = view.read_with(visual, |view, _| view.pages.clone().unwrap());
    let page = *engine.0.borrow().bounds.keys().next().unwrap();
    for inspector in [false, true] {
        if inspector {
            let mut pages = pages.borrow_mut();
            // Fake engine only: the public DevTools entry requires a non-null
            // parent identity but never dereferences this test sentinel.
            pages.set_parent(std::ptr::dangling_mut::<u8>().cast());
            let tab = pages.shown().expect("shown tab");
            pages
                .run_devtools(tab, crate::pages::devtools::Command::Toggle)
                .unwrap();
        } else {
            pages.borrow_mut().focus_shown();
        }
        visual.run_until_parked();
        assert!(pages.borrow().capture_resize_focus().has_native_focus());
        engine.0.borrow_mut().focus.clear();
        let old = width(&view, visual);
        let start = begin(visual);
        let at = move_to(visual, start, old, old + 70.);
        assert_eq!(engine.0.borrow().focus, vec![NativeFocus::Ui]);
        engine.0.borrow_mut().focus.clear();

        visual.deactivate_window();
        visual.run_until_parked();
        assert!(!visual.update(|window, _| window.is_window_active()));
        assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_none()));
        assert_eq!(width(&view, visual), old);
        assert!(
            pages.borrow().capture_resize_focus().is_ui(),
            "native focus stays suppressed until original snapshot is revalidated"
        );
        assert!(view.read_with(visual, |view, _| {
            view.sidebar_resize_focus_on_activation.is_some()
        }));
        assert!(
            engine.0.borrow().focus.is_empty(),
            "inactive cancellation must not focus any responder"
        );
        end(visual, at);
        assert!(
            engine.0.borrow().focus.is_empty(),
            "late MouseUp must not restore focus while inactive"
        );

        visual.update(|window, _| window.activate_window());
        visual.run_until_parked();
        let expected = if inspector {
            NativeFocus::Inspector(engine::InspectorId::from_raw(11))
        } else {
            NativeFocus::Page(page)
        };
        let state = engine.0.borrow();
        assert!(
            !state.focus.is_empty(),
            "normal activation restores the responder"
        );
        assert!(
            state.focus.iter().all(|call| *call == expected),
            "activation must retain the exact native kind: {:?}",
            state.focus
        );
        drop(state);
        assert!(view.read_with(visual, |view, _| {
            view.sidebar_resize_focus_on_activation.is_none()
        }));
    }
}

#[gpui::test]
fn inactive_cancel_does_not_steal_a_sidebar_text_input(cx: &mut TestAppContext) {
    let (view, visual, engine) = open(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert!(visual.update(|window, _| window.is_window_active()));
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    view.update(visual, |view, cx| view.begin_space_creation(origin, cx));
    settle(visual);
    let name = visual.debug_bounds("space-creation-name").unwrap().center();
    visual.simulate_mouse_move(name, None, Modifiers::none());
    visual.simulate_click(name, Modifiers::none());
    visual.simulate_input("Keep");
    engine.0.borrow_mut().focus.clear();
    let old = width(&view, visual);
    let start = begin(visual);
    let at = move_to(visual, start, old, 420.);
    visual.deactivate_window();
    visual.run_until_parked();
    assert!(!visual.update(|window, _| window.is_window_active()));
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_none()));
    assert_eq!(width(&view, visual), old);
    end(visual, at);
    assert_eq!(width(&view, visual), old);
    assert!(view.read_with(visual, |view, _| {
        view.sidebar_resize_focus_on_activation.is_none()
    }));
    assert!(engine.0.borrow().focus.is_empty());
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert!(
        engine.0.borrow().focus.is_empty(),
        "UI intent cannot become native focus on activation"
    );
    // No refocus click after cancellation or activation.
    visual.simulate_input(" typing");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |view, _| view
            .sidebar_navigation
            .controller
            .draft()
            .unwrap()
            .name
            .clone()),
        "Keep typing"
    );
}

#[gpui::test]
fn lost_button_cancels_and_release_outside_root_finishes(cx: &mut TestAppContext) {
    let (view, visual, engine) = open(cx);
    let start = begin(visual);
    let at = move_to(visual, start, 240., 410.);
    assert_layout(visual, &engine, 410.);
    visual.simulate_mouse_move(at + point(px(20.), px(0.)), None, Modifiers::none());
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_none()));
    assert_layout(visual, &engine, 240.);
    end(visual, at);
    assert_eq!(width(&view, visual), 240.);

    let start = begin(visual);
    move_to(visual, start, 240., 370.);
    // Outside both the handle and the root hitbox; global capture must release
    // the gesture instead of leaving it to consume the next mouse movement.
    let outside = point(start.x + px(130.), px(1100.));
    end(visual, outside);
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_none()));
    assert_layout(visual, &engine, 370.);
    visual.simulate_mouse_move(start, Some(MouseButton::Left), Modifiers::none());
    visual.run_until_parked();
    assert_eq!(width(&view, visual), 370.);
}

#[gpui::test]
fn inactive_resize_redraw_and_profile_replacement_never_focus_the_replacement(
    cx: &mut TestAppContext,
) {
    let (view, visual, engine) = open(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert!(visual.update(|window, _| window.is_window_active()));
    let pages = view.read_with(visual, |view, _| view.pages.clone().unwrap());
    pages.borrow_mut().focus_shown();
    let original = pages.borrow().capture_resize_focus();
    assert!(original.has_native_focus());
    let original_views = engine.0.borrow().bounds.keys().copied().collect::<Vec<_>>();
    let start = begin(visual);
    move_to(visual, start, 240., 380.);
    engine.0.borrow_mut().focus.clear();
    visual.deactivate_window();
    visual.run_until_parked();
    assert!(!visual.update(|window, _| window.is_window_active()));
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_none()));
    // Inactive redraw uses the same production Pages::sync as render.
    for _ in 0..2 {
        visual.update(|window, cx| {
            let _ = window.draw(cx);
        });
        visual.run_until_parked();
        assert!(engine.0.borrow().focus.is_empty());
    }
    view.update(visual, |view, cx| {
        let space = view.browser.space().id;
        view.edit_profile(|browser| {
            let destination = browser.create_user_profile("Inactive replacement").unwrap();
            browser
                .assign_space_user_profile(space, destination)
                .unwrap();
        });
        view.refresh_profile_projection();
        cx.notify();
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        let _ = window.draw(cx);
    });
    assert!(
        engine.0.borrow().focus.is_empty(),
        "inactive replacement cannot take focus"
    );
    assert!(
        engine
            .0
            .borrow()
            .bounds
            .keys()
            .all(|view| !original_views.contains(view)),
        "the document generation was actually replaced"
    );
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert!(
        engine.0.borrow().focus.is_empty(),
        "activation reconciliation cannot focus a replacement before rejecting the original snapshot"
    );
    assert!(view.read_with(visual, |view, _| {
        view.sidebar_resize_focus_on_activation.is_none()
    }));
    assert!(pages.borrow().capture_resize_focus().is_ui());
    assert!(!pages.borrow_mut().restore_resize_focus(original));
    assert!(engine.0.borrow().focus.is_empty());
}

#[gpui::test]
fn cmd_s_during_resize_cancels_and_releases_native_focus_without_reactivation(
    cx: &mut TestAppContext,
) {
    let (view, visual, engine) = open(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert!(visual.update(|window, _| window.is_window_active()));
    let pages = view.read_with(visual, |view, _| view.pages.clone().unwrap());
    pages.borrow_mut().focus_shown();
    let original_views = engine.0.borrow().bounds.keys().copied().collect::<Vec<_>>();
    let before = session(&view, visual);
    let documents = engine.0.borrow().documents;
    let start = begin(visual);
    let at = move_to(visual, start, 240., 420.);
    assert_eq!(width(&view, visual), 420.);
    engine.0.borrow_mut().focus.clear();
    visual.simulate_keystrokes("cmd-s");
    settle(visual);
    assert!(visual.update(|window, _| window.is_window_active()));
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_none()));
    assert!(view.read_with(visual, |view, _| {
        view.sidebar_resize_focus_on_activation.is_none()
    }));
    assert_eq!(
        width(&view, visual),
        240.,
        "Cmd+S cancels the gesture before toggling"
    );
    assert!(!view.read_with(visual, |view, _| view.sidebar_visible));
    assert!(
        pages.borrow().capture_resize_focus().has_native_focus(),
        "active cancellation restores the original native responder immediately"
    );
    assert!(
        engine
            .0
            .borrow()
            .focus
            .iter()
            .any(|call| matches!(call, NativeFocus::Page(id) if original_views.contains(id)))
    );
    end(visual, at);
    assert_eq!(
        width(&view, visual),
        240.,
        "late MouseUp cannot commit the cancelled width"
    );
    // No deactivate/reactivate cycle: suppression must already be released.
    engine.0.borrow_mut().focus.clear();
    pages.borrow_mut().request_page_focus();
    pages.borrow_mut().focus_shown();
    assert!(
        engine
            .0
            .borrow()
            .focus
            .iter()
            .any(|call| matches!(call, NativeFocus::Page(id) if original_views.contains(id))),
        "normal page focus must remain usable"
    );
    visual.simulate_keystrokes("cmd-s");
    settle(visual);
    assert!(view.read_with(visual, |view, _| view.sidebar_visible));
    assert_eq!(width(&view, visual), 240.);
    assert_eq!(engine.0.borrow().documents, documents);
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn floating_resize_survives_root_to_owned_child_key_transition_but_cancels_external_focus(
    cx: &mut TestAppContext,
) {
    let (view, visual, engine) = open(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    let mut child = floating(&view, visual);
    let initial_viewport = viewport(&mut child);
    let documents = engine.0.borrow().documents;
    let before = session(&view, visual);
    let start = begin_floating(&mut child);
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_some()));
    // Native evidence: child MouseDown installs the gesture before AppKit
    // delivers root resign-key and owned-child become-key notifications.
    child.update(|window, _| window.activate_window());
    child.run_until_parked();
    assert!(!visual.update(|window, _| window.is_window_active()));
    assert!(child.update(|window, _| window.is_window_active()));
    child.executor().advance_clock(Duration::from_millis(40));
    child.run_until_parked();
    assert!(
        view.read_with(visual, |view, _| view.sidebar_resize.is_some()),
        "owned-child key transition keeps the exact gesture"
    );
    let at = move_to(&mut child, start, 240., 400.);
    assert_eq!(width(&view, visual), 400.);
    deliver_requested_resize(&mut child, initial_viewport.width + px(160.));
    assert_eq!(
        child.debug_bounds("floating-sidebar").unwrap().size.width,
        px(400.)
    );
    end(&mut child, at);
    assert_eq!(width(&view, visual), 400.);
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_none()));

    // An external app loses the child's key status too. The root remaining a
    // native mainWindow is deliberately irrelevant to this cancellation.
    let start = begin_floating(&mut child);
    move_to(&mut child, start, 400., 480.);
    assert_eq!(width(&view, visual), 480.);
    child.deactivate_window();
    child.executor().advance_clock(Duration::from_millis(40));
    child.run_until_parked();
    assert!(!visual.update(|window, _| window.is_window_active()));
    assert!(!child.update(|window, _| window.is_window_active()));
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_none()));
    assert_eq!(width(&view, visual), 400.);
    deliver_requested_resize(&mut child, initial_viewport.width + px(160.));
    assert_eq!(engine.0.borrow().documents, documents);
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn unrelated_browser_key_window_cannot_preserve_a_floating_resize(cx: &mut TestAppContext) {
    let (view, visual, engine) = open(cx);
    let documents = engine.0.borrow().documents;
    let mut child = floating(&view, visual);
    let start = begin_floating(&mut child);
    child.update(|window, _| window.activate_window());
    child.run_until_parked();
    child.executor().advance_clock(Duration::from_millis(40));
    child.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.sidebar_resize.is_some()));
    move_to(&mut child, start, 240., 380.);
    assert_eq!(width(&view, visual), 380.);
    let profile = view.read_with(visual, |view, _| view.profile.handle.clone());
    let other = visual.cx.update(|cx| {
        use gpui::AppContext;
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| BrowserWindow::with_profile(profile, None, None, window, cx))
        })
        .unwrap()
    });
    other
        .update(&mut visual.cx, |_, window, _| window.activate_window())
        .unwrap();
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(40));
    visual.run_until_parked();
    assert!(!child.update(|window, _| window.is_window_active()));
    assert!(
        view.read_with(visual, |view, _| view.sidebar_resize.is_none()),
        "a shared-workspace host is not this gesture's owned child"
    );
    assert_eq!(width(&view, visual), 240.);
    assert_eq!(engine.0.borrow().documents, documents);
}
