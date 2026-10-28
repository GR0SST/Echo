//! Fake-only resize seam regression coverage; no native windows or engine pump.
use super::*;

#[derive(Default)]
struct FocusState {
    calls: Vec<(ViewId, Option<engine::InspectorId>)>,
    frontend: Option<engine::InspectorStatus>,
    inspector_events: Vec<engine::InspectorEvent>,
}
struct FocusEngine(Rc<RefCell<FocusState>>);
impl WebEngine for FocusEngine {
    fn take_inspector_events(&mut self) -> Vec<engine::InspectorEvent> {
        std::mem::take(&mut self.0.borrow_mut().inspector_events)
    }
    fn inspector_status(&self, id: engine::InspectorId) -> Option<engine::InspectorStatus> {
        self.0.borrow().frontend.clone().filter(|status| status.id == id)
    }
    fn focus_inspector(&mut self, id: engine::InspectorId) {
        let mut state = self.0.borrow_mut();
        let view = state.frontend.as_ref().unwrap().inspected;
        state.calls.push((view, Some(id)));
    }
    fn create(&mut self, _: &str, _: Bounds) -> Option<ViewId> { Some(ViewId::from_raw(99)) }
    fn set_bounds(&mut self, _: ViewId, _: Bounds) {}
    fn set_top_corners_rounded(&mut self, _: ViewId, _: bool) {}
    fn set_visible(&mut self, _: ViewId, _: bool) {}
    fn focus_page(&mut self, view: ViewId) { self.0.borrow_mut().calls.push((view, None)); }
    fn focus_ui(&mut self) { panic!("resize focus must not focus UI"); }
    fn navigate(&mut self, _: ViewId, _: &str) {}
    fn go_back(&mut self, _: ViewId) {}
    fn go_forward(&mut self, _: ViewId) {}
    fn undo(&mut self, _: ViewId) {}
    fn redo(&mut self, _: ViewId) {}
    fn reload(&mut self, _: ViewId) {}
    fn force_reload(&mut self, _: ViewId) {}
    fn stop_loading(&mut self, _: ViewId) {}
    fn set_audio_muted(&mut self, _: ViewId, _: bool) {}
    fn audio_muted(&self, _: ViewId) -> Option<bool> { None }
    fn zoom(&mut self, _: ViewId, _: i32) {}
    fn find(&mut self, _: ViewId, _: &str, _: bool, _: bool) {}
    fn stop_finding(&mut self, _: ViewId) {}
    fn close(&mut self, _: ViewId) {}
    fn pump_fn(&self) -> fn() { || {} }
    fn take_events(&mut self) -> Vec<Event> { Vec::new() }
}
fn fixture(inspector: bool) -> (Pages, Rc<RefCell<FocusState>>, TabId) {
    fixture_for_tab(inspector, serde_json::from_str("1").unwrap())
}
fn fixture_for_tab(inspector: bool, tab: TabId) -> (Pages, Rc<RefCell<FocusState>>, TabId) {
    let state = Rc::new(RefCell::new(FocusState::default()));
    let mut pages = Pages::new(Box::new(FocusEngine(state.clone())));
    let identity = PageIdentity { tab, profile: serde_json::from_str("1").unwrap(), generation: 7 };
    let view = ViewId::from_raw(9);
    pages.shown = Some(tab);
    pages.wanted_focus = Some(tab);
    pages.visible.insert(tab);
    pages.page_has_focus = !inspector;
    pages.devtools_focused = inspector;
    {
        let mut store = pages.store.borrow_mut();
        store.identities.insert(tab, identity);
        store.pages.insert(tab, Page {
            input_attachment: 3, identity, source_space: serde_json::from_str("1").unwrap(), owner: pages.host,
            frame: Bounds::default(), visible: true, view, url: "https://example.com".into(),
            state: PageState::default(), muted: false, color_scheme: None, web_palette: None, snapshot: None, snapshot_pending: None,
            snapshot_origin: None, snapshot_good_origin: None,
        });
        if inspector {
            let id = engine::InspectorId::from_raw(11);
            store.devtools.insert(tab, devtools::Dock { id, tab, view, host: pages.host, side: engine::InspectorDock::Right, page_bounds: None, applied: None });
            state.borrow_mut().frontend = Some(engine::InspectorStatus {
                id, inspected: view, generation: 13, panel: engine::InspectorPanel::Elements,
                selected_panel: Some(engine::InspectorPanel::Elements), phase: engine::InspectorPhase::FrontendReady,
                url: "https://example.com/frontend".into(),
            });
        }
    }
    (pages, state, tab)
}

#[test]
fn capture_is_read_only_and_restore_preserves_original_native_kind() {
    for inspector in [false, true] {
        let (mut pages, state, _) = fixture(inspector);
        let snapshot = pages.capture_resize_focus();
        assert!(!snapshot.is_ui());
        assert!(snapshot.has_native_focus());
        assert!(state.borrow().calls.is_empty());
        assert_eq!(pages.devtools_focused, inspector);
        assert_eq!(pages.page_has_focus, !inspector);
        // The splitter may temporarily own the native responder.
        pages.request_ui_focus();
        assert!(pages.restore_resize_focus(snapshot));
        assert_eq!(state.borrow().calls.as_slice(), &[(ViewId::from_raw(9), inspector.then(|| engine::InspectorId::from_raw(11)))]);
        assert_eq!(pages.devtools_focused, inspector);
        assert_eq!(pages.page_has_focus, !inspector);
    }
}

fn synced_fixture(inspector: bool) -> (Pages, Rc<RefCell<FocusState>>, Browser, TabId) {
    let mut browser = Browser::new_blank();
    let tab = browser.new_tab("https://example.com", std::time::SystemTime::UNIX_EPOCH);
    let (mut pages, state, _) = fixture_for_tab(inspector, tab);
    {
        let mut store = pages.store.borrow_mut();
        let identity = store.identities.get_mut(&tab).unwrap();
        identity.profile = browser.user_profile_id_for_tab(tab).unwrap();
        let identity = *identity;
        store.pages.get_mut(&tab).unwrap().identity = identity;
        store.next_generation = identity.generation;
    }
    pages.bounds = Some(Bounds { width: 900., height: 700., ..Bounds::default() });
    (pages, state, browser, tab)
}

#[test]
fn suppression_survives_repeated_sync_and_activation_until_exact_restore() {
    for inspector in [false, true] {
        let (mut pages, state, browser, tab) = synced_fixture(inspector);
        let snapshot = pages.capture_resize_focus();
        pages.suppress_resize_focus();
        assert!(!pages.resume_resize_focus_intent(snapshot.clone()));
        assert!(!pages.restore_resize_focus(snapshot.clone()));
        for _ in 0..3 {
            pages.sync(&browser, true);
            pages.activate_host_with_visibility(&browser, true);
            if inspector {
                state.borrow_mut().inspector_events.push(engine::InspectorEvent::FocusRequested {
                    inspector: engine::InspectorId::from_raw(11),
                });
            }
            pages.take_routed_events();
            assert!(pages.resize_focus_suppressed);
            assert!(!pages.page_focus);
            assert!(!pages.page_has_focus);
            assert_eq!(pages.shown, Some(tab));
            assert!(pages.is_visible(tab));
            assert!(state.borrow().calls.is_empty());
        }
        // Abandonment retains the snapshot and suppression through activation.
        pages.store.borrow_mut().active_host = 0;
        if !inspector {
            pages.sync(&browser, true);
            pages.sync(&browser, true);
        }
        pages.activate_host_with_visibility(&browser, true);
        assert!(state.borrow().calls.is_empty());
        pages.end_resize_focus_suppression();
        assert!(!pages.page_focus);
        assert!(pages.restore_resize_focus(snapshot));
        assert_eq!(state.borrow().calls.as_slice(), &[(ViewId::from_raw(9), inspector.then(|| engine::InspectorId::from_raw(11)))]);
    }
}

#[test]
fn suppressed_retirement_and_activation_never_focus_replacement() {
    for inspector in [false, true] {
        let (mut pages, state, browser, tab) = synced_fixture(inspector);
        let snapshot = pages.capture_resize_focus();
        pages.suppress_resize_focus();
        // Inventory retirement must not focus UI; recreation uses the same TabId.
        pages.store.borrow_mut().identities.get_mut(&tab).unwrap().profile = serde_json::from_str("999").unwrap();
        pages.sync(&browser, true);
        assert_eq!(pages.view(tab), Some(ViewId::from_raw(99)));
        pages.store.borrow_mut().active_host = 0;
        pages.sync(&browser, true);
        pages.activate_host_with_visibility(&browser, true);
        pages.take_routed_events();
        assert!(state.borrow().calls.is_empty());
        pages.end_resize_focus_suppression();
        assert!(!pages.restore_resize_focus(snapshot));
        assert!(!pages.page_focus);
        pages.sync(&browser, true);
        assert!(state.borrow().calls.is_empty());
        // A subsequent explicit user selection can focus the replacement.
        pages.request_page_focus();
        pages.sync(&browser, true);
        assert_eq!(state.borrow().calls.as_slice(), &[(ViewId::from_raw(99), None)]);
    }
}

#[test]
fn suppression_ignores_selection_and_native_focus_callbacks() {
    let (mut pages, state, mut browser, _) = synced_fixture(false);
    pages.suppress_resize_focus();
    let replacement = browser.new_tab("https://replacement.example", std::time::SystemTime::UNIX_EPOCH);
    pages.request_page_focus();
    pages.sync(&browser, true);
    pages.focus_shown();
    pages.record(&Event::Focused { view: ViewId::from_raw(99) });
    pages.store.borrow_mut().devtools_restore.insert(pages.host);
    pages.take_routed_events();
    pages.activate_host_with_visibility(&browser, true);
    assert_eq!(pages.shown, Some(replacement));
    assert!(!pages.page_focus);
    assert!(!pages.page_has_focus);
    assert!(state.borrow().calls.is_empty());
    pages.end_resize_focus_suppression();
    pages.request_page_focus();
    pages.sync(&browser, true);
    assert_eq!(state.borrow().calls.as_slice(), &[(ViewId::from_raw(99), None)]);
}

#[test]
fn ui_and_unavailable_snapshots_never_issue_focus() {
    let (mut pages, state, _) = fixture(false);
    pages.request_ui_focus();
    let snapshot = pages.capture_resize_focus();
    assert!(snapshot.is_ui());
    assert!(!snapshot.has_native_focus());
    assert!(!pages.resume_resize_focus_intent(snapshot.clone()));
    assert!(!pages.restore_resize_focus(snapshot));
    pages.request_page_focus();
    pages.shown = None;
    let snapshot = pages.capture_resize_focus();
    assert!(!snapshot.is_ui());
    assert!(!snapshot.has_native_focus());
    assert!(!pages.resume_resize_focus_intent(snapshot.clone()));
    assert!(!pages.restore_resize_focus(snapshot));
    assert!(state.borrow().calls.is_empty());
}

#[test]
fn stale_page_and_host_snapshots_never_focus_a_replacement() {
    for inspector in [false, true] {
        for mutation in 0..14 {
            let (mut pages, state, tab) = fixture(inspector);
            let snapshot = pages.capture_resize_focus();
            pages.request_ui_focus();
            match mutation {
                0 => pages.store.borrow_mut().pages.get_mut(&tab).unwrap().identity.generation += 1,
                1 => pages.store.borrow_mut().pages.get_mut(&tab).unwrap().view = ViewId::from_raw(99),
                // Includes a page transferred away and back to the same host.
                2 => pages.store.borrow_mut().pages.get_mut(&tab).unwrap().input_attachment += 2,
                3 => pages.store.borrow_mut().pages.get_mut(&tab).unwrap().owner += 1,
                4 => pages.store.borrow_mut().pages.get_mut(&tab).unwrap().visible = false,
                5 => { pages.visible.clear(); }
                6 => pages.shown = None,
                7 => pages.wanted_focus = None,
                8 => pages.store.borrow_mut().active_host += 1,
                9 => { pages.store.borrow_mut().hosts.remove(&pages.host); }
                10 => { pages.store.borrow_mut().pages.clear(); }
                11 => pages.parent = 1usize as *mut c_void,
                12 => pages.host += 1,
                13 => pages.store.borrow_mut().pages.get_mut(&tab).unwrap().identity.profile = serde_json::from_str("2").unwrap(),
                _ => unreachable!(),
            }
            assert!(!pages.resume_resize_focus_intent(snapshot.clone()), "mutation {mutation}");
            assert!(!pages.page_focus);
            assert!(!pages.devtools_focused);
            assert!(!pages.page_has_focus);
            assert!(!pages.restore_resize_focus(snapshot), "mutation {mutation}");
            assert!(state.borrow().calls.is_empty());
        }
    }
    let (pages, _, _) = fixture(false);
    let snapshot = pages.capture_resize_focus();
    let (mut replacement, state, _) = fixture(false);
    replacement.request_ui_focus();
    assert!(!replacement.resume_resize_focus_intent(snapshot.clone()));
    assert!(!replacement.page_focus);
    assert!(!replacement.devtools_focused);
    assert!(!replacement.page_has_focus);
    assert!(!replacement.restore_resize_focus(snapshot));
    assert!(state.borrow().calls.is_empty());
}

#[test]
fn stale_frontend_does_not_fall_back_to_page_focus() {
    for mutation in 0..11 {
        let (mut pages, state, _) = fixture(true);
        let snapshot = pages.capture_resize_focus();
        pages.request_ui_focus();
        match mutation {
            0 => pages.store.borrow_mut().devtools.clear(),
            1 => pages.store.borrow_mut().devtools.values_mut().next().unwrap().id = engine::InspectorId::from_raw(99),
            2 => state.borrow_mut().frontend.as_mut().unwrap().generation += 1,
            3 => state.borrow_mut().frontend.as_mut().unwrap().inspected = ViewId::from_raw(99),
            4 => state.borrow_mut().frontend.as_mut().unwrap().phase = engine::InspectorPhase::Detached,
            5 => state.borrow_mut().frontend.as_mut().unwrap().phase = engine::InspectorPhase::LoadFailed,
            6 => state.borrow_mut().frontend = None,
            7 => pages.store.borrow_mut().devtools.values_mut().next().unwrap().host += 1,
            8 => {
                // The dock now belongs to another pane's tab.
                let mut store = pages.store.borrow_mut();
                let mut dock = store.devtools.drain().next().unwrap().1;
                dock.tab = serde_json::from_str("2").unwrap();
                store.devtools.insert(dock.tab, dock);
            }
            9 => pages.store.borrow_mut().devtools.values_mut().next().unwrap().view = ViewId::from_raw(99),
            10 => state.borrow_mut().frontend.as_mut().unwrap().id = engine::InspectorId::from_raw(99),
            _ => unreachable!(),
        }
        assert!(!pages.resume_resize_focus_intent(snapshot.clone()), "mutation {mutation}");
        assert!(!pages.page_focus);
        assert!(!pages.devtools_focused);
        assert!(!pages.page_has_focus);
        assert!(!pages.restore_resize_focus(snapshot), "mutation {mutation}");
        assert!(state.borrow().calls.is_empty());
    }
}

#[test]
fn stale_resume_preserves_newer_page_intent() {
    let (mut pages, state, tab) = fixture(true);
    let snapshot = pages.capture_resize_focus();
    pages.request_ui_focus();
    pages.request_page_focus();
    pages.store.borrow_mut().pages.get_mut(&tab).unwrap().input_attachment += 1;
    assert!(!pages.resume_resize_focus_intent(snapshot));
    assert!(pages.page_focus);
    assert!(!pages.devtools_focused);
    assert!(!pages.page_has_focus);
    assert!(state.borrow().calls.is_empty());
}
