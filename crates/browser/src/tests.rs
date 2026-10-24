//! Drives the real window through gpui's test platform: keystrokes and
//! clicks go through the same key bindings, focus and hit-testing as the app.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use engine::{Bounds, Event, Shortcut, ViewId};
use gpui::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext};

use crate::pages::WebEngine;
use crate::window::{BrowserWindow, Suggestion};
use crate::{demo_browser, init};

fn open(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext) {
    open_with(cx, None)
}

fn open_with(
    cx: &mut TestAppContext,
    engine: Option<Box<dyn WebEngine>>,
) -> (Entity<BrowserWindow>, &mut VisualTestContext) {
    cx.update(init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(demo_browser(), engine, window, cx);
        if let Some(pages) = &view.pages {
            pages.borrow_mut().set_clock(cx.background_executor().clone());
        }
        view.focus(window);
        view
    });
    cx.run_until_parked();
    (view, cx)
}

fn another_profile_window(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> (Entity<BrowserWindow>, VisualTestContext) {
    let profile = view.read_with(cx, |view, _| view.profile.handle.clone());
    let handle = cx.cx.update(|cx| {
        use gpui::AppContext;
        cx.open_window(gpui::WindowOptions::default(), |window, cx| cx.new(|cx| {
            let view = BrowserWindow::with_profile(profile, None, None, window, cx);
            view.focus(window);
            view
        })).unwrap()
    });
    let view = handle.root(&mut cx.cx).unwrap();
    let visual = VisualTestContext::from_window(handle.into(), &cx.cx);
    visual.run_until_parked();
    (view, visual)
}

#[gpui::test]
fn shared_profile_windows_show_the_same_tabs_with_independent_selection(cx: &mut TestAppContext) {
    let (a, visual_a) = open(cx);
    visual_a.simulate_keystrokes("cmd-t");
    bar(&a, visual_a).simulate_input("https://first.example/window-a");
    bar(&a, visual_a).simulate_keystrokes("enter");
    let first = a.read_with(visual_a, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut visual_b) = another_profile_window(&a, visual_a);
    assert!(b.read_with(&visual_b, |view, _| view.browser.spaces.iter().all(|space| space.active_tab.is_none())));
    visual_b.simulate_keystrokes("cmd-t");
    bar(&b, &mut visual_b).simulate_input("https://second.example/window-b");
    bar(&b, &mut visual_b).simulate_keystrokes("enter");
    let second = b.read_with(&visual_b, |view, _| view.browser.active_tab().unwrap().id);
    assert_ne!(first, second);
    a.read_with(visual_a, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, first);
        assert!(view.browser.all_tabs().any(|tab| tab.id == second));
    });
    visual_b.simulate_keystrokes("cmd-d");
    a.read_with(visual_a, |view, _| {
        assert!(view.browser.space().pinned.iter().any(|tab| tab.id == second));
        assert_eq!(view.browser.active_tab().unwrap().id, first);
    });
    visual_b.simulate_keystrokes("cmd-l");
    bar(&b, &mut visual_b).simulate_input("https://second.example/changed");
    bar(&b, &mut visual_b).simulate_keystrokes("enter");
    assert!(a.read_with(visual_a, |view, _| view.browser.all_tabs().any(|tab| tab.id == second && tab.url == "https://second.example/changed")));
}

#[gpui::test]
fn shared_profile_space_reordering_preserves_each_windows_visible_space(cx: &mut TestAppContext) {
    let (a, visual_a) = open(cx);
    let (b, mut visual_b) = another_profile_window(&a, visual_a);
    visual_b.simulate_keystrokes("ctrl-2");
    visual_b.executor().advance_clock(Duration::from_secs(1));
    visual_b.run_until_parked();
    let work = b.read_with(&visual_b, |view, _| view.browser.space().id);
    a.update(visual_a, |view, _| view.edit_browser(|browser| browser.move_space(work, 0)));
    visual_a.run_until_parked();
    a.read_with(visual_a, |view, _| {
        assert_eq!(view.browser.space().name, "Personal");
        assert_eq!(view.pager.position, 2.);
    });
    b.read_with(&visual_b, |view, _| {
        assert_eq!(view.browser.space().name, "Work");
        assert_eq!(view.pager.position, 1.);
    });
}

#[gpui::test]
fn shared_profile_edits_do_not_overwrite_other_edits_waiting_for_notification(cx: &mut TestAppContext) {
    let (a, visual_a) = open(cx);
    let (b, visual_b) = another_profile_window(&a, visual_a);
    let before = a.read_with(visual_a, |view, _| view.browser.all_tabs().count());
    let (first, second) = visual_a.cx.update(|cx| {
        let first = a.update(cx, |view, _| view.edit_browser(|browser| browser.new_tab("https://first.example", std::time::SystemTime::now())));
        let second = b.update(cx, |view, _| view.edit_browser(|browser| browser.new_tab("https://second.example", std::time::SystemTime::now())));
        (first, second)
    });
    visual_a.run_until_parked();
    assert_ne!(first, second);
    for (view, selected) in [(&a, first), (&b, second)] {
        view.read_with(&visual_b, |view, _| {
            assert_eq!(view.browser.all_tabs().count(), before + 2);
            assert_eq!(view.browser.active_tab().unwrap().id, selected);
        });
    }
}

#[gpui::test]
fn shared_profile_keeps_one_session_writer_after_the_first_window_closes(cx: &mut TestAppContext) {
    let path = std::env::temp_dir().join(format!("echo-shared-profile-{}.json", std::process::id()));
    let competing = path.with_extension("competing.json");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&competing);
    let (a, visual_a) = open(cx);
    let (b, visual_b) = another_profile_window(&a, visual_a);
    a.update(visual_a, |view, cx| view.save_session_to(path.clone(), cx));
    b.update(visual_a, |view, cx| view.save_session_to(competing.clone(), cx));
    b.update(visual_a, |view, _| view.edit_browser(|browser| browser.new_tab("https://second.example/before-close", std::time::SystemTime::now())));
    visual_a.update(|window, _| window.remove_window());
    drop(a);
    visual_a.cx.update(|_| {});
    assert!(browser_core::session::load(&path).unwrap().unwrap().all_tabs().any(|tab| tab.url == "https://second.example/before-close"));
    b.update(visual_a, |view, _| view.edit_browser(|browser| browser.new_tab("https://second.example/after-close", std::time::SystemTime::now())));
    visual_b.executor().advance_clock(Duration::from_secs(2));
    visual_b.run_until_parked();
    assert!(browser_core::session::load(&path).unwrap().unwrap().all_tabs().any(|tab| tab.url == "https://second.example/after-close"));
    assert!(!competing.exists());
    std::fs::remove_file(path).unwrap();
}

#[gpui::test]
fn shared_profile_session_excludes_another_windows_uncommitted_split_draft(cx: &mut TestAppContext) {
    let (a, visual_a) = open(cx);
    visual_a.simulate_keystrokes("ctrl-shift-=");
    assert!(a.read_with(visual_a, |view, _| view.split_draft.is_some()));
    let (b, mut visual_b) = another_profile_window(&a, visual_a);
    visual_b.simulate_keystrokes("cmd-t");
    bar(&b, &mut visual_b).simulate_input("https://second.example/committed");
    bar(&b, &mut visual_b).simulate_keystrokes("enter");
    b.read_with(&visual_b, |view, _| {
        let restored = browser_core::session::from_json(&view.session_json()).unwrap();
        assert!(!restored.all_tabs().any(|tab| tab.url == crate::split::EMPTY_PANE_URL));
        assert!(restored.all_tabs().any(|tab| tab.url == "https://second.example/committed"));
        assert_eq!(restored.active_tab().unwrap().url, "https://second.example/committed");
    });
    bar(&a, visual_a).simulate_keystrokes("escape");
    assert!(b.read_with(&visual_b, |view, _| view.browser.active_tab().is_some_and(|tab| tab.url == "https://second.example/committed")));
}

#[gpui::test]
fn shared_profile_idle_archive_protects_tabs_selected_in_other_windows(cx: &mut TestAppContext) {
    let (a, visual_a) = open(cx);
    let first = a.read_with(visual_a, |view, _| view.browser.space().today[0].id);
    let (b, visual_b) = another_profile_window(&a, visual_a);
    let second = b.update(visual_a, |view, _| view.edit_browser(|browser| {
        let left = browser.new_tab("https://second.example/left", std::time::SystemTime::UNIX_EPOCH);
        let right = browser.new_tab("https://second.example/right", std::time::SystemTime::UNIX_EPOCH);
        browser.add_split(left, right, browser_core::splits::Placement::Right, std::time::SystemTime::UNIX_EPOCH).unwrap();
        right
    }));
    let (empty, _) = another_profile_window(&a, visual_a);
    empty.update(visual_a, |view, _| view.edit_browser(|browser| browser.active_user_profile_mut().archive_after = Some(Duration::ZERO)));
    visual_b.executor().advance_clock(Duration::from_secs(61));
    visual_b.run_until_parked();
    for view in [&a, &b] {
        view.read_with(&visual_b, |view, _| {
            assert!(view.browser.all_tabs().any(|tab| tab.id == first));
            assert!(view.browser.all_tabs().any(|tab| tab.id == second));
            assert!(!view.browser.archive.is_empty());
            assert_eq!(view.browser.split_for(second).unwrap().panes.len(), 2);
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Call {
    Create(u64, String),
    MoveHost(u64, usize, Bounds),
    Visible(u64, bool),
    TopCorners(u64, bool),
    FocusPage(u64),
    FocusUi,
    Navigate(u64, String),
    Back(u64),
    Forward(u64),
    Undo(u64),
    Redo(u64),
    ForceReload(u64),
    StopLoading(u64),
    Mute(u64, bool),
    Zoom(u64, i32),
    Find(u64, String, bool, bool),
    StopFinding(u64),
    Reload(u64),
    Print(u64),
    PasteStyle(u64),
    InspectorPasteStyle(engine::InspectorId),
    Close(u64),
    Snapshot(u64, u64),
    InspectorOpen(u64, usize, Bounds, engine::InspectorPanel, bool),
    InspectorFocus(engine::InspectorId),
    InspectorClose(engine::InspectorId),
}

/// Records what the window asks of the engine; `events` are returned by
/// the next pump, as if the page reported them.
#[derive(Default)]
struct FakeState {
    paste_busy: usize,
    paste_hook: Option<Box<dyn FnOnce()>>,
    print_busy: usize,
    print_hook: Option<Box<dyn FnOnce()>>,
    storage: std::collections::HashMap<u64, crate::pages::StorageSelection>,
    fork_storage: Vec<crate::pages::StorageSelection>,
    forks: Vec<FakeEngine>,
    calls: Vec<Call>,
    /// Latest bounds given to each view.
    bounds: std::collections::HashMap<u64, Bounds>,
    rounded_top: std::collections::HashMap<u64, bool>,
    next_view: u64,
    focused_page: Option<ViewId>,
    events: Vec<Event>,
    muted: std::collections::HashSet<u64>,
    focus_on_create: bool,
    reject_move: bool,
    reject_snapshot: bool,
    capture_supported: bool,
    inspector_supported: bool,
    inspector_accept: bool,
    next_inspector: u64,
    inspector: Option<engine::InspectorStatus>,
    inspector_events: Vec<engine::InspectorEvent>,
    inspector_bounds: Option<Bounds>,
    reject_create: bool,
    pending_storage: std::collections::HashSet<crate::pages::StorageSelection>,
    storage_attempts: Vec<crate::pages::StorageSelection>,
    identity_attempts: Vec<(crate::pages::PageIdentity, browser_core::SpaceId, crate::pages::StorageSelection, String)>,
    snapshots: Vec<engine::PageSnapshot>,
}

#[derive(Clone, Default)]
struct FakeEngine(Rc<RefCell<FakeState>>);

impl FakeEngine {
    fn shortcut(&self, shortcut: Shortcut) -> Event {
        Event::Shortcut { view: self.0.borrow().focused_page.expect("native shortcut source page"), shortcut }
    }
    fn take_calls(&self) -> Vec<Call> {
        std::mem::take(&mut self.0.borrow_mut().calls)
    }

    fn send(&self, cx: &mut VisualTestContext, event: Event) {
        if let Event::Focused { view } = &event { self.0.borrow_mut().focused_page = Some(*view); }
        self.0.borrow_mut().events.push(event);
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
    }

    fn call(&self, call: Call) {
        self.0.borrow_mut().calls.push(call);
    }
}

impl WebEngine for FakeEngine {
    fn prepare_paste_and_match_style(&mut self, view: ViewId) -> Option<crate::pages::NativePaste> {
        let fake = self.clone();
        Some(Box::new(move || {
            { let mut state = fake.0.borrow_mut();
                if state.paste_busy > 0 { state.paste_busy -= 1; return engine::PasteDispatch::Busy; }
            }
            let hook = fake.0.borrow_mut().paste_hook.take();
            if let Some(hook) = hook { hook(); }
            fake.call(Call::PasteStyle(raw(view)));
            engine::PasteDispatch::Submitted
        }))
    }
    fn prepare_inspector_paste_and_match_style(&mut self, id: engine::InspectorId) -> Option<crate::pages::NativePaste> {
        let fake = self.clone();
        Some(Box::new(move || { fake.call(Call::InspectorPasteStyle(id)); engine::PasteDispatch::Submitted }))
    }
    fn supports_printing(&self) -> bool { true }
    fn prepare_print(&mut self, view: ViewId) -> Option<crate::pages::NativePrint> {
        let state = self.0.clone();
        Some(Box::new(move || {
            {
                let mut state = state.borrow_mut();
                if state.print_busy > 0 { state.print_busy -= 1; return engine::PrintDispatch::Busy; }
            }
            let hook = state.borrow_mut().print_hook.take();
            if let Some(hook) = hook { hook(); }
            state.borrow_mut().calls.push(Call::Print(raw(view)));
            engine::PrintDispatch::Submitted
        }))
    }

    fn create_page_with_identity(&mut self, identity: crate::pages::PageIdentity, source_space: browser_core::SpaceId,
        storage: &crate::pages::StorageSelection, parent: *mut std::ffi::c_void, url: &str, bounds: Bounds) -> crate::pages::PageCreation {
        self.0.borrow_mut().identity_attempts.push((identity, source_space, storage.clone(), url.into()));
        self.create_page_in_storage(storage, parent, url, bounds)
    }
    fn create_page_in_storage(&mut self, storage: &crate::pages::StorageSelection, parent: *mut std::ffi::c_void, url: &str, bounds: Bounds) -> crate::pages::PageCreation {
        if self.0.borrow().pending_storage.contains(storage) {
            self.0.borrow_mut().storage_attempts.push(storage.clone());
            return crate::pages::PageCreation::Pending;
        }
        self.create_in_storage(storage, parent, url, bounds).map_or(crate::pages::PageCreation::Failed, crate::pages::PageCreation::Created)
    }
    fn fork_in_storage(&self, storage: &crate::pages::StorageSelection) -> Option<Box<dyn WebEngine>> {
        self.0.borrow_mut().fork_storage.push(storage.clone());
        self.fork_with_shared_storage()
    }
    fn create_in_storage(&mut self, storage: &crate::pages::StorageSelection, _parent: *mut std::ffi::c_void, url: &str, bounds: Bounds) -> Option<ViewId> {
        {
            let mut state = self.0.borrow_mut();
            state.storage_attempts.push(storage.clone());
            if state.pending_storage.contains(storage) { return None; }
        }
        let view = self.create(url, bounds)?;
        self.0.borrow_mut().storage.insert(raw(view), storage.clone());
        Some(view)
    }
    fn fork_with_shared_storage(&self) -> Option<Box<dyn WebEngine>> {
        let child = Self::default();
        self.0.borrow_mut().forks.push(child.clone());
        Some(Box::new(child))
    }
    fn supports_inspector(&self) -> bool { self.0.borrow().inspector_supported }
    fn open_inspector(&mut self, view: ViewId, parent: *mut std::ffi::c_void, bounds: Bounds, panel: engine::InspectorPanel, inspect: bool) -> Result<engine::InspectorId, String> {
        self.call(Call::InspectorOpen(raw(view), parent as usize, bounds, panel, inspect));
        let mut state = self.0.borrow_mut();
        if !state.inspector_accept { return Err("Recording engine rejected inspector creation".into()); }
        let id = match &state.inspector {
            Some(status) if status.inspected == view => status.id,
            _ => { state.next_inspector += 1; engine::InspectorId::from_raw(state.next_inspector) }
        };
        state.inspector = Some(engine::InspectorStatus {
            id, inspected: view, generation: state.next_inspector, panel,
            selected_panel: Some(panel), phase: engine::InspectorPhase::FrontendReady,
            url: engine::INSPECTOR_FRONTEND_URL.into(),
        });
        state.inspector_bounds = Some(bounds);
        Ok(id)
    }

    fn inspector_status(&self, id: engine::InspectorId) -> Option<engine::InspectorStatus> {
        self.0.borrow().inspector.clone().filter(|status| status.id == id)
    }
    fn focus_inspector(&mut self, id: engine::InspectorId) { self.call(Call::InspectorFocus(id)); }
    fn set_inspector_frame(&mut self, _id: engine::InspectorId, bounds: Bounds) { self.0.borrow_mut().inspector_bounds = Some(bounds); }
    fn take_inspector_events(&mut self) -> Vec<engine::InspectorEvent> { std::mem::take(&mut self.0.borrow_mut().inspector_events) }
    fn close_inspector(&mut self, id: engine::InspectorId) {
        self.call(Call::InspectorClose(id));
        let mut state = self.0.borrow_mut();
        if state.inspector.as_ref().is_some_and(|status| status.id == id) { state.inspector = None; state.inspector_bounds = None; }
    }
    fn supports_snapshots(&self) -> bool { self.0.borrow().capture_supported }
    fn request_snapshot(&mut self, view: ViewId, request: u64) -> bool {
        self.call(Call::Snapshot(raw(view), request));
        !self.0.borrow().reject_snapshot
    }
    fn take_snapshots(&mut self) -> Vec<engine::PageSnapshot> { std::mem::take(&mut self.0.borrow_mut().snapshots) }
    fn move_view(&mut self, view: ViewId, parent: *mut std::ffi::c_void, bounds: Bounds) -> bool {
        let mut state = self.0.borrow_mut();
        state.calls.push(Call::MoveHost(raw(view), parent as usize, bounds));
        if state.reject_move { return false; }
        state.bounds.insert(raw(view), bounds);
        true
    }
    fn undo(&mut self, view: ViewId) { self.call(Call::Undo(raw(view))); }
    fn redo(&mut self, view: ViewId) { self.call(Call::Redo(raw(view))); }
    fn create(&mut self, url: &str, bounds: Bounds) -> Option<ViewId> {
        let mut state = self.0.borrow_mut();
        if state.reject_create { return None; }
        state.next_view += 1;
        let id = state.next_view;
        state.bounds.insert(id, bounds);
        state.rounded_top.insert(id, true);
        state.calls.push(Call::Create(id, url.to_string()));
        if state.focus_on_create { state.events.push(Event::Focused { view: ViewId::from_raw(id) }); }
        Some(ViewId::from_raw(id))
    }
    fn set_bounds(&mut self, view: ViewId, bounds: Bounds) {
        self.0.borrow_mut().bounds.insert(raw(view), bounds);
    }
    fn set_top_corners_rounded(&mut self, view: ViewId, rounded: bool) {
        let mut state = self.0.borrow_mut();
        if state.rounded_top.insert(raw(view), rounded) != Some(rounded) {
            state.calls.push(Call::TopCorners(raw(view), rounded));
        }
    }
    fn set_visible(&mut self, view: ViewId, visible: bool) {
        self.call(Call::Visible(raw(view), visible));
    }
    fn focus_page(&mut self, view: ViewId) {
        self.0.borrow_mut().focused_page = Some(view);
        self.call(Call::FocusPage(raw(view)));
        let mut state = self.0.borrow_mut();
        if state.focus_on_create { state.events.push(Event::Focused { view }); }
    }
    fn focus_ui(&mut self) {
        self.call(Call::FocusUi);
    }
    fn navigate(&mut self, view: ViewId, url: &str) {
        self.call(Call::Navigate(raw(view), url.to_string()));
    }
    fn go_back(&mut self, view: ViewId) {
        self.call(Call::Back(raw(view)));
    }
    fn go_forward(&mut self, view: ViewId) { self.call(Call::Forward(raw(view))); }
    fn reload(&mut self, view: ViewId) {
        self.call(Call::Reload(raw(view)));
    }
    fn force_reload(&mut self, view: ViewId) { self.call(Call::ForceReload(raw(view))); }
    fn stop_loading(&mut self, view: ViewId) { self.call(Call::StopLoading(raw(view))); }
    fn set_audio_muted(&mut self, view: ViewId, muted: bool) {
        if muted { self.0.borrow_mut().muted.insert(raw(view)); } else { self.0.borrow_mut().muted.remove(&raw(view)); }
        self.call(Call::Mute(raw(view), muted));
    }
    fn audio_muted(&self, view: ViewId) -> Option<bool> { Some(self.0.borrow().muted.contains(&raw(view))) }
    fn zoom(&mut self, view: ViewId, direction: i32) { self.call(Call::Zoom(raw(view), direction)); }
    fn find(&mut self, view: ViewId, text: &str, forward: bool, next: bool) { self.call(Call::Find(raw(view), text.into(), forward, next)); }
    fn stop_finding(&mut self, view: ViewId) { self.call(Call::StopFinding(raw(view))); }
    fn close(&mut self, view: ViewId) {
        self.call(Call::Close(raw(view)));
    }
    fn pump_fn(&self) -> fn() {
        || {}
    }
    fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.0.borrow_mut().events)
    }
}

fn raw(view: ViewId) -> u64 {
    format!("{view:?}")
        .trim_start_matches("ViewId(")
        .trim_end_matches(')')
        .parse()
        .unwrap()
}

fn page(id: u64) -> ViewId {
    ViewId::from_raw(id)
}

// GPUI's test platform requires static selectors, including generated tab ids.
fn test_selector(selector: String) -> &'static str { Box::leak(selector.into_boxed_str()) }

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("`{selector}` is not on screen"));
    // Hover first, like a real pointer: some controls only appear on hover.
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// The Space bar plus opens the New menu; New Space opens the sidebar form.
fn open_new_space_from_menu(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    click(cx, "new-space");
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| w.new_menu.as_ref().and_then(|menu| menu.overlay)).expect("new menu is open");
    click(&mut VisualTestContext::from_window(overlay.into(), cx), "new-menu-space");
}

/// Creation lives in the main window; wait for its sidebar page to settle
/// before hitting form controls. Typing below also exercises deferred name focus.
fn assert_space_creation_form(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert!(w.command_bar.is_none());
        assert!(w.new_menu.is_none());
        assert!(w.sidebar_visible);
        assert_eq!(w.sidebar_navigation.controller.selected(), crate::sidebar_pages::SidebarPage::CreateSpace);
        assert!(w.sidebar_navigation.form.is_some());
    });
    assert!(cx.debug_bounds("space-creation-form").is_some());
    assert!(cx.debug_bounds("space-creation-name").is_some());
}

fn today_urls(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |w, _| {
        w.browser.space().today.iter().map(|t| t.url.clone()).collect()
    })
}

fn active_url(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |w, _| w.browser.active_tab().map(|t| t.url.clone()))
}

fn bar_text(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |w, cx| {
        w.command_bar
            .as_ref()
            .map(|bar| bar.input.read(cx).text().to_string())
    })
}

/// The command bar's overlay window, for typing and clicking into the bar.
fn bar(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay = view
        .read_with(cx, |w, _| w.command_bar.as_ref().and_then(|bar| bar.overlay))
        .expect("command bar overlay is not open");
    VisualTestContext::from_window(overlay.into(), cx)
}

fn bar_input_focused(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> bool {
    bar(view, cx).update(|window, cx| {
        let w = view.read(cx);
        let bar = w.command_bar.as_ref().expect("command bar is closed");
        bar.input.read(cx).focus_handle(cx).is_focused(window)
    })
}

#[gpui::test]
fn cmd_t_opens_command_bar_instead_of_a_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);

    cx.simulate_keystrokes("cmd-t");

    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    assert_eq!(today_urls(&view, cx), before);
}

#[gpui::test]
fn command_bar_domain_completion_can_be_overwritten_or_deleted(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("wiki");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("wikipedia.org"));
    bar(&view, cx).simulate_input("x");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("wikix"));
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("wiki");
    bar(&view, cx).simulate_keystrokes("backspace");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("wiki"));
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.google.com/search?q=wiki"));
}

#[gpui::test]
fn command_bar_tab_and_space_scope_wikipedia_and_backspace_unscopes(cx: &mut TestAppContext) {
    for separator in ["tab", "space"] {
        let (view, cx) = open(cx);
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input("wikipedia");
        if separator == "space" { bar(&view, cx).simulate_input(" "); }
        else { bar(&view, cx).simulate_keystrokes("tab"); }
        assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
        assert!(view.read_with(cx, |view, _| view.command_bar.as_ref().unwrap().site_search.is_some()));
        assert!(bar(&view, cx).debug_bounds("site-search-pill").is_some());
        bar(&view, cx).simulate_input("rust language");
        let rows = view.read_with(cx, |view, cx| view.suggestions(cx));
        assert_eq!(rows.len(), 1);
        bar(&view, cx).simulate_keystrokes("enter");
        assert_eq!(active_url(&view, cx).as_deref(), Some("https://en.wikipedia.org/w/index.php?search=rust+language"));
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input("wikipedia ");
        bar(&view, cx).simulate_keystrokes("backspace");
        assert!(view.read_with(cx, |view, _| view.command_bar.as_ref().unwrap().site_search.is_none()));
    }
}

#[gpui::test]
fn command_bar_calculator_copies_answer_without_creating_a_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("(2+3)*4");
    assert!(view.read_with(cx, |view, cx| matches!(view.suggestions(cx).first(), Some(Suggestion::Calculator { answer }) if answer == "20")));
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(today_urls(&view, cx), before);
    assert_eq!(cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())).as_deref(), Some("20"));
    assert!(view.read_with(cx, |view, _| view.command_bar.is_none()));
}

#[gpui::test]
fn command_bar_uses_profile_engine_and_remote_rows_are_selectable(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, _| view.edit_browser(|browser| {
        browser.active_user_profile_mut().settings.search.engine = browser_core::omnibox::SearchEngine::DuckDuckGo;
        browser.active_user_profile_mut().settings.search.suggestions = false;
    }));
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("rust language");
    view.update(cx, |view, cx| {
        view.edit_browser(|browser| browser.active_user_profile_mut().settings.search.suggestions = true);
        let bar = view.command_bar.as_mut().unwrap();
        assert!(bar.remote_task.is_none());
        bar.remote = vec!["rust language tutorial".into()];
        cx.notify();
    });
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://duckduckgo.com/?q=rust+language+tutorial"));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("another query");
    assert!(view.read_with(cx, |view, _| view.command_bar.as_ref().unwrap().remote.is_empty()));
}

#[gpui::test]
fn command_bar_empty_field_shows_five_rows_and_privacy_disclosure_dismisses(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    assert_eq!(view.read_with(cx, |view, cx| view.suggestions(cx).len()), 5);
    click(&mut bar(&view, cx), "suggestions-info");
    assert!(bar(&view, cx).debug_bounds("suggestions-privacy").is_some());
    click(&mut bar(&view, cx), "dismiss-suggestions-privacy");
    assert!(!view.read_with(cx, |view, _| view.command_bar.as_ref().unwrap().suggestions_privacy));
    assert!(view.read_with(cx, |view, _| view.browser.active_user_profile().settings.search.notice_dismissed));
    assert!(bar_input_focused(&view, cx));
}

#[gpui::test]
fn typing_an_address_and_enter_opens_it_in_a_new_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let count = today_urls(&view, cx).len();

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("github.com/zed-industries/zed");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("github.com/zed-industries/zed"));
    bar(&view, cx).simulate_keystrokes("enter");

    assert_eq!(bar_text(&view, cx), None);
    let urls = today_urls(&view, cx);
    assert_eq!(urls.len(), count + 1);
    assert_eq!(urls[0], "https://github.com/zed-industries/zed");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com/zed-industries/zed"));
}

#[gpui::test]
fn typing_words_searches_google(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("как дела");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://www.google.com/search?q=%D0%BA%D0%B0%D0%BA+%D0%B4%D0%B5%D0%BB%D0%B0")
    );
    let title = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone());
    assert_eq!(title, "как дела");
}

#[gpui::test]
fn backspace_and_arrows_edit_the_text(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("helo");
    bar(&view, cx).simulate_keystrokes("left");
    bar(&view, cx).simulate_input("l");
    bar(&view, cx).simulate_keystrokes("end backspace backspace");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("hel"));
}

#[gpui::test]
fn escape_closes_without_opening_a_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("something");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(today_urls(&view, cx), before);

    // Window shortcuts work again once the bar is gone.
    cx.simulate_keystrokes("cmd-w");
    assert_eq!(today_urls(&view, cx).len(), before.len() - 1);
}

#[gpui::test]
fn arrow_down_and_enter_switch_to_a_matching_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("gpui");

    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(matches!(suggestions[0], Suggestion::Load(_)));
    assert!(matches!(&suggestions[1], Suggestion::SwitchToTab { url, .. } if url == "https://www.gpui.rs"));

    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    assert_eq!(today_urls(&view, cx), before);
}

#[gpui::test]
fn cmd_l_edits_the_current_tab_url(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx).len();
    let current = active_url(&view, cx).unwrap();

    cx.simulate_keystrokes("cmd-l");
    assert_eq!(bar_text(&view, cx), Some(current));
    // The URL is selected, so typing replaces it.
    bar(&view, cx).simulate_input("example.com");
    bar(&view, cx).simulate_keystrokes("enter");

    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com"));
    assert_eq!(today_urls(&view, cx).len(), before);
}

#[gpui::test]
fn clicking_new_tab_opens_command_bar(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "new-tab");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("rust");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://www.google.com/search?q=rust")
    );
}

#[gpui::test]
fn clicking_a_suggestion_runs_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("chromium");
    click(&mut bar(&view, cx), "suggestion-1");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn clicking_outside_dismisses_but_inside_does_not(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    click(&mut bar(&view, cx), "command-bar-input");
    assert!(bar_text(&view, cx).is_some());
    assert!(bar_input_focused(&view, cx));

    let mut overlay = bar(&view, cx);
    let backdrop = overlay.debug_bounds("command-bar-backdrop").unwrap();
    overlay.simulate_click(
        backdrop.bottom_right() - gpui::point(gpui::px(20.), gpui::px(20.)),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn clicking_tabs_activates_and_closes_them(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (first, last) = view.read_with(cx, |w, _| {
        let today = &w.browser.space().today;
        (today[0].id.get(), today.last().unwrap().id.get())
    });
    let tab: &'static str = Box::leak(format!("tab-{last}").into_boxed_str());
    click(cx, tab);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), last);

    let before = today_urls(&view, cx).len();
    let close: &'static str = Box::leak(format!("close-{first}").into_boxed_str());
    click(cx, close);
    assert_eq!(today_urls(&view, cx).len(), before - 1);
}

#[gpui::test]
fn space_shortcuts_and_dots_switch_spaces(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-2");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 1);
    click(cx, "space-2");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 2);
    cx.simulate_keystrokes("cmd-alt-right");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
    cx.simulate_keystrokes("cmd-alt-left");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 2);
}

#[gpui::test]
fn cmd_alt_arrows_and_cmd_digits_pick_tabs(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    // Sidebar: GitHub, Gmail | Chromium, GPUI, Arc is shutting down.
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com"));
    cx.simulate_keystrokes("cmd-alt-down");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://mail.google.com"));
    cx.simulate_keystrokes("cmd-alt-up cmd-alt-up");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://thebrowser.company"));
    cx.simulate_keystrokes("cmd-3");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    cx.simulate_keystrokes("cmd-9");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://thebrowser.company"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
}

#[gpui::test]
fn cmd_w_archives_and_cmd_shift_t_restores(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    let closed = active_url(&view, cx).unwrap();
    cx.simulate_keystrokes("cmd-w");
    assert!(!today_urls(&view, cx).contains(&closed));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);

    cx.simulate_keystrokes("cmd-shift-t");
    assert_eq!(active_url(&view, cx), Some(closed));
    assert_eq!(today_urls(&view, cx).len(), before.len());
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 0);
}

#[gpui::test]
fn cmd_shift_k_clears_background_today_and_preserves_the_selected_page(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let count = today_urls(&view, cx).len();
    let active = active_url(&view, cx);
    cx.simulate_keystrokes("cmd-shift-k");
    assert_eq!(today_urls(&view, cx), vec![active.clone().unwrap()]);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), count - 1);
    assert_eq!(active_url(&view, cx), active);
}

#[gpui::test]
fn closing_a_pinned_tab_keeps_it_pinned(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let pinned = view.read_with(cx, |w, _| w.browser.space().pinned[0].id.get());
    let close: &'static str = Box::leak(format!("close-{pinned}").into_boxed_str());
    click(cx, close);
    let urls: Vec<String> = view.read_with(cx, |w, _| {
        w.browser.space().pinned.iter().map(|t| t.url.clone()).collect()
    });
    assert_eq!(urls, vec!["https://github.com", "https://mail.google.com"]);
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
}

#[gpui::test]
fn the_session_is_saved_and_restored(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("browser-session-test-{}", std::process::id()));
    let path = dir.join("session.json");
    let _ = std::fs::remove_dir_all(&dir);

    let (view, cx) = open(cx);
    view.update(cx, |w, cx| w.save_session_to(path.clone(), cx));
    cx.simulate_keystrokes("ctrl-2");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();

    let restored = browser_core::session::load(&path).unwrap().expect("session saved");
    assert_eq!(restored.active_space, 1);
    assert_eq!(restored.spaces.len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn cmd_s_hides_the_sidebar(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    assert!(!view.read_with(cx, |w, _| w.sidebar_visible));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("sidebar-clip").unwrap().size.width, gpui::px(0.));
}

// Web pages, through the fake engine.

fn open_pages(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    let fake = FakeEngine::default();
    let (view, cx) = open_with(cx, Some(Box::new(fake.clone())));
    (view, cx, fake)
}

#[gpui::test]
fn the_active_tab_gets_a_visible_focused_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let url = active_url(&view, cx).unwrap();
    assert_eq!(
        fake.take_calls(),
        vec![Call::Create(1, url), Call::Visible(1, true), Call::FocusPage(1)]
    );
}

fn active_page_status(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |w, _| {
        w.pages.as_ref().unwrap().borrow().state(w.browser.active_tab().unwrap().id).status
    })
}

#[gpui::test]
fn hovered_link_events_preserve_the_exact_target_and_do_not_change_page_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    let target = "https://example.com/a?utm_source=hover&token=a%2Fb#section";
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    assert_eq!(active_page_status(&view, cx), target);
    assert!(fake.take_calls().is_empty());
    fake.send(cx, Event::StatusChanged { view: page(1), text: String::new() });
    assert!(active_page_status(&view, cx).is_empty());
    assert!(fake.take_calls().is_empty());
}

#[gpui::test]
fn hovered_links_are_cleared_on_navigation_and_cannot_leak_from_hidden_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let target = "https://example.com/target";
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    fake.send(cx, Event::LoadingChanged { view: page(1), loading: true, can_go_back: false, can_go_forward: false });
    assert!(active_page_status(&view, cx).is_empty());
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://example.com/new".into() });
    assert!(active_page_status(&view, cx).is_empty());
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    click(cx, "tab-7");
    assert!(active_page_status(&view, cx).is_empty());
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    fake.send(cx, Event::StatusChanged { view: page(999), text: target.into() });
    assert!(active_page_status(&view, cx).is_empty());
    click(cx, "tab-8");
    assert!(active_page_status(&view, cx).is_empty(), "returning to a tab needs a fresh pointer event");
}

#[gpui::test]
fn link_status_expands_after_a_dwell_and_restarts_the_timer_for_a_new_target(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    let target = format!("https://example.com/{}", "long-path/".repeat(50));
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.clone() });
    let overlay = view.read_with(cx, |w, _| w.link_status.as_ref().unwrap().overlay.unwrap());
    let frame = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_bounds()[0]);
    let mut status_window = VisualTestContext::from_window(overlay.into(), cx);
    let preview = status_window.debug_bounds("link-status").unwrap();
    assert!(preview.size.width <= gpui::px(360.));
    assert!(preview.origin.x >= gpui::px(frame.x as f32) && preview.bottom_right().y <= gpui::px((frame.y + frame.height) as f32), "{preview:?}, page {frame:?}");
    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.run_until_parked();
    fake.send(cx, Event::StatusChanged { view: page(1), text: format!("{target}next") });
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(!view.read_with(cx, |w, _| w.link_status.as_ref().unwrap().expanded));
    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.link_status.as_ref().unwrap().expanded));
    let expanded = status_window.debug_bounds("link-status").unwrap();
    assert!(expanded.size.width > preview.size.width);
    assert!(expanded.bottom_right().x <= gpui::px((frame.x + frame.width) as f32));
    assert!(fake.take_calls().is_empty(), "passive status must not affect the page or its focus");
    cx.simulate_keystrokes("cmd-t");
    assert!(view.read_with(cx, |w, _| w.link_status.is_none()));
    assert!(bar_input_focused(&view, cx));
    fake.send(cx, Event::StatusChanged { view: page(1), text: target });
    assert!(view.read_with(cx, |w, _| w.link_status.is_none()));
}

#[gpui::test]
fn link_status_uses_the_hovered_split_pane_without_activating_it(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let active = active_url(&view, cx);
    fake.take_calls();
    fake.send(cx, Event::StatusChanged { view: page(1), text: "https://example.com/left".into() });
    assert_eq!(active_url(&view, cx), active);
    let overlay = view.read_with(cx, |w, _| w.link_status.as_ref().unwrap().overlay.unwrap());
    let mut status_window = VisualTestContext::from_window(overlay.into(), cx);
    let bounds = status_window.debug_bounds("link-status").unwrap();
    let first = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_bounds()[0]);
    assert!(bounds.origin.x >= gpui::px(first.x as f32) && bounds.bottom_right().x <= gpui::px((first.x + first.width) as f32));
    fake.send(cx, Event::StatusChanged { view: page(2), text: String::new() });
    assert!(view.read_with(cx, |w, _| w.link_status.is_some()), "another pane's empty status cannot clear this target");
    assert!(fake.take_calls().is_empty());
    cx.simulate_mouse_move(gpui::point(gpui::px(30.), gpui::px(200.)), None, Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.link_status.is_none()));
}

#[gpui::test]
fn the_page_stays_visible_under_the_command_bar(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();

    cx.simulate_keystrokes("cmd-t");
    assert!(bar_input_focused(&view, cx));
    assert_eq!(fake.take_calls(), vec![], "the page is neither hidden nor unfocused");

    bar(&view, cx).simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(fake.take_calls(), vec![Call::FocusPage(1)]);
}

#[gpui::test]
fn opening_a_url_creates_a_page_for_the_new_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("zed.dev");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();

    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Create(2, "https://zed.dev".into())), "{calls:?}");
    assert_eq!(calls.last(), Some(&Call::FocusPage(2)));
}

#[gpui::test]
fn cmd_l_loads_the_new_url_in_the_same_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();

    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("example.com");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();

    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Navigate(1, "https://example.com".into())), "{calls:?}");
    assert!(!calls.iter().any(|c| matches!(c, Call::Create(..))), "{calls:?}");
}

#[gpui::test]
fn switching_tabs_swaps_pages_and_closing_destroys_them(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    let (first, last, last_url) = view.read_with(cx, |w, _| {
        let today = &w.browser.space().today;
        let last = today.last().unwrap();
        (today[0].id.get(), last.id.get(), last.url.clone())
    });

    let tab: &'static str = Box::leak(format!("tab-{last}").into_boxed_str());
    click(cx, tab);
    assert_eq!(
        fake.take_calls(),
        vec![
            Call::Visible(1, false),
            Call::FocusUi,
            Call::Create(2, last_url),
            Call::Visible(2, true),
            Call::FocusPage(2),
        ]
    );

    let close: &'static str = Box::leak(format!("close-{first}").into_boxed_str());
    click(cx, close);
    assert_eq!(fake.take_calls(), vec![Call::Close(1)]);
}

#[gpui::test]
fn closing_a_pinned_tab_unloads_its_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-1");
    cx.run_until_parked();
    fake.take_calls();

    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(2)), "{calls:?}");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned.len()), 2);

    // Focus moved to the next pinned tab (page 3); reopening GitHub loads
    // its pinned page again.
    cx.simulate_keystrokes("cmd-1");
    cx.run_until_parked();
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Create(4, "https://github.com".into())), "{calls:?}");
}

#[gpui::test]
fn page_reports_update_the_sidebar_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();

    fake.send(cx, Event::TitleChanged { view: page(1), title: "Chromium Home".into() });
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://www.chromium.org/home".into() });

    let tab = view.read_with(cx, |w, _| w.browser.active_tab().cloned().unwrap());
    assert_eq!(tab.title, "Chromium Home");
    assert_eq!(tab.url, "https://www.chromium.org/home");
    assert_eq!(fake.take_calls(), vec![], "redirects must not trigger a reload");
}

#[gpui::test]
fn popups_open_as_new_tabs(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    fake.send(cx, Event::OpenInNewTab { opener: page(1), url: "https://docs.rs".into() });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://docs.rs"));
    assert!(fake.take_calls().contains(&Call::Create(2, "https://docs.rs".into())));
}

#[gpui::test]
fn shortcuts_pressed_inside_a_page_reach_the_browser(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let shortcut = |key: &str| Shortcut { cmd: true, key: key.into(), ..Default::default() };

    fake.send(cx, fake.shortcut(shortcut("t")));
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_keystrokes("escape");
    fake.take_calls();

    fake.send(cx, fake.shortcut(shortcut("[")));
    fake.send(cx, fake.shortcut(shortcut("r")));
    assert_eq!(fake.take_calls(), vec![Call::Back(1), Call::Reload(1)]);
}

#[test]
fn every_window_shortcut_is_known_to_the_engine() {
    let shortcuts = crate::window::page_shortcuts();
    assert_eq!(shortcuts.len(), crate::window::WINDOW_SHORTCUTS.len());
    assert!(shortcuts.contains(&Shortcut { cmd: true, key: "t".into(), ..Default::default() }));
    assert!(shortcuts.contains(&Shortcut { cmd: true, alt: true, key: "down".into(), ..Default::default() }));
    assert!(shortcuts.contains(&Shortcut { ctrl: true, key: "1".into(), ..Default::default() }));
    assert!(shortcuts.contains(&Shortcut { ctrl: true, shift: true, key: "tab".into(), ..Default::default() }));
}

#[gpui::test]
fn long_titles_do_not_widen_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let title = "A very long page title that is much wider than the window ".repeat(20);
    fake.send(cx, Event::TitleChanged { view: page(1), title });
    cx.run_until_parked();

    let window_width = f64::from(f32::from(cx.update(|window, _| window.viewport_size().width)));
    let bounds = fake.0.borrow().bounds[&1];
    let sidebar = view.read_with(cx, |w, _| w.sidebar_visible);
    assert!(sidebar);
    let sidebar_width = f64::from(f32::from(cx.debug_bounds("sidebar").unwrap().size.width));
    assert_eq!(
        (bounds.x, bounds.y, bounds.width),
        (sidebar_width, 10.0, window_width - sidebar_width - 10.0),
        "page must exactly cover the content card"
    );
}

#[gpui::test]
fn ctrl_tab_cycles_through_tabs(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let order: Vec<String> = view.read_with(cx, |w, _| {
        let space = w.browser.space();
        space.pinned.iter().chain(&space.today).map(|t| t.url.clone()).collect()
    });
    let start = order.iter().position(|u| Some(u) == active_url(&view, cx).as_ref()).unwrap();

    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_url(&view, cx), Some(order[(start + 1) % order.len()].clone()));
    cx.simulate_keystrokes("ctrl-shift-tab ctrl-shift-tab");
    assert_eq!(active_url(&view, cx), Some(order[(start + order.len() - 1) % order.len()].clone()));
}

fn swipe(cx: &mut VisualTestContext, over: &'static str, dx: f32, steps: usize) {
    use gpui::{point, px, ScrollDelta, ScrollWheelEvent, TouchPhase};
    cx.run_until_parked();
    let position = cx.debug_bounds(over).unwrap().center();
    for i in 0..=steps {
        let touch_phase = match i {
            0 => TouchPhase::Started,
            i if i == steps => TouchPhase::Ended,
            _ => TouchPhase::Moved,
        };
        let delta = if i == 0 { 0.0 } else { dx / steps as f32 };
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(delta), px(0.5))),
            modifiers: Modifiers::none(),
            touch_phase,
        });
    }
    // Momentum after the fingers lift must not switch again.
    for _ in 0..5 {
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(dx / steps as f32), px(0.))),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
    }
    cx.run_until_parked();
}

#[gpui::test]
fn swiping_the_sidebar_switches_one_space_per_gesture(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let space = |cx: &mut VisualTestContext| view.read_with(cx, |w, _| w.browser.active_space);

    swipe(cx, "sidebar", -200.0, 10);
    assert_eq!(space(cx), 1);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    // Also over the tab list, which scrolls vertically.
    swipe(cx, "new-tab", -200.0, 10);
    assert_eq!(space(cx), 2);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    swipe(cx, "sidebar", -200.0, 10);
    assert_eq!(space(cx), 2, "the creation page retains the last real Space");
    assert_space_creation_form(&view, cx);
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces.len()), 3);
    swipe(cx, "space-creation-form", 200.0, 10);
    assert_eq!(space(cx), 2);
    assert!(view.read_with(cx, |w, _| w.sidebar_navigation.controller.selected() == crate::sidebar_pages::SidebarPage::Space(w.browser.space().id)));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    swipe(cx, "sidebar", 200.0, 10);
    assert_eq!(space(cx), 1);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    // A short or mostly vertical gesture does nothing.
    swipe(cx, "sidebar", -30.0, 3);
    assert_eq!(space(cx), 1);
}

fn scroll(cx: &mut VisualTestContext, over: &'static str, phase: gpui::TouchPhase, dx: f32, dy: f32) {
    use gpui::{point, px, ScrollDelta, ScrollWheelEvent};
    let position = cx.debug_bounds(over).unwrap().center();
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
        modifiers: Modifiers::none(),
        touch_phase: phase,
    });
}

#[gpui::test]
fn the_sidebar_follows_the_fingers_and_settles_on_release(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    cx.run_until_parked();
    let state = |cx: &mut VisualTestContext| view.read_with(cx, |w, _| (w.pager.position, w.browser.active_space));

    scroll(cx, "sidebar", Started, 0.0, 0.0);
    for _ in 0..6 {
        scroll(cx, "sidebar", Moved, -20.0, 0.0);
    }
    // Halfway through 120 of 240 px: between spaces, nothing switched yet.
    let (position, active) = state(cx);
    assert!((position - 1.5).abs() < 0.01, "{position}");
    assert_eq!(active, 0);

    scroll(cx, "sidebar", Ended, 0.0, 0.0);
    assert_eq!(state(cx).1, 1);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(state(cx), (2.0, 1));
}

#[gpui::test]
fn a_small_drag_springs_back(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    cx.run_until_parked();
    scroll(cx, "sidebar", Started, 0.0, 0.0);
    scroll(cx, "sidebar", Moved, -30.0, 0.0);
    scroll(cx, "sidebar", Ended, 0.0, 0.0);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| (w.pager.position, w.browser.active_space)), (1.0, 0));
}

#[gpui::test]
fn a_fast_short_sidebar_flick_commits_by_velocity(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    scroll(cx, "sidebar", Started, 0., 0.);
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(15));
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(15));
    scroll(cx, "sidebar", Moved, -10., 0.);
    // 30px is below the distance threshold, but 667px/s is a flick.
    scroll(cx, "sidebar", Ended, 0., 0.);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 1);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.pager.position), 2.);
}

#[gpui::test]
fn pausing_before_releasing_a_short_sidebar_drag_cancels_its_flick_velocity(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    scroll(cx, "sidebar", Started, 0., 0.);
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(15));
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(15));
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(60));
    scroll(cx, "sidebar", Ended, 0., 0.);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.pager.position), 1.);
}

#[gpui::test]
fn dragging_past_the_ends_resists(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-3");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    open_new_space_from_menu(&view, cx);
    assert_space_creation_form(&view, cx);
    let edge = view.read_with(cx, |w, _| w.sidebar_page_index() as f32);
    scroll(cx, "space-creation-form", Started, 0.0, 0.0);
    scroll(cx, "space-creation-form", Moved, -120.0, 0.0);
    let position = view.read_with(cx, |w, _| w.pager.position);
    assert!((position - edge - 0.15).abs() < 0.01, "{position}");
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces.len()), 3);
}

#[gpui::test]
fn vertical_scrolling_does_not_page(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    cx.run_until_parked();
    scroll(cx, "sidebar", Started, 0.0, 0.0);
    scroll(cx, "sidebar", Moved, -2.0, 10.0);
    scroll(cx, "sidebar", Moved, -200.0, 0.0);
    scroll(cx, "sidebar", Ended, 0.0, 0.0);
    assert_eq!(view.read_with(cx, |w, _| (w.pager.position, w.browser.active_space)), (1.0, 0));
}

#[gpui::test]
fn keyboard_space_switches_animate_the_pager(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-3");
    let mid = view.read_with(cx, |w, _| w.pager.position);
    assert!((1.0..3.0).contains(&mid), "starts animating from the old space: {mid}");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.pager.position), 3.0);
}

#[gpui::test]
fn long_input_scrolls_inside_the_command_bar(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    let mut overlay = bar(&view, cx);
    overlay.simulate_input(&"sdfg".repeat(60));
    overlay.run_until_parked();

    let panel = overlay.debug_bounds("command-bar").unwrap();
    let field = overlay.debug_bounds("command-bar-input").unwrap();
    assert!(field.right() <= panel.right(), "field {field:?} sticks out of {panel:?}");
    let scroll = view.read_with(cx, |w, cx| w.command_bar.as_ref().unwrap().input.read(cx).scroll_x());
    assert!(scroll > gpui::px(0.), "the cursor at the end is scrolled into view");

    overlay.simulate_keystrokes("home");
    overlay.run_until_parked();
    let scroll = view.read_with(cx, |w, cx| w.command_bar.as_ref().unwrap().input.read(cx).scroll_x());
    assert_eq!(scroll, gpui::px(0.));
}

fn floating(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Option<VisualTestContext> {
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| w.floating_sidebar.as_ref().and_then(|f| f.overlay))?;
    Some(VisualTestContext::from_window(overlay.into(), cx))
}

fn move_mouse(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_mouse_move(gpui::point(gpui::px(x), gpui::px(y)), None, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_hidden_sidebar_floats_in_from_the_left_edge(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    move_mouse(cx, 300.0, 300.0);
    assert!(floating(&view, cx).is_none(), "only the edge reveals it");

    move_mouse(cx, 2.0, 300.0);
    let mut overlay = floating(&view, cx).expect("floating sidebar opens");
    let panel = overlay.debug_bounds("floating-sidebar").unwrap();

    // Over the panel, then away: it slides out and its window goes.
    move_mouse(&mut overlay, 100.0, 300.0);
    // Clicking a tab in it works like in the sidebar.
    assert!(overlay.debug_bounds("new-tab").is_some());
    move_mouse(cx, f32::from(panel.right()) + 360.0, 300.0);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.floating_sidebar.is_none()));
}

#[gpui::test]
fn a_revealed_sidebar_hides_if_never_entered(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    move_mouse(cx, 2.0, 300.0);
    assert!(floating(&view, cx).is_some());
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.floating_sidebar.is_none()));
}

#[gpui::test]
fn new_tab_from_the_floating_sidebar_uses_the_browser_window_bounds(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    move_mouse(cx, 2.0, 300.0);
    let mut sidebar = floating(&view, cx).unwrap();
    // GPUI's element Animation uses std::time::Instant, not the test clock.
    std::thread::sleep(Duration::from_millis(200));
    sidebar.update(|_, cx| cx.refresh_windows());
    sidebar.run_until_parked();
    move_mouse(&mut sidebar, 100.0, 300.0);
    click(&mut sidebar, "new-tab");
    let mut popup = bar(&view, cx);
    let panel = popup.debug_bounds("command-bar").unwrap();
    assert!(panel.size.width > gpui::px(500.), "popup should fit the full browser window: {panel:?}");
    assert!(panel.origin.x >= gpui::px(0.));
    popup.simulate_input("example.com/from-floating-sidebar");
    popup.simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com/from-floating-sidebar"));
}

#[gpui::test]
fn showing_the_sidebar_again_removes_the_floating_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    move_mouse(cx, 2.0, 300.0);
    assert!(floating(&view, cx).is_some());
    cx.simulate_keystrokes("cmd-s");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.sidebar_visible && w.floating_sidebar.is_none()));
}

#[gpui::test]
fn command_bar_runs_actions_and_preserves_search(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx).len();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pin tab");
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(matches!(&suggestions[0], Suggestion::Action { label, .. } if label == "Pin Tab"));
    assert!(suggestions.iter().any(|s| matches!(s, Suggestion::Load(_))));
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().pinned_url.is_some()));
    assert_eq!(today_urls(&view, cx).len(), before - 1);
    assert_eq!(bar_text(&view, cx), None);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("unpin tab");
    click(&mut bar(&view, cx), "suggestion-0");
    assert_eq!(today_urls(&view, cx).len(), before);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("hide sidebar");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(!view.read_with(cx, |w, _| w.sidebar_visible));
    cx.simulate_keystrokes("cmd-s");
    assert!(view.read_with(cx, |w, _| w.sidebar_visible));
}

#[gpui::test]
fn command_bar_switches_spaces_by_name(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("switch to research");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 2);
    assert!(today_urls(&view, cx).is_empty());
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn command_bar_finds_and_focuses_tabs_in_other_spaces(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-2 cmd-t");
    bar(&view, cx).simulate_input("gpui");
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(matches!(&suggestions[1], Suggestion::SwitchToTab { space, .. } if space == "Personal"));
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    assert_eq!(today_urls(&view, cx).len(), 3);
}

#[gpui::test]
fn unavailable_command_is_not_offered_and_escape_does_not_execute(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("restore last closed tab");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|s| matches!(s, Suggestion::Action { .. }))));
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("clear today");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(today_urls(&view, cx).len(), 3);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("clear today");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(today_urls(&view, cx).len(), 1);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("reopen closed tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(today_urls(&view, cx).len(), 2);
}


#[gpui::test]
fn page_controls_work_from_ui_and_from_page_focus(cx: &mut TestAppContext) {
    let (_, cx, fake) = open_pages(cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-shift-r cmd-. cmd-= cmd-- cmd-0 cmd-left cmd-right");
    assert_eq!(fake.take_calls(), vec![Call::ForceReload(1), Call::StopLoading(1), Call::Zoom(1, 1), Call::Zoom(1, -1), Call::Zoom(1, 0), Call::Back(1), Call::Forward(1)]);
    for (key, shift) in [("r", true), (".", false), ("=", false), ("-", false), ("0", false)] {
        fake.send(cx, fake.shortcut(Shortcut { cmd: true, shift, key: key.into(), ..Default::default() }));
    }
    assert_eq!(fake.take_calls(), vec![Call::ForceReload(1), Call::StopLoading(1), Call::Zoom(1, 1), Call::Zoom(1, -1), Call::Zoom(1, 0)]);
}

#[gpui::test]
fn page_controls_are_available_by_name(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    for (query, expected) in [("hard reload", Call::ForceReload(1)), ("stop loading", Call::StopLoading(1)), ("zoom in", Call::Zoom(1, 1)), ("zoom out", Call::Zoom(1, -1)), ("reset zoom", Call::Zoom(1, 0))] {
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        bar(&view, cx).simulate_keystrokes("enter");
        assert!(fake.take_calls().contains(&expected), "{query}");
    }
}


#[gpui::test]
fn clear_button_archives_background_today_without_touching_the_active_page_or_pins(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    // Tidy and Clear appear only while the pointer is over the sidebar.
    move_mouse(cx, 600., 400.);
    assert!(!view.read_with(cx, |w, _| w.sidebar_hovered));
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    move_mouse(cx, f32::from(sidebar.center().x), f32::from(sidebar.center().y));
    assert!(view.read_with(cx, |w, _| w.sidebar_hovered));
    assert!(cx.debug_bounds("tidy-today").is_some());
    click(cx, "clear-today");
    assert_eq!(today_urls(&view, cx), vec!["https://www.chromium.org".to_string()]);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned.len()), 2);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 2);

    cx.simulate_keystrokes("cmd-shift-t");
    assert_eq!(today_urls(&view, cx).len(), 2);
    cx.run_until_parked();
    assert!(cx.debug_bounds("clear-today").is_some());
}

#[gpui::test]
fn pinned_section_collapse_is_local_persistent_and_available_by_name(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let expanded_y = cx.debug_bounds("new-tab").unwrap().origin.y;
    click(cx, "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    // GPUI retains old debug bounds, so check the actual layout movement.
    cx.update(|window, cx| { let _ = window.draw(cx); });
    assert!(cx.debug_bounds("new-tab").unwrap().origin.y < expanded_y);
    let saved = view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser));
    let restored = browser_core::session::from_json(&saved).unwrap();
    assert!(restored.space().pinned_collapsed);
    assert!(!restored.spaces[1].pinned_collapsed);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("expand pinned section");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(!view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("new-tab").unwrap().origin.y, expanded_y);
}

#[gpui::test]
fn dock_creates_named_space_and_rejects_blank_names(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    open_new_space_from_menu(&view, cx);
    assert_space_creation_form(&view, cx);
    let origin = view.read_with(cx, |w, _| w.browser.space().id);
    cx.simulate_input("   ");
    click(cx, "space-creation-submit");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.spaces.len(), 3);
        assert_eq!(w.browser.space().id, origin);
        assert_eq!(w.sidebar_navigation.controller.draft().unwrap().name, "   ");
        assert!(w.sidebar_navigation.form.is_some());
    });
    // The disabled button click leaves the name field; select it again to edit.
    assert!(cx.update(|window, cx| {
        view.read(cx).sidebar_navigation.form.as_ref().unwrap().read(cx).focus_handle(cx).is_focused(window)
    }));
    click(cx, "space-creation-name");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("  Reading 📚  ");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.spaces.len(), 3);
        assert_eq!(w.browser.space().id, origin);
        assert_eq!(w.sidebar_navigation.controller.draft().unwrap().name, "  Reading 📚  ");
        assert!(w.sidebar_navigation.form.is_some());
    });
    click(cx, "space-creation-submit");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Reading 📚");
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces.len()), 4);
    assert!(today_urls(&view, cx).is_empty());
    view.read_with(cx, |w, _| {
        assert!(w.command_bar.is_none());
        assert!(w.sidebar_navigation.form.is_none());
        assert!(w.sidebar_navigation.controller.draft().is_none());
        assert_eq!(w.sidebar_navigation.controller.selected(), crate::sidebar_pages::SidebarPage::Space(w.browser.space().id));
    });
    let restored = view.read_with(cx, |w, _| browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap());
    assert_eq!(restored.space().name, "Reading 📚");
}

#[gpui::test]
fn create_and_rename_space_actions_keep_focus_and_can_be_cancelled(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("create space");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_space_creation_form(&view, cx);
    assert_eq!(view.read_with(cx, |w, _| w.sidebar_navigation.controller.draft().unwrap().name.clone()), "");
    cx.simulate_input("Reading");
    click(cx, "space-creation-submit");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Reading");
    let id = view.read_with(cx, |w, _| w.browser.space().id);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename space");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Reading"));
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("Research Archive");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Research Archive");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().id), id);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename space");
    bar(&view, cx).simulate_keystrokes("enter");
    bar(&view, cx).simulate_input("Cancelled");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Research Archive");
    open_new_space_from_menu(&view, cx);
    assert_space_creation_form(&view, cx);
    let before_cancel = view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser));
    cx.simulate_input("Cancelled Space");
    assert_eq!(view.read_with(cx, |w, _| w.sidebar_navigation.controller.draft().unwrap().name.clone()), "Cancelled Space");
    click(cx, "space-creation-cancel");
    view.read_with(cx, |w, _| {
        assert_eq!(browser_core::session::to_json(&w.browser), before_cancel);
        assert!(w.sidebar_navigation.form.is_none());
        assert!(w.sidebar_navigation.controller.draft().is_none());
        assert_eq!(w.sidebar_navigation.controller.selected(), crate::sidebar_pages::SidebarPage::Space(id));
    });
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces.len()), 4);
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces[0].pinned.len()), 2);
}

fn find_overlay(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| w.find_bar.as_ref().and_then(|bar| bar.overlay)).expect("find overlay is not open");
    VisualTestContext::from_window(overlay.into(), cx)
}

#[gpui::test]
fn find_in_page_searches_as_you_type_and_navigates_matches(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-f");
    let mut find = find_overlay(&view, cx);
    let focused = find.update(|window, cx| view.read(cx).find_bar.as_ref().unwrap().input.read(cx).focus_handle(cx).is_focused(window));
    assert!(focused);
    find.simulate_input("needle");
    assert!(fake.take_calls().contains(&Call::Find(1, "needle".into(), true, false)));
    find.simulate_keystrokes("enter shift-enter cmd-g cmd-shift-g");
    let calls = fake.take_calls();
    assert_eq!(calls, vec![Call::Find(1, "needle".into(), true, true), Call::Find(1, "needle".into(), false, true), Call::Find(1, "needle".into(), true, true), Call::Find(1, "needle".into(), false, true)]);
    click(&mut find, "find-next");
    assert!(fake.take_calls().contains(&Call::Find(1, "needle".into(), true, true)));
    click(&mut find, "find-previous");
    assert!(fake.take_calls().contains(&Call::Find(1, "needle".into(), false, true)));
    fake.send(cx, Event::FindResult { view: page(1), count: 4, active: 2, final_update: true });
    assert_eq!(view.read_with(cx, |w, _| w.find_bar.as_ref().map(|bar| (bar.count, bar.active, bar.searching))), Some((Some(4), 2, false)));
    find.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::StopFinding(1)));
    assert!(calls.contains(&Call::FocusPage(1)));
}

#[gpui::test]
fn find_handles_no_matches_empty_input_and_refocus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-f");
    let mut find = find_overlay(&view, cx);
    find.simulate_input("missing");
    fake.send(cx, Event::FindResult { view: page(1), count: 0, active: 0, final_update: true });
    assert_eq!(view.read_with(cx, |w, _| w.find_bar.as_ref().unwrap().count), Some(0));
    find.simulate_keystrokes("cmd-f");
    find.simulate_input("replacement");
    assert_eq!(view.read_with(cx, |w, cx| w.find_bar.as_ref().unwrap().input.read(cx).text().to_string()), "replacement");
    fake.take_calls();
    find.simulate_keystrokes("cmd-a backspace");
    assert!(fake.take_calls().contains(&Call::StopFinding(1)));
    // A late engine response after clearing the input cannot restore the old count.
    fake.send(cx, Event::FindResult { view: page(1), count: 5, active: 1, final_update: true });
    assert_eq!(view.read_with(cx, |w, _| w.find_bar.as_ref().unwrap().count), None);
    click(&mut find, "find-close");
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
}

#[gpui::test]
fn find_shortcut_from_page_focus_and_tab_switch_clear_find(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, fake.shortcut(Shortcut { cmd: true, key: "f".into(), ..Default::default() }));
    find_overlay(&view, cx).simulate_input("page text");
    fake.take_calls();
    // Clicking another sidebar item must not keep searching the old page.
    click(cx, "tab-4");
    cx.update(|window, cx| { let _ = window.draw(cx); });
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
    assert!(fake.take_calls().contains(&Call::StopFinding(1)));
}

#[gpui::test]
fn find_command_opens_a_small_panel_with_input_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("find in page");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, _| w.command_bar.is_none()));
    let mut find = find_overlay(&view, cx);
    let panel = find.update(|window, _| window.bounds());
    let main = cx.update(|window, _| window.bounds());
    assert!(panel.size.height < main.size.height / 4.0);
    assert!(panel.size.width < main.size.width);
    assert!(find.update(|window, cx| view.read(cx).find_bar.as_ref().unwrap().input.read(cx).focus_handle(cx).is_focused(window)));
    find.simulate_input("text");
    assert!(fake.take_calls().contains(&Call::Find(1, "text".into(), true, false)));
}

#[gpui::test]
fn auto_archive_interval_is_only_offered_in_settings_profiles(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("archive tabs after");
    let options = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(!options.iter().any(|s| matches!(s, Suggestion::Action { label, .. } if label.contains("Archive Tabs After"))));
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-popup-Archive tabs after");
    for index in 0..4 { assert!(settings.debug_bounds(test_selector(format!("settings-choice-{index}"))).is_some()); }
    assert!(settings.debug_bounds("settings-choice-4").is_none());
    settings.simulate_keystrokes("escape");
}

#[gpui::test]
fn copy_url_cleans_trackers_and_keeps_page_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://example.com/article?utm_source=mail&id=4#part".into() });
    fake.take_calls();
    fake.send(cx, fake.shortcut(Shortcut { cmd: true, shift: true, key: "c".into(), ..Default::default() }));
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(clipboard.as_deref(), Some("https://example.com/article?id=4#part"));
    assert_eq!(view.read_with(cx, |w, _| w.toast.as_ref().map(|toast| toast.message.clone())).as_deref(), Some("Copied a clean link without trackers"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusUi)));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_keystrokes("cmd-v");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("https://example.com/article?id=4#part"));
}

#[gpui::test]
fn markdown_copy_and_command_actions_write_links(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, Event::TitleChanged { view: page(1), title: "A [page]".into() });
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://example.com/a(b)?fbclid=track&lang=en".into() });
    cx.simulate_keystrokes("cmd-alt-shift-c");
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(clipboard.as_deref(), Some("[A \\[page\\]](https://example.com/a%28b%29?lang=en)"));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("copy url as markdown");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())), clipboard);
}

#[gpui::test]
fn repeated_toasts_replace_each_other_and_expire_without_stealing_focus(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-shift-c");
    assert!(view.read_with(cx, |w, _| w.toast.is_some()));
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-alt-shift-c");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.toast.as_ref().map(|toast| toast.message.clone())).as_deref(), Some("Copied URL as Markdown"));
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.toast.is_none()));
}

fn open_archive(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("cmd-t");
    bar(view, cx).simulate_input("view archive");
    bar(view, cx).simulate_keystrokes("enter");
}

#[gpui::test]
fn archive_search_restores_a_selected_tab_to_its_original_space(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w ctrl-2");
    open_archive(&view, cx);
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert_eq!(suggestions.len(), 2);
    assert!(matches!(&suggestions[0], Suggestion::ArchivedTab { title, detail, .. } if title == "GPUI" && detail.contains("Manually Closed") && detail.contains("Personal")));
    bar(&view, cx).simulate_input("chromium");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(view.read_with(cx, |w, _| w.toast.as_ref().map(|toast| toast.message.clone())).as_deref(), Some("Tab restored to Personal"));
}

#[gpui::test]
fn archive_filters_spaces_and_deletes_without_restoring(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w ctrl-2 cmd-t");
    bar(&view, cx).simulate_input("https://work.example");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-w ctrl-1");
    open_archive(&view, cx);
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 3);
    click(&mut bar(&view, cx), "archive-this-space");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 2);
    bar(&view, cx).simulate_input("gpui");
    click(&mut bar(&view, cx), "delete-archived-7");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 2);
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    assert_eq!(bar_text(&view, cx).as_deref(), Some("gpui"));
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://thebrowser.company"));
    bar(&view, cx).simulate_keystrokes("cmd-a backspace");
    click(&mut bar(&view, cx), "archive-all-spaces");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 2);
}

#[gpui::test]
fn clearing_the_archive_requires_confirmation_and_can_be_cancelled(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w");
    open_archive(&view, cx);
    click(&mut bar(&view, cx), "clear-archive");
    bar(&view, cx).simulate_input("Clear Archive");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 2);
    open_archive(&view, cx);
    click(&mut bar(&view, cx), "clear-archive");
    // Enter without the confirmation phrase returns to the archive.
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 2);
    click(&mut bar(&view, cx), "clear-archive");
    bar(&view, cx).simulate_input("Clear Archive");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
    assert_eq!(bar_text(&view, cx), None);
    cx.simulate_keystrokes("cmd-shift-t");
    assert_eq!(today_urls(&view, cx).len(), 1);
}

#[gpui::test]
fn cmd_t_and_cmd_l_restart_the_command_overlay_from_archive_mode(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w");
    open_archive(&view, cx);
    bar(&view, cx).simulate_input("no matches here");
    bar(&view, cx).simulate_keystrokes("cmd-t");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    bar(&view, cx).simulate_keystrokes("cmd-l");
    assert_eq!(bar_text(&view, cx), active_url(&view, cx));
    bar(&view, cx).simulate_input("example.com");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
}

#[gpui::test]
fn reset_pinned_tab_navigates_back_to_the_original_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-d cmd-l");
    bar(&view, cx).simulate_input("https://example.com/elsewhere");
    bar(&view, cx).simulate_keystrokes("enter");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("reset pinned tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    assert!(fake.take_calls().contains(&Call::Navigate(1, "https://www.chromium.org".into())));
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned.len()), 3);
}

#[gpui::test]
fn replace_pinned_url_changes_the_page_used_after_closing(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-d cmd-l");
    bar(&view, cx).simulate_input("https://example.com/replacement");
    bar(&view, cx).simulate_keystrokes("enter");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("replace pin with current page");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().pinned_url.clone()).as_deref(), Some("https://example.com/replacement"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Navigate(_, _))));
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("https://example.com/elsewhere");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-w");
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned.iter().any(|tab| tab.url == "https://example.com/replacement" && tab.pinned_url.as_deref() == Some("https://example.com/replacement"))));
}

#[gpui::test]
fn reset_space_discards_pinned_pages_and_reopens_the_active_original(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original_id = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("cmd-d cmd-l");
    bar(&view, cx).simulate_input("https://example.com/elsewhere");
    bar(&view, cx).simulate_keystrokes("enter");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("reset space");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), original_id);
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)));
    assert!(calls.contains(&Call::Create(2, "https://www.chromium.org".into())));
    assert_eq!(today_urls(&view, cx).len(), 2);
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
}

#[gpui::test]
fn exact_command_names_outrank_longer_actions_with_shared_words(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    for (query, expected) in [("pin tab", "Pin Tab"), ("archive tab", "Archive Tab"), ("settings", "Settings…"), ("archive", "View Archive"), ("copy url", "Copy URL"), ("copy url as markdown", "Copy URL as Markdown"), ("reset space", "Reset Space")] {
        bar(&view, cx).simulate_keystrokes("cmd-a");
        bar(&view, cx).simulate_input(query);
        let label = view.read_with(cx, |w, cx| match w.suggestions(cx).first() { Some(Suggestion::Action { label, .. }) => label.clone(), _ => String::new() });
        assert_eq!(label, expected, "query: {query}");
    }
}

fn double_click(cx: &mut VisualTestContext, selector: &'static str) {
    click(cx, selector);
    let position = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_event(gpui::MouseDownEvent { position, button: gpui::MouseButton::Left, modifiers: Modifiers::none(), click_count: 2, first_mouse: false });
    cx.simulate_event(gpui::MouseUpEvent { position, button: gpui::MouseButton::Left, modifiers: Modifiers::none(), click_count: 2 });
    cx.run_until_parked();
}

fn inline_name(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |w, cx| w.tab_rename.as_ref().map(|editor| editor.input.read(cx).text().to_string()))
}

fn inline_name_focused(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| view.read(cx).tab_rename.as_ref().is_some_and(|editor| editor.input.read(cx).focus_handle(cx).is_focused(window)))
}

#[gpui::test]
fn double_click_renames_a_tab_and_engine_titles_do_not_overwrite_it(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    double_click(cx, "tab-8");
    assert_eq!(inline_name(&view, cx).as_deref(), Some("Chromium"));
    assert!(inline_name_focused(&view, cx));
    assert!(bar_text(&view, cx).is_none());
    let row = cx.debug_bounds("tab-8").unwrap();
    let editor = cx.debug_bounds("inline-tab-rename").unwrap();
    assert!(row.contains(&editor.origin) && row.contains(&editor.bottom_right()));
    cx.simulate_input("Engine research");
    cx.simulate_keystrokes("enter");
    fake.send(cx, Event::TitleChanged { view: page(1), title: "Updated by website".into() });
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Engine research");
    let saved = view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser));
    assert_eq!(browser_core::session::from_json(&saved).unwrap().active_tab().unwrap().title, "Engine research");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("reset tab name");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Updated by website");
}

#[gpui::test]
fn rename_tab_command_keeps_focus_and_escape_does_not_change_the_name(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename current tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(inline_name(&view, cx).as_deref(), Some("Chromium"));
    assert!(inline_name_focused(&view, cx));
    cx.simulate_input("Cancelled name");
    cx.simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Chromium");
    double_click(cx, "tab-8");
    cx.simulate_input("  ");
    cx.simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Chromium");
    assert!(inline_name(&view, cx).is_none());
}

#[gpui::test]
fn clearing_an_inline_name_restores_the_latest_page_title(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Custom name");
    cx.simulate_keystrokes("enter");
    double_click(cx, "tab-8");
    fake.send(cx, Event::TitleChanged { view: page(1), title: "Latest website title".into() });
    assert_eq!(inline_name(&view, cx).as_deref(), Some("Custom name"));
    cx.simulate_keystrokes("backspace enter");
    assert!(inline_name(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Latest website title");
    assert!(!view.read_with(cx, |w, _| w.browser.item_has_custom_name(w.browser.active_tab().unwrap().id)));
}

#[gpui::test]
fn leaving_an_inline_name_by_clicking_another_tab_commits_without_reloading_the_original(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Saved on click");
    fake.take_calls();
    click(cx, "tab-7");
    assert!(inline_name(&view, cx).is_none());
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().find(|tab| tab.id.get() == 8).unwrap().title.clone()), "Saved on click");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Close(1) | Call::Navigate(1, _) | Call::Reload(1))));
}

#[gpui::test]
fn returning_native_focus_to_the_page_commits_its_inline_name(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    double_click(cx, "tab-8");
    assert!(fake.take_calls().contains(&Call::FocusUi));
    cx.simulate_input("Saved on native focus");
    fake.send(cx, Event::Focused { view: page(1) });
    assert!(inline_name(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Saved on native focus");
    assert!(fake.take_calls().is_empty());
}

#[gpui::test]
fn opening_the_command_bar_commits_the_inline_name_and_restores_normal_input(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Saved before search");
    cx.simulate_keystrokes("cmd-t");
    assert!(inline_name(&view, cx).is_none());
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("independent query");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("independent query"));
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Saved before search");
}

#[gpui::test]
fn hiding_the_sidebar_finishes_inline_editing_instead_of_leaving_an_invisible_input(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Saved before hiding");
    cx.simulate_keystrokes("cmd-s");
    assert!(inline_name(&view, cx).is_none());
    assert!(!view.read_with(cx, |w, _| w.sidebar_visible));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Saved before hiding");
}

#[gpui::test]
fn closing_a_tab_during_inline_editing_preserves_the_draft_in_the_archive(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Archived custom name");
    cx.simulate_keystrokes("cmd-w");
    assert!(inline_name(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.last().unwrap().tab.title.clone()), "Archived custom name");
    cx.simulate_keystrokes("cmd-shift-t");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Archived custom name");
}

#[gpui::test]
fn renaming_a_hidden_pinned_tab_reveals_its_sidebar_row_without_recreating_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-d");
    run_named_command(&view, cx, "collapse pinned section");
    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    fake.take_calls();
    run_named_command(&view, cx, "rename current tab");
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.sidebar_visible && !w.browser.space().pinned_collapsed));
    assert!(inline_name_focused(&view, cx));
    assert!(cx.debug_bounds("inline-tab-rename").is_some());
    cx.simulate_input("Pinned name");
    cx.simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Pinned name");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn move_commands_change_space_and_section_without_recreating_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let id = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    fake.take_calls();
    for (query, space, pinned) in [("move to pinned in work", 1, true), ("move to today in research", 2, false), ("move to work", 1, false)] {
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        bar(&view, cx).simulate_keystrokes("enter");
        assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), space);
        assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), id);
        assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().pinned_url.is_some()), pinned);
        assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
        assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
    }
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(_, _) | Call::Close(_) | Call::Navigate(_, _))));
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces[0].today.len()), 2);
}

#[gpui::test]
fn pin_to_space_command_preserves_a_custom_tab_name(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename tab");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_input("Engine notes");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pin to work");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Engine notes");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned.len()), 1);
}

fn menu_action(menu: &str, label: &str) -> Box<dyn gpui::Action> {
    fn find(items: Vec<gpui::MenuItem>, label: &str) -> Option<Box<dyn gpui::Action>> {
        items.into_iter().find_map(|item| match item {
            gpui::MenuItem::Action { name, action, .. } if name.as_ref() == label => Some(action),
            gpui::MenuItem::Submenu(menu) => find(menu.items, label),
            _ => None,
        })
    }
    find(crate::menus::build().into_iter().find(|item| item.name.as_ref() == menu).unwrap().items, label)
        .unwrap_or_else(|| panic!("menu item missing: {menu} / {label}"))
}

#[gpui::test]
fn view_source_from_menu_shortcut_and_command_bar_opens_a_new_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx).unwrap();
    let count = view.read_with(cx, |w, _| w.browser.all_tabs().count());
    for route in 0..3 {
        fake.take_calls();
        match route {
            0 => dispatch_menu(cx, "View", "View Source"),
            1 => { cx.simulate_keystrokes("cmd-alt-u"); cx.run_until_parked(); }
            _ => run_named_command(&view, cx, "view source"),
        }
        let source = format!("view-source:{original}");
        assert_eq!(active_url(&view, cx).as_deref(), Some(source.as_str()));
        assert!(bar_text(&view, cx).is_none());
        assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), count + 1);
        let calls = fake.take_calls();
        assert!(calls.iter().any(|call| matches!(call, Call::Create(_, url) if url == &source)));
        assert!(!calls.iter().any(|call| matches!(call, Call::Close(_) | Call::Navigate(..) | Call::Reload(_))));
        assert!(!cx.update(|window, cx| window.is_action_available(&crate::window::ViewSource, cx)));
        cx.simulate_keystrokes("cmd-alt-u");
        assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), count + 1);
        cx.simulate_keystrokes("cmd-w");
        cx.run_until_parked();
        assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
    }
}

#[gpui::test]
fn view_source_from_native_split_focus_preserves_the_original_group(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let original = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    fake.send(cx, fake.shortcut(Shortcut { cmd: true, alt: true, key: "u".into(), ..Default::default() }));
    assert_eq!(active_url(&view, cx).as_deref(), Some("view-source:https://pane1.example"));
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        assert_eq!(split.id, original.id);
        assert_eq!(split.panes, original.panes);
        assert_eq!(split.active, original.active);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn internal_pages_do_not_offer_view_source(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-y");
    let original = active_url(&view, cx);
    assert!(!cx.update(|window, cx| window.is_action_available(&crate::window::ViewSource, cx)));
    cx.simulate_keystrokes("cmd-alt-u");
    assert_eq!(active_url(&view, cx), original);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("view source");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|row| matches!(row, Suggestion::Action { command: crate::window::Command::ViewSource, .. }))));
}

fn dispatch_menu(cx: &mut VisualTestContext, menu: &str, label: &str) {
    let action = menu_action(menu, label);
    cx.update(|window, cx| window.dispatch_action(action, cx));
    cx.run_until_parked();
}

fn paste_test_text(cx: &mut VisualTestContext, text: &str) {
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned())));
    cx.simulate_keystrokes("cmd-v");
}

#[gpui::test]
fn text_undo_and_redo_follow_arc_per_key_and_menu_actions_without_navigating(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    let mut input = bar(&view, cx);
    input.simulate_input("abc");
    input.simulate_keystrokes("cmd-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("ab"));
    dispatch_menu(&mut input, "Edit", "Undo");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("a"));
    dispatch_menu(&mut input, "Edit", "Redo");
    input.simulate_keystrokes("cmd-shift-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("abc"));
    input.simulate_keystrokes("cmd-z");
    input.simulate_input("X");
    input.simulate_keystrokes("cmd-shift-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("abX"));
    assert!(bar_input_focused(&view, cx));
    assert!(fake.take_calls().is_empty());
}

#[gpui::test]
fn text_undo_restores_selected_url_and_paste_is_one_edit(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let original = active_url(&view, cx).unwrap();
    cx.simulate_keystrokes("cmd-l");
    let mut input = bar(&view, cx);
    paste_test_text(&mut input, "replacement");
    input.simulate_keystrokes("cmd-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(original.as_str()));
    input.simulate_input("x");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("x"), "undo must restore the selection, not only the URL");
    input.simulate_keystrokes("cmd-z cmd-shift-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("x"));
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
}

#[gpui::test]
fn text_undo_restores_graphemes_caret_and_cut_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    let mut input = bar(&view, cx);
    paste_test_text(&mut input, "A🙂e\u{301}Z");
    input.simulate_keystrokes("end left backspace");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("A🙂Z"));
    input.simulate_keystrokes("cmd-z");
    input.simulate_input("!");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("A🙂e\u{301}!Z"));
    input.simulate_keystrokes("cmd-a cmd-x cmd-z");
    input.simulate_input("R");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("R"));
}

#[gpui::test]
fn text_undo_noop_deletion_preserves_redo_and_retargeting_clears_history(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-l");
    let mut input = bar(&view, cx);
    paste_test_text(&mut input, "abc");
    input.simulate_keystrokes("cmd-z");
    // Undo restores the original selected URL. A no-op delete at its start
    // must not discard the available redo.
    input.simulate_keystrokes("home backspace cmd-shift-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("abc"));
    input.simulate_keystrokes("cmd-t cmd-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
}

fn mark_command_text(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, text: &str, selection: std::ops::Range<usize>) {
    use gpui::EntityInputHandler;
    let input = view.read_with(cx, |w, _| w.command_bar.as_ref().unwrap().input.clone());
    cx.update(|window, cx| input.update(cx, |input, cx|
        input.replace_and_mark_text_in_range(None, text, Some(selection), window, cx)));
    cx.run_until_parked();
}

#[gpui::test]
fn text_undo_treats_ime_candidates_as_one_edit_and_uses_relative_utf16_selection(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    let mut input_window = bar(&view, cx);
    paste_test_text(&mut input_window, "🙂");
    mark_command_text(&view, &mut input_window, "か", 0..1);
    let input = view.read_with(cx, |w, _| w.command_bar.as_ref().unwrap().input.clone());
    let selection = input_window.update(|window, cx| input.update(cx, |input, cx|
        input.selected_text_range(false, window, cx).unwrap().range));
    assert_eq!(selection, 2..3);
    mark_command_text(&view, &mut input_window, "かな", 2..2);
    input_window.update(|window, cx| input.update(cx, |input, cx|
        input.replace_text_in_range(None, "仮名", window, cx)));
    input_window.run_until_parked();
    assert_eq!(bar_text(&view, cx).as_deref(), Some("🙂仮名"));
    input_window.simulate_keystrokes("cmd-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("🙂"));
    input_window.simulate_keystrokes("cmd-shift-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("🙂仮名"));
}

#[gpui::test]
fn text_undo_ime_unmark_commits_and_cancelling_does_not_destroy_redo(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    let mut input_window = bar(&view, cx);
    mark_command_text(&view, &mut input_window, "日本", 2..2);
    let input = view.read_with(cx, |w, _| w.command_bar.as_ref().unwrap().input.clone());
    input_window.update(|window, cx| input.update(cx, |input, cx| input.unmark_text(window, cx)));
    input_window.run_until_parked();
    input_window.simulate_keystrokes("cmd-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    mark_command_text(&view, &mut input_window, "に", 1..1);
    input_window.update(|window, cx| input.update(cx, |input, cx|
        input.replace_text_in_range(None, "", window, cx)));
    input_window.run_until_parked();
    input_window.simulate_keystrokes("cmd-shift-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("日本"));
}

#[gpui::test]
fn text_undo_updates_live_find_and_inline_rename_without_changing_page_identity(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-f");
    let mut find = find_overlay(&view, cx);
    paste_test_text(&mut find, "needle");
    fake.take_calls();
    find.simulate_keystrokes("cmd-z");
    assert!(fake.take_calls().contains(&Call::StopFinding(1)));
    find.simulate_keystrokes("cmd-shift-z");
    assert!(fake.take_calls().contains(&Call::Find(1, "needle".into(), true, false)));
    find.simulate_keystrokes("escape");
    double_click(cx, "tab-8");
    paste_test_text(cx, "Custom name");
    cx.simulate_keystrokes("cmd-z enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Chromium");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn text_undo_native_page_menus_target_its_focused_pane_but_not_an_empty_name_editor(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.send(cx, Event::Focused { view: page(1) });
    fake.take_calls();
    dispatch_menu(cx, "Edit", "Undo");
    assert_eq!(fake.take_calls(), vec![Call::Undo(1)]);
    fake.send(cx, Event::Focused { view: page(2) });
    dispatch_menu(cx, "Edit", "Redo");
    assert!(fake.take_calls().contains(&Call::Redo(2)));
    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    let active = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    double_click(cx, test_selector(format!("tab-{}", active.get())));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-z");
    assert!(fake.take_calls().is_empty());
    assert!(inline_name(&view, cx).is_some());
}

#[gpui::test]
fn application_menu_actions_are_reachable_from_the_browser_window(cx: &mut TestAppContext) {
    // Native document actions require a loaded page and its active host.
    let (_, cx, _) = open_pages(cx);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    for menu in crate::menus::build() {
        for item in menu.items {
            if let gpui::MenuItem::Action { name, action, os_action: None } = item {
                // TestPlatform does not automatically activate newly opened windows.
                let available = cx.update(|window, cx| window.is_action_available(action.as_ref(), cx))
                    || cx.cx.update(|cx| cx.is_action_available(action.as_ref()));
                assert!(available, "unreachable menu action: {} / {name}", menu.name);
            }
        }
    }
}

#[gpui::test]
fn menu_actions_open_editors_and_edit_the_command_input(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    dispatch_menu(cx, "Spaces", "New Space…");
    assert_space_creation_form(&view, cx);
    cx.simulate_input("Menu Space");
    click(cx, "space-creation-submit");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Menu Space");
    dispatch_menu(cx, "Spaces", "Rename Space…");
    bar(&view, cx).simulate_input("Renamed Space");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Renamed Space");
    dispatch_menu(cx, "File", "New Tab…");
    let mut input = bar(&view, cx);
    input.simulate_input("clipboard text");
    dispatch_menu(&mut input, "Edit", "Select All");
    dispatch_menu(&mut input, "Edit", "Copy");
    dispatch_menu(&mut input, "Edit", "Cut");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    dispatch_menu(&mut input, "Edit", "Paste");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("clipboard text"));
}

// TestPlatform::minimize is unimplemented. Exercise the production teardown
// separately; real Cmd+M/action routing and native miniaturization need drive.sh.
#[gpui::test]
fn minimizing_popup_teardown_waits_for_activation_before_focusing_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    for popup in ["command", "find", "tab-menu"] {
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        let mut overlay = match popup {
            "command" => { cx.simulate_keystrokes("cmd-t"); bar(&view, cx) }
            "find" => { cx.simulate_keystrokes("cmd-f"); find_overlay(&view, cx) }
            _ => { right_click(cx, "tab-4"); tab_menu(&view, cx) }
        };
        overlay.update(|window, _| window.activate_window());
        overlay.run_until_parked();
        fake.take_calls();
        view.update(cx, |w, cx| w.close_popups_for_window_operation(true, cx));
        cx.run_until_parked();
        assert!(view.read_with(cx, |w, _| w.command_bar.is_none() && w.find_bar.is_none() && w.tab_menu.is_none()));
        assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
        let calls = fake.take_calls();
        assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))), "{popup}: {calls:?}");
        if popup == "find" { assert!(calls.contains(&Call::StopFinding(1))); }
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        assert!(fake.take_calls().contains(&Call::FocusPage(1)), "{popup}: activation must restore the page responder");
    }
    // Ordinary dismissal must still immediately restore the page responder.
    cx.simulate_keystrokes("cmd-t");
    fake.take_calls();
    bar(&view, cx).simulate_keystrokes("escape");
    assert!(fake.take_calls().contains(&Call::FocusPage(1)));
}

#[gpui::test]
fn minimize_teardown_invalidates_focus_restoration_already_queued_by_dismissal(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.update(|window, _| window.activate_window());
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).update(|window, _| window.activate_window());
    cx.run_until_parked();
    fake.take_calls();
    // Both updates belong to one dispatch, before deferred overlay teardown.
    view.update(cx, |w, cx| {
        w.close_command_bar(cx);
        w.close_popups_for_window_operation(true, cx);
    });
    cx.run_until_parked();
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
    let calls = fake.take_calls();
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))), "{calls:?}");
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(fake.take_calls().contains(&Call::FocusPage(1)), "cancelled pending restoration must resume on activation");
}

#[gpui::test]
fn minimize_without_popup_preserves_history_search_focus_on_activation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.update(|window, _| window.activate_window());
    cx.simulate_keystrokes("cmd-y");
    cx.simulate_input("retained");
    let (_, mut other) = another_profile_window(&view, cx);
    other.update(|window, _| window.activate_window());
    other.run_until_parked();
    fake.take_calls();
    view.update(cx, |w, cx| w.close_popups_for_window_operation(true, cx));
    cx.run_until_parked();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(cx.update(|window, cx| view.read(cx).history_page.input.read(cx).focus_handle(cx).is_focused(window)));
    cx.simulate_input(" search");
    assert_eq!(view.read_with(cx, |w, cx| w.history_page.input.read(cx).text().to_string()), "retained search");
    let calls = fake.take_calls();
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_) | Call::InspectorFocus(_))), "{calls:?}");
}

#[gpui::test]
fn minimize_without_popup_preserves_docked_devtools_focus_on_activation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.update(|window, _| window.activate_window());
    accept_recording_inspector(&view, cx, &fake);
    cx.simulate_keystrokes("cmd-alt-i");
    cx.run_until_parked();
    let inspector = inspector_status(&view, cx).unwrap().id;
    inspector_event(&fake, cx, engine::InspectorEvent::Focused { inspector });
    let (_, mut other) = another_profile_window(&view, cx);
    other.update(|window, _| window.activate_window());
    other.run_until_parked();
    fake.take_calls();
    view.update(cx, |w, cx| w.close_popups_for_window_operation(true, cx));
    cx.run_until_parked();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert_eq!(inspector_status(&view, cx).unwrap().id, inspector);
    let calls = fake.take_calls();
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))), "activation must not replace the frontend responder: {calls:?}");
    // Reconciliation after activation must continue respecting frontend focus.
    cx.cx.refresh().unwrap();
    cx.run_until_parked();
    let calls = fake.take_calls();
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))), "{calls:?}");
}

#[gpui::test]
fn fullscreen_targets_the_main_window_from_pages_and_overlays(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("ctrl-cmd-f");
    assert!(cx.update(|window, _| window.is_fullscreen()));
    fake.send(cx, fake.shortcut(Shortcut { ctrl: true, cmd: true, key: "f".into(), ..Default::default() }));
    assert!(!cx.update(|window, _| window.is_fullscreen()));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_keystrokes("ctrl-cmd-f");
    assert_eq!(bar_text(&view, cx), None);
    assert!(cx.update(|window, _| window.is_fullscreen()));
    cx.simulate_keystrokes("cmd-f");
    find_overlay(&view, cx).simulate_keystrokes("ctrl-cmd-f");
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
    assert!(!cx.update(|window, _| window.is_fullscreen()));
    for (query, expected) in [("enter full screen", true), ("enter full screen", true), ("exit full screen", false)] {
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        bar(&view, cx).simulate_keystrokes("enter");
        assert_eq!(cx.update(|window, _| window.is_fullscreen()), expected);
    }
}

fn record_history_fixture(fake: &FakeEngine, cx: &mut VisualTestContext) {
    for (url, title) in [("https://example.com/one", "First page"), ("https://example.com/two", "Second page")] {
        fake.send(cx, Event::UrlChanged { view: page(1), url: url.into() });
        fake.send(cx, Event::TitleChanged { view: page(1), title: title.into() });
    }
}

#[gpui::test]
fn history_opens_a_gpui_today_tab_without_a_chromium_view(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    fake.take_calls();
    let before = today_urls(&view, cx).len();
    cx.simulate_keystrokes("cmd-y");
    assert_eq!(active_url(&view, cx).as_deref(), Some(crate::history::HISTORY_URL));
    assert_eq!(today_urls(&view, cx).len(), before + 1);
    assert!(cx.debug_bounds("history-page").is_some());
    assert!(cx.update(|window, cx| view.read(cx).history_page.input.read(cx).focus_handle(cx).is_focused(window)));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Visible(1, false)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(_, _))));
    cx.simulate_keystrokes("cmd-y");
    assert_eq!(today_urls(&view, cx).len(), before + 1);
    cx.simulate_input("first page");
    cx.simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com/one"));
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::Create(_, url) if url == "https://example.com/one")));
}

#[gpui::test]
fn history_deletion_and_confirmation_preserve_tabs_and_can_be_cancelled(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-y");
    let tabs = today_urls(&view, cx);
    click(cx, "history-remove-0");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().history.len()), 1);
    assert_eq!(active_url(&view, cx).as_deref(), Some(crate::history::HISTORY_URL));
    click(cx, "clear-history");
    click(cx, "history-clear-cancel");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().history.len()), 1);
    click(cx, "clear-history");
    cx.simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().history.len()), 1);
    click(cx, "clear-history");
    click(cx, "history-clear-confirm");
    assert!(view.read_with(cx, |w, _| w.browser.active_user_profile().history.is_empty()));
    assert_eq!(today_urls(&view, cx), tabs);
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
}

#[gpui::test]
fn internal_navigation_closes_the_old_engine_view_and_returns_focus_to_gpui(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("browser://history");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some(crate::history::HISTORY_URL));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)));
    assert!(calls.contains(&Call::FocusUi));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(_, url) if url.starts_with("browser://"))));
    cx.simulate_input("search text");
    cx.simulate_keystrokes("cmd-f");
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
    cx.simulate_input("replacement query");
    assert_eq!(view.read_with(cx, |w, cx| w.history_page.input.read(cx).text().to_string()), "replacement query");
}

#[gpui::test]
fn history_actions_work_from_the_command_bar_and_menu(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("view history");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(active_url(&view, cx).as_deref(), Some(crate::history::HISTORY_URL));
    let count = today_urls(&view, cx).len();
    dispatch_menu(cx, "Archive", "View History");
    assert_eq!(today_urls(&view, cx).len(), count);
}

#[gpui::test]
fn native_menu_new_tab_toggles_an_existing_overlay_without_leaking_windows(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("old query");
    for _ in 0..3 {
        // macOS menu dispatch may target the main window while a child is key.
        dispatch_menu(cx, "File", "New Tab…");
        assert_eq!(bar_text(&view, cx), None);
        assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
        dispatch_menu(cx, "File", "New Tab…");
        assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
        assert!(bar_input_focused(&view, cx));
        assert_eq!(cx.cx.update(|cx| cx.windows().len()), 2);
    }
}

#[gpui::test]
fn repeating_command_bar_shortcuts_dismisses_and_different_shortcuts_switch_mode(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("unfinished query");
    bar(&view, cx).simulate_keystrokes("cmd-t");
    assert_eq!(bar_text(&view, cx), None);
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_keystrokes("cmd-l");
    assert_eq!(bar_text(&view, cx), None);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_keystrokes("cmd-l");
    assert_eq!(bar_text(&view, cx), active_url(&view, cx));
    bar(&view, cx).simulate_keystrokes("cmd-t");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    assert_eq!(today_urls(&view, cx), before);
}

#[gpui::test]
fn confirming_the_current_url_reloads_without_replacing_the_title(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, Event::TitleChanged { view: page(1), title: "Page title".into() });
    fake.take_calls();
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_keystrokes("enter");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Reload(1)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Navigate(..) | Call::Create(..))));
    assert_eq!(today_urls(&view, cx), before);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Page title");
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn command_bar_finds_closed_pages_in_history_and_opens_them(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-w cmd-t");
    bar(&view, cx).simulate_input("first page");
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(matches!(&suggestions[1], Suggestion::History { title, url } if title == "First page" && url == "https://example.com/one"));
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com/one"));
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn history_completion_in_cmd_l_navigates_the_current_tab_and_keeps_its_custom_name(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-w cmd-t");
    bar(&view, cx).simulate_input("rename tab");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_input("Custom label");
    cx.simulate_keystrokes("enter");
    let before = today_urls(&view, cx).len();
    let id = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("first page");
    click(&mut bar(&view, cx), "suggestion-1");
    assert_eq!(today_urls(&view, cx).len(), before);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), id);
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com/one"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Custom label");
}

#[gpui::test]
fn open_tabs_match_their_page_titles_without_duplicate_history_rows(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename tab");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_input("Renamed tab");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("second page");
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(suggestions.iter().any(|suggestion| matches!(suggestion, Suggestion::SwitchToTab { title, .. } if title == "Renamed tab")));
    assert!(!suggestions.iter().any(|suggestion| matches!(suggestion, Suggestion::History { url, .. } if url == "https://example.com/two")));
}

#[gpui::test]
fn deleting_a_history_suggestion_keeps_the_bar_and_does_not_open_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-w cmd-t");
    let before = today_urls(&view, cx);
    bar(&view, cx).simulate_input("first page");
    bar(&view, cx).simulate_keystrokes("down");
    click(&mut bar(&view, cx), "delete-suggestion-1");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("first page"));
    assert!(bar_input_focused(&view, cx));
    assert_eq!(today_urls(&view, cx), before);
    view.read_with(cx, |w, cx| {
        assert_eq!(w.command_bar.as_ref().unwrap().selected, 0);
        assert!(!w.suggestions(cx).iter().any(|s| matches!(s, Suggestion::History { .. })));
        let restored = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(restored.active_user_profile().history.len(), 1);
        assert_eq!(restored.active_user_profile().history[0].url, "https://example.com/two");
        assert_eq!(restored.archive.len(), w.browser.archive.len());
    });
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("first page");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 1);
}

#[gpui::test]
fn favorites_are_shared_and_keep_the_loaded_page_across_spaces(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx).unwrap();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("favorite tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().favorites.len()), 1);
    assert!(cx.debug_bounds("favorite-8").is_some());
    fake.take_calls();
    cx.simulate_keystrokes("ctrl-2");
    assert!(active_url(&view, cx).is_none());
    click(cx, "favorite-8");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 1);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    // Favorites come before the Space's Pinned and Today items by default.
    cx.simulate_keystrokes("ctrl-1 cmd-2");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com"));
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("chromium");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|s| matches!(s, Suggestion::SwitchToTab { space, .. } if space == "Favorites"))));
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
}

#[gpui::test]
fn closing_a_favorite_unloads_it_and_clicking_it_reopens_the_original_url(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx).unwrap();
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1");
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://example.com/favorite-navigation".into() });
    fake.take_calls();
    let archive = view.read_with(cx, |w, _| w.browser.archive.len());
    // Favorites have no hover close button; the context menu closes them.
    assert!(cx.debug_bounds("close-favorite-8").is_none());
    right_click(cx, "favorite-8");
    click(&mut tab_menu(&view, cx), "tab-menu-close");
    assert!(fake.take_calls().contains(&Call::Close(1)));
    assert!(active_url(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), archive);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().favorites.len()), 1);
    click(cx, "favorite-8");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
    assert!(fake.take_calls().contains(&Call::Create(2, original)));
}

#[gpui::test]
fn unfavorite_returns_the_shared_tab_to_the_current_spaces_today(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("unfavorite tab");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert!(w.browser.active_user_profile().favorites.is_empty());
        assert_eq!(w.browser.space().today.len(), 1);
        assert_eq!(w.browser.space().today[0].id.get(), 8);
        let restored = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert!(restored.active_user_profile().favorites.is_empty());
        assert_eq!(restored.active_tab().unwrap().id.get(), 8);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn a_favorite_can_move_to_the_current_spaces_pinned_section(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("move to pinned in work");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert!(w.browser.active_user_profile().favorites.is_empty());
        assert_eq!(w.browser.space().pinned[0].id.get(), 8);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        assert_ne!(w.browser.spaces[0].active_tab, w.browser.space().active_tab);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn engine_favicons_follow_tabs_into_favorites_and_survive_closing_and_sessions(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx).unwrap();
    let png = include_bytes!("../testdata/favicon.png").to_vec();
    fake.send(cx, Event::Favicon { view: page(1), page_url: original.clone(), png: png.clone() });
    assert_eq!(view.read_with(cx, |w, _| w.browser.favicon(w.browser.active_user_profile_id(), &original).map(Vec::from)), Some(png.clone()));
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1");
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://elsewhere.example/page".into() });
    cx.simulate_keystrokes("cmd-w");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_user_profile().favorites[0].url, original);
        let restored = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(restored.favicon(restored.active_user_profile_id(), &original), Some(png.as_slice()));
    });
    click(cx, "favorite-8");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
}

fn right_click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let center = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_move(center, None, Modifiers::none());
    cx.simulate_mouse_down(center, gpui::MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(center, gpui::MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
}

fn tab_menu(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| w.tab_menu.as_ref().and_then(|menu| menu.overlay)).unwrap();
    VisualTestContext::from_window(overlay.into(), cx)
}

#[gpui::test]
fn context_copy_uses_the_clicked_tab_and_keeps_the_current_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-4");
    assert_eq!(active_url(&view, cx), original);
    let mut menu = tab_menu(&view, cx);
    let panel = menu.debug_bounds("tab-context-menu").unwrap();
    let size = menu.update(|window, _| window.bounds().size);
    assert!(panel.origin.x >= gpui::px(0.) && panel.right() <= size.width);
    assert!(panel.origin.y >= gpui::px(0.) && panel.bottom() <= size.height);
    menu.simulate_keystrokes("down enter");
    assert_eq!(cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())).as_deref(), Some("https://github.com"));
    assert_eq!(active_url(&view, cx), original);
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Close(..))));
}

#[gpui::test]
fn context_pin_and_rename_do_not_activate_the_clicked_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let original = active_url(&view, cx);
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-pin");
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned.iter().any(|tab| tab.id.get() == 7)));
    assert_eq!(active_url(&view, cx), original);
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-rename");
    assert_eq!(inline_name(&view, cx).as_deref(), Some("GPUI"));
    click(cx, "inline-tab-rename");
    assert_eq!(active_url(&view, cx), original);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("Renamed from menu");
    cx.simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().find(|tab| tab.id.get() == 7).unwrap().title.clone()), "Renamed from menu");
    assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
}

#[gpui::test]
fn context_favorite_and_unfavorite_preserve_the_other_active_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let original = active_url(&view, cx);
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-favorite");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().favorites[0].id.get()), 7);
    right_click(cx, "favorite-7");
    click(&mut tab_menu(&view, cx), "tab-menu-favorite");
    assert_eq!(active_url(&view, cx), original);
    assert!(view.read_with(cx, |w, _| w.browser.active_user_profile().favorites.is_empty()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().today[0].id.get()), 7);
}

#[gpui::test]
fn context_archive_targets_the_clicked_tab_and_escape_or_outside_click_cancel(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let original = active_url(&view, cx);
    for _ in 0..3 {
        right_click(cx, "tab-7");
        tab_menu(&view, cx).simulate_keystrokes("escape");
        assert!(view.read_with(cx, |w, _| w.tab_menu.is_none()));
        assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
    }
    right_click(cx, "tab-7");
    let mut menu = tab_menu(&view, cx);
    menu.simulate_click(gpui::point(gpui::px(900.), gpui::px(650.)), Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_none()));
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-close");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive[0].tab.id.get()), 7);
}

#[gpui::test]
fn context_reset_and_replace_original_url_work_on_an_inactive_pinned_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "tab-4");
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("https://github.com/elsewhere");
    bar(&view, cx).simulate_keystrokes("enter");
    click(cx, "tab-8");
    let original = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-4");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-edit-pinned-page");
    click(&mut menu, "tab-menu-reset");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned[0].url.clone()), "https://github.com");
    assert!(fake.take_calls().contains(&Call::Close(2)), "reset unloads an inactive page immediately");
    click(cx, "tab-4");
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("https://github.com/new-original");
    bar(&view, cx).simulate_keystrokes("enter");
    click(cx, "tab-8");
    right_click(cx, "tab-4");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-edit-pinned-page");
    click(&mut menu, "tab-menu-replace");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned[0].pinned_url.clone()).as_deref(), Some("https://github.com/new-original"));
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-close");
    assert!(fake.take_calls().contains(&Call::Close(3)));
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned[0].url.clone()), "https://github.com/new-original");
}

#[gpui::test]
fn mute_command_and_sidebar_button_control_the_current_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("mute tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(fake.take_calls().contains(&Call::Mute(1, true)));
    assert!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
    click(cx, "unmute-8");
    assert!(fake.take_calls().contains(&Call::Mute(1, false)));
    assert_eq!(active_url(&view, cx), original);
    assert!(!view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
}

#[gpui::test]
fn context_muting_a_background_page_does_not_activate_it(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "tab-7");
    click(cx, "tab-8");
    fake.take_calls();
    let original = active_url(&view, cx);
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-mute");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Mute(2, true)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::FocusPage(2))));
    assert_eq!(active_url(&view, cx), original);
    click(cx, "unmute-7");
    assert!(fake.take_calls().contains(&Call::Mute(2, false)));
    assert_eq!(active_url(&view, cx), original);
}

#[gpui::test]
fn mute_all_reaches_unloaded_tabs_and_unmute_all_updates_background_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    dispatch_menu(cx, "Tabs", "Mute All Tabs");
    assert!(view.read_with(cx, |w, _| w.browser.all_tabs().all(|tab| tab.muted)));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Mute(1, true)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..))));
    cx.simulate_keystrokes("cmd-1");
    let calls = fake.take_calls();
    let mute = calls.iter().position(|call| *call == Call::Mute(2, true)).unwrap();
    let show = calls.iter().position(|call| *call == Call::Visible(2, true)).unwrap();
    assert!(mute < show, "a restored muted page is muted before being shown: {calls:?}");
    dispatch_menu(cx, "Tabs", "Unmute All Tabs");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Mute(1, false)) && calls.contains(&Call::Mute(2, false)));
    assert!(!view.read_with(cx, |w, _| w.browser.all_tabs().any(|tab| tab.muted)));
}

#[gpui::test]
fn a_closed_muted_favorite_reopens_muted(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    dispatch_menu(cx, "Tabs", "Mute/Unmute Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1 cmd-w");
    fake.take_calls();
    click(cx, "favorite-8");
    let calls = fake.take_calls();
    let mute = calls.iter().position(|call| *call == Call::Mute(2, true)).unwrap();
    let focus = calls.iter().position(|call| *call == Call::FocusPage(2)).unwrap();
    assert!(mute < focus, "mute is applied before returning keyboard focus: {calls:?}");
    assert!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
}

#[gpui::test]
fn internal_history_pages_do_not_offer_mute_commands(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-y");
    dispatch_menu(cx, "Tabs", "Mute/Unmute Tab");
    assert!(!view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("mute tab");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|s| matches!(s, Suggestion::Action { command: crate::window::Command::ToggleMute, .. }))));
}

#[gpui::test]
fn engine_initialization_cannot_clear_a_saved_mute_intent(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Mute/Unmute Tab");
    fake.take_calls();
    // CEF initialization may replace the host's initial audio flag after creation.
    fake.0.borrow_mut().muted.clear();
    fake.send(cx, Event::LoadingChanged { view: page(1), loading: false, can_go_back: false, can_go_forward: false });
    assert!(fake.take_calls().contains(&Call::Mute(1, true)));
    assert!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().audio_muted(w.browser.active_tab().unwrap().id)), Some(true));
}

fn open_split_pages(cx: &mut TestAppContext, count: usize) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new();
    let now = std::time::SystemTime::now();
    let first = browser.new_tab("https://pane0.example", now);
    for index in 1..count {
        let next = browser.new_tab(format!("https://pane{index}.example"), now);
        browser.add_split(first, next, browser_core::splits::Placement::Right, now).unwrap();
    }
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    (view, cx, fake)
}

#[gpui::test]
fn split_pages_are_visible_together_with_nonoverlapping_native_surfaces(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 4);
    let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
    assert_eq!(surfaces.len(), 4);
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 4);
    for id in 1..=4 { assert!(calls.contains(&Call::Visible(id, true))); }
    for pair in surfaces.windows(2) {
        assert!(pair[0].1.x + pair[0].1.width + crate::split_layout::GAP <= pair[1].1.x + 0.001);
        assert_eq!(pair[0].1.y, pair[1].1.y);
    }
    assert!(calls.contains(&Call::FocusPage(2)), "the last added pane is inserted beside the anchor and focused");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 4);
}

#[gpui::test]
fn cef_pane_focus_updates_shortcut_targets_without_refocusing_or_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    fake.send(cx, Event::Focused { view: page(1) });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    assert!(fake.take_calls().is_empty(), "native focus is already correct; do not bounce it back to CEF");
    cx.simulate_keystrokes("cmd-r");
    assert!(fake.take_calls().contains(&Call::Reload(1)));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("unfinished query");
    fake.send(cx, Event::Focused { view: page(2) });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    assert!(bar_input_focused(&view, cx));
}

#[gpui::test]
fn switching_split_orientation_keeps_existing_views_and_navigation_state(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    view.update(cx, |w, cx| { let id = w.browser.active_tab().unwrap().id; w.edit_browser(|browser| browser.set_split_axis(id, browser_core::splits::Axis::Vertical)); cx.notify(); });
    cx.run_until_parked();
    let frames = fake.0.borrow().bounds.clone();
    assert_eq!(frames[&1].x, frames[&2].x);
    assert!(frames[&1].y + frames[&1].height < frames[&2].y);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn closing_and_restoring_a_split_closes_and_recreates_all_its_engine_views(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    let anchor = view.read_with(cx, |w, _| w.browser.active_split().unwrap().anchor.get());
    click(cx, test_selector(format!("close-{anchor}")));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)) && calls.contains(&Call::Close(2)));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    assert!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().is_empty()));
    cx.simulate_keystrokes("cmd-shift-t");
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 2);
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 2);
}

#[gpui::test]
fn hidden_split_pages_cannot_steal_the_new_tabs_shortcut_target(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://outside.example");
    bar(&view, cx).simulate_keystrokes("enter");
    fake.take_calls();
    fake.send(cx, Event::Focused { view: page(1) });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://outside.example"));
    cx.simulate_keystrokes("cmd-r");
    assert!(fake.take_calls().contains(&Call::Reload(3)));
}

fn run_named_command(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, text: &str) {
    cx.simulate_keystrokes("cmd-t");
    bar(view, cx).simulate_input(text);
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).first(), Some(Suggestion::Action { .. }))), "missing command: {text}");
    bar(view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn split_picker_is_cancelable_and_can_use_an_existing_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |w, _| w.browser.all_tabs().count());
    cx.simulate_keystrokes("ctrl-shift-=");
    assert!(bar_input_focused(&view, cx));
    assert!(view.read_with(cx, |w, _| matches!(w.command_bar.as_ref().unwrap().target, crate::window::Target::Split { .. })));
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), before);
    run_named_command(&view, cx, "add split view");
    bar(&view, cx).simulate_input("github");
    bar(&view, cx).simulate_keystrokes("down enter");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_ne!(w.browser.active_tab().unwrap().id.get(), 4);
        assert_eq!(w.browser.active_tab().unwrap().url, "https://github.com");
        assert!(w.browser.space().pinned.iter().any(|tab| tab.id.get() == 4));
        assert_eq!(w.browser.all_tabs().count(), before + 1);
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.browser.is_split_secondary(w.browser.active_tab().unwrap().id)));
    assert!(cx.debug_bounds("tab-8").is_some());
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 2);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Close(..))));
}

#[gpui::test]
fn split_picker_adds_urls_in_the_requested_direction_and_enforces_four_panes(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    run_named_command(&view, cx, "add top split");
    bar(&view, cx).simulate_input("https://third.example");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        assert_eq!(split.axis, browser_core::splits::Axis::Vertical);
        assert_eq!(split.panes[1], w.browser.active_tab().unwrap().id);
    });
    cx.simulate_keystrokes("ctrl-shift-=");
    bar(&view, cx).simulate_input("https://fourth.example");
    bar(&view, cx).simulate_keystrokes("enter");
    let count = view.read_with(cx, |w, _| w.browser.all_tabs().count());
    cx.simulate_keystrokes("ctrl-shift-=");
    assert_eq!(bar_text(&view, cx), None);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 4);
        assert_eq!(w.browser.all_tabs().count(), count);
        assert_eq!(w.toast.as_ref().unwrap().message, "Cannot Add New Pane");
    });
}

#[gpui::test]
fn pane_header_focus_and_chrome_match_native_geometry_in_both_axes(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    fake.take_calls();
    click(cx, "pane-header-2");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 2);
    assert!(fake.take_calls().contains(&Call::FocusPage(1)));
    for vertical in [false, true] {
        if vertical { run_named_command(&view, cx, "convert to vertical split"); }
        let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
        for (id, body) in surfaces {
            let selector = match id.get() { 2 => "pane-header-2", 3 => "pane-header-3", 5 => "pane-header-5", _ => panic!("unexpected pane") };
            let header = cx.debug_bounds(selector).unwrap();
            assert!(header.origin.y >= gpui::px(8.));
            assert!((f64::from(f32::from(header.origin.x)) - body.x).abs() < 0.6);
            assert!((f64::from(f32::from(header.bottom())) - body.y).abs() < 0.6, "vertical={vertical} header={header:?} body={body:?}");
            assert!((f64::from(f32::from(header.size.width)) - body.width).abs() < 0.6);
        }
    }
}

#[gpui::test]
fn pane_close_shortcut_archives_only_the_selected_page_and_header_separates_without_reload(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let closed = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("ctrl-shift--");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.browser.archive.last().unwrap().tab.id, closed);
    });
    fake.take_calls();
    click(cx, "pane-menu-2");
    click(&mut tab_menu(&view, cx), "tab-menu-separate-pane");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 2);
    assert!(cx.debug_bounds("tab-2").is_some());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn split_sidebar_item_remembers_its_pane_and_favorites_show_one_group(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    click(cx, "pane-header-2");
    run_named_command(&view, cx, "favorite tab");
    assert!(cx.debug_bounds("favorite-2").is_some());
    assert!(cx.debug_bounds("favorite-3").is_none());
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://outside.example");
    bar(&view, cx).simulate_keystrokes("enter");
    click(cx, "favorite-2");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    run_named_command(&view, cx, "rename tab");
    bar(&view, cx).simulate_input("Comparison");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| assert_eq!(w.browser.item_title(w.browser.active_tab().unwrap().id), "Comparison"));
    run_named_command(&view, cx, "reset tab name");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().title.is_none()));
}

#[gpui::test]
fn split_commands_move_focus_order_and_separate_without_navigation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    fake.take_calls();
    run_named_command(&view, cx, "focus pane 1");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    run_named_command(&view, cx, "move pane right");
    view.read_with(cx, |w, _| assert_eq!(w.browser.active_split().unwrap().panes[1], w.browser.active_tab().unwrap().id));
    run_named_command(&view, cx, "focus next pane");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane1.example"));
    run_named_command(&view, cx, "separate all tabs");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn expanding_a_split_from_the_menu_and_command_bar_keeps_panes_loaded(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let original = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    fake.take_calls();
    dispatch_menu(cx, "View", "Expand Current Split");
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        let active = split.panes.iter().position(|id| *id == split.active).unwrap();
        assert!(split.weights.iter().enumerate().all(|(index, weight)| index == active || *weight < split.weights[active]));
    });
    for vertical in [false, true] {
        if vertical { run_named_command(&view, cx, "convert to vertical split"); }
        click(cx, "pane-header-2");
        run_named_command(&view, cx, "expand current split");
        let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
        let active = surfaces.iter().find(|(id, _)| id.get() == 2).unwrap().1;
        for (_, bounds) in surfaces.iter().filter(|(id, _)| id.get() != 2) {
            let (large, small) = if vertical { (active.height, bounds.height) } else { (active.width, bounds.width) };
            assert!(large >= small, "active pane should expand: {surfaces:?}");
            assert!((small - if vertical { 220. - crate::split_layout::HEADER_HEIGHT - 2. * crate::split_layout::BORDER } else { 220. - 2. * crate::split_layout::BORDER }).abs() < 1.);
        }
        view.read_with(cx, |w, _| {
            let split = w.browser.active_split().unwrap();
            assert_eq!(split.id, original.id);
            assert_eq!(split.panes, original.panes);
            assert_eq!(split.active.get(), 2);
            let restored = browser_core::session::from_json(&w.session_json()).unwrap();
            assert_eq!(restored.active_split().unwrap().weights, split.weights);
        });
    }
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn expand_current_split_does_nothing_to_a_standalone_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    dispatch_menu(cx, "View", "Expand Current Split");
    assert_eq!(active_url(&view, cx), original);
    assert!(fake.take_calls().is_empty());
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("expand current split");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|row| matches!(row, Suggestion::Action { command: crate::window::Command::ExpandCurrentSplit, .. }))));
}

#[gpui::test]
fn expanding_four_panes_in_a_small_window_keeps_every_page_visible(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 4);
    cx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(480.)));
    cx.run_until_parked();
    fake.take_calls();
    dispatch_menu(cx, "View", "Expand Current Split");
    let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
    assert_eq!(surfaces.len(), 4);
    for (_, bounds) in &surfaces {
        assert!(bounds.width > 1. && bounds.height > 1.);
        assert!((bounds.width - surfaces[0].1.width).abs() < 1.);
    }
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Visible(_, false))));
}

#[gpui::test]
fn command_search_and_archive_search_preserve_group_name_and_match_secondary_urls(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    run_named_command(&view, cx, "rename tab");
    bar(&view, cx).simulate_input("Comparison");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pane1");
    let rows = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert_eq!(rows.iter().filter(|row| matches!(row, Suggestion::SwitchToTab { .. })).count(), 1);
    assert!(rows.iter().any(|row| matches!(row, Suggestion::SwitchToTab { title, .. } if title == "Comparison")));
    bar(&view, cx).simulate_keystrokes("escape");
    let anchor = view.read_with(cx, |w, _| w.browser.active_split().unwrap().anchor.get());
    click(cx, test_selector(format!("close-{anchor}")));
    open_archive(&view, cx);
    bar(&view, cx).simulate_input("pane1");
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).first(), Some(Suggestion::ArchivedTab { title, .. }) if title == "Comparison")));
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 2);
}

#[gpui::test]
fn internal_pages_cannot_join_split_view_and_native_menu_uses_the_same_picker(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    dispatch_menu(cx, "View", "Add Split View…");
    bar(&view, cx).simulate_input("browser://history");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    bar(&view, cx).simulate_keystrokes("escape");
    dispatch_menu(cx, "Archive", "View History");
    cx.simulate_keystrokes("ctrl-shift-=");
    assert_eq!(bar_text(&view, cx), None);
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
}

#[gpui::test]
fn split_shortcuts_from_native_page_focus_use_the_same_picker_and_close_action(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let shortcut = |key: &str| Shortcut { ctrl: true, shift: true, key: key.into(), ..Default::default() };
    let shortcuts = crate::window::page_shortcuts();
    assert!(shortcuts.contains(&shortcut("=")));
    assert!(shortcuts.contains(&shortcut("-")));
    fake.send(cx, fake.shortcut(shortcut("=")));
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_keystrokes("escape");
    fake.send(cx, fake.shortcut(shortcut("-")));
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
}

#[gpui::test]
fn pane_menu_targets_an_inactive_pane_and_moves_it_without_changing_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let active = active_url(&view, cx);
    fake.take_calls();
    click(cx, "pane-menu-2");
    assert_eq!(active_url(&view, cx), active);
    click(&mut tab_menu(&view, cx), "tab-menu-move-forward");
    assert_eq!(active_url(&view, cx), active);
    view.read_with(cx, |w, _| assert_eq!(w.browser.active_split().unwrap().panes[1].get(), 2));
    right_click(cx, "pane-header-2");
    click(&mut tab_menu(&view, cx), "tab-menu-orientation");
    view.read_with(cx, |w, _| assert_eq!(w.browser.active_split().unwrap().axis, browser_core::splits::Axis::Vertical));
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn sidebar_tab_options_mute_only_their_pane_and_header_close_preserves_other_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let panes = view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.clone());
    for pane in panes.iter().take(2) {
        right_click(cx, "tab-2");
        let mut menu = tab_menu(&view, cx);
        let title = view.read_with(cx, |w, _| crate::tab_menu::short_title(&w.browser.all_tabs().find(|tab| tab.id == *pane).unwrap().title));
        let selector = format!("tab-menu-{}", title.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>());
        let selector = selector.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-");
        let row = menu.debug_bounds(test_selector(selector)).unwrap();
        menu.simulate_mouse_move(row.center(), None, Modifiers::none());
        menu.run_until_parked();
        click(&mut menu, "tab-menu-mute");
        assert!(view.read_with(cx, |w, _| w.browser.all_tabs().find(|tab| tab.id == *pane).unwrap().muted));
    }
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().filter(|tab| tab.muted).count()), 2);
    let active = active_url(&view, cx);
    click(cx, "pane-close-2");
    assert_eq!(active_url(&view, cx), active);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.browser.archive.last().unwrap().tab.id.get(), 2);
    });
    assert!(fake.take_calls().contains(&Call::Close(1)));
}

#[gpui::test]
fn pane_menu_adds_at_the_clicked_pane_and_separates_it_with_its_existing_engine(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    click(cx, "pane-menu-2");
    click(&mut tab_menu(&view, cx), "tab-menu-add-left");
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("https://left.example");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes[0], w.browser.active_tab().unwrap().id);
        assert_eq!(w.browser.active_split().unwrap().panes[1].get(), 2);
    });
    fake.take_calls();
    click(cx, "pane-menu-2");
    click(&mut tab_menu(&view, cx), "tab-menu-separate-pane");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn sidebar_split_menu_resets_group_name_and_separates_the_whole_group(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    right_click(cx, "tab-2");
    click(&mut tab_menu(&view, cx), "tab-menu-rename");
    bar(&view, cx).simulate_input("Comparison");
    bar(&view, cx).simulate_keystrokes("enter");
    right_click(cx, "tab-2");
    click(&mut tab_menu(&view, cx), "tab-menu-reset-name");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().title.is_none()));
    right_click(cx, "tab-2");
    click(&mut tab_menu(&view, cx), "tab-menu-separate-all");
    assert!(view.read_with(cx, |w, _| w.browser.space().splits.is_empty()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 2);
}

#[gpui::test]
fn duplicate_command_creates_an_independent_page_beside_the_source(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let source = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    let url = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "duplicate current tab");
    view.read_with(cx, |w, _| {
        assert_ne!(w.browser.active_tab().unwrap().id, source);
        assert_eq!(w.browser.space().today[0].id, source);
        assert_eq!(w.browser.space().today[1].id, w.browser.active_tab().unwrap().id);
    });
    assert_eq!(active_url(&view, cx), url);
    assert_eq!(bar_text(&view, cx), None);
    let calls = fake.take_calls();
    assert!(calls.iter().any(|call| matches!(call, Call::Create(2, _))));
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    assert!(!fake.take_calls().contains(&Call::Close(1)));
    click(cx, "tab-8");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), source);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..))));
}

#[gpui::test]
fn context_duplicate_targets_the_clicked_pin_and_preserves_its_original_url(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-duplicate");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().pinned.len(), 3);
        assert_eq!(w.browser.space().pinned[0].id.get(), 4);
        assert_eq!(w.browser.space().pinned[1].id, w.browser.active_tab().unwrap().id);
        assert_eq!(w.browser.active_tab().unwrap().pinned_url.as_deref(), Some("https://github.com"));
    });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn duplicate_menu_copies_the_whole_split_and_closing_it_keeps_original_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let source = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    fake.take_calls();
    dispatch_menu(cx, "Tabs", "Duplicate Current Tab");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().splits.len(), 2);
        let copy = w.browser.active_split().unwrap();
        assert_ne!(copy.id, source.id);
        assert_eq!(copy.panes.len(), 3);
        assert!(copy.panes.iter().all(|id| !source.panes.contains(id)));
    });
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Create(..))).count(), 3);
    let anchor = view.read_with(cx, |w, _| w.browser.active_split().unwrap().anchor.get());
    click(cx, test_selector(format!("close-{anchor}")));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().id, source.id);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 3);
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Close(..))).count(), 3);
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
}

#[gpui::test]
fn native_duplicate_menu_dismisses_command_input_and_regular_context_menu_can_start_a_split(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-t");
    dispatch_menu(&mut bar(&view, cx), "Tabs", "Duplicate Current Tab");
    assert_eq!(bar_text(&view, cx), None);
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-add-right");
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("https://beside-pin.example");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert_ne!(w.browser.active_split().unwrap().anchor.get(), 4);
        assert!(w.browser.space().pinned.iter().any(|tab| tab.id.get() == 4));
        assert_eq!(w.browser.space().today.iter().filter(|tab| w.browser.related_tabs(w.browser.active_tab().unwrap().id).contains(&tab.id)).count(), 2);
    });
}

#[gpui::test]
fn context_archive_above_removes_the_active_page_and_focuses_its_reference_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "tab-7");
    click(cx, "tab-8");
    fake.take_calls();
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-archive-above");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.archive.len(), 1);
        assert_eq!(w.browser.archive[0].tab.id.get(), 8);
        assert_eq!(w.browser.space().pinned.len(), 2);
        assert_eq!(w.browser.space().today.len(), 2);
        assert_eq!(w.toast.as_ref().unwrap().message, "Archived 1 tab");
    });
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
}

#[gpui::test]
fn context_archive_below_an_inactive_reference_keeps_the_current_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "tab-6");
    click(cx, "tab-8");
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-archive-below");
    assert_eq!(active_url(&view, cx), active);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive[0].tab.id.get()), 6);
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(2)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::FocusPage(2))));
}

#[gpui::test]
fn archive_other_command_closes_a_split_atomically_and_restore_reopens_its_panes(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://reference.example");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    fake.take_calls();
    run_named_command(&view, cx, "archive other tabs");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.all_tabs().count(), 1);
        assert_eq!(w.browser.archive.len(), 1);
        assert_eq!(w.browser.archive[0].split.as_ref().unwrap().view.panes.len(), 3);
    });
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Close(..))).count(), 3);
    cx.simulate_keystrokes("cmd-shift-t");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 3);
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 3);
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Create(..))).count(), 3);
}

#[gpui::test]
fn a_tall_context_menu_fits_a_small_window_and_keyboard_selection_scrolls_to_the_last_row(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(320.)));
    cx.run_until_parked();
    right_click(cx, "tab-4");
    let mut menu = tab_menu(&view, cx);
    let panel = menu.debug_bounds("tab-context-menu").unwrap();
    let size = menu.update(|window, _| window.bounds().size);
    assert!(panel.origin.y >= gpui::px(8.) && panel.bottom() <= size.height - gpui::px(8.));
    menu.simulate_keystrokes("up");
    menu.run_until_parked();
    let close = menu.debug_bounds("tab-menu-close").unwrap();
    assert!(close.origin.y >= panel.origin.y && close.bottom() <= panel.bottom());
    menu.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_none() && w.browser.archive.is_empty()));
}

fn open_long_sidebar(cx: &mut TestAppContext, pinned: bool) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new();
    let now = std::time::SystemTime::now();
    let target = browser.new_tab("https://target.example", now);
    if pinned { browser.toggle_pin(target); }
    else {
        let second = browser.new_tab("https://secondary.example", now);
        browser.add_split(target, second, browser_core::splits::Placement::Right, now).unwrap();
    }
    for index in 0..30 {
        let id = browser.new_tab(format!("https://sidebar{index}.example"), now);
        if pinned { browser.toggle_pin(id); }
    }
    if pinned {
        let target = browser.space().pinned.last().unwrap().id;
        browser.activate_tab(target, now);
        browser.spaces[0].pinned_collapsed = true;
    }
    // Focus the group after the standalone pages have taken focus.
    if !pinned {
        let second = browser.space().splits[0].panes[1];
        browser.activate_tab(second, now);
    }
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    (view, cx, fake)
}

#[gpui::test]
fn reveal_tab_unfolds_and_scrolls_pinned_without_changing_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_long_sidebar(cx, true);
    let active = active_url(&view, cx);
    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    fake.take_calls();
    run_named_command(&view, cx, "reveal tab in sidebar");
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert!(w.sidebar_visible);
        assert!(!w.browser.space().pinned_collapsed);
    });
    assert_eq!(active_url(&view, cx), active);
    let row = cx.debug_bounds("tab-32").unwrap();
    let list = cx.debug_bounds("pinned-list").unwrap();
    assert!(row.origin.y >= list.origin.y && row.bottom() <= list.bottom(), "row={row:?}, list={list:?}");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn reveal_tab_scrolls_to_the_split_anchor_and_keeps_its_selected_pane(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_long_sidebar(cx, false);
    let active = active_url(&view, cx);
    fake.take_calls();
    dispatch_menu(cx, "Tabs", "Reveal Tab in Sidebar");
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    let row = cx.debug_bounds("tab-2").unwrap();
    let list = cx.debug_bounds("today-list").unwrap();
    assert!(row.origin.y >= list.origin.y && row.bottom() <= list.bottom(), "row={row:?}, list={list:?}");
    assert_eq!(active_url(&view, cx), active);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 3);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn reveal_tab_from_a_floating_sidebar_closes_the_overlay_and_restores_the_docked_sidebar(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-s");
    cx.simulate_mouse_move(gpui::point(gpui::px(2.), gpui::px(200.)), None, Modifiers::none());
    let mut sidebar = floating(&view, cx).unwrap();
    let active = active_url(&view, cx);
    fake.take_calls();
    std::thread::sleep(Duration::from_millis(200));
    sidebar.update(|_, cx| cx.refresh_windows());
    sidebar.run_until_parked();
    click(&mut sidebar, "address");
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("reveal tab in sidebar");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.sidebar_visible && w.floating_sidebar.is_none()));
    assert_eq!(active_url(&view, cx), active);
    assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn archive_reason_filters_preserve_the_query_and_combine_with_space_filters(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w");
    view.update(cx, |w, _| { w.edit_browser(|browser| browser.archive[0].reason = browser_core::ArchiveReason::Automatic); });
    open_archive(&view, cx);
    click(&mut bar(&view, cx), "archive-reason-automatic");
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).as_slice(), [Suggestion::ArchivedTab { title, .. }] if title == "Chromium")));
    bar(&view, cx).simulate_input("chromium");
    click(&mut bar(&view, cx), "archive-reason-manual");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    assert_eq!(bar_text(&view, cx).as_deref(), Some("chromium"));
    click(&mut bar(&view, cx), "archive-reason-all");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 1);
    bar(&view, cx).simulate_keystrokes("cmd-a backspace");
    click(&mut bar(&view, cx), "archive-reason-manual");
    click(&mut bar(&view, cx), "archive-this-space");
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).as_slice(), [Suggestion::ArchivedTab { title, .. }] if title == "GPUI")));
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
}

#[gpui::test]
fn archive_date_headings_do_not_change_keyboard_restore_indices(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w");
    view.update(cx, |w, _| {
        use chrono::{Local, TimeZone};
        let yesterday = Local::now().date_naive().pred_opt().unwrap().and_hms_opt(12, 0, 0).unwrap();
        w.edit_browser(|browser| browser.archive[0].archived_at = Local.from_local_datetime(&yesterday).single().unwrap().into());
    });
    open_archive(&view, cx);
    let mut overlay = bar(&view, cx);
    assert!(overlay.debug_bounds("archive-date-0").is_some());
    assert!(overlay.debug_bounds("archive-date-1").is_some());
    view.read_with(cx, |w, cx| {
        let rows = w.suggestions(cx);
        assert!(matches!(&rows[0], Suggestion::ArchivedTab { date, .. } if date == "Today"));
        assert!(matches!(&rows[1], Suggestion::ArchivedTab { date, .. } if date == "Yesterday"));
    });
    overlay.simulate_keystrokes("down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
}

#[gpui::test]
fn archive_show_more_reaches_older_entries_and_scrolls_inside_a_short_window(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |w, _| {
        let now = std::time::SystemTime::now();
        for index in 0..20 {
            let id = w.edit_browser(|browser| browser.new_tab(format!("https://archived{index}.example"), now));
            w.edit_browser(|browser| browser.close_tab(id, now));
        }
    });
    cx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(480.)));
    open_archive(&view, cx);
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).last(), Some(Suggestion::MoreArchive { remaining: 12 }))));
    let mut overlay = bar(&view, cx);
    for _ in 0..8 { overlay.simulate_keystrokes("down"); }
    overlay.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).last(), Some(Suggestion::MoreArchive { remaining: 4 }))));
    for _ in 0..8 { overlay.simulate_keystrokes("down"); }
    overlay.simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 20);
    for _ in 0..3 { overlay.simulate_keystrokes("down"); }
    overlay.run_until_parked();
    let last = overlay.debug_bounds("suggestion-19").unwrap();
    let list = overlay.debug_bounds("command-suggestions").unwrap();
    assert!(last.origin.y >= list.origin.y && last.bottom() <= list.bottom());
    let panel = overlay.debug_bounds("command-bar").unwrap();
    let size = overlay.update(|window, _| window.bounds().size);
    assert!(panel.bottom() <= size.height);
    overlay.simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://archived0.example"));
}

#[gpui::test]
fn space_header_visibility_changes_only_the_current_space_and_survives_sessions(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    let before = cx.debug_bounds("pinned-list").unwrap().origin.y;
    fake.take_calls();
    run_named_command(&view, cx, "hide space header");
    let after = cx.debug_bounds("pinned-list").unwrap().origin.y;
    assert!(after < before);
    assert_eq!(active_url(&view, cx), active);
    view.read_with(cx, |w, _| {
        assert!(!w.browser.space().show_header);
        assert!(w.browser.spaces[1].show_header);
        let saved = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert!(!saved.space().show_header);
    });
    dispatch_menu(cx, "Spaces", "Show/Hide Space Header");
    assert!(view.read_with(cx, |w, _| w.browser.space().show_header));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn numbered_sidebar_shortcuts_can_exclude_and_include_favorites(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 8);
    run_named_command(&view, cx, "exclude favorites from ordering");
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 4);
    cx.simulate_keystrokes("cmd-9");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 6);
    view.read_with(cx, |w, _| {
        let saved = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert!(!saved.active_user_profile().include_favorites_in_ordering);
    });
    dispatch_menu(cx, "Tabs", "Include/Exclude Favorites in Ordering");
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 8);
}

#[gpui::test]
fn hidden_cycle_shortcuts_work_from_ui_and_native_page_focus_and_keep_split_groups_together(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let split_page = active_url(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://next.example");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("ctrl-cmd-left");
    assert_eq!(active_url(&view, cx), split_page);
    cx.simulate_keystrokes("ctrl-cmd-right");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://next.example"));
    let shortcut = |key: &str| Shortcut { ctrl: true, cmd: true, key: key.into(), ..Default::default() };
    assert!(crate::window::page_shortcuts().contains(&shortcut("left")));
    assert!(crate::window::page_shortcuts().contains(&shortcut("right")));
    fake.send(cx, fake.shortcut(shortcut("left")));
    assert_eq!(active_url(&view, cx), split_page);
    fake.send(cx, fake.shortcut(shortcut("right")));
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://next.example"));
}

#[gpui::test]
fn tab_icon_picker_searches_complex_emoji_and_no_results_cannot_change_the_icon(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "customize tab icon");
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("two symbols");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(bar_text(&view, cx).is_some());
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("👨‍👩‍👧‍👦");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.item_icon(w.browser.active_tab().unwrap().id).map(str::to_string)).as_deref(), Some("👨‍👩‍👧‍👦"));
    assert!(cx.debug_bounds("item-icon-8").is_some());
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn context_icon_editor_targets_an_inactive_tab_and_reset_name_and_icon_restores_its_defaults(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-icon");
    click(&mut bar(&view, cx), "icon-picker-symbols");
    click(&mut bar(&view, cx), "icon-cell-0");
    assert!(bar_text(&view, cx).is_some());
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(active_url(&view, cx), active);
    view.read_with(cx, |w, _| assert_eq!(w.browser.space().pinned[0].custom_icon.as_deref(), Some("ion:star")));
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-reset-customization");
    assert_eq!(active_url(&view, cx), active);
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned[0].custom_icon.is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn split_group_icon_survives_favorite_and_separation_reveals_the_original_page_icons(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    dispatch_menu(cx, "Tabs", "Customize Tab Icon…");
    bar(&view, cx).simulate_input("🔬");
    bar(&view, cx).simulate_keystrokes("enter");
    run_named_command(&view, cx, "favorite tab");
    view.read_with(cx, |w, _| assert_eq!(w.browser.active_split().unwrap().custom_icon.as_deref(), Some("🔬")));
    cx.run_until_parked();
    assert!(cx.debug_bounds("item-icon-2").is_some());
    dispatch_menu(cx, "Tabs", "Reset Name and Icon");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().custom_icon.is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn space_icon_picker_searches_emoji_and_remove_restores_the_default(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    dispatch_menu(cx, "Spaces", "Change Space Icon…");
    bar(&view, cx).simulate_input("🧭");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("space-icon-0").is_some());
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().custom_icon.as_deref(), Some("🧭"));
        assert_eq!(w.browser.spaces[1].custom_icon, None);
        let saved = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(saved.space().custom_icon.as_deref(), Some("🧭"));
    });
    click(cx, "space-1");
    run_named_command(&view, cx, "change space icon");
    bar(&view, cx).simulate_input("👩🏽‍💻");
    bar(&view, cx).simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.browser.space().custom_icon.is_none()));
    click(cx, "space-0");
    dispatch_menu(cx, "Spaces", "Change Space Icon…");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    click(&mut bar(&view, cx), "icon-picker-remove");
    assert!(view.read_with(cx, |w, _| w.browser.space().custom_icon.is_none()));
    assert!(bar_text(&view, cx).is_some(), "removing an icon keeps the picker open");
    bar(&view, cx).simulate_keystrokes("escape");
}

fn open_folder_pages(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new();
    let now = std::time::SystemTime::now();
    let space = browser.space().id;
    let root = browser.create_folder(space, None, "Projects").unwrap();
    let nested = browser.create_folder(space, Some(root), "Research").unwrap();
    let a = browser.new_tab("https://a.example", now);
    browser.set_title(a, "A");
    browser.toggle_pin(a);
    let b = browser.new_tab("https://b.example", now);
    browser.set_title(b, "B");
    browser.add_split(a, b, browser_core::splits::Placement::Right, now).unwrap();
    browser.move_item_to_folder(a, nested);
    browser.new_tab("https://outside.example", now);
    browser.activate_tab(b, now);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    (view, cx, fake)
}

#[gpui::test]
fn folder_creation_and_nested_creation_keep_the_page_and_cancel_without_creating_items(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "new folder");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    bar(&view, cx).simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.is_empty()));
    dispatch_menu(cx, "Spaces", "New Folder…");
    bar(&view, cx).simulate_input("Projects");
    bar(&view, cx).simulate_keystrokes("enter");
    click(cx, "folder-new-9");
    bar(&view, cx).simulate_input("Research");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().folders.len(), 2);
        assert_eq!(w.browser.folder_path(w.browser.space().folders[1].id), "Projects / Research");
    });
    assert!(cx.debug_bounds("folder-10").unwrap().origin.x > cx.debug_bounds("folder-9").unwrap().origin.x);
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn context_new_folder_pins_the_clicked_tab_while_preserving_the_other_active_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-7");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-move-to");
    click(&mut menu, "tab-menu-new-folder-with-tab");
    bar(&view, cx).simulate_input("Reading");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        let folder = &w.browser.space().folders[0];
        assert_eq!(folder.name, "Reading");
        assert_eq!(w.browser.folder_tabs(folder.id).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![7]);
        assert!(!w.browser.space().today.iter().any(|tab| tab.id.get() == 7));
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(cx.debug_bounds("tab-7").unwrap().origin.x > cx.debug_bounds("folder-9").unwrap().origin.x);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn collapsing_a_folder_keeps_its_active_split_visible_and_reveal_unfolds_its_ancestors(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    fake.take_calls();
    click(cx, "folder-2");
    view.read_with(cx, |w, _| {
        assert!(w.browser.space().folders[0].collapsed);
        assert_eq!(w.browser.pinned_rows(w.browser.space().id, false).len(), 2);
    });
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 5);
    assert!(cx.debug_bounds("tab-4").is_some());
    click(cx, "tab-7");
    fake.take_calls();
    assert_eq!(view.read_with(cx, |w, _| w.browser.pinned_rows(w.browser.space().id, false).len()), 1);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("b.example");
    bar(&view, cx).simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 5);
    assert!(view.read_with(cx, |w, _| w.browser.space().folders[0].collapsed));
    run_named_command(&view, cx, "reveal tab in sidebar");
    view.read_with(cx, |w, _| {
        assert!(w.browser.space().folders.iter().all(|folder| !folder.collapsed));
        assert_eq!(w.browser.active_tab().unwrap().id.get(), 5);
    });
    assert!(cx.debug_bounds("tab-4").unwrap().origin.x > cx.debug_bounds("folder-3").unwrap().origin.x);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn moving_a_split_to_a_closed_folder_in_another_space_keeps_its_engine_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let destination = view.update(cx, |w, _| {
        let space = w.edit_browser(|browser| browser.add_space("Work", 100.));
        let folder = w.edit_browser(|browser| browser.create_folder(space, None, "Saved")).unwrap();
        w.edit_browser(|browser| browser.set_folder_collapsed(folder, true));
        folder
    });
    fake.take_calls();
    run_named_command(&view, cx, "move to folder");
    bar(&view, cx).simulate_input("Work");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert!(w.browser.folder(destination).unwrap().collapsed);
        assert_eq!(w.browser.active_tab().unwrap().id.get(), 5);
        assert_eq!(w.browser.folder_ancestors(w.browser.active_tab().unwrap().id), vec![destination]);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    right_click(cx, "folder-9");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-move");
    bar(&view, cx).simulate_input("pinned in personal");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Personal");
        assert_eq!(w.browser.active_tab().unwrap().id.get(), 5);
        assert_eq!(w.browser.folder_space(destination), Some(w.browser.space().id));
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn folder_context_actions_rename_customize_and_copy_all_nested_page_links(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "folder-3");
    tab_menu(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Research"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_))), "the closing menu must not focus Chromium behind its replacement editor");
    bar(&view, cx).simulate_input("References");
    bar(&view, cx).simulate_keystrokes("enter");
    right_click(cx, "folder-3");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-icon");
    assert_eq!(bar(&view, cx).debug_bounds("icon-picker").unwrap().size, gpui::size(gpui::px(272.), gpui::px(144.)));
    click(&mut bar(&view, cx), "icon-cell-0");
    view.read_with(cx, |w, _| {
        let folder = &w.browser.space().folders[1];
        assert_eq!(w.browser.folder_path(folder.id), "Projects / References");
        assert_eq!(folder.custom_icon.as_deref(), Some("ion:star"));
    });
    assert!(bar_text(&view, cx).is_some());
    bar(&view, cx).simulate_keystrokes("escape");
    right_click(cx, "folder-3");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-icon");
    click(&mut bar(&view, cx), "icon-picker-remove");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders[1].custom_icon.is_none()));
    assert!(bar_text(&view, cx).is_some());
    bar(&view, cx).simulate_keystrokes("escape");
    right_click(cx, "folder-2");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-copy-markdown");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some("[A](https://a.example)\n[B](https://b.example)"));
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn folder_destination_picker_rejects_cycles_and_empty_folder_deletion_keeps_other_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "folder-2");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-move");
    view.read_with(cx, |w, cx| {
        let root = w.browser.space().folders[0].id;
        let nested = w.browser.space().folders[1].id;
        assert!(!w.suggestions(cx).iter().any(|suggestion| matches!(suggestion, Suggestion::FolderDestination { parent: Some(parent), .. } if *parent == root || *parent == nested)));
    });
    bar(&view, cx).simulate_keystrokes("escape");
    run_named_command(&view, cx, "new folder");
    bar(&view, cx).simulate_input("Empty");
    bar(&view, cx).simulate_keystrokes("enter");
    right_click(cx, "folder-8");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-delete");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().folders.len()), 2);
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn named_folder_expansion_and_collapse_all_leave_the_selected_page_intact(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "collapse all folders");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.iter().all(|folder| folder.collapsed)));
    run_named_command(&view, cx, "expand folder Projects / Research");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.iter().all(|folder| !folder.collapsed)));
    dispatch_menu(cx, "Tabs", "Collapse All Folders");
    dispatch_menu(cx, "Tabs", "Expand All Folders");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.iter().all(|folder| !folder.collapsed)));
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn moving_to_a_folder_when_none_exist_creates_one_around_the_active_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "move to folder");
    assert!(bar_input_focused(&view, cx));
    assert!(view.read_with(cx, |w, _| matches!(w.command_bar.as_ref().unwrap().target, crate::window::Target::NewFolder { .. })));
    bar(&view, cx).simulate_input("Current project");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_tab().unwrap().id.get(), 8);
        assert_eq!(w.browser.folder_tabs(w.browser.space().folders[0].id).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![8]);
        assert_eq!(w.browser.space().pinned.iter().find(|tab| tab.id.get() == 8).unwrap().pinned_url.as_deref(), active.as_deref());
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn duplicating_a_closed_folder_keeps_focus_and_loads_independent_split_pages_only_when_selected(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let active = active_url(&view, cx);
    run_named_command(&view, cx, "collapse all folders");
    fake.take_calls();
    right_click(cx, "folder-2");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-duplicate");
    cx.run_until_parked();
    let (folder, nested, copy, focused) = view.read_with(cx, |w, _| {
        let folders = &w.browser.space().folders;
        assert_eq!(folders.len(), 4);
        let copy = folders[2].id;
        let nested = folders[3].id;
        assert!(folders[2].collapsed && folders[3].collapsed);
        let pages = w.browser.folder_tabs(copy);
        assert_eq!(pages.len(), 2);
        assert_eq!(w.browser.all_tabs().count(), 5);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        let focused = w.browser.split_for(pages[0]).unwrap().active;
        assert!(!pages.iter().any(|id| id.get() == 4 || id.get() == 5));
        (copy, nested, pages[0], focused)
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    click(cx, Box::leak(format!("folder-{}", folder.get()).into_boxed_str()));
    click(cx, Box::leak(format!("folder-{}", nested.get()).into_boxed_str()));
    click(cx, Box::leak(format!("tab-{}", copy.get()).into_boxed_str()));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().active_tab, Some(focused));
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        let restored = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(restored.folder_tabs(folder), w.browser.folder_tabs(folder));
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 2);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn duplicating_an_empty_folder_preserves_the_active_page_and_can_be_deleted_independently(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    run_named_command(&view, cx, "new folder");
    bar(&view, cx).simulate_input("Empty");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    fake.take_calls();
    right_click(cx, "folder-9");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-duplicate");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().folders.len()), 2);
    right_click(cx, "folder-10");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-delete");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().folders.iter().map(|folder| folder.id.get()).collect::<Vec<_>>()), vec![9]);
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn turning_a_folder_into_a_space_keeps_the_active_split_and_its_native_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
    fake.take_calls();
    right_click(cx, "folder-2");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-into-space");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Projects");
        assert_eq!(w.browser.spaces.len(), 2);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout(), surfaces);
        assert_eq!(w.browser.space().folders.len(), 1);
        assert_eq!(w.browser.space().folders[0].id.get(), 3);
        assert!(w.browser.spaces[0].pinned.is_empty());
        assert_eq!(w.browser.spaces[0].today[0].id.get(), 7);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn turning_an_empty_folder_into_a_space_leaves_other_tabs_and_their_engine_views_intact(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    run_named_command(&view, cx, "new folder");
    bar(&view, cx).simulate_input("Empty");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    fake.take_calls();
    right_click(cx, "folder-9");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-into-space");
    cx.run_until_parked();
    assert!(active_url(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Empty");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("alt-cmd-right");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn pin_to_a_named_nested_folder_in_another_space_preserves_the_active_split_and_closed_folders(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let destination = view.update(cx, |w, _| {
        let work = w.edit_browser(|browser| browser.add_space("Work", 100.));
        let root = w.edit_browser(|browser| browser.create_folder(work, None, "Saved")).unwrap();
        let nested = w.edit_browser(|browser| browser.create_folder(work, Some(root), "Reference")).unwrap();
        w.edit_browser(|browser| browser.set_folder_collapsed(root, true));
        w.edit_browser(|browser| browser.set_folder_collapsed(nested, true));
        nested
    });
    fake.take_calls();
    run_named_command(&view, cx, "pin to saved / reference");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.folder_ancestors(w.browser.space().active_tab.unwrap()).last(), Some(&destination));
        assert!(w.browser.space().folders.iter().all(|folder| folder.collapsed));
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.command_bar.is_none());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn pin_to_new_folder_edits_a_name_then_pins_the_current_page_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "pin to new folder");
    assert!(bar_input_focused(&view, cx));
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    bar(&view, cx).simulate_input("Reading");
    bar(&view, cx).simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.is_empty()));
    run_named_command(&view, cx, "pin to new folder");
    bar(&view, cx).simulate_input("Reading");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        let folder = &w.browser.space().folders[0];
        assert_eq!(folder.name, "Reading");
        assert_eq!(w.browser.folder_tabs(folder.id), vec![w.browser.space().active_tab.unwrap()]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn named_folder_commands_disambiguate_duplicate_names_by_space_and_skip_the_current_parent(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let destination = view.update(cx, |w, cx| {
        let work = w.edit_browser(|browser| browser.add_space("Work", 100.));
        let folder = w.edit_browser(|browser| browser.create_folder(work, None, "Research")).unwrap();
        let outside = w.browser.space().today[0].id;
        w.edit_browser(|browser| browser.activate_tab(outside, std::time::SystemTime::now()));
        cx.notify();
        folder
    });
    cx.run_until_parked();
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pin to research");
    let index = view.read_with(cx, |w, cx| {
        let suggestions = w.suggestions(cx);
        let labels: Vec<_> = suggestions.iter().filter_map(|row| match row { Suggestion::Action { command: crate::window::Command::PinToFolder(_), label } => Some(label.as_str()), _ => None }).collect();
        assert_eq!(labels, vec!["Pin to Projects / Research · Personal", "Pin to Research · Work"]);
        suggestions.iter().position(|row| matches!(row, Suggestion::Action { command: crate::window::Command::PinToFolder(id), .. } if *id == destination)).unwrap()
    });
    for _ in 0..index { bar(&view, cx).simulate_keystrokes("down"); }
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pin to research");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|row| matches!(row, Suggestion::Action { command: crate::window::Command::PinToFolder(id), .. } if *id == destination))));
}

#[gpui::test]
fn space_header_context_menu_can_rename_or_cancel_without_changing_the_active_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "space-header-0");
    click(&mut tab_menu(&view, cx), "tab-menu-space-rename");
    assert!(bar_input_focused(&view, cx));
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Personal"));
    bar(&view, cx).simulate_input("Cancel this");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Personal");
    right_click(cx, "space-header-0");
    click(&mut tab_menu(&view, cx), "tab-menu-space-rename");
    bar(&view, cx).simulate_input("Home");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Home");
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn inactive_space_context_actions_change_only_the_clicked_spaces_name_and_icon(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "space-1");
    click(&mut tab_menu(&view, cx), "tab-menu-space-icon");
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("🧭");
    bar(&view, cx).simulate_keystrokes("enter");
    right_click(cx, "space-1");
    click(&mut tab_menu(&view, cx), "tab-menu-space-rename");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Work"));
    bar(&view, cx).simulate_input("Office");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_space, 0);
        assert_eq!(w.browser.spaces[1].name, "Office");
        assert_eq!(w.browser.spaces[1].custom_icon.as_deref(), Some("🧭"));
        assert!(w.browser.spaces[0].custom_icon.is_none());
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn space_context_menu_remains_available_in_the_dock_after_hiding_its_header(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    let pinned_top = cx.debug_bounds("pinned-list").unwrap().top();
    fake.take_calls();
    // The reference's Space menu has no header toggle; the command remains.
    run_named_command(&view, cx, "hide space header");
    assert!(!view.read_with(cx, |w, _| w.browser.space().show_header));
    assert!(cx.debug_bounds("pinned-list").unwrap().top() < pinned_top);
    right_click(cx, "space-0");
    assert!(tab_menu(&view, cx).debug_bounds("tab-menu-space-rename").is_some());
    cx.simulate_keystrokes("escape");
    view.update(cx, |w, cx| w.close_tab_menu(cx));
    cx.run_until_parked();
    run_named_command(&view, cx, "show space header");
    assert!(view.read_with(cx, |w, _| w.browser.space().show_header));
    assert_eq!(cx.debug_bounds("pinned-list").unwrap().top(), pinned_top);
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn new_folder_from_an_inactive_space_menu_uses_that_space_and_retains_the_source_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "space-1");
    click(&mut tab_menu(&view, cx), "tab-menu-space-new-folder");
    assert!(bar_input_focused(&view, cx));
    assert_eq!(active_url(&view, cx), original);
    bar(&view, cx).simulate_input("Projects");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().folders[0].name, "Projects");
        assert!(w.browser.spaces[0].folders.is_empty());
        assert_eq!(w.browser.all_tabs().count(), 5);
    });
    cx.simulate_keystrokes("alt-cmd-left");
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

fn drag_between(cx: &mut VisualTestContext, source: &'static str, destination: &'static str) {
    begin_sidebar_drag(cx, source);
    // Drag-only targets can change the layout; aim at the rendered destination.
    let end = cx.debug_bounds(destination).unwrap().center();
    finish_sidebar_drag(cx, end);
}

fn finish_sidebar_drag(cx: &mut VisualTestContext, end: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn begin_sidebar_drag(cx: &mut VisualTestContext, source: &'static str) {
    cx.run_until_parked();
    let start = cx.debug_bounds(source).unwrap().center();
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(start + gpui::point(gpui::px(5.), gpui::px(0.)), gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn begin_drag_to_point(cx: &mut VisualTestContext, source: &'static str, end: gpui::Point<gpui::Pixels>) {
    begin_sidebar_drag(cx, source);
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn drag_to_row_edge(cx: &mut VisualTestContext, source: &'static str, destination: &'static str, after: bool) {
    begin_sidebar_drag(cx, source);
    let bounds = cx.debug_bounds(destination).unwrap();
    let y = if after { bounds.bottom() - gpui::px(4.) } else { bounds.top() + gpui::px(4.) };
    finish_sidebar_drag(cx, gpui::point(bounds.center().x, y));
}

#[gpui::test]
fn dragging_the_active_space_reorders_the_dock_without_switching_pages_or_animating_other_spaces(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "space-0", "space-2");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.spaces.iter().map(|space| space.name.as_str()).collect::<Vec<_>>(), vec!["Work", "Research", "Personal"]);
        assert_eq!(w.browser.active_space, 2);
        let loaded = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(loaded.space().name, "Personal");
        assert_eq!(loaded.active_space, 2);
    });
    let header = cx.debug_bounds("space-header-2").unwrap();
    assert!(header.left() >= gpui::px(0.) && header.right() <= gpui::px(240.));
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_an_inactive_space_keeps_the_focused_split_and_all_its_page_views(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { w.edit_browser(|browser| browser.add_space("Work", 100.)); w.edit_browser(|browser| browser.add_space("Research", 200.)); cx.notify(); });
    cx.run_until_parked();
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "space-2", "space-0");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.spaces.iter().map(|space| space.name.as_str()).collect::<Vec<_>>(), vec!["Research", "Personal", "Work"]);
        assert_eq!(w.browser.active_space, 1);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    let header = cx.debug_bounds("space-header-1").unwrap();
    assert!(header.left() >= gpui::px(0.) && header.right() <= gpui::px(240.));
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_space_outside_the_dock_cancels_and_leaves_click_navigation_working(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "space-2", "address");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.spaces.iter().map(|space| space.name.as_str()).collect::<Vec<_>>(), vec!["Personal", "Work", "Research"]);
        assert_eq!(w.browser.active_space, 0);
        assert!(w.command_bar.is_none());
    });
    assert_eq!(active_url(&view, cx), original);
    click(cx, "space-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_today_tab_into_a_closed_folder_pins_it_and_reuses_the_other_loaded_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { let folder = w.browser.space().folders[1].id; w.edit_browser(|browser| browser.set_folder_collapsed(folder, true)); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-7", "folder-3");
    view.read_with(cx, |w, _| {
        let tab = w.browser.space().active_tab.unwrap();
        assert_eq!(tab.get(), 7);
        assert!(w.browser.space().today.is_empty());
        assert_eq!(w.browser.folder_ancestors(tab).last().unwrap().get(), 3);
        assert!(w.browser.space().folders[1].collapsed);
        assert_eq!(w.browser.all_tabs().count(), 3);
        assert_eq!(w.browser.space().pinned.iter().find(|tab| tab.id.get() == 7).unwrap().pinned_url.as_deref(), Some("https://outside.example"));
        assert_eq!(w.browser.space().splits[0].active.get(), 5);
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
    run_named_command(&view, cx, "expand folder projects / research");
    click(cx, "tab-4");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 2);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_split_into_another_closed_folder_moves_the_complete_item_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let folder = view.update(cx, |w, cx| {
        let folder = w.edit_browser(|browser| browser.create_folder(browser.space().id, None, "Saved")).unwrap();
        w.edit_browser(|browser| browser.set_folder_collapsed(folder, true));
        cx.notify();
        folder
    });
    cx.run_until_parked();
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_between(cx, "tab-4", "folder-8");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.folder_tabs(folder).iter().map(|tab| tab.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert!(w.browser.folder(folder).unwrap().collapsed);
        assert!(w.browser.space().folders[1].items.is_empty());
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        let loaded = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(loaded.folder_tabs(folder), w.browser.folder_tabs(folder));
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_favorite_into_a_folder_preserves_the_loaded_page_and_original_url(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    run_named_command(&view, cx, "favorite tab");
    let folder = view.update(cx, |w, cx| {
        let folder = w.edit_browser(|browser| browser.create_folder(browser.space().id, None, "Saved")).unwrap();
        w.edit_browser(|browser| browser.set_folder_collapsed(folder, true));
        cx.notify();
        folder
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "favorite-8", "folder-9");
    view.read_with(cx, |w, _| {
        assert!(w.browser.active_user_profile().favorites.is_empty());
        assert_eq!(w.browser.folder_tabs(folder).iter().map(|tab| tab.get()).collect::<Vec<_>>(), vec![8]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        assert!(w.browser.folder(folder).unwrap().collapsed);
        assert_eq!(w.browser.space().pinned.iter().find(|tab| tab.id.get() == 8).unwrap().pinned_url.as_deref(), original.as_deref());
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn cancelling_an_inactive_tab_drag_does_not_activate_or_load_it(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    let before = today_urls(&view, cx);
    fake.take_calls();
    drag_between(cx, "tab-6", "address");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(today_urls(&view, cx), before);
    assert!(bar_text(&view, cx).is_none());
    click(cx, "tab-8");
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_folder_with_the_focused_split_moves_its_subtree_without_recreating_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let destination = view.update(cx, |w, cx| {
        let folder = w.edit_browser(|browser| browser.create_folder(browser.space().id, None, "Saved")).unwrap();
        w.edit_browser(|browser| browser.set_folder_collapsed(folder, true));
        cx.notify();
        folder
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "folder-2", "folder-8");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.iter().map(|id| id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.browser.folder_ancestors(w.browser.space().active_tab.unwrap()).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![8, 2, 3]);
        assert_eq!(w.browser.folder_tabs(destination).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert!(w.browser.folder(destination).unwrap().collapsed);
        assert_eq!(w.browser.all_tabs().count(), 3);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        let loaded = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(loaded.folder_tabs(destination), w.browser.folder_tabs(destination));
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn folder_drags_onto_self_or_descendants_are_rejected_without_toggling_disclosure(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let before = view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser));
    fake.take_calls();
    drag_between(cx, "folder-2", "folder-3");
    assert_eq!(view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser)), before);
    drag_between(cx, "folder-2", "folder-2");
    assert_eq!(view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser)), before);
    click(cx, "folder-3");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders[1].collapsed));
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_an_empty_inactive_folder_preserves_the_active_item_and_the_destination_stays_closed(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let (empty, destination) = view.update(cx, |w, cx| {
        let empty = w.edit_browser(|browser| browser.create_folder(browser.space().id, None, "Empty")).unwrap();
        let destination = w.edit_browser(|browser| browser.create_folder(browser.space().id, None, "Destination")).unwrap();
        w.edit_browser(|browser| browser.set_folder_collapsed(destination, true));
        cx.notify();
        (empty, destination)
    });
    cx.run_until_parked();
    let active = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "folder-8", "folder-9");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.folder_path(empty), "Destination / Empty");
        assert!(w.browser.folder(destination).unwrap().collapsed);
        assert!(w.browser.folder(empty).unwrap().items.is_empty());
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_between_pinned_and_today_keeps_page_identity_and_does_not_trigger_header_clicks(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "tab-8", "space-header-0");
    view.read_with(cx, |w, _| {
        assert!(!w.browser.space().pinned_collapsed);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        assert_eq!(w.browser.space().pinned.last().unwrap().pinned_url.as_deref(), original.as_deref());
    });
    drag_between(cx, "tab-8", "today-header");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().today[0].id.get(), 8);
        assert!(w.browser.space().today[0].pinned_url.is_none());
        assert_eq!(w.browser.all_tabs().count(), 5);
        assert!(w.command_bar.is_none());
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn pinned_root_drop_targets_extract_split_items_and_nested_folders_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_between(cx, "tab-4", "space-header-0");
    view.read_with(cx, |w, _| {
        assert!(w.browser.folder_ancestors(w.browser.space().active_tab.unwrap()).is_empty());
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert!(w.browser.space().folders[1].items.is_empty());
    });
    drag_between(cx, "folder-3", "space-header-0");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.folder_path(w.browser.space().folders[1].id), "Research");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_on_a_space_header_creates_the_first_pin_without_creating_a_new_page(cx: &mut TestAppContext) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new();
    browser.new_tab("https://single.example", std::time::SystemTime::now());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-2", "space-header-0");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().pinned[0].id.get(), 2);
        assert_eq!(w.browser.space().pinned[0].pinned_url.as_deref(), Some("https://single.example"));
        assert!(w.browser.space().today.is_empty());
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 2);
        assert!(w.command_bar.is_none());
    });
    assert!(cx.debug_bounds("pinned-list").is_some());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_into_the_favorites_dock_and_back_to_today_reuses_the_live_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let pin = w.browser.space().pinned[0].id;
        w.edit_browser(|browser| browser.favorite_tab(pin));
        w.edit_browser(|browser| browser.activate_tab(active, std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
    // A lone favorite fills the row, so the drop lands on its tile.
    drag_between(cx, "tab-8", "favorites");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_user_profile().favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![8, 4]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
    });
    drag_between(cx, "favorite-8", "new-tab");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_user_profile().favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4]);
        assert_eq!(w.browser.space().today[0].id.get(), 8);
        assert!(w.browser.space().today[0].pinned_url.is_none());
        assert!(w.command_bar.is_none());
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn a_full_favorites_dock_rejects_a_drop_without_moving_or_loading_the_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        for index in 0..browser_core::favorites::MAX_FAVORITES {
            let id = w.edit_browser(|browser| browser.new_tab(format!("https://favorite{index}.example"), std::time::SystemTime::now()));
            assert!(w.edit_browser(|browser| browser.favorite_tab(id)));
        }
        w.edit_browser(|browser| browser.activate_tab(active, std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-8", "favorites");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorite_count(), browser_core::favorites::MAX_FAVORITES);
        assert!(w.browser.space().today.iter().any(|tab| tab.id.get() == 8));
        assert!(!w.browser.is_favorite(w.browser.space().active_tab.unwrap()));
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_split_into_a_scrolled_today_list_reveals_its_new_first_row_and_keeps_both_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        for index in 0..40 { w.edit_browser(|browser| browser.new_tab(format!("https://long{index}.example"), std::time::SystemTime::now())); }
        w.edit_browser(|browser| browser.activate_tab(active, std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    scroll(cx, "today-list", gpui::TouchPhase::Moved, 0., -5000.);
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-4", "new-tab");
    let row = cx.debug_bounds("tab-4").unwrap();
    let list = cx.debug_bounds("today-list").unwrap();
    assert!(row.top() >= list.top() && row.bottom() <= list.bottom());
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().today[0].id.get(), 4);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert!(w.browser.space().today.iter().filter(|tab| tab.id.get() == 4 || tab.id.get() == 5).all(|tab| tab.pinned_url.is_none()));
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.command_bar.is_none());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn reordering_favorite_icons_preserves_the_current_page_for_both_inactive_and_active_items(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let pins: Vec<_> = w.browser.space().pinned.iter().map(|tab| tab.id).collect();
        for tab in pins { w.edit_browser(|browser| browser.favorite_tab(tab)); }
        w.edit_browser(|browser| browser.activate_tab(active, std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "favorite-5", "favorite-4");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>()), vec![5, 4]);
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    click(cx, "favorite-5");
    fake.take_calls();
    drag_between(cx, "favorite-5", "favorite-4");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_user_profile().favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn reordering_a_favorite_split_keeps_pane_focus_and_numbered_navigation_uses_the_new_order(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    run_named_command(&view, cx, "favorite tab");
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let other = w.browser.space().today[0].id;
        w.edit_browser(|browser| browser.favorite_tab(other));
        w.edit_browser(|browser| browser.activate_tab(active, std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_between(cx, "favorite-4", "favorite-7");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_user_profile().favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![7, 4, 5]);
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 7);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-2");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_new_favorite_on_an_icon_inserts_it_at_that_position_without_copying_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let pin = w.browser.space().pinned[0].id;
        w.edit_browser(|browser| browser.favorite_tab(pin));
        w.edit_browser(|browser| browser.activate_tab(active, std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-8", "favorite-4");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_user_profile().favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![8, 4]);
        assert_eq!(w.browser.all_tabs().count(), 5);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        let loaded = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(loaded.active_user_profile().favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![8, 4]);
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn a_full_favorites_dock_still_allows_existing_items_to_be_reordered(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        for index in 0..browser_core::favorites::MAX_FAVORITES {
            let id = w.edit_browser(|browser| browser.new_tab(format!("https://favorite{index}.example"), std::time::SystemTime::now()));
            w.edit_browser(|browser| browser.favorite_tab(id));
        }
        w.edit_browser(|browser| browser.activate_tab(active, std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "favorite-9", "favorite-20");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorite_count(), browser_core::favorites::MAX_FAVORITES);
        assert_eq!(w.browser.active_user_profile().favorites.last().unwrap().id.get(), 9);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_today_rows_before_or_after_an_item_preserves_the_loaded_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_to_row_edge(cx, "tab-8", "tab-6", true);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>()), vec![7, 6, 8]);
    drag_to_row_edge(cx, "tab-8", "tab-7", false);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>()), vec![8, 7, 6]);
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_split_after_a_folder_sibling_and_before_today_keeps_both_panes_and_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let folder = w.browser.space().folders[1].id;
        let other = w.edit_browser(|browser| browser.new_tab("https://sibling.example", std::time::SystemTime::now()));
        w.edit_browser(|browser| browser.move_item_to_folder(other, folder));
        w.edit_browser(|browser| browser.activate_tab(active, std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_to_row_edge(cx, "tab-4", "tab-8", true);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().pinned.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![8, 4, 5]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().id, split);
    });
    drag_to_row_edge(cx, "tab-4", "tab-7", false);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4, 5, 7]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert!(w.browser.space().today.iter().filter(|tab| tab.id.get() == 4 || tab.id.get() == 5).all(|tab| tab.pinned_url.is_none()));
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.command_bar.is_none());
    });
    let row = cx.debug_bounds("tab-4").unwrap();
    let list = cx.debug_bounds("today-list").unwrap();
    assert!(row.top() >= list.top() && row.bottom() <= list.bottom());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_before_a_closed_folder_peek_item_pins_in_that_folder_and_preserves_its_split(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { let folder = w.browser.space().folders[1].id; w.edit_browser(|browser| browser.set_folder_collapsed(folder, true)); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    drag_to_row_edge(cx, "tab-7", "tab-4", false);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 7);
        assert_eq!(w.browser.folder_ancestors(w.browser.space().active_tab.unwrap()).last().unwrap().get(), 3);
        assert!(w.browser.space().folders[1].collapsed);
        assert_eq!(w.browser.space().splits[0].active.get(), 5);
        assert_eq!(w.browser.space().pinned.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![7, 4, 5]);
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
    run_named_command(&view, cx, "expand folder projects / research");
    click(cx, "tab-4");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_favorite_after_a_today_row_unfavorites_without_reloading_or_opening_the_target(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    run_named_command(&view, cx, "favorite tab");
    fake.take_calls();
    drag_to_row_edge(cx, "favorite-8", "tab-6", true);
    view.read_with(cx, |w, _| {
        assert!(w.browser.active_user_profile().favorites.is_empty());
        assert_eq!(w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![7, 6, 8]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        assert!(w.browser.space().today.last().unwrap().pinned_url.is_none());
        assert!(w.command_bar.is_none());
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_an_inactive_today_tab_focuses_the_moved_page_and_keeps_the_previous_page_loaded(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_to_row_edge(cx, "tab-6", "tab-8", false);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>()), vec![6, 8, 7]);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 6);
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
    click(cx, "tab-8");
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_today_tab_on_a_space_icon_preserves_its_section_and_live_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "tab-8", "space-1");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().today[0].id.get(), 8);
        assert!(w.browser.space().pinned.is_empty());
        assert!(w.browser.space().today[0].pinned_url.is_none());
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_pinned_split_on_a_space_icon_moves_all_panes_without_recreating_them(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { w.edit_browser(|browser| browser.add_space("Work", 100.)); cx.notify(); });
    cx.run_until_parked();
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_between(cx, "tab-4", "space-1");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().pinned.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.browser.spaces[0].pinned.is_empty());
        assert_eq!(w.browser.spaces[0].today[0].id.get(), 7);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

fn add_drag_destination(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |w, cx| {
        let work = w.edit_browser(|browser| browser.add_space("Work", 100.));
        let folder = w.edit_browser(|browser| browser.create_folder(work, None, "Saved")).unwrap();
        w.edit_browser(|browser| browser.set_folder_collapsed(folder, true));
        w.edit_browser(|browser| browser.switch_space(1));
        w.edit_browser(|browser| browser.new_tab("https://work.example", std::time::SystemTime::now()));
        w.edit_browser(|browser| browser.switch_space(0));
        cx.notify();
    });
    cx.run_until_parked();
}

#[gpui::test]
fn hovering_a_space_during_drag_waits_then_opens_it_and_accepts_a_split_in_its_closed_folder(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    add_drag_destination(&view, cx);
    fake.take_calls();
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "tab-4", dock);
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Personal");
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Create(..))).count(), 1, "hover loads only the destination's current page");
    let destination = cx.debug_bounds("folder-9").unwrap().center();
    cx.simulate_mouse_move(destination, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(destination, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert!(w.browser.space().folders[0].collapsed);
        assert_eq!(w.browser.folder_tabs(w.browser.space().folders[0].id).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.browser.spaces[0].pinned.is_empty());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn leaving_a_space_icon_before_its_drag_hover_delay_cancels_navigation_and_loading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { w.edit_browser(|browser| browser.add_space("Work", 100.)); cx.notify(); });
    cx.run_until_parked();
    let original = active_url(&view, cx);
    fake.take_calls();
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "tab-7", dock);
    cx.executor().advance_clock(Duration::from_millis(250));
    let outside = cx.debug_bounds("address").unwrap().center();
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Personal");
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), original);
    assert!(bar_text(&view, cx).is_none());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_folder_on_another_space_moves_the_complete_focused_subtree(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { w.edit_browser(|browser| browser.add_space("Work", 100.)); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "folder-2", "space-1");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.space().folders.len(), 2);
        assert!(w.browser.spaces[0].folders.is_empty());
        assert_eq!(w.browser.active_split().unwrap().panes.iter().map(|id| id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn hovering_with_an_empty_folder_then_nesting_it_preserves_the_destinations_current_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    add_drag_destination(&view, cx);
    view.update(cx, |w, cx| { w.edit_browser(|browser| browser.create_folder(browser.space().id, None, "Empty")); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "folder-11", dock);
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    fake.take_calls();
    let destination = cx.debug_bounds("folder-9").unwrap().center();
    cx.simulate_mouse_move(destination, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(destination, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 10);
        assert!(w.browser.space().folders[0].collapsed);
        assert_eq!(w.browser.folder_path(w.browser.space().folders[1].id), "Saved / Empty");
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("alt-cmd-left");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_favorite_split_to_a_space_icon_turns_it_into_local_pins_without_losing_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    run_named_command(&view, cx, "favorite tab");
    view.update(cx, |w, cx| { w.edit_browser(|browser| browser.add_space("Work", 100.)); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "favorite-4", "space-1");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert!(w.browser.active_user_profile().favorites.is_empty());
        assert_eq!(w.browser.space().pinned.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn hovering_with_a_focused_folder_then_dropping_it_restores_its_original_split_pane(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    add_drag_destination(&view, cx);
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "folder-2", dock);
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 10);
    fake.take_calls();
    cx.simulate_mouse_up(dock, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.browser.spaces[0].pinned.is_empty());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn a_shared_favorite_drag_keeps_its_source_pane_when_hovering_another_spaces_pane(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    run_named_command(&view, cx, "favorite tab");
    view.update(cx, |w, cx| {
        w.edit_browser(|browser| browser.add_space("Work", 100.));
        let panes = w.browser.active_user_profile().favorite_splits[0].panes.clone();
        w.edit_browser(|browser| browser.switch_space(1));
        w.edit_browser(|browser| browser.activate_tab(panes[0], std::time::SystemTime::now()));
        w.edit_browser(|browser| browser.switch_space(0));
        w.edit_browser(|browser| browser.activate_tab(panes[1], std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "favorite-4", dock);
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    // Native focus follows the destination Space's other pane while hovering.
    fake.send(cx, Event::Focused { view: page(1) });
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 4);
    fake.take_calls();
    cx.simulate_mouse_up(dock, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert!(w.browser.active_user_profile().favorites.is_empty());
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().active.get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dock_reopen_restores_a_closed_window_and_accepts_keyboard_input(cx: &mut TestAppContext) {
    use gpui::AppContext;
    cx.update(init);
    let create = |cx: &mut gpui::App| {
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| {
                let view = BrowserWindow::new(demo_browser(), None, window, cx);
                view.focus(window);
                view
            })
        }).unwrap();
    };
    cx.update(|cx| crate::reopen_with(cx, create));
    let first = cx.read(|cx| cx.windows()[0]);
    let visual = VisualTestContext::from_window(first, cx);
    visual.run_until_parked();
    first.update(cx, |_, window, _| window.remove_window()).unwrap();
    cx.run_until_parked();
    assert!(cx.read(|cx| cx.windows().is_empty()));

    cx.update(|cx| crate::reopen_with(cx, create));
    let reopened = cx.read(|cx| {
        assert_eq!(cx.windows().len(), 1);
        cx.windows()[0]
    });
    assert_ne!(first.window_id(), reopened.window_id());
    let mut visual = VisualTestContext::from_window(reopened, cx);
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-t");
    let view = reopened.downcast::<BrowserWindow>().unwrap().root(cx).unwrap();
    bar(&view, &mut visual).simulate_input("example.com");
    bar(&view, &mut visual).simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().url, "https://example.com");
    });
    cx.update(|cx| crate::reopen_with(cx, |_| panic!("must reuse the existing browser window")));
    assert_eq!(cx.read(|cx| cx.windows().len()), 1);
}

#[gpui::test]
fn incognito_shortcut_opens_an_independent_window_with_command_input(cx: &mut TestAppContext) {
    let (regular, visual) = open(cx);
    let original = regular.read_with(visual, |view, _| browser_core::session::to_json(&view.browser));
    visual.simulate_keystrokes("cmd-shift-n");
    visual.cx.refresh().unwrap();
    visual.run_until_parked();
    let private = visual.cx.update(|cx| {
        let roots: Vec<_> = cx.windows().into_iter().filter_map(|window| window.downcast::<BrowserWindow>())
            .filter_map(|window| window.root(cx).ok()).collect();
        roots.into_iter().find(|view| view.read(cx).browser.is_private()).unwrap()
    });
    private.read_with(visual, |view, cx| {
        assert!(view.command_bar.is_some());
        assert!(view.command_bar.as_ref().unwrap().input.read(cx).text().is_empty());
        assert!(view.browser.all_tabs().next().is_none());
        assert!(view.browser.active_user_profile().favorites.is_empty());
        assert_eq!(view.browser.space().name, "Incognito");
    });
    assert_eq!(regular.read_with(visual, |view, _| browser_core::session::to_json(&view.browser)), original);
    let handle = private.read_with(visual, |view, _| view.main_window);
    let mut private_visual = VisualTestContext::from_window(handle, &visual.cx);
    bar(&private, &mut private_visual).simulate_input("https://private.example/first-window");
    bar(&private, &mut private_visual).simulate_keystrokes("enter");
    private_visual.simulate_keystrokes("cmd-shift-n");
    private_visual.run_until_parked();
    private.read_with(&private_visual, |view, _| {
        assert_eq!(view.browser.space().today.len(), 1);
        assert_eq!(view.browser.active_tab().unwrap().url, "https://private.example/first-window");
        assert!(view.browser.active_user_profile().history.is_empty());
    });
    let roots = private_visual.cx.update(|cx| {
        let views: Vec<_> = cx.windows().into_iter().filter_map(|window| window.downcast::<BrowserWindow>())
            .filter_map(|window| window.root(cx).ok()).collect();
        views.into_iter().filter(|view| view.read(cx).browser.is_private()).collect::<Vec<_>>()
    });
    assert_eq!(roots.len(), 2);
    assert_eq!(roots.iter().filter(|view| view.read_with(&private_visual, |view, _| view.browser.all_tabs().next().is_none())).count(), 1);
}

#[gpui::test]
fn incognito_command_bar_action_opens_a_private_window(cx: &mut TestAppContext) {
    let (regular, visual) = open(cx);
    run_named_command(&regular, visual, "new incognito window");
    visual.cx.refresh().unwrap();
    visual.run_until_parked();
    let private_count = visual.cx.update(|cx| {
        let views: Vec<_> = cx.windows().into_iter().filter_map(|window| window.downcast::<BrowserWindow>())
            .filter_map(|window| window.root(cx).ok()).collect();
        views.into_iter().filter(|view| view.read(cx).browser.is_private()).count()
    });
    assert_eq!(private_count, 1);
    assert!(regular.read_with(visual, |view, _| view.command_bar.is_none()));
    let private = visual.cx.update(|cx| {
        let views: Vec<_> = cx.windows().into_iter().filter_map(|window| window.downcast::<BrowserWindow>())
            .filter_map(|window| window.root(cx).ok()).collect();
        views.into_iter().find(|view| view.read(cx).browser.is_private()).unwrap()
    });
    let mut private_visual = VisualTestContext::from_window(private.read_with(visual, |view, _| view.main_window), &visual.cx);
    bar(&private, &mut private_visual).simulate_input("https://private.example/from-command");
    bar(&private, &mut private_visual).simulate_keystrokes("enter");
    private.read_with(&private_visual, |view, _| {
        assert!(view.command_bar.is_none());
        assert_eq!(view.browser.active_tab().unwrap().url, "https://private.example/from-command");
    });
}

#[gpui::test]
fn incognito_never_saves_the_private_window_to_a_session(cx: &mut TestAppContext) {
    let path = std::env::temp_dir().join(format!("echo-incognito-session-{}.json", std::process::id()));
    std::fs::write(&path, "existing regular session").unwrap();
    cx.update(init);
    let (view, visual) = cx.add_window_view(|window, cx| {
        let mut view = BrowserWindow::new(browser_core::Browser::new_private(), None, window, cx);
        view.save_session_to(path.clone(), cx);
        view.focus(window);
        view
    });
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("https://private.example/secret");
    bar(&view, visual).simulate_keystrokes("enter");
    visual.executor().advance_clock(Duration::from_secs(2));
    visual.run_until_parked();
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().url, "https://private.example/secret");
        assert!(view.browser.active_user_profile().history.is_empty());
    });
    visual.simulate_keystrokes("cmd-shift-w");
    drop(view);
    visual.cx.update(|_| {});
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing regular session");
    std::fs::remove_file(path).unwrap();
}

#[gpui::test]
fn moving_a_live_page_to_a_host_keeps_its_view_and_navigation_state(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_split_pages(cx, 2);
    fake.send(visual, Event::LoadingChanged { view: page(2), loading: false, can_go_back: true, can_go_forward: true });
    let (tab, original) = view.read_with(visual, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        pages.surface_layout()[1]
    });
    let destination = Bounds { x: 24., y: 40., width: 640., height: 480. };
    fake.take_calls();
    view.update(visual, |view, _| {
        let mut pages = view.pages.as_ref().unwrap().borrow_mut();
        assert!(pages.move_to_host(tab, 0x1234usize as *mut _, destination));
        assert!(pages.state(tab).can_go_back);
        assert!(pages.state(tab).can_go_forward);
        assert_eq!(pages.tab_for(page(2)), Some(tab));
        assert!(pages.move_to_host(tab, 0x5678usize as *mut _, original));
    });
    assert_eq!(fake.take_calls(), vec![Call::MoveHost(2, 0x1234, destination), Call::MoveHost(2, 0x5678, original)]);
}

#[gpui::test]
fn a_rejected_page_host_transfer_preserves_the_existing_page(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_split_pages(cx, 2);
    let (tab, bounds) = view.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().surface_layout()[1]);
    fake.0.borrow_mut().reject_move = true;
    fake.take_calls();
    view.update(visual, |view, _| {
        let mut pages = view.pages.as_ref().unwrap().borrow_mut();
        assert!(!pages.move_to_host(tab, std::ptr::null_mut(), bounds));
        assert!(!pages.move_to_host(tab, 0x1234usize as *mut _, bounds));
        assert_eq!(pages.tab_for(page(2)), Some(tab));
        assert!(pages.is_visible(tab));
    });
    assert_eq!(fake.take_calls(), vec![Call::MoveHost(2, 0x1234, bounds)]);
}

#[gpui::test]
fn close_window_shortcut_preserves_tabs_and_saves_the_session(cx: &mut TestAppContext) {
    let path = std::env::temp_dir().join(format!("echo-close-window-{}.json", std::process::id()));
    let (view, visual) = open(cx);
    let before = view.read_with(visual, |view, _| browser_core::session::to_json(&view.browser));
    view.update(visual, |view, cx| view.save_session_to(path.clone(), cx));
    visual.simulate_keystrokes("cmd-shift-w");
    visual.run_until_parked();
    assert!(visual.cx.read(|cx| cx.windows().is_empty()));
    assert_eq!(view.read_with(visual, |view, _| browser_core::session::to_json(&view.browser)), before);
    drop(view);
    visual.cx.update(|_| {});
    let restored = browser_core::session::load(&path).unwrap().unwrap();
    assert_eq!(browser_core::session::to_json(&restored), before);
    std::fs::remove_file(path).unwrap();
}

#[gpui::test]
fn close_window_from_command_input_removes_its_child_window(cx: &mut TestAppContext) {
    let (view, visual) = open(cx);
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("unfinished query");
    bar(&view, visual).simulate_keystrokes("cmd-shift-w");
    visual.run_until_parked();
    assert!(visual.cx.read(|cx| cx.windows().is_empty()));
    view.read_with(visual, |view, _| {
        assert!(view.command_bar.is_none());
        assert!(view.browser.archive.is_empty());
    });
}

#[gpui::test]
fn close_window_from_page_shortcut_keeps_the_other_window_open(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (first, visual) = open_with(cx, Some(Box::new(fake.clone())));
    let other = visual.cx.update(|cx| {
        use gpui::AppContext;
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| BrowserWindow::new(demo_browser(), None, window, cx))
        }).unwrap()
    });
    fake.send(visual, fake.shortcut(Shortcut {
        cmd: true, shift: true, key: "w".into(), ..Default::default()
    }));
    visual.run_until_parked();
    assert_eq!(visual.cx.read(|cx| cx.windows().len()), 1);
    assert!(other.root(visual).is_ok());
    assert!(first.read_with(visual, |view, _| view.browser.archive.is_empty()));
}

#[gpui::test]
fn closing_the_window_saves_changes_before_the_periodic_save(cx: &mut TestAppContext) {
    let path = std::env::temp_dir().join(format!("browser-close-session-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let (view, visual) = open(cx);
    view.update(visual, |view, cx| {
        view.save_session_to(path.clone(), cx);
        view.edit_browser(|browser| browser.new_tab("https://example.com/just-before-close", std::time::SystemTime::now()));
    });
    visual.update(|window, _| window.remove_window());
    drop(view);
    visual.cx.update(|_| {});
    visual.run_until_parked();
    let restored = browser_core::session::load(&path).unwrap().unwrap();
    assert_eq!(restored.active_tab().unwrap().url, "https://example.com/just-before-close");
    std::fs::remove_file(path).unwrap();
}

#[gpui::test]
fn cmd_w_closes_only_the_focused_split_pane_from_ui_and_native_page_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let first = view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap());
    fake.take_calls();
    cx.simulate_keystrokes("cmd-w");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.browser.archive.last().unwrap().tab.id, first);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Close(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
    fake.send(cx, fake.shortcut(Shortcut { cmd: true, key: "w".into(), ..Default::default() }));
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 1);
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Close(..))).count(), 1);
}

#[gpui::test]
fn add_split_immediately_shows_an_empty_pane_with_search_inside_it_and_escape_rolls_back(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |w, _| w.browser.all_tabs().count());
    let active = active_url(&view, cx);
    fake.take_calls();
    cx.simulate_keystrokes("ctrl-shift-=");
    let (anchor, pane) = view.read_with(cx, |w, _| {
        let draft = w.split_draft.as_ref().unwrap();
        assert_eq!(w.browser.active_tab().unwrap().url, crate::split::EMPTY_PANE_URL);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.browser.all_tabs().count(), before + 1);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 1);
        (draft.anchor, draft.pane)
    });
    assert!(cx.debug_bounds(test_selector(format!("sidebar-pane-{}", anchor.get()))).is_some());
    assert!(cx.debug_bounds(test_selector(format!("sidebar-pane-{}", pane.get()))).is_some());
    let pane_header = cx.debug_bounds(test_selector(format!("pane-header-{}", pane.get()))).unwrap();
    let command = bar(&view, cx).debug_bounds("command-bar").unwrap();
    assert!(command.origin.x >= pane_header.origin.x);
    assert!(command.right() <= pane_header.right());
    assert!(bar_input_focused(&view, cx));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(active_url(&view, cx), active);
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), before);
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty() && w.browser.active_split().is_none()));
}

#[gpui::test]
fn splitting_a_pin_or_favorite_keeps_the_original_and_creates_independent_today_panes(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    click(cx, "tab-4");
    let source = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().clone());
    cx.simulate_keystrokes("ctrl-shift-=");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), source.id);
    for favorite in [false, true] {
        if favorite { run_named_command(&view, cx, "favorite tab"); }
        cx.simulate_keystrokes("ctrl-shift-=");
        bar(&view, cx).simulate_input("https://second.example");
        bar(&view, cx).simulate_keystrokes("enter");
        let anchor = view.read_with(cx, |w, _| {
            let split = w.browser.active_split().unwrap();
            assert!(!split.panes.contains(&source.id));
            assert!(split.panes.iter().all(|id| w.browser.space().today.iter().any(|tab| tab.id == *id)));
            let original = w.browser.all_tabs().find(|tab| tab.id == source.id).unwrap();
            assert_eq!(original.url, source.url);
            assert_eq!(original.pinned_url, source.pinned_url);
            assert!(w.browser.split_for(source.id).is_none());
            split.anchor
        });
        click(cx, test_selector(format!("close-{}", anchor.get())));
        if favorite { click(cx, "favorite-4"); } else { click(cx, "tab-4"); }
    }
}

#[gpui::test]
fn pending_split_is_excluded_from_sessions_and_cancel_restores_axis_weights_and_focus(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 3);
    view.update(cx, |w, _| {
        let active = w.browser.space().active_tab.unwrap();
        w.edit_browser(|browser| browser.set_split_weights(active, &[0.2, 0.3, 0.5]));
    });
    let original = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    run_named_command(&view, cx, "add bottom split");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 4);
        let saved = browser_core::session::from_json(&w.session_json()).unwrap();
        let split = saved.active_split().unwrap();
        assert_eq!(split.panes, original.panes);
        assert_eq!(split.axis, original.axis);
        assert_eq!(split.weights, original.weights);
        assert!(!saved.all_tabs().any(|tab| tab.url == crate::split::EMPTY_PANE_URL));
    });
    bar(&view, cx).simulate_keystrokes("escape");
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        assert_eq!(split.panes, original.panes);
        assert_eq!(split.axis, original.axis);
        assert_eq!(split.weights, original.weights);
        assert_eq!(split.active, original.active);
        assert!(w.browser.archive.is_empty());
    });
}

#[gpui::test]
fn sidebar_split_segments_focus_individual_panes_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    click(cx, "sidebar-pane-2");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    click(cx, "sidebar-pane-3");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane1.example"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn cmd_w_in_the_empty_split_search_cancels_without_archiving_the_source(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let original = active_url(&view, cx);
    cx.simulate_keystrokes("ctrl-shift-=");
    bar(&view, cx).simulate_keystrokes("cmd-w");
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(active_url(&view, cx), original);
    assert!(view.read_with(cx, |w, _| w.split_draft.is_none() && w.browser.active_split().is_none() && w.browser.archive.is_empty()));
}

#[gpui::test]
fn empty_vertical_pane_search_stays_inside_the_pane(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    run_named_command(&view, cx, "add bottom split");
    let pane = view.read_with(cx, |w, _| w.split_draft.as_ref().unwrap().pane);
    let header = cx.debug_bounds(test_selector(format!("pane-header-{}", pane.get()))).unwrap();
    let search = bar(&view, cx).debug_bounds("command-bar").unwrap();
    assert!(search.origin.y >= header.bottom());
    let pane_bounds = cx.debug_bounds(test_selector(format!("pane-body-{}", pane.get()))).unwrap();
    assert!(search.bottom() <= pane_bounds.bottom());
    bar(&view, cx).simulate_keystrokes("escape");
}

#[gpui::test]
fn split_headers_and_pages_share_one_outer_corner_contour_without_recreating_views(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    assert_eq!(fake.0.borrow().rounded_top[&1], true);
    fake.take_calls();
    cx.simulate_keystrokes("ctrl-shift-=");
    assert!(fake.take_calls().contains(&Call::TopCorners(1, false)));
    bar(&view, cx).simulate_keystrokes("escape");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::TopCorners(1, true)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));

    cx.simulate_keystrokes("ctrl-shift-=");
    bar(&view, cx).simulate_input("https://beside.example");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(fake.0.borrow().rounded_top.values().all(|rounded| !rounded));
    let panes = view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.clone());
    for pane in panes {
        let frame = cx.debug_bounds(test_selector(format!("split-pane-{}", pane.get()))).unwrap();
        let header = cx.debug_bounds(test_selector(format!("pane-header-{}", pane.get()))).unwrap();
        let body = cx.debug_bounds(test_selector(format!("pane-body-{}", pane.get()))).unwrap();
        assert_eq!(frame.origin + gpui::point(gpui::px(1.), gpui::px(1.)), header.origin);
        assert_eq!(header.bottom(), body.origin.y);
        assert_eq!(frame.bottom() - gpui::px(1.), body.bottom());
        assert_eq!(frame.size.width - gpui::px(2.), header.size.width);
        assert_eq!(frame.size.width - gpui::px(2.), body.size.width);
    }
    run_named_command(&view, cx, "convert to vertical split");
    assert!(fake.0.borrow().rounded_top.values().all(|rounded| !rounded));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-w");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::TopCorners(1, true)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
}

#[gpui::test]
fn space_name_retracts_pins_progressively_and_reverses_mid_animation_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    assert!(cx.debug_bounds("pinned-section").is_none());
    let expanded = cx.debug_bounds("new-tab").unwrap().origin.y;
    let active = active_url(&view, cx);
    fake.take_calls();
    click(cx, "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(48));
    cx.run_until_parked();
    let intermediate = cx.debug_bounds("new-tab").unwrap().origin.y;
    assert!(intermediate < expanded);
    assert!(cx.debug_bounds("pinned-reveal").unwrap().size.height > gpui::px(0.));
    click(cx, "space-header-0");
    assert_eq!(cx.debug_bounds("new-tab").unwrap().origin.y, intermediate);
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("new-tab").unwrap().origin.y, expanded);
    assert!(!view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::FocusPage(..))));
}

#[gpui::test]
fn dropping_on_a_collapsed_space_name_reveals_the_moved_pin(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-8", "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert!(!w.browser.space().pinned_collapsed);
        assert!(w.browser.space().pinned.iter().any(|tab| tab.id.get() == 8));
    });
    let pin = cx.debug_bounds("tab-8").unwrap();
    let header = cx.debug_bounds("space-header-0").unwrap();
    assert!(pin.origin.y >= header.bottom());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn space_header_matches_reference_control_positions_and_more_does_not_toggle_pins(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    view.update(cx, |w, cx| {
        w.edit_browser(|browser| browser.set_space_icon(browser.space().id, Some("🍕")));
        cx.notify();
    });
    cx.run_until_parked();
    let header = cx.debug_bounds("space-header-0").unwrap();
    let icon = cx.debug_bounds("space-header-icon-0").unwrap();
    let name = cx.debug_bounds("space-header-name-0").unwrap();
    assert_eq!(header.size.height, gpui::px(38.));
    assert_eq!(icon.origin.x, header.origin.x + gpui::px(9.));
    assert_eq!(name.origin.x, icon.right() + gpui::px(9.));
    assert!(icon.right() < name.origin.x);
    let original = active_url(&view, cx);
    fake.take_calls();
    cx.simulate_mouse_move(header.center(), None, Modifiers::none());
    cx.run_until_parked();
    let chevron = cx.debug_bounds("space-header-chevron-0").unwrap();
    let more = cx.debug_bounds("space-header-more-0").unwrap();
    assert_eq!(chevron.origin, icon.origin);
    assert_eq!(chevron.size, icon.size);
    assert!(chevron.right() < name.origin.x);
    assert!(more.origin.x >= name.right());
    assert_eq!(cx.debug_bounds("space-header-name-0").unwrap(), name);
    click(cx, "space-header-more-0");
    view.read_with(cx, |w, _| {
        assert_eq!(w.tab_menu.as_ref().unwrap().target, crate::tab_menu::MenuTarget::Space(w.browser.space().id));
        assert!(!w.browser.space().pinned_collapsed);
    });
    click(&mut tab_menu(&view, cx), "tab-menu-space-rename");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Personal"));
    bar(&view, cx).simulate_keystrokes("escape");
    click(cx, "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    assert_eq!(cx.debug_bounds("space-header-name-0").unwrap(), name);
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}


#[gpui::test]
fn sidebar_slide_reverses_without_jumping_and_moves_content_with_it(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    cx.executor().advance_clock(Duration::from_millis(48));
    cx.run_until_parked();
    let closing = cx.debug_bounds("sidebar-clip").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    assert!(closing.size.width > gpui::px(0.) && closing.size.width < gpui::px(240.));
    assert_eq!(sidebar.right(), closing.right());
    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("sidebar-clip").unwrap(), closing, "reverses from the visible position");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("sidebar-clip").unwrap().size.width, gpui::px(240.));
}

#[gpui::test]
fn floating_sidebar_keeps_a_pointer_buffer_without_covering_the_page(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    move_mouse(cx, 2., 300.);
    let mut overlay = floating(&view, cx).unwrap();
    move_mouse(&mut overlay, 100., 300.);
    move_mouse(cx, 300., 300.);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(floating(&view, cx).is_some(), "54 px outside the panel stays open");
    assert_eq!(overlay.update(|window, _| window.viewport_size().width), gpui::px(270.));
    move_mouse(cx, 350., 300.);
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert!(floating(&view, cx).is_none(), "beyond the buffer it closes");
}


#[gpui::test]
fn sidebar_toolbar_routes_navigation_and_switches_reload_to_stop(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    click(cx, "go-back");
    click(cx, "go-forward");
    click(cx, "reload-page");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Back(1)));
    assert!(calls.contains(&Call::Forward(1)));
    assert!(calls.contains(&Call::Reload(1)));
    fake.send(cx, Event::LoadingChanged { view: page(1), loading: true, can_go_back: true, can_go_forward: false });
    click(cx, "reload-page");
    assert!(fake.take_calls().contains(&Call::StopLoading(1)));
    click(cx, "toggle-sidebar");
    assert!(!view.read_with(cx, |w, _| w.sidebar_visible));
}

#[gpui::test]
fn split_sidebar_segments_fill_the_row_and_long_titles_stay_inside(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    view.update(cx, |w, cx| {
        let ids = w.browser.active_split().unwrap().panes.clone();
        for id in ids { w.edit_browser(|browser| browser.set_title(id, "A very long page title that should truncate inside its own split segment")); }
        cx.notify();
    });
    cx.run_until_parked();
    move_mouse(cx, 400., 400.);
    let row = cx.debug_bounds("tab-2").unwrap();
    let contents = cx.debug_bounds("sidebar-split-2").unwrap();
    let first = cx.debug_bounds("sidebar-pane-2").unwrap();
    let last = cx.debug_bounds("sidebar-pane-3").unwrap();
    assert_eq!(contents.origin.x, row.origin.x + gpui::px(4.));
    assert_eq!(contents.right(), row.right() - gpui::px(4.));
    assert_eq!(first.size.width, last.size.width);
    assert_eq!(first.origin.x, contents.origin.x);
    assert_eq!(last.right(), contents.right());
    move_mouse(cx, f32::from(row.center().x), f32::from(row.center().y));
    assert_eq!(cx.debug_bounds("sidebar-pane-3").unwrap(), last, "hover close must not shrink a segment");
    let close = cx.debug_bounds("close-2").unwrap();
    assert!(close.origin.x >= last.origin.x && close.right() <= last.right());
    let title = cx.debug_bounds("sidebar-pane-title-3").unwrap();
    assert!(title.right() <= close.origin.x, "the hover close must not cover the title");
    fake.take_calls();
    click(cx, "sidebar-pane-3");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane1.example"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    click(cx, "close-2");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
}

#[gpui::test]
fn folder_hover_replaces_its_icon_without_shifting_the_title(cx: &mut TestAppContext) {
    let (view, cx, _) = open_folder_pages(cx);
    let folder = view.read_with(cx, |w, _| w.browser.space().folders[0].id);
    let key = folder.get();
    move_mouse(cx, 400., 400.);
    let name = cx.debug_bounds(test_selector(format!("folder-name-{key}"))).unwrap();
    let icon = cx.debug_bounds(test_selector(format!("folder-icon-{key}"))).unwrap();
    let row = cx.debug_bounds(test_selector(format!("folder-{key}"))).unwrap();
    assert_eq!(icon.origin.x, row.origin.x + gpui::px(9.));
    assert_eq!(name.origin.x, icon.right() + gpui::px(9.));
    move_mouse(cx, f32::from(row.center().x), f32::from(row.center().y));
    assert_eq!(cx.debug_bounds(test_selector(format!("folder-chevron-{key}"))).unwrap(), icon);
    assert_eq!(cx.debug_bounds(test_selector(format!("folder-name-{key}"))).unwrap(), name);
    let was_collapsed = view.read_with(cx, |w, _| w.browser.folder(folder).unwrap().collapsed);
    click(cx, test_selector(format!("folder-menu-{key}")));
    assert_eq!(view.read_with(cx, |w, _| w.browser.folder(folder).unwrap().collapsed), was_collapsed);
}

#[gpui::test]
fn sidebar_matches_reference_metrics_and_hover_controls(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://www.youtube.com/watch?v=1".into() });
    view.update(cx, |w, cx| {
        for tab in w.browser.space().pinned.iter().map(|tab| tab.id).collect::<Vec<_>>() { w.edit_browser(|browser| browser.favorite_tab(tab)); }
        cx.notify();
    });
    cx.run_until_parked();
    // The address shows only the site.
    assert_eq!(crate::window::address_host("https://www.youtube.com/watch?v=1"), "youtube.com");
    assert_eq!(crate::window::address_host("about:blank"), "about:blank");
    let address = cx.debug_bounds("address").unwrap();
    assert_eq!(address.size.height, gpui::px(36.));
    // Favorites stretch across the row with equal tiles.
    let first = cx.debug_bounds("favorite-4").unwrap();
    let second = cx.debug_bounds("favorite-5").unwrap();
    assert_eq!(first.size, second.size);
    assert_eq!(first.size.height, gpui::px(46.));
    assert_eq!(first.origin.x, address.origin.x);
    assert!(address.right() - second.right() < gpui::px(1.));
    // Rows are 38 px with a 3 px gap.
    let today = view.read_with(cx, |w, _| w.browser.space().today[0].id.get());
    let tab = cx.debug_bounds(test_selector(format!("tab-{today}"))).unwrap();
    assert_eq!(tab.size.height, gpui::px(38.));
    let new_tab = cx.debug_bounds("new-tab").unwrap();
    assert_eq!(new_tab.size.height, gpui::px(38.));
    // Traffic lights light up only while the pointer is over them.
    move_mouse(cx, 400., 400.);
    assert!(!view.read_with(cx, |w, _| w.traffic_lights_hovered));
    let lights = cx.debug_bounds("traffic-lights").unwrap();
    move_mouse(cx, f32::from(lights.center().x), f32::from(lights.center().y));
    assert!(view.read_with(cx, |w, _| w.traffic_lights_hovered));
    move_mouse(cx, 400., 400.);
    assert!(!view.read_with(cx, |w, _| w.traffic_lights_hovered));
    // The hover-only copy link button copies the page URL.
    click(cx, "address-copy-link");
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(clipboard, active_url(&view, cx));
    assert!(view.read_with(cx, |w, _| w.command_bar.is_none()));
}

#[gpui::test]
fn space_bar_and_site_controls_match_the_reference_layout(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    // Library on the left, Spaces centered, New Space on the right, all on one axis.
    let library = cx.debug_bounds("library").unwrap();
    let space = cx.debug_bounds("space-0").unwrap();
    let plus = cx.debug_bounds("new-space").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    assert_eq!(library.center().y, space.center().y);
    assert_eq!(plus.center().y, space.center().y);
    assert!(library.right() < space.origin.x && space.right() < plus.origin.x);
    assert_eq!(library.origin.x - sidebar.origin.x, sidebar.right() - plus.right());
    // The site controls button opens the panel and Escape-like dismissal closes it.
    click(cx, "address-site-controls");
    assert!(view.read_with(cx, |w, _| w.site_controls.is_some()));
    view.update(cx, |w, cx| w.close_site_controls(cx));
    assert!(view.read_with(cx, |w, _| w.site_controls.is_none()));
}

#[gpui::test]
fn new_menu_offers_reference_items_and_opens_the_matching_editor(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let menu = |view: &Entity<BrowserWindow>, cx: &mut VisualTestContext| {
        cx.run_until_parked();
        let overlay = view.read_with(cx, |w, _| w.new_menu.as_ref().and_then(|menu| menu.overlay)).expect("new menu is open");
        VisualTestContext::from_window(overlay.into(), cx)
    };
    click(cx, "new-space");
    let mut overlay = menu(&view, cx);
    for item in ["new-menu-space", "new-menu-folder", "new-menu-boost", "new-menu-split", "new-menu-tab"] {
        assert!(overlay.debug_bounds(item).is_some(), "{item}");
    }
    assert!(overlay.debug_bounds("new-menu-easel").is_none());
    click(&mut overlay, "new-menu-tab");
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.new_menu.is_none()));
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    bar(&view, cx).simulate_keystrokes("escape");
    // The keyboard walks the rows; Enter runs the selected one.
    click(cx, "new-space");
    let mut overlay = menu(&view, cx);
    overlay.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert_space_creation_form(&view, cx);
    cx.simulate_input("Menu draft");
    assert_eq!(view.read_with(cx, |w, _| w.sidebar_navigation.controller.draft().unwrap().name.clone()), "Menu draft");
    // Escape first leaves the name field and preserves the draft at the form root.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert!(w.sidebar_navigation.form.is_some());
        assert_eq!(w.sidebar_navigation.controller.draft().unwrap().name, "Menu draft");
        assert_eq!(w.browser.spaces.len(), 3);
    });
    assert!(cx.update(|window, cx| {
        view.read(cx).sidebar_navigation.form.as_ref().unwrap().read(cx).focus_handle(cx).is_focused(window)
    }));
    // Only Escape from the root cancels creation.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.sidebar_navigation.form.is_none() && w.sidebar_navigation.controller.draft().is_none()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces.len()), 3);
}

#[gpui::test]
fn sidebar_controls_animate_feedback_without_changing_layout(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let copy = cx.debug_bounds("address-copy-link").unwrap();
    click(cx, "address-copy-link");
    assert_eq!(view.read_with(cx, |w, _| w.copy_feedback.map(|(count, _)| count)), Some(0));
    assert_eq!(cx.debug_bounds("address-copy-link").unwrap(), copy);
    cx.executor().advance_clock(Duration::from_millis(1400));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("address-copy-link").unwrap(), copy);
    // The plus turns while the New menu is open and back when it closes.
    let plus = cx.debug_bounds("new-space").unwrap();
    click(cx, "new-space");
    assert_eq!(view.read_with(cx, |w, _| w.new_menu_toggles), 1);
    view.update(cx, |w, cx| w.close_new_menu(cx));
    assert_eq!(view.read_with(cx, |w, _| w.new_menu_toggles), 2);
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("new-space").unwrap(), plus);
}

#[gpui::test]
fn finished_copy_feedback_does_not_replay_when_the_controls_reappear(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    click(cx, "address-copy-link");
    assert!(cx.debug_bounds("copy-check").is_some());
    // Feedback that already ran out stays finished across hover changes.
    view.update(cx, |w, cx| { w.copy_feedback = w.copy_feedback.map(|(count, _)| (count, std::time::Instant::now() - Duration::from_secs(5))); cx.notify(); });
    cx.run_until_parked();
    assert!(cx.debug_bounds("copy-check").is_none());
    move_mouse(cx, 600., 400.);
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    move_mouse(cx, f32::from(sidebar.center().x), f32::from(sidebar.center().y));
    assert!(cx.debug_bounds("copy-check").is_none());
}

#[gpui::test]
fn sidebar_context_menus_follow_the_reference_layout(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    // The address field has its own menu; Copy URL copies the current page.
    right_click(cx, "address");
    let mut menu = tab_menu(&view, cx);
    for row in ["tab-menu-copy", "tab-menu-share", "tab-menu-capture", "tab-menu-capture-full-page", "tab-menu-show-as-toolbar", "tab-menu-disable-shared-quotes"] {
        assert!(menu.debug_bounds(row).is_some(), "{row}");
    }
    click(&mut menu, "tab-menu-copy");
    assert_eq!(cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())), active_url(&view, cx));
    // Empty sidebar space opens the Space menu.
    let page = cx.debug_bounds("space-page").unwrap();
    let empty = gpui::point(page.center().x, page.bottom() - gpui::px(4.));
    cx.simulate_mouse_move(empty, None, Modifiers::none());
    cx.simulate_mouse_down(empty, gpui::MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(empty, gpui::MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.tab_menu.as_ref().map(|menu| menu.target)), Some(crate::tab_menu::MenuTarget::Space(view.read_with(cx, |w, _| w.browser.space().id))));
    let mut menu = tab_menu(&view, cx);
    for row in ["tab-menu-space-icon", "tab-menu-space-rename", "tab-menu-edit-theme-color", "tab-menu-set-profile", "tab-menu-space-new-folder", "tab-menu-live-folders", "tab-menu-share-space", "tab-menu-manage-spaces", "tab-menu-delete-space"] {
        assert!(menu.debug_bounds(row).is_some(), "{row}");
    }
    // Set Profile now opens the reference sheet for this stable Space ID;
    // it is no longer a placeholder submenu containing only Default.
    assert!(menu.debug_bounds("tab-context-submenu").is_none());
    click(&mut menu, "tab-menu-set-profile");
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_none()));
    let overlay = view.read_with(cx, |w, _| w.user_profile_dialog.as_ref().unwrap().overlay.unwrap());
    let mut sheet = VisualTestContext::from_window(overlay.into(), &cx.cx);
    sheet.run_until_parked();
    assert!(sheet.debug_bounds("user-profile-sheet").is_some());
    assert!(sheet.debug_bounds("profile-dropdown").is_some());
    sheet.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.user_profile_dialog.is_none()));
}

#[gpui::test]
fn sidebar_pin_undo_and_redo_work_from_shortcuts_and_edit_menu_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = view.read_with(cx, |w, _| w.browser.space().today.iter().map(|tab| tab.id).collect::<Vec<_>>());
    let tab = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-d cmd-z");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().today.iter().map(|tab| tab.id).collect::<Vec<_>>()), original);
    dispatch_menu(cx, "Edit", "Redo");
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned.iter().any(|entry| entry.id == tab)));
    dispatch_menu(cx, "Edit", "Undo");
    fake.send(cx, fake.shortcut(Shortcut { cmd: true, shift: true, key: "z".into(), ..Default::default() }));
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned.iter().any(|entry| entry.id == tab)));
    assert!(fake.take_calls().iter().all(|call| !matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Undo(..) | Call::Redo(..))));
}

#[gpui::test]
fn web_edit_focus_keeps_page_undo_independent_of_sidebar_history(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-d");
    fake.send(cx, Event::EditFocusChanged { view: page(1), editable: true });
    fake.take_calls();
    dispatch_menu(cx, "Edit", "Undo");
    dispatch_menu(cx, "Edit", "Redo");
    assert_eq!(fake.take_calls(), vec![Call::Undo(1), Call::Redo(1)]);
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned.iter().any(|tab| tab.id.get() == 8)));
    fake.send(cx, Event::EditFocusChanged { view: page(1), editable: false });
    dispatch_menu(cx, "Edit", "Undo");
    assert!(view.read_with(cx, |w, _| w.browser.space().today.iter().any(|tab| tab.id.get() == 8)));
}

#[gpui::test]
fn command_bar_undo_does_not_consume_sidebar_history(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-d cmd-t");
    let mut input = bar(&view, cx);
    input.simulate_input("q");
    input.simulate_keystrokes("cmd-z");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    input.simulate_keystrokes("cmd-z escape");
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned.iter().any(|tab| tab.id.get() == 8)));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-z");
    assert!(view.read_with(cx, |w, _| w.browser.space().today.iter().any(|tab| tab.id.get() == 8)));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Undo(..))));
}

#[gpui::test]
fn archive_undo_and_redo_restore_tab_identity_order_and_engine_views_through_keys_and_menus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = today_urls(&view, cx);
    let closed = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-w");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    assert!(fake.take_calls().contains(&Call::Close(1)));
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(today_urls(&view, cx), before);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), closed);
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::Create(_, url) if url == "https://www.chromium.org")));
    dispatch_menu(cx, "Edit", "Redo");
    assert_eq!(today_urls(&view, cx).len(), before.len() - 1);
    dispatch_menu(cx, "Edit", "Undo");
    assert_eq!(today_urls(&view, cx), before);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), closed);
}

#[gpui::test]
fn clear_today_undo_is_atomic_and_never_reloads_the_selected_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = today_urls(&view, cx);
    fake.take_calls();
    dispatch_menu(cx, "Tabs", "Clear Today");
    assert_eq!(today_urls(&view, cx), vec!["https://www.chromium.org".to_string()]);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 2);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(today_urls(&view, cx), before);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 0);
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(today_urls(&view, cx).len(), 1);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 2);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn whole_split_archive_undo_restores_all_panes_and_redo_closes_the_group(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let group = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    right_click(cx, "tab-2");
    click(&mut tab_menu(&view, cx), "tab-menu-close");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    assert!(view.read_with(cx, |w, _| w.browser.all_tabs().next().is_none()));
    fake.take_calls();
    dispatch_menu(cx, "Edit", "Undo");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().id), group);
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 2);
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Create(..))).count(), 2);
    cx.simulate_keystrokes("cmd-shift-z");
    assert!(view.read_with(cx, |w, _| w.browser.all_tabs().next().is_none()));
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Close(..))).count(), 2);
}

#[gpui::test]
fn clear_today_keeps_the_focused_split_and_undo_restores_background_items(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        w.edit_browser(|browser| browser.new_tab("https://background.example", std::time::SystemTime::now()));
        w.edit_browser(|browser| browser.activate_tab(active, std::time::SystemTime::now()));
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
    cx.simulate_keystrokes("cmd-shift-k");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 2);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 2);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 3);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 2);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn empty_history_search_owns_undo_instead_of_restoring_an_archived_tab(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-w cmd-y");
    let archived = view.read_with(cx, |w, _| w.browser.archive.len());
    cx.simulate_keystrokes("cmd-z");
    dispatch_menu(cx, "Edit", "Undo");
    assert!(view.read_with(cx, |w, _| w.history_is_open()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), archived);
    assert_eq!(view.read_with(cx, |w, cx| w.history_page.input.read(cx).text().to_string()), "");
}

#[gpui::test]
fn archive_undo_from_another_space_restores_the_source_space_and_settles_its_sidebar(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-w ctrl-2 cmd-z");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 0);
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    assert!(cx.debug_bounds("tab-8").is_some());
}

#[gpui::test]
fn pane_close_undo_reconstructs_the_pair_and_keeps_the_surviving_native_page_focused(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let group = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    cx.simulate_keystrokes("cmd-w");
    let survivor = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().id), group);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), survivor);
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 2);
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..) | Call::FocusPage(3))));
    dispatch_menu(cx, "Edit", "Redo");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 0);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Close(..))).count(), 2);
    dispatch_menu(cx, "Edit", "Undo");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().id), group);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), survivor);
}

#[gpui::test]
fn header_close_undo_restores_the_anchor_of_a_three_pane_group_without_recreating_survivors(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let anchor = view.read_with(cx, |w, _| w.browser.active_split().unwrap().anchor);
    click(cx, "pane-close-2");
    let survivor = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    fake.take_calls();
    dispatch_menu(cx, "Edit", "Undo");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().anchor), anchor);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), survivor);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 3);
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn web_editor_undo_does_not_restore_a_closed_pane_until_edit_focus_leaves_the_field(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    cx.simulate_keystrokes("cmd-w");
    fake.send(cx, Event::EditFocusChanged { view: page(1), editable: true });
    fake.take_calls();
    dispatch_menu(cx, "Edit", "Undo");
    assert_eq!(fake.take_calls(), vec![Call::Undo(1)]);
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 1);
    fake.send(cx, Event::EditFocusChanged { view: page(1), editable: false });
    dispatch_menu(cx, "Edit", "Undo");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 2);
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Create(..))).count(), 1);
}

#[gpui::test]
fn creating_the_restored_native_pane_cannot_steal_the_surviving_panes_selection(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    cx.simulate_keystrokes("cmd-w");
    let survivor = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    fake.0.borrow_mut().focus_on_create = true;
    fake.take_calls();
    cx.simulate_keystrokes("cmd-z");
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), survivor);
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().shown()), Some(survivor));
    assert!(fake.take_calls().contains(&Call::FocusPage(1)));
}

#[gpui::test]
fn cancelled_split_draft_keeps_the_previous_pin_redo(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let source = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("cmd-d cmd-z ctrl-shift-=");
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-shift-z");
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned.iter().any(|tab| tab.id == source)));
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty() && w.browser.active_split().is_none()));
}

#[gpui::test]
fn cancelled_split_draft_on_a_pinned_group_discards_every_copy_without_consuming_redo(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    cx.simulate_keystrokes("cmd-d cmd-t");
    bar(&view, cx).simulate_input("https://closed.example");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-w cmd-z cmd-1 ctrl-shift-=");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 3);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned.len()), 2);
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 2);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 2);
}

#[gpui::test]
fn cancelled_split_draft_with_cmd_w_keeps_a_previous_pane_close_redo(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    cx.simulate_keystrokes("cmd-w cmd-z ctrl-shift-=");
    bar(&view, cx).simulate_keystrokes("cmd-w");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 2);
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 0);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
}

fn another_live_profile_window(a: &Entity<BrowserWindow>, visual: &mut VisualTestContext) -> (Entity<BrowserWindow>, VisualTestContext) {
    let (profile, pages) = a.read_with(visual, |view, _| {
        let mut pages = view.pages.as_ref().unwrap().borrow_mut();
        pages.set_parent(1usize as *mut _);
        (view.profile.handle.clone(), pages.for_host(2usize as *mut _))
    });
    let handle = visual.cx.update(|cx| {
        use gpui::AppContext;
        cx.open_window(gpui::WindowOptions::default(), |window, cx| cx.new(|cx| {
            let view = BrowserWindow::with_profile_pages(profile, None, Some(pages), window, cx);
            view.focus(window);
            view
        })).unwrap()
    });
    let view = handle.root(&mut visual.cx).unwrap();
    let mut visual_b = VisualTestContext::from_window(handle.into(), &visual.cx);
    view.update(&mut visual_b, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    visual_b.run_until_parked();
    (view, visual_b)
}

#[gpui::test]
fn shared_live_pages_move_once_and_route_events_to_the_host_window(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (a, visual_a) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(visual_a, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut visual_b) = another_live_profile_window(&a, visual_a);
    fake.take_calls();
    click(&mut visual_b, test_selector(format!("tab-{}", tab.get())));
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::MoveHost(1, 2, _))).count(), 1, "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))), "{calls:?}");
    assert!(!a.read_with(visual_a, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
    assert!(b.read_with(&visual_b, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
    fake.send(&mut visual_b, Event::LoadingChanged { view: page(1), loading: false, can_go_back: true, can_go_forward: false });
    fake.send(&mut visual_b, Event::UrlChanged { view: page(1), url: "https://shared.example/changed".into() });
    for (view, visual) in [(&a, &mut *visual_a), (&b, &mut visual_b)] {
        view.read_with(visual, |view, _| {
            assert_eq!(view.browser.active_tab().unwrap().url, "https://shared.example/changed");
            assert!(view.pages.as_ref().unwrap().borrow().state(tab).can_go_back);
        });
    }
    visual_b.simulate_keystrokes("cmd-[");
    assert!(fake.take_calls().contains(&Call::Back(1)));
    a.update(visual_a, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    let calls = fake.take_calls();
    assert!(calls.iter().any(|call| matches!(call, Call::MoveHost(1, 1, _))), "{calls:?}");
    b.update(&mut visual_b, |_, cx| cx.notify());
    visual_b.run_until_parked();
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::MoveHost(..) | Call::Create(..) | Call::Navigate(..))));
}

#[gpui::test]
fn shared_live_pages_survive_closing_the_window_that_hosts_them(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (a, visual_a) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(visual_a, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut visual_b) = another_live_profile_window(&a, visual_a);
    click(&mut visual_b, test_selector(format!("tab-{}", tab.get())));
    fake.take_calls();
    visual_b.simulate_keystrokes("cmd-shift-w");
    let calls = fake.take_calls();
    assert!(calls.iter().any(|call| matches!(call, Call::MoveHost(1, 1, _))), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Create(..))), "{calls:?}");
    a.update(visual_a, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    assert!(a.read_with(visual_a, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
    assert!(fake.take_calls().contains(&Call::Visible(1, true)));
    drop(b);
}

#[gpui::test]
fn shared_live_pages_reject_a_failed_transfer_without_creating_a_duplicate(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (a, visual_a) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(visual_a, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut visual_b) = another_live_profile_window(&a, visual_a);
    fake.0.borrow_mut().reject_move = true;
    fake.take_calls();
    click(&mut visual_b, test_selector(format!("tab-{}", tab.get())));
    visual_b.simulate_keystrokes("cmd-r");
    let calls = fake.take_calls();
    assert!(calls.iter().any(|call| matches!(call, Call::MoveHost(1, 2, _))), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Reload(..))), "{calls:?}");
    assert!(a.read_with(visual_a, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
    assert!(!b.read_with(&visual_b, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
}

#[gpui::test]
fn shared_live_pages_distinguish_two_tabs_with_the_same_url(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (a, visual_a) = open_with(cx, Some(Box::new(fake.clone())));
    let url = active_url(&a, visual_a).unwrap();
    let (b, mut visual_b) = another_live_profile_window(&a, visual_a);
    fake.take_calls();
    visual_b.simulate_keystrokes("cmd-t");
    // A pasted URL is submitted as a new tab, even if a matching tab is suggested.
    bar(&b, &mut visual_b).simulate_input(&url);
    bar(&b, &mut visual_b).simulate_keystrokes("enter");
    let calls = fake.take_calls();
    assert!(calls.iter().any(|call| matches!(call, Call::Create(2, loaded) if loaded == &url)), "{calls:?}");
    assert_eq!(active_url(&a, visual_a), Some(url));
}

#[gpui::test]
fn background_renderer_metadata_preserves_the_foreground_session_selection(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (a, visual_a) = open_with(cx, Some(Box::new(fake.clone())));
    let (b, mut visual_b) = another_profile_window(&a, visual_a);
    visual_b.simulate_keystrokes("cmd-t");
    bar(&b, &mut visual_b).simulate_input("https://foreground.example");
    bar(&b, &mut visual_b).simulate_keystrokes("enter");
    let foreground = b.read_with(&visual_b, |view, _| view.browser.active_tab().unwrap().id);
    fake.send(visual_a, Event::TitleChanged { view: page(1), title: "Background loaded".into() });
    fake.send(visual_a, Event::UrlChanged { view: page(1), url: "https://background.example/redirect".into() });
    let json = a.read_with(visual_a, |view, _| view.profile.session_json(&view.split_draft));
    let restored = browser_core::session::from_json(&json).unwrap();
    assert_eq!(restored.active_tab().unwrap().id, foreground);
    assert_eq!(active_url(&b, &mut visual_b).as_deref(), Some("https://foreground.example"));
}

#[gpui::test]
fn shared_live_split_moves_every_pane_and_closes_only_the_focused_page(cx: &mut TestAppContext) {
    let (a, visual_a, fake) = open_split_pages(cx, 2);
    let active = a.read_with(visual_a, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut visual_b) = another_live_profile_window(&a, visual_a);
    fake.take_calls();
    click(&mut visual_b, test_selector(format!("sidebar-pane-{}", active.get())));
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::MoveHost(_, 2, _))).count(), 2, "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))), "{calls:?}");
    assert_eq!(b.read_with(&visual_b, |view, _| view.pages.as_ref().unwrap().borrow().surface_layout().len()), 2);
    visual_b.simulate_keystrokes("cmd-w");
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Close(_))).count(), 1, "{calls:?}");
    assert!(calls.contains(&Call::Close(2)), "{calls:?}");
    assert_eq!(b.read_with(&visual_b, |view, _| view.pages.as_ref().unwrap().borrow().surface_layout().len()), 1);
    assert!(!a.read_with(visual_a, |view, _| view.browser.all_tabs().any(|tab| tab.id == active)));
}

#[gpui::test]
fn queued_shared_page_callbacks_keep_fifo_order_when_the_page_changes_host(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (a, visual_a) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(visual_a, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut visual_b) = another_live_profile_window(&a, visual_a);
    let old = Event::UrlChanged { view: page(1), url: "https://fifo.example/old".into() };
    let new = Event::UrlChanged { view: page(1), url: "https://fifo.example/new".into() };
    fake.0.borrow_mut().events.push(old.clone());
    b.update(&mut visual_b, |view, _| assert!(view.pages.as_ref().unwrap().borrow_mut().take_events().is_empty()));
    b.update(&mut visual_b, |view, _| {
        view.edit_browser(|browser| browser.activate_tab(tab, std::time::SystemTime::now()));
        view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser);
    });
    fake.0.borrow_mut().events.push(new.clone());
    let events = b.update(&mut visual_b, |view, _| view.pages.as_ref().unwrap().borrow_mut().take_events());
    assert_eq!(events, vec![old, new]);
    a.update(visual_a, |view, _| assert!(view.pages.as_ref().unwrap().borrow_mut().take_events().is_empty()));
}

#[gpui::test]
fn reactivating_a_host_refreshes_its_intent_before_moving_the_live_page(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (a, visual_a) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(visual_a, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut visual_b) = another_live_profile_window(&a, visual_a);
    click(&mut visual_b, test_selector(format!("tab-{}", tab.get())));
    fake.take_calls();
    // Both edits happen before profile notification or another render of A.
    visual_a.cx.update(|cx| {
        b.update(cx, |view, _| {
            view.pages.as_ref().unwrap().borrow_mut().record(&Event::UrlChanged { view: page(1), url: "https://fresh.example/navigated".into() });
            view.edit_profile(|browser| browser.set_url(tab, "https://fresh.example/navigated"));
        });
        a.update(cx, |view, _| {
            view.refresh_profile_projection();
            view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser);
        });
    });
    let calls = fake.take_calls();
    assert!(calls.iter().any(|call| matches!(call, Call::MoveHost(1, 1, _))), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Navigate(..) | Call::Create(..) | Call::Close(..))), "{calls:?}");
    assert_eq!(active_url(&a, visual_a).as_deref(), Some("https://fresh.example/navigated"));
}

#[gpui::test]
fn reactivating_a_shared_page_host_preserves_its_inline_editor_focus(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (a, visual_a) = open_with(cx, Some(Box::new(fake.clone())));
    let (b, mut visual_b) = another_live_profile_window(&a, visual_a);
    a.update(visual_a, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    double_click(visual_a, "tab-8");
    visual_a.simulate_input("Draft name");
    b.update(&mut visual_b, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    fake.take_calls();
    a.update(visual_a, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_))));
    assert_eq!(inline_name(&a, visual_a).as_deref(), Some("Draft name"));
    visual_a.simulate_input(" continued");
    assert_eq!(inline_name(&a, visual_a).as_deref(), Some("Draft name continued"));
    visual_a.simulate_keystrokes("enter");
    fake.take_calls();
    click(visual_a, "tab-7");
    assert!(fake.take_calls().contains(&Call::FocusPage(2)));
}


#[gpui::test]
fn shared_page_snapshots_coalesce_and_keep_the_last_good_image_on_failure(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (a, visual_a) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(visual_a, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut visual_b) = another_live_profile_window(&a, visual_a);
    fake.take_calls();
    let first = a.update(visual_a, |view, _| view.pages.as_ref().unwrap().borrow_mut().request_snapshot(tab).unwrap());
    let again = b.update(&mut visual_b, |view, _| view.pages.as_ref().unwrap().borrow_mut().request_snapshot(tab).unwrap());
    assert_eq!(first, again);
    assert_eq!(fake.take_calls(), vec![Call::Snapshot(1, first)]);
    fake.0.borrow_mut().snapshots.push(engine::PageSnapshot { view: page(1), request: first, png: Some(vec![1, 2, 3]) });
    b.update(&mut visual_b, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    for (view, visual) in [(&a, &mut *visual_a), (&b, &mut visual_b)] {
        view.read_with(visual, |view, _| {
            let pages = view.pages.as_ref().unwrap().borrow();
            assert_eq!(pages.snapshot(tab).unwrap().1.as_slice(), &[1, 2, 3]);
            assert!(!pages.snapshot_pending(tab, first));
        });
    }
    let second = a.update(visual_a, |view, _| view.pages.as_ref().unwrap().borrow_mut().request_snapshot(tab).unwrap());
    assert_ne!(first, second);
    fake.0.borrow_mut().snapshots.push(engine::PageSnapshot { view: page(1), request: first, png: Some(vec![9]) });
    a.update(visual_a, |view, _| {
        let mut pages = view.pages.as_ref().unwrap().borrow_mut();
        pages.take_events();
        assert!(pages.snapshot_pending(tab, second));
        assert_eq!(pages.snapshot(tab).unwrap().0, first);
    });
    fake.0.borrow_mut().snapshots.push(engine::PageSnapshot { view: page(1), request: second, png: None });
    a.update(visual_a, |view, _| {
        let mut pages = view.pages.as_ref().unwrap().borrow_mut();
        pages.take_events();
        assert!(!pages.snapshot_pending(tab, second));
        assert_eq!(pages.snapshot(tab).unwrap().1.as_slice(), &[1, 2, 3]);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Close(..))));
}

#[gpui::test]
fn a_snapshot_of_a_discarded_renderer_cannot_replace_its_reopened_tab(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (view, visual) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = view.read_with(visual, |view, _| view.browser.active_tab().unwrap().id);
    let first = view.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().request_snapshot(tab).unwrap());
    view.update(visual, |view, _| {
        let mut pages = view.pages.as_ref().unwrap().borrow_mut();
        pages.discard(tab);
        pages.sync(&view.browser, true);
    });
    fake.0.borrow_mut().snapshots.push(engine::PageSnapshot { view: page(1), request: first, png: Some(vec![9]) });
    view.update(visual, |view, _| {
        let mut pages = view.pages.as_ref().unwrap().borrow_mut();
        pages.take_events();
        assert!(pages.snapshot(tab).is_none());
        assert!(!pages.snapshot_pending(tab, first));
    });
    fake.0.borrow_mut().reject_snapshot = true;
    fake.take_calls();
    view.update(visual, |view, _| assert!(view.pages.as_ref().unwrap().borrow_mut().request_snapshot(tab).is_none()));
    assert_eq!(fake.take_calls().len(), 1);
    fake.0.borrow_mut().reject_snapshot = false;
    assert!(view.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().request_snapshot(tab).is_some()));
}


#[gpui::test]
fn a_late_initial_renderer_does_not_take_the_open_command_bars_native_focus(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    fake.0.borrow_mut().reject_create = true;
    let (view, visual) = open_with(cx, Some(Box::new(fake.clone())));
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("https://late.example/first");
    fake.take_calls();
    fake.0.borrow_mut().reject_create = false;
    view.update(visual, |_, cx| cx.notify());
    visual.run_until_parked();
    let calls = fake.take_calls();
    assert!(calls.iter().any(|call| matches!(call, Call::Create(1, _))), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))), "{calls:?}");
    assert!(bar_input_focused(&view, visual));
    bar(&view, visual).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, visual).as_deref(), Some("https://late.example/first"));
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_))));
}

#[path = "page_handoff_tests.rs"]
mod page_handoff_tests;

fn enable_recording_inspector(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, fake: &FakeEngine) {
    fake.0.borrow_mut().inspector_supported = true;
    view.update(cx, |view, cx| {
        view.pages.as_ref().unwrap().borrow_mut().set_parent(1usize as *mut _);
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
}

#[gpui::test]
fn devtools_menu_shortcuts_and_command_bar_share_live_page_actions(cx: &mut TestAppContext) {
    use engine::InspectorPanel::{Elements, Console, Network};
    let (view, cx, fake) = open_pages(cx);
    enable_recording_inspector(&view, cx, &fake);
    let (tab, count) = view.read_with(cx, |view, _| {
        let tab = view.browser.active_tab().unwrap().id;
        (tab, view.browser.all_tabs().count())
    });
    for (label, shortcut, panel, inspect) in [
        ("Developer Tools", Some("cmd-alt-i"), Elements, false),
        ("Inspect Elements", Some("cmd-alt-c"), Elements, true),
        ("JavaScript Console", Some("cmd-alt-j"), Console, false),
        ("Network Inspector", None, Network, false),
    ] {
        for route in 0..3 {
            fake.take_calls();
            match route {
                0 => dispatch_menu(cx, "View", label),
                1 if shortcut.is_some() => { cx.simulate_keystrokes(shortcut.unwrap()); cx.run_until_parked(); }
                _ => run_named_command(&view, cx, label),
            }
            let calls = fake.take_calls();
            let opens: Vec<_> = calls.iter().filter(|call| matches!(call, Call::InspectorOpen(..))).collect();
            assert!(matches!(opens.as_slice(), [Call::InspectorOpen(id, 1, bounds, selected, mode)]
                if view.read_with(cx, |view, _| view.pages.as_ref().unwrap().borrow().tab_for(ViewId::from_raw(*id))) == Some(tab) && *selected == panel && *mode == inspect && bounds.width > 0. && bounds.height > 0.), "{label}, route {route}: {calls:?}");
            assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(_) | Call::Close(_))), "{calls:?}");
            assert!(bar_text(&view, cx).is_none());
            assert_eq!(view.read_with(cx, |view, _| (view.browser.active_tab().unwrap().id, view.browser.all_tabs().count())), (tab, count));
        }
    }
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::InspectorOpen(_, 1, _, Elements, false))));
    for shortcut in ["cmd-alt-i", "f12", "cmd-alt-c", "cmd-alt-j"] {
        assert_eq!(crate::window::WINDOW_SHORTCUTS.iter().filter(|entry| **entry == shortcut).count(), 1);
    }
}

#[gpui::test]
fn devtools_requires_supported_engine_native_parent_and_live_internal_free_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    assert!(!view.read_with(cx, |view, _| view.devtools_available()));
    fake.0.borrow_mut().inspector_supported = true;
    assert!(!view.read_with(cx, |view, _| view.devtools_available()));
    enable_recording_inspector(&view, cx, &fake);
    assert!(view.read_with(cx, |view, _| view.devtools_available()));
    view.update(cx, |view, cx| {
        view.edit_browser(|browser| browser.navigate(browser.active_tab().unwrap().id, "browser://history"));
        view.pages.as_ref().unwrap().borrow_mut().sync(&view.browser, true);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(!cx.update(|window, cx| window.is_action_available(&crate::window::DeveloperTools, cx)));
    assert!(!cx.update(|window, cx| window.is_action_available(&crate::window::InspectElements, cx)));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-alt-i cmd-alt-c cmd-alt-j f12");
    cx.run_until_parked();
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::InspectorOpen(..))));
    cx.simulate_keystrokes("cmd-t");
    assert!(!view.read_with(cx, |view, cx| view.suggestions(cx).iter().any(|row| matches!(row, Suggestion::Action { command: crate::window::Command::DevTools(_), .. }))));
}

#[gpui::test]
fn devtools_targets_focused_split_pane_not_anchor_and_rejects_wrong_host(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    enable_recording_inspector(&view, cx, &fake);
    for index in [0, 1] {
        view.update(cx, |view, cx| {
            view.edit_browser(|browser| browser.focus_split_pane(index, std::time::SystemTime::now()));
            view.pages.as_ref().unwrap().borrow_mut().sync(&view.browser, true);
            cx.notify();
        });
        cx.run_until_parked();
        let focused = view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id);
        fake.take_calls();
        cx.simulate_keystrokes("cmd-alt-j");
        cx.run_until_parked();
        assert!(fake.take_calls().iter().any(|call| matches!(call, Call::InspectorOpen(id, 1, _, engine::InspectorPanel::Console, false) if view.read_with(cx, |view, _| view.pages.as_ref().unwrap().borrow().tab_for(ViewId::from_raw(*id))) == Some(focused))));
    }
    let (_, mut other) = another_live_profile_window(&view, cx);
    other.run_until_parked();
    assert!(!view.read_with(cx, |view, _| view.devtools_available()));
    fake.take_calls();
    view.update(cx, |view, cx| view.run_devtools(crate::devtools::Command::Console, cx));
    cx.run_until_parked();
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::InspectorOpen(..))));
}

#[test]
fn developer_menu_preserves_observed_arc_order_without_visible_f12_duplicate() {
    let view = crate::menus::build().into_iter().find(|menu| menu.name.as_ref() == "View").unwrap();
    let developer = view.items.into_iter().find_map(|item| match item {
        gpui::MenuItem::Submenu(menu) if menu.name.as_ref() == "Developer" => Some(menu),
        _ => None,
    }).unwrap();
    let labels: Vec<_> = developer.items.into_iter().filter_map(|item| match item {
        gpui::MenuItem::Action { name, .. } => Some(name.to_string()),
        _ => None,
    }).collect();
    assert_eq!(labels, ["View Source", "Developer Tools", "Inspect Elements", "JavaScript Console", "Network Inspector"]);
}

fn accept_recording_inspector(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, fake: &FakeEngine) {
    fake.0.borrow_mut().inspector_accept = true;
    enable_recording_inspector(view, cx, fake);
}

fn inspector_status(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Option<engine::InspectorStatus> {
    view.read_with(cx, |view, _| view.pages.as_ref().unwrap().borrow().devtools_status())
}

fn inspector_event(fake: &FakeEngine, cx: &mut VisualTestContext, event: engine::InspectorEvent) {
    match event {
        event @ (engine::InspectorEvent::Focused { .. } | engine::InspectorEvent::Shortcut { .. }) => fake.0.borrow_mut().events.push(Event::InspectorInput(event)),
        event => fake.0.borrow_mut().inspector_events.push(event),
    }
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
}

#[gpui::test]
fn devtools_switches_observed_panels_repeats_inspect_and_closes_only_frontend(cx: &mut TestAppContext) {
    use engine::InspectorPanel::{Elements, Console, Network};
    let (view, cx, fake) = open_pages(cx);
    accept_recording_inspector(&view, cx, &fake);
    let original_url = active_url(&view, cx);
    let count = view.read_with(cx, |view, _| view.browser.all_tabs().count());
    cx.simulate_keystrokes("cmd-alt-i");
    cx.run_until_parked();
    let initial = inspector_status(&view, cx).unwrap();
    assert_eq!(initial.selected_panel, Some(Elements));
    cx.simulate_keystrokes("cmd-alt-j");
    cx.run_until_parked();
    let console = inspector_status(&view, cx).unwrap();
    assert_eq!((console.id, console.selected_panel), (initial.id, Some(Console)));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-alt-j");
    cx.run_until_parked();
    assert!(inspector_status(&view, cx).is_none());
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::InspectorClose(initial.id)));
    assert!(calls.contains(&Call::FocusPage(raw(initial.inspected))));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(_) | Call::Navigate(..) | Call::Reload(_))));
    run_named_command(&view, cx, "Network Inspector");
    let network = inspector_status(&view, cx).unwrap();
    assert_eq!(network.selected_panel, Some(Network));
    run_named_command(&view, cx, "Network Inspector");
    assert!(inspector_status(&view, cx).is_none());
    cx.simulate_keystrokes("cmd-alt-c");
    cx.run_until_parked();
    let inspect = inspector_status(&view, cx).unwrap();
    fake.take_calls();
    cx.simulate_keystrokes("cmd-alt-c");
    cx.run_until_parked();
    assert_eq!(inspector_status(&view, cx).unwrap().id, inspect.id);
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::InspectorOpen(_, _, _, Elements, true))));
    cx.simulate_keystrokes("cmd-alt-j cmd-alt-i");
    cx.run_until_parked();
    assert!(inspector_status(&view, cx).is_none());
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    let reopened = inspector_status(&view, cx).unwrap();
    assert_eq!(reopened.selected_panel, Some(Elements));
    fake.take_calls();
    inspector_event(&fake, cx, engine::InspectorEvent::CloseRequested { inspector: reopened.id });
    assert!(inspector_status(&view, cx).is_none());
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::FocusPage(raw(initial.inspected))));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(_) | Call::Navigate(..) | Call::Reload(_))));
    assert_eq!(active_url(&view, cx), original_url);
    assert_eq!(view.read_with(cx, |view, _| view.browser.all_tabs().count()), count);
}

#[gpui::test]
fn devtools_native_frames_and_mouse_holes_exclude_gpui_divider(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    accept_recording_inspector(&view, cx, &fake);
    let (tab, layout) = view.read_with(cx, |view, _| (view.browser.active_tab().unwrap().id, view.pages.as_ref().unwrap().borrow().surface_layout()));
    cx.simulate_keystrokes("cmd-alt-i");
    cx.run_until_parked();
    let status = inspector_status(&view, cx).unwrap();
    let original = layout.iter().find(|(id, _)| *id == tab).unwrap().1;
    let (page, divider, frontend) = crate::devtools::frames(original).unwrap();
    assert_eq!(fake.0.borrow().bounds[&raw(status.inspected)], page);
    assert_eq!(fake.0.borrow().inspector_bounds, Some(frontend));
    view.read_with(cx, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        let actual = pages.surface_layout();
        assert_eq!(actual.iter().find(|(id, _)| *id == tab).unwrap().1, page);
        for (other, bounds) in layout.iter().filter(|(id, _)| *id != tab) {
            assert_eq!(actual.iter().find(|(id, _)| id == other).unwrap().1, *bounds);
        }
        let holes = pages.surface_bounds();
        assert_eq!(holes.len(), 3);
        assert!(holes.contains(&page) && holes.contains(&frontend));
        let x = divider.x + divider.width / 2.;
        let y = divider.y + divider.height / 2.;
        assert!(!holes.iter().any(|hole| x >= hole.x && x < hole.x + hole.width && y >= hole.y && y < hole.y + hole.height));
        assert_eq!(pages.devtools_divider(), Some(divider));
    });
    fake.take_calls();
    cx.simulate_keystrokes("cmd-alt-i");
    cx.run_until_parked();
    assert_eq!(fake.0.borrow().bounds[&raw(status.inspected)], original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(_) | Call::Close(_))));
}

#[gpui::test]
fn devtools_frontend_shortcuts_validate_owner_and_preserve_command_bar_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    accept_recording_inspector(&view, cx, &fake);
    cx.simulate_keystrokes("cmd-alt-i");
    cx.run_until_parked();
    let inspector = inspector_status(&view, cx).unwrap().id;
    let shortcut = |key: &str, alt| engine::Shortcut { cmd: true, alt, key: key.into(), ..Default::default() };
    inspector_event(&fake, cx, engine::InspectorEvent::Shortcut { inspector: engine::InspectorId::from_raw(999), shortcut: shortcut("j", true) });
    assert_eq!(inspector_status(&view, cx).unwrap().selected_panel, Some(engine::InspectorPanel::Elements));
    inspector_event(&fake, cx, engine::InspectorEvent::Shortcut { inspector, shortcut: shortcut("j", true) });
    assert_eq!(inspector_status(&view, cx).unwrap().selected_panel, Some(engine::InspectorPanel::Console));
    inspector_event(&fake, cx, engine::InspectorEvent::Shortcut { inspector, shortcut: shortcut("t", false) });
    assert!(bar_text(&view, cx).is_some());
    fake.take_calls();
    inspector_event(&fake, cx, engine::InspectorEvent::FocusRequested { inspector });
    inspector_event(&fake, cx, engine::InspectorEvent::CloseRequested { inspector });
    let calls = fake.take_calls();
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_) | Call::InspectorFocus(_))), "{calls:?}");
    assert!(bar_text(&view, cx).is_some());
    bar(&view, cx).simulate_input("overlay still owns input");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("overlay still owns input"));
    assert!(inspector_status(&view, cx).is_none());
}

#[gpui::test]
fn devtools_uses_the_private_windows_current_page_engine(cx: &mut TestAppContext) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new_private();
    browser.new_tab("https://private.example/document", std::time::SystemTime::now());
    let (view, visual) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    visual.run_until_parked();
    accept_recording_inspector(&view, visual, &fake);

    let focused = view.read_with(visual, |view, _| {
        assert!(view.browser.is_private());
        view.browser.active_tab().unwrap().id
    });
    visual.simulate_keystrokes("cmd-alt-j");
    visual.run_until_parked();
    let page = inspector_status(&view, visual).unwrap().inspected;
    assert_eq!(view.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().tab_for(page)), Some(focused));
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::InspectorOpen(id, 1, _, engine::InspectorPanel::Console, false) if *id == raw(page))));
    view.read_with(visual, |view, _| {
        assert!(view.browser.active_user_profile().history.is_empty());
        assert_eq!(view.browser.all_tabs().count(), 1);
        assert_eq!(view.browser.active_tab().unwrap().url, "https://private.example/document");
    });
}

#[gpui::test]
fn focusing_a_split_inspector_selects_its_pane_without_focusing_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    accept_recording_inspector(&view, cx, &fake);
    view.update(cx, |view, cx| {
        view.edit_browser(|browser| browser.focus_split_pane(0, std::time::SystemTime::now()));
        view.pages.as_ref().unwrap().borrow_mut().sync(&view.browser, true);
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-alt-i");
    cx.run_until_parked();
    let inspector = inspector_status(&view, cx).unwrap();
    let (a, b) = view.read_with(cx, |view, _| {
        let panes = &view.browser.active_split().unwrap().panes;
        (panes[0], panes[1])
    });
    let b_view = ViewId::from_raw(*fake.0.borrow().bounds.keys().find(|id| **id != raw(inspector.inspected)).unwrap());
    fake.send(cx, Event::Focused { view: b_view });
    assert_eq!(view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id), b);
    inspector_event(&fake, cx, engine::InspectorEvent::Focused { inspector: engine::InspectorId::from_raw(999) });
    assert_eq!(view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id), b);
    fake.take_calls();
    inspector_event(&fake, cx, engine::InspectorEvent::Focused { inspector: inspector.id });
    assert_eq!(view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id), a);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_))), "Frontend focus must not bounce to a page");
    inspector_event(&fake, cx, engine::InspectorEvent::Shortcut {
        inspector: inspector.id,
        shortcut: Shortcut { cmd: true, alt: true, key: "j".into(), ..Default::default() },
    });
    assert_eq!(inspector_status(&view, cx).unwrap().inspected, inspector.inspected);
    assert_eq!(inspector_status(&view, cx).unwrap().selected_panel, Some(engine::InspectorPanel::Console));
    dispatch_menu(cx, "View", "Network Inspector");
    assert_eq!(inspector_status(&view, cx).unwrap().inspected, inspector.inspected);
    assert_eq!(inspector_status(&view, cx).unwrap().selected_panel, Some(engine::InspectorPanel::Network));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_))), "Panel commands must retain frontend keyboard focus");
    fake.send(cx, Event::Focused { view: b_view });
    assert_eq!(view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id), b);
    fake.take_calls();
    // Focus and shortcut can arrive in the same pump. Model selection must be
    // repaired before the deferred Cmd+W action executes.
    fake.0.borrow_mut().events.extend([
        Event::Focused { view: b_view },
        Event::InspectorInput(engine::InspectorEvent::Focused { inspector: inspector.id }),
        Event::InspectorInput(engine::InspectorEvent::Shortcut {
            inspector: inspector.id,
            shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() },
        }),
    ]);
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.browser.all_tabs().any(|tab| tab.id == a));
        assert!(view.browser.all_tabs().any(|tab| tab.id == b));
        assert_eq!(view.browser.active_tab().unwrap().id, b);
    });
    assert!(inspector_status(&view, cx).is_none());
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(raw(inspector.inspected))));
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(id) if *id == raw(b_view))));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Reload(_) | Call::Navigate(..))));
}

fn assert_same_drain_split_focus_order(cx: &mut TestAppContext, frontend_last: bool) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    accept_recording_inspector(&view, cx, &fake);
    view.update(cx, |view, cx| {
        view.edit_browser(|browser| browser.focus_split_pane(0, std::time::SystemTime::now()));
        view.pages.as_ref().unwrap().borrow_mut().sync(&view.browser, true);
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-alt-i");
    cx.run_until_parked();
    let inspector = inspector_status(&view, cx).unwrap();
    let (a, b) = view.read_with(cx, |view, _| {
        let panes = &view.browser.active_split().unwrap().panes;
        (panes[0], panes[1])
    });
    let b_view = ViewId::from_raw(*fake.0.borrow().bounds.keys().find(|id| **id != raw(inspector.inspected)).unwrap());
    let page_focus = Event::Focused { view: b_view };
    let frontend_focus = Event::InspectorInput(engine::InspectorEvent::Focused { inspector: inspector.id });
    let close = Shortcut { cmd: true, key: "w".into(), ..Default::default() };
    let batch = if frontend_last {
        vec![page_focus, frontend_focus, Event::InspectorInput(engine::InspectorEvent::Shortcut { inspector: inspector.id, shortcut: close })]
    } else {
        vec![frontend_focus, page_focus, Event::Shortcut { view: b_view, shortcut: close }]
    };
    fake.take_calls();
    // Both focus callbacks and Cmd+W enter one queue before a single drain.
    fake.0.borrow_mut().events.extend(batch);
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    let (closed, remaining, closed_view) = if frontend_last { (a, b, inspector.inspected) } else { (b, a, b_view) };
    view.read_with(cx, |view, _| {
        assert!(!view.browser.all_tabs().any(|tab| tab.id == closed));
        assert!(view.browser.all_tabs().any(|tab| tab.id == remaining));
        assert_eq!(view.browser.active_tab().unwrap().id, remaining);
    });
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(raw(closed_view))), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Reload(_) | Call::Navigate(..))), "{calls:?}");
}

#[gpui::test]
fn same_drain_page_then_frontend_focus_makes_cmd_w_close_inspected_pane(cx: &mut TestAppContext) {
    assert_same_drain_split_focus_order(cx, true);
}

#[gpui::test]
fn same_drain_frontend_then_page_focus_makes_cmd_w_close_page_pane(cx: &mut TestAppContext) {
    assert_same_drain_split_focus_order(cx, false);
}

fn assert_focus_after_shortcut_keeps_original_target(cx: &mut TestAppContext, frontend_shortcut: bool, console: bool) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    accept_recording_inspector(&view, cx, &fake);
    view.update(cx, |view, cx| {
        view.edit_browser(|browser| browser.focus_split_pane(0, std::time::SystemTime::now()));
        view.pages.as_ref().unwrap().borrow_mut().sync(&view.browser, true);
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-alt-i");
    cx.run_until_parked();
    let inspector = inspector_status(&view, cx).unwrap();
    let (a, b) = view.read_with(cx, |view, _| {
        let panes = &view.browser.active_split().unwrap().panes;
        (panes[0], panes[1])
    });
    let b_view = ViewId::from_raw(*fake.0.borrow().bounds.keys().find(|id| **id != raw(inspector.inspected)).unwrap());
    let focus_a = if frontend_shortcut {
        Event::InspectorInput(engine::InspectorEvent::Focused { inspector: inspector.id })
    } else {
        Event::Focused { view: inspector.inspected }
    };
    let shortcut = Shortcut { cmd: true, alt: console, key: if console { "j" } else { "w" }.into(), ..Default::default() };
    let action = if frontend_shortcut {
        Event::InspectorInput(engine::InspectorEvent::Shortcut { inspector: inspector.id, shortcut })
    } else {
        Event::Shortcut { view: inspector.inspected, shortcut }
    };
    fake.take_calls();
    fake.0.borrow_mut().events.extend([focus_a, action, Event::Focused { view: b_view }]);
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, b);
        assert!(view.browser.all_tabs().any(|tab| tab.id == b));
        assert_eq!(view.browser.all_tabs().any(|tab| tab.id == a), console);
    });
    let calls = fake.take_calls();
    if console {
        let status = inspector_status(&view, cx).unwrap();
        assert_eq!(status.inspected, inspector.inspected);
        assert_eq!(status.selected_panel, Some(engine::InspectorPanel::Console));
        assert!(!calls.iter().any(|call| matches!(call, Call::InspectorOpen(id, _, _, _, _) if *id == raw(b_view))), "{calls:?}");
    } else {
        assert!(calls.contains(&Call::Close(raw(inspector.inspected))), "{calls:?}");
        assert!(!calls.contains(&Call::Close(raw(b_view))), "{calls:?}");
    }
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Reload(_) | Call::Navigate(..))), "{calls:?}");
}

#[gpui::test]
fn same_drain_frontend_cmd_w_precedes_later_page_focus(cx: &mut TestAppContext) {
    assert_focus_after_shortcut_keeps_original_target(cx, true, false);
}

#[gpui::test]
fn same_drain_page_cmd_w_precedes_later_page_focus(cx: &mut TestAppContext) {
    assert_focus_after_shortcut_keeps_original_target(cx, false, false);
}

#[gpui::test]
fn same_drain_deferred_console_action_precedes_later_page_focus(cx: &mut TestAppContext) {
    assert_focus_after_shortcut_keeps_original_target(cx, true, true);
}

#[path = "blank_window_tests.rs"]
mod blank_window_tests;

#[path = "profile_routing_tests.rs"]
mod profile_routing_tests;

#[path = "user_profile_host_tests.rs"]
mod user_profile_host_tests;

#[gpui::test]
fn cmd_q_seals_window_and_command_bar_producers_before_final_quit(cx: &mut TestAppContext) {
    let (view, visual) = open(cx);
    let before = visual.cx.update(|cx| cx.windows().len());
    visual.simulate_keystrokes("cmd-q");
    visual.simulate_keystrokes("cmd-t");
    visual.simulate_keystrokes("cmd-n");
    visual.run_until_parked();
    view.read_with(visual, |view, _| assert!(view.command_bar.is_none()));
    visual.cx.update(|cx| {
        assert!(crate::quit::is_quitting());
        assert_eq!(cx.windows().len(), before);
        assert!(cx.windows().iter().any(|window| window.downcast::<BrowserWindow>().is_some()));
        // Test only pre-quit routing: no CEF-global shutdown is started here.
        crate::quit::reset_for_test(cx);
    });
}

#[test]
fn quit_retires_every_pool_even_while_stale_layout_closures_hold_pages() {
    let engines = [FakeEngine::default(), FakeEngine::default(), FakeEngine::default()];
    // Independent normal/private/Blank pools plus shared normal-window layout.
    let layouts: Vec<_> = engines.iter().map(|engine| Rc::new(RefCell::new(crate::pages::Pages::new(Box::new(engine.clone()))))).collect();
    let stale = layouts.clone();
    let shared = layouts[0].borrow().for_host(std::ptr::null_mut());
    let owners = crate::pages::take_shutdown_engines().unwrap();
    assert_eq!(owners.len(), 3);
    drop(layouts);
    for engine in &engines { assert_eq!(Rc::strong_count(&engine.0), 2); }
    drop(owners);
    for engine in &engines { assert_eq!(Rc::strong_count(&engine.0), 1); }
    assert!(shared.independent_with_shared_storage(std::ptr::null_mut()).is_none());
    for pages in &stale { assert!(pages.borrow_mut().open_inspector_probe().is_err()); }
}

#[path = "library_route_tests.rs"]
mod library_route_tests;
#[path = "library_downloads_route_tests.rs"]
mod library_downloads_route_tests;

#[path = "download_probe_page_tests.rs"]
mod download_probe_page_tests;

#[path = "library_live_page_tests.rs"]
mod library_live_page_tests;

#[gpui::test]
fn app_download_fixture_normal_arm_is_once_identity_scoped_and_status_is_app_owned(cx: &mut TestAppContext) {
    use crate::{download_acceptance::{self, PreparedArm, AppDownloadAcceptance}, downloads::AppDownloads, window::download_probe::admission};
    admission::reset();
    let fake = FakeEngine::default();
    let (view, visual) = open_with(cx, Some(Box::new(fake.clone())));
    let arm = PreparedArm::parse("http://127.0.0.1:18473 continue-normal").unwrap();
    let url = arm.config.fixture_url();
    let before = view.read_with(visual, |v, _| v.browser.space().today.len());
    view.update(visual, |v, cx| {
        crate::quit::reset_for_test(cx);
        v.pages.as_ref().unwrap().borrow_mut().set_parent(1usize as *mut std::ffi::c_void);
        let captured = v.browser.active_user_profile_id();
        v.arm_app_download_fixture(arm, cx).unwrap();
        let descriptor = cx.global::<AppDownloadAcceptance>().descriptor.clone().unwrap();
        assert_eq!(descriptor.borrow().source.identity.profile, captured);
        assert_eq!(descriptor.borrow().config.fixture_url(), url);
        assert!(cx.has_global::<AppDownloads>());
        assert!(v.arm_download_probe(descriptor.borrow().config.clone(), cx).is_err());
        let status = download_acceptance::status(cx).unwrap();
        assert_eq!(status.source.as_ref().unwrap(), &descriptor.borrow().source);
        assert!(!status.completed());
        assert_eq!(v.browser.space().today.len(), before + 1);
    });
    let second = PreparedArm::parse("http://127.0.0.1:18473 continue-normal").unwrap();
    view.update(visual, |v, cx| {
        assert!(v.arm_app_download_fixture(second, cx).is_err());
        assert_eq!(v.browser.space().today.len(), before + 1);
        v.pages.as_ref().unwrap().borrow_mut().set_parent(2usize as *mut std::ffi::c_void);
        assert!(v.close_app_download_source(cx).is_err());
        assert_eq!(v.browser.space().today.len(), before + 1);
        v.pages.as_ref().unwrap().borrow_mut().set_parent(1usize as *mut std::ffi::c_void);
        v.close_app_download_source(cx).unwrap();
        assert!(download_acceptance::status(cx).is_ok()); // No source Entity in accessor.
    });
    assert!(fake.0.borrow().identity_attempts.iter().any(|attempt| attempt.3 == url));
    visual.cx.update(|cx| {
        // FakeEngine covers Pages only. Hand off after all assertions so the
        // scheduled AppDownloads task cannot enter real CEF during teardown.
        cx.global::<AppDownloads>().begin_shutdown().unwrap();
    });
    admission::reset();
}
#[gpui::test]
fn app_download_fixture_non_native_arm_fails_before_tab_or_global(cx: &mut TestAppContext) {
    use crate::{download_acceptance::PreparedArm, downloads::AppDownloads, window::download_probe::admission};
    admission::reset();
    let (view, visual) = open(cx);
    let arm = PreparedArm::parse("http://127.0.0.1:18473 continue-normal").unwrap();
    view.update(visual, |v, cx| {
        crate::quit::reset_for_test(cx);
        let before = v.browser.space().today.len();
        assert!(v.arm_app_download_fixture(arm, cx).is_err());
        assert_eq!(v.browser.space().today.len(), before);
        assert!(!cx.has_global::<AppDownloads>());
        assert!(!admission::occupied());
    });
}


#[gpui::test]
fn app_download_fixture_pending_creation_retry_keeps_same_consumer_and_read_only_status(cx: &mut TestAppContext) {
    use crate::{download_acceptance::{self, PreparedArm}, downloads::AppDownloads, window::download_probe::admission};
    use crate::pages::StorageSelection;
    admission::reset();
    let fake = FakeEngine::default();
    let (view, visual) = open_with(cx, Some(Box::new(fake.clone())));
    view.update(visual, |v, cx| {
        crate::quit::reset_for_test(cx);
        fake.0.borrow_mut().pending_storage.insert(StorageSelection::Default);
        v.pages.as_ref().unwrap().borrow_mut().set_parent(1usize as *mut _);
        v.arm_app_download_fixture(PreparedArm::parse("http://127.0.0.1:18473 continue-normal").unwrap(), cx).unwrap();
        let service = cx.global::<AppDownloads>().service.clone();
        let status = download_acceptance::status(cx).unwrap();
        assert!(v.pages.as_ref().unwrap().borrow().creation_retry_pending());
        assert_eq!(status.admission, Some(admission::Phase::Waiting));
        assert_eq!(status.sequence, 0);
        let consumer = status.consumer.unwrap();
        assert!(consumer.scheduled && !consumer.running && !consumer.busy);
        assert!(consumer.legacy_consumer_excluded);
        fake.0.borrow_mut().pending_storage.clear();
        // No second wake: ordinary Pages retry succeeds after Pending.
        assert!(v.pages.as_ref().unwrap().borrow_mut().retry_pending_creation(&v.browser));
        let after = download_acceptance::status(cx).unwrap();
        assert_eq!(after.sequence, 0); // Delayed first callback remains quiet.
        assert_eq!(after.consumer.unwrap(), consumer);
        assert!(Rc::ptr_eq(&cx.global::<AppDownloads>().service, &service));
        // Preserve the scheduling/status assertions above, then prevent the
        // real service task from entering CEF behind the Pages-only FakeEngine.
        cx.global::<AppDownloads>().begin_shutdown().unwrap();
    });
    admission::reset();
}
#[gpui::test]
fn app_download_fixture_private_blank_and_quit_refuse_before_tab_creation(cx: &mut TestAppContext) {
    use crate::{download_acceptance::PreparedArm, downloads::AppDownloads, window::download_probe::admission};
    admission::reset();
    cx.update(init);
    for browser in [browser_core::Browser::new_private(), browser_core::Browser::new_blank(), demo_browser()] {
        let normal = !browser.is_private() && !browser.is_blank();
        let (view, visual) = cx.add_window_view(|window, cx| BrowserWindow::new(browser, None, window, cx));
        view.update(visual, |v, cx| {
            crate::quit::reset_for_test(cx);
            if normal { crate::quit::seal(); }
            let before = v.browser.all_tabs().count();
            assert!(v.arm_app_download_fixture(PreparedArm::parse("http://127.0.0.1:18473 continue-normal").unwrap(), cx).is_err());
            assert_eq!(v.browser.all_tabs().count(), before);
            assert!(!cx.has_global::<AppDownloads>());
            assert!(!admission::occupied());
            crate::quit::reset_for_test(cx);
        });
    }
}
#[gpui::test]
fn app_download_fixture_captures_named_profile_across_selection_change(cx: &mut TestAppContext) {
    use crate::{download_acceptance::{self, PreparedArm}, window::download_probe::admission};
    admission::reset();
    let (view, visual) = open_with(cx, Some(Box::new(FakeEngine::default())));
    view.update(visual, |v, cx| {
        crate::quit::reset_for_test(cx);
        let captured = v.edit_browser(|browser| {
            let named = browser.create_user_profile("Fixture named owner").unwrap();
            browser.assign_space_user_profile(browser.space().id, named).unwrap();
            named
        });
        v.pages.as_ref().unwrap().borrow_mut().set_parent(1usize as *mut _);
        v.arm_app_download_fixture(PreparedArm::parse("http://127.0.0.1:18473 continue-normal").unwrap(), cx).unwrap();
        v.edit_browser(|browser| {
            let alternate = browser.add_space("Fixture alternate selection", 120.);
            browser.assign_space_user_profile(alternate, browser.default_user_profile_id()).unwrap();
            browser.switch_space(browser.spaces.len() - 1);
        });
        assert_ne!(v.browser.active_user_profile_id(), captured);
        let status = download_acceptance::status(cx).unwrap();
        assert_eq!(status.captured_profile, captured);
        assert_eq!(status.source.unwrap().identity.profile, captured);
        // This host test has no CEF runtime; the real-task regression supplies
        // its own per-service driver when testing pump/timer/drain behavior.
        cx.global::<crate::downloads::AppDownloads>().begin_shutdown().unwrap();
    });
    admission::reset();
}

#[gpui::test]
fn app_download_fixture_status_survives_source_entity_drop_with_retained_canonical_owner(cx: &mut TestAppContext) {
    use crate::{download_acceptance::{self, PreparedArm}, downloads::AppDownloads, window::download_probe::admission};
    admission::reset();
    let (source, visual) = open_with(cx, Some(Box::new(FakeEngine::default())));
    let weak = source.downgrade();
    let shutdown = source.update(visual, |v, cx| {
        crate::quit::reset_for_test(cx);
        v.pages.as_ref().unwrap().borrow_mut().set_parent(1usize as *mut _);
        v.arm_app_download_fixture(PreparedArm::parse("http://127.0.0.1:18473 continue-normal").unwrap(), cx).unwrap();
        // Transfer ownership before the scheduled task can enter the engine.
        // This test proves Entity lifetime/status, not native window retirement.
        cx.global::<AppDownloads>().begin_shutdown().unwrap()
    });
    visual.update(|window, _| window.remove_window());
    drop(source);
    visual.run_until_parked();
    assert!(weak.upgrade().is_none());
    visual.cx.update(|cx| {
        let status = download_acceptance::status(cx).unwrap();
        assert!(status.source.is_some());
        assert_eq!(format!("{:?}", status.consumer.unwrap().owner), "Shutdown");
        shutdown.flush_retained_with(crate::profile::ProfileHandle::same_workspace, |_| Ok(())).unwrap();
        shutdown.finish().unwrap();
        assert_eq!(format!("{:?}", download_acceptance::status(cx).unwrap().consumer.unwrap().owner), "Finished");
    });
    admission::reset();
}

#[path = "printing_tests.rs"]
mod printing_tests;
#[path = "paste_style_tests.rs"]
mod paste_style_tests;

#[gpui::test]
fn sidebar_labels_and_unknown_site_tiles_keep_reference_metrics(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    crate::sidebar_title::take_painted_titles();
    // A page without a downloaded favicon uses the same quiet placeholder
    // before and after activation, rather than a title initial/accent tile.
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://unknown.example/geometry");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    let titles = crate::sidebar_title::take_painted_titles();
    assert!(!titles.is_empty());
    assert!(titles.iter().all(|title| title.font_size == gpui::px(13.)));
    let placeholder = cx.debug_bounds("favicon-placeholder").unwrap();
    assert_eq!(placeholder.size, gpui::size(gpui::px(14.), gpui::px(14.)));
    let address = cx.debug_bounds("address").unwrap();
    assert_eq!(address.left(), gpui::px(8.));
    assert_eq!(address.top(), gpui::px(46.));
    assert_eq!(address.size.height, gpui::px(36.));
    let row = cx.debug_bounds("new-tab").unwrap();
    assert_eq!(row.size.height, gpui::px(38.));
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let dock_mark = cx.debug_bounds("space-0").unwrap();
    assert_eq!(dock_mark.center().y, sidebar.bottom() - gpui::px(25.));
    click(cx, "new-tab");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    bar(&view, cx).simulate_keystrokes("escape");
    click(cx, "address");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("https://unknown.example/geometry"));
}

#[gpui::test]
fn plain_space_heading_has_no_invented_dot_or_reserved_icon_slot(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let name = cx.debug_bounds("space-header-name-0").unwrap();
    assert_eq!(name.left(), gpui::px(16.));
    assert!(cx.debug_bounds("space-header-icon-0").is_none());
    let header = cx.debug_bounds("space-header-0").unwrap();
    cx.simulate_mouse_move(header.center(), None, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("space-header-name-0").unwrap(), name);
    click(cx, "space-header-more-0");
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_some()));
}
#[path = "empty_canvas_tests.rs"]
mod empty_canvas_tests;

#[gpui::test]
fn split_pane_menu_matches_arc_order_and_keeps_unavailable_moves_visible(cx: &mut TestAppContext) {
    use crate::tab_menu::Entry;
    let (view, cx, fake) = open_split_pages(cx, 2);
    let active = active_url(&view, cx);
    fake.take_calls();
    click(cx, "pane-menu-2");
    let entries = view.read_with(cx, |w, _| w.menu_entries());
    let labels: Vec<_> = entries.iter().map(|entry| match entry {
        Entry::Item(item) => item.label.as_ref(), Entry::Separator => "---", Entry::Header(label) => label,
    }).collect();
    assert_eq!(labels, ["Move Left", "Move Right", "---", "Add Right Split", "Add Left Split", "---",
        "Separate Tab from Split", "Convert to Vertical Split View", "---", "Share Split View…"]);
    assert!(matches!(&entries[0], Entry::Item(item) if !item.enabled));
    assert!(matches!(&entries[1], Entry::Item(item) if item.enabled));
    click(&mut tab_menu(&view, cx), "tab-menu-move-backward");
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_some()));
    assert_eq!(active_url(&view, cx), active);
    tab_menu(&view, cx).simulate_keystrokes("escape");
    click(cx, "pane-menu-3");
    assert!(view.read_with(cx, |w, _| matches!(&w.menu_entries()[1], Entry::Item(item) if !item.enabled)));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn vertical_split_pane_menu_uses_axis_specific_additions_and_disables_them_at_four(cx: &mut TestAppContext) {
    use crate::tab_menu::Entry;
    let (view, cx, fake) = open_split_pages(cx, 4);
    run_named_command(&view, cx, "convert to vertical split");
    fake.take_calls();
    click(cx, "pane-menu-2");
    view.read_with(cx, |w, _| {
        let rows: Vec<_> = w.menu_entries().into_iter().filter_map(|entry| if let Entry::Item(item) = entry { Some(item) } else { None }).collect();
        assert_eq!(rows.iter().map(|item| item.label.as_ref()).collect::<Vec<_>>(),
            ["Move Up", "Move Down", "Add Top Split", "Add Bottom Split", "Separate Tab from Split", "Convert to Horizontal Split View", "Share Split View…"]);
        assert!(!rows[0].enabled);
        assert!(!rows[2].enabled && !rows[3].enabled);
    });
    click(&mut tab_menu(&view, cx), "tab-menu-add-bottom");
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_some() && w.split_draft.is_none() && w.browser.active_split().unwrap().panes.len() == 4));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

fn assert_split_divider_resize(cx: &mut TestAppContext, count: usize, vertical: bool) {
    let (view, cx, fake) = open_split_pages(cx, count);
    view.update(cx, |w, cx| {
        let anchor = w.browser.active_split().unwrap().anchor;
        w.edit_browser(|browser| browser.set_split_weights(anchor, &vec![1.; count]));
        cx.notify();
    });
    cx.simulate_resize(gpui::size(gpui::px(1800.), gpui::px(1400.)));
    cx.run_until_parked();
    if vertical { run_named_command(&view, cx, "convert to vertical split"); }
    let before = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    let active = active_url(&view, cx);
    let selector = test_selector(format!("split-divider-{}", count - 2));
    let bounds = cx.debug_bounds(selector).unwrap();
    let start = bounds.center();
    let original_pane = cx.debug_bounds(test_selector(format!("split-pane-{}", before.panes[count - 2].get()))).unwrap();
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::none());
    let motion = if vertical { gpui::point(gpui::px(0.), gpui::px(-20.)) } else { gpui::point(gpui::px(-20.), gpui::px(0.)) };
    cx.simulate_mouse_move(start + motion, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    fake.take_calls();
    let end = if vertical { gpui::point(start.x, original_pane.top() + gpui::px(10.)) }
        else { gpui::point(original_pane.left() + gpui::px(10.), start.y) };
    finish_sidebar_drag(cx, end);
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        assert_eq!(split.panes, before.panes);
        assert_eq!(split.active, before.active);
        for index in 0..count - 2 { assert!((split.weights[index] - before.weights[index]).abs() < 0.001); }
        let pair_before = before.weights[count - 2] + before.weights[count - 1];
        assert!((split.weights[count - 2] + split.weights[count - 1] - pair_before).abs() < 0.001);
    });
    let pane = cx.debug_bounds(test_selector(format!("split-pane-{}", before.panes[count - 2].get()))).unwrap();
    let extent = f32::from(if vertical { pane.size.height } else { pane.size.width });
    assert!((extent - 220.).abs() < 1., "minimum pane extent: {extent}");
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..))));
    // A completed gesture cannot keep changing the split on ordinary movement.
    let weights = view.read_with(cx, |w, _| w.browser.active_split().unwrap().weights.clone());
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().weights.clone()), weights);
}

#[gpui::test]
fn two_pane_divider_resizes_without_reloading_or_changing_focus(cx: &mut TestAppContext) { assert_split_divider_resize(cx, 2, false); }

#[gpui::test]
fn three_pane_divider_preserves_the_unrelated_pane(cx: &mut TestAppContext) { assert_split_divider_resize(cx, 3, false); }

#[gpui::test]
fn four_pane_divider_preserves_the_unrelated_panes(cx: &mut TestAppContext) { assert_split_divider_resize(cx, 4, false); }

#[gpui::test]
fn vertical_divider_resizes_without_reloading_or_changing_focus(cx: &mut TestAppContext) { assert_split_divider_resize(cx, 3, true); }

#[gpui::test]
fn split_focus_contour_stays_outside_native_pages_and_focusing_does_not_reflow_them(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let before = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
    for (id, page) in &before {
        let pane = cx.debug_bounds(test_selector(format!("split-pane-{}", id.get()))).unwrap();
        assert!((page.x - f64::from(f32::from(pane.left())) - 1.).abs() < 0.6);
        assert!((f64::from(f32::from(pane.right())) - page.x - page.width - 1.).abs() < 0.6);
        assert!((f64::from(f32::from(pane.bottom())) - page.y - page.height - 1.).abs() < 0.6);
    }
    fake.take_calls();
    click(cx, "pane-header-2");
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout()), before);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..))));
}

fn settings_window(cx: &mut VisualTestContext) -> (Entity<crate::settings::SettingsWindow>, VisualTestContext) {
    let handle = cx.cx.update(|cx| cx.windows().into_iter().find_map(|w| w.downcast::<crate::settings::SettingsWindow>()).expect("Settings window"));
    let view = handle.root(&mut cx.cx).unwrap();
    let visual = VisualTestContext::from_window(handle.into(), &cx.cx);
    visual.run_until_parked();
    (view, visual)
}

#[gpui::test]
fn settings_shortcut_opens_one_native_window_and_cmd_w_preserves_tabs(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    let bounds = settings.update(|window, _| window.bounds());
    assert_eq!(bounds.size, gpui::size(gpui::px(684.), gpui::px(650.)));
    cx.simulate_keystrokes("cmd-,");
    let count = cx.cx.update(|cx| cx.windows().iter().filter(|w| w.downcast::<crate::settings::SettingsWindow>().is_some()).count());
    assert_eq!(count, 1);
    settings.simulate_keystrokes("cmd-w");
    assert_eq!(today_urls(&view, cx), before);
}

#[gpui::test]
fn settings_profile_search_and_archive_controls_edit_canonical_state(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (peer, peer_cx) = another_profile_window(&view, cx);
    cx.simulate_keystrokes("cmd-,");
    let (settings_view, mut settings) = settings_window(cx);
    click(&mut settings, "settings-button-Profile Settings…");
    assert_eq!(settings_view.read_with(&settings, |s, _| s.tab), crate::settings::Tab::Profiles);
    click(&mut settings, "settings-popup-Search engine");
    click(&mut settings, "settings-choice-2");
    click(&mut settings, "settings-popup-Archive tabs after");
    click(&mut settings, "settings-choice-2");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().archive_after), Some(Duration::from_secs(604800)));
    assert_eq!(peer.read_with(&peer_cx, |w, _| w.browser.active_user_profile().settings.search.engine), browser_core::settings::SearchEngine::DuckDuckGo);
    settings.simulate_keystrokes("cmd-w");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rust settings");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://duckduckgo.com/?q=rust+settings"));
    let saved = view.read_with(cx, |w, _| w.profile.session_json(&w.split_draft));
    let restored = browser_core::session::from_json(&saved).unwrap();
    assert_eq!(restored.active_user_profile().archive_after, Some(Duration::from_secs(604800)));
    assert_eq!(restored.active_user_profile().settings.search.engine, browser_core::settings::SearchEngine::DuckDuckGo);
}

#[gpui::test]
fn settings_toggles_persist_without_affecting_other_profiles(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let other = view.update(cx, |w, _| w.edit_profile(|b| b.create_user_profile("Work").unwrap()));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("settings");
    bar(&view, cx).simulate_keystrokes("enter");
    let (settings_view, mut settings) = settings_window(cx);
    click(&mut settings, "settings-toggle-WarnBeforeQuitting");
    click(&mut settings, "settings-tab-Advanced");
    click(&mut settings, "settings-toggle-Boosts");
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-profile-Work");
    click(&mut settings, "settings-search-suggestions");
    assert_eq!(settings_view.read_with(&settings, |s, _| s.selected_profile), other);
    settings.simulate_keystrokes("escape");
    let saved = view.read_with(cx, |w, _| w.profile.session_json(&w.split_draft));
    let browser = browser_core::session::from_json(&saved).unwrap();
    assert!(!browser.settings.enabled(browser_core::settings::Toggle::WarnBeforeQuitting));
    assert!(!browser.settings.enabled(browser_core::settings::Toggle::Boosts));
    assert!(!browser.user_profile(other).unwrap().settings.search.suggestions);
    assert!(browser.active_user_profile().settings.search.suggestions);
}

#[gpui::test]
fn settings_menu_and_native_page_shortcut_open_the_same_window(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    dispatch_menu(cx, "Echo", "Settings…");
    let (settings, mut visual) = settings_window(cx);
    visual.simulate_keystrokes("cmd-w");
    fake.send(cx, fake.shortcut(Shortcut { cmd: true, key: ",".into(), ..Default::default() }));
    let (second, _) = settings_window(cx);
    assert_ne!(settings, second);
    assert_eq!(active_url(&view, cx), active);
    assert!(crate::window::WINDOW_SHORTCUTS.contains(&"cmd-,"));
}

#[gpui::test]
fn settings_archive_dropdown_keyboard_cancel_preserves_legacy_policy(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let legacy = Some(Duration::from_secs(12345));
    view.update(cx, |w, _| w.edit_profile(|b| b.active_user_profile_mut().archive_after = legacy));
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-popup-Archive tabs after");
    settings.simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().archive_after), legacy);
    click(&mut settings, "settings-popup-Archive tabs after");
    settings.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_user_profile().archive_after), Some(Duration::from_secs(604800)));
}

#[gpui::test]
fn settings_profile_creation_dismisses_settings_and_cancel_preserves_the_model(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = view.read_with(cx, |w, _| w.profile.session_json(&w.split_draft));
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-add-profile");
    let count = cx.cx.update(|cx| cx.windows().iter().filter(|w| w.downcast::<crate::settings::SettingsWindow>().is_some()).count());
    assert_eq!(count, 0);
    let overlay = view.read_with(cx, |w, _| w.user_profile_dialog.as_ref().unwrap().overlay.unwrap());
    let mut dialog = VisualTestContext::from_window(overlay.into(), &cx.cx);
    dialog.run_until_parked();
    assert!(dialog.debug_bounds("user-profile-sheet").is_some());
    dialog.simulate_input("Settings test");
    dialog.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.user_profile_dialog.is_none()));
    assert_eq!(view.read_with(cx, |w, _| w.profile.session_json(&w.split_draft)), before);
}

#[gpui::test]
fn settings_tabs_remain_clickable_after_shortcuts(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    cx.simulate_keystrokes("cmd-,");
    let (view, mut settings) = settings_window(cx);
    for (tab, label) in [(crate::settings::Tab::Shortcuts, "settings-tab-Shortcuts"), (crate::settings::Tab::Icon, "settings-tab-Icon"), (crate::settings::Tab::Advanced, "settings-tab-Advanced"), (crate::settings::Tab::Links, "settings-tab-Links"), (crate::settings::Tab::Profiles, "settings-tab-Profiles"), (crate::settings::Tab::General, "settings-tab-General")] {
        click(&mut settings, label);
        assert_eq!(view.read_with(&settings, |s, _| s.tab), tab);
    }
}

struct ThemeRegionProbe {
    browser: Entity<BrowserWindow>,
    region: gpui::Bounds<gpui::Pixels>,
}
impl gpui::Render for ThemeRegionProbe {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        let region=self.region;
        gpui::div().size_full().child(gpui::div().absolute().left(region.origin.x).top(region.origin.y)
            .w(region.size.width).h(region.size.height).rounded(gpui::px(6.)).overflow_hidden()
            .child(self.browser.read(cx).theme_region(region,window.viewport_size())))
    }
}

#[gpui::test]
fn theme_region_keeps_the_window_field_aligned_across_half_panes(cx: &mut TestAppContext) {
    use gpui::AppContext;
    let (browser,source)=open(cx);
    let browser_for_probe=browser.clone();
    let region=gpui::Bounds::new(gpui::point(gpui::px(240.),gpui::px(10.)),gpui::size(gpui::px(300.),gpui::px(400.)));
    let handle=source.cx.update(|cx|cx.open_window(gpui::WindowOptions::default(),|_,cx|cx.new(|_|ThemeRegionProbe {browser:browser_for_probe,region})).unwrap());
    let mut preview=VisualTestContext::from_window(handle.into(),source);
    preview.run_until_parked();
    let viewport=preview.update(|window,_|window.viewport_size());
    assert_eq!(preview.debug_bounds("theme-region").unwrap(),region);
    let field=preview.debug_bounds("theme-region-field").unwrap();
    assert_eq!(field.origin,gpui::point(gpui::px(0.),gpui::px(0.)));
    assert_eq!(field.size,viewport);
    let next=gpui::Bounds::new(gpui::point(gpui::px(540.),gpui::px(10.)),gpui::size(gpui::px(250.),gpui::px(200.)));
    preview.cx.update(|cx|handle.update(cx,|probe,_,cx|{probe.region=next;cx.notify();})).unwrap();
    preview.run_until_parked();
    assert_eq!(preview.debug_bounds("theme-region").unwrap(),next);
    assert_eq!(preview.debug_bounds("theme-region-field").unwrap(),field);

}

fn space_theme_editor(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay=view.read_with(cx,|w,_|w.theme_editor.as_ref().and_then(|e|e.overlay)).expect("theme editor overlay");
    let editor=VisualTestContext::from_window(overlay.into(),cx);
    editor.run_until_parked();editor
}

#[gpui::test]
fn space_theme_editor_color_modes_palettes_and_undo_are_one_persistent_edit(cx: &mut TestAppContext) {
    let (view,cx)=open(cx);
    let id=view.read_with(cx,|w,_|w.browser.space().id);
    let tabs=view.read_with(cx,|w,_|w.browser.all_tabs().count());
    dispatch_menu(cx,"Spaces","Edit Theme…");
    let mut editor=space_theme_editor(&view,cx);
    let bounds=editor.debug_bounds("theme-editor").unwrap();
    assert_eq!(bounds.size,gpui::size(gpui::px(354.),gpui::px(508.)));
    click(&mut editor,"theme-preset-4");
    click(&mut editor,"theme-add-color");click(&mut editor,"theme-add-color");click(&mut editor,"theme-add-color");
    view.read_with(cx,|w,_|assert_eq!(w.theme_editor.as_ref().unwrap().draft.colors.len(),3));
    click(&mut editor,"theme-remove-color");
    click(&mut editor,"theme-palette-next");click(&mut editor,"theme-preset-7");
    click(&mut editor,"theme-light");assert!(!view.read_with(cx,|w,_|w.theme_is_dark()));
    click(&mut editor,"theme-dark");assert!(view.read_with(cx,|w,_|w.theme_is_dark()));
    click(&mut editor,"theme-texture");
    let expected=view.read_with(cx,|w,_|w.theme_editor.as_ref().unwrap().draft.clone());
    assert_eq!(expected.texture,browser_core::themes::NoiseTexture::Sand);
    // A draft previews immediately, but dismissal makes a single Undo entry.
    assert!(view.read_with(cx,|w,_|w.browser.space().theme.is_none()));
    editor.simulate_keystrokes("escape");cx.run_until_parked();
    view.read_with(cx,|w,_|{
        assert!(w.theme_editor.is_none());assert_eq!(w.browser.space().id,id);
        assert_eq!(w.browser.space().theme(),expected);assert_eq!(w.browser.all_tabs().count(),tabs);
        assert_eq!(w.browser.sidebar_undo_action(),Some("Update Theme"));
        let saved=browser_core::session::to_json(&w.browser);
        assert_eq!(browser_core::session::from_json(&saved).unwrap().space().theme(),expected);
    });
    cx.simulate_keystrokes("cmd-z");cx.run_until_parked();
    assert!(view.read_with(cx,|w,_|w.browser.space().theme.is_none()));
    cx.simulate_keystrokes("cmd-shift-z");cx.run_until_parked();
    assert_eq!(view.read_with(cx,|w,_|w.browser.space().theme()),expected);
}

#[gpui::test]
fn space_theme_context_menu_targets_clicked_space_and_outside_dismissal_keeps_selection(cx: &mut TestAppContext) {
    let (view,cx)=open(cx);
    let active=view.read_with(cx,|w,_|w.browser.space().id);
    let other=view.read_with(cx,|w,_|w.browser.spaces.iter().find(|s|s.id!=active).unwrap().id);
    cx.update(|window,cx|view.update(cx,|w,cx|w.open_space_menu(other,gpui::point(gpui::px(100.),gpui::px(200.)),window,cx)));
    let mut menu=tab_menu(&view,cx);click(&mut menu,"tab-menu-edit-theme-color");
    let mut editor=space_theme_editor(&view,cx);
    click(&mut editor,"theme-dark");
    assert!(view.read_with(cx,|w,_|w.theme_editor_is_dark()));
    click(&mut editor,"theme-light");
    assert!(!view.read_with(cx,|w,_|w.theme_editor_is_dark()));
    click(&mut editor,"theme-preset-6");
    let outside = gpui::point(gpui::px(10.),gpui::px(10.));
    editor.simulate_mouse_down(outside,gpui::MouseButton::Left,Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(outside,gpui::MouseButton::Left,Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx,|w,_|{
        assert!(w.theme_editor.is_none());assert_eq!(w.browser.space().id,active);
        assert!(w.browser.space().theme.is_none());
        assert!(w.browser.spaces.iter().find(|s|s.id==other).unwrap().theme.is_some());
    });
}

#[gpui::test]
fn space_theme_color_wheel_drags_and_automatic_appearance_follow_system(cx: &mut TestAppContext) {
    let (view,cx)=open(cx);
    view.update(cx,|w,cx|{w.theme_system_dark=true;cx.notify();});
    assert!(view.read_with(cx,|w,_|w.theme_is_dark()));
    dispatch_menu(cx,"Spaces","Edit Theme…");
    let mut editor=space_theme_editor(&view,cx);
    let field=editor.debug_bounds("theme-color-field").unwrap();
    let start=field.center();let end=start+gpui::point(gpui::px(80.),gpui::px(0.));
    editor.simulate_mouse_down(start,gpui::MouseButton::Left,Modifiers::none());
    editor.simulate_mouse_move(end,Some(gpui::MouseButton::Left),Modifiers::none());
    editor.simulate_mouse_up(end,gpui::MouseButton::Left,Modifiers::none());editor.run_until_parked();
    view.read_with(cx,|w,_|{let(h,s,_)=w.theme_editor.as_ref().unwrap().draft.colors[0].hsv();assert!((h-90.).abs()<0.01);assert!(s>0.5);});
    click(&mut editor,"theme-light");assert!(!view.read_with(cx,|w,_|w.theme_is_dark()));
    click(&mut editor,"theme-automatic");assert!(view.read_with(cx,|w,_|w.theme_is_dark()));
    editor.simulate_keystrokes("escape");cx.run_until_parked();
    view.update(cx,|w,cx|{w.theme_system_dark=false;cx.notify();});
    assert!(!view.read_with(cx,|w,_|w.theme_is_dark()));
}

#[gpui::test]
fn space_theme_paint_uses_the_live_window_instead_of_an_automation_sentinel(cx: &mut TestAppContext) {
    let (view,cx)=open(cx);
    view.update(cx,|w,cx|{w.native_view=Some(1usize as *mut std::ffi::c_void);cx.notify();});
    cx.run_until_parked();
    assert!(cx.debug_bounds("sidebar").is_some());
    view.update(cx,|w,_|w.native_view=None);
}

#[gpui::test]
fn space_icon_popover_uses_catalog_cells_and_dismisses_without_navigating(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    dispatch_menu(cx, "Spaces", "Change Space Icon…");
    let mut picker = bar(&view, cx);
    assert!(picker.debug_bounds("command-bar").is_none());
    let panel = picker.debug_bounds("icon-picker").unwrap();
    assert_eq!(panel.size, gpui::size(gpui::px(272.), gpui::px(272.)));
    assert!(picker.debug_bounds("icon-picker-search").is_some());
    click(&mut picker, "icon-picker-symbols");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 56);
    assert!(picker.debug_bounds("icon-cell-55").is_some());
    click(&mut picker, "icon-cell-2");
    view.read_with(cx, |w, _| assert_eq!(w.browser.space().custom_icon.as_deref(), Some("ion:heart")));
    assert!(bar_text(&view, cx).is_some(), "pointer selection keeps the picker open");
    bar(&view, cx).simulate_keystrokes("escape");
    dispatch_menu(cx, "Spaces", "Change Space Icon…");
    let mut picker = bar(&view, cx);
    click(&mut picker, "icon-picker-emoji");
    picker.simulate_input("compass");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx)), vec![Suggestion::SaveIcon { icon: Some("🧭".into()) }]);
    picker.simulate_keystrokes("escape");
    assert!(bar_text(&view, cx).is_none());
    view.read_with(cx, |w, _| assert_eq!(w.browser.space().custom_icon.as_deref(), Some("ion:heart")));
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn icon_picker_category_search_clear_and_keyboard_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    dispatch_menu(cx, "Spaces", "Change Space Icon…");
    let mut picker = bar(&view, cx);
    click(&mut picker, "icon-category-7");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), crate::icon_catalog::EMOJI.len());
    assert!(view.read_with(cx, |w, _| w.command_bar.as_ref().unwrap().rows_scroll.offset().y < gpui::px(0.)));
    picker.simulate_input("compass");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx)), vec![Suggestion::SaveIcon { icon: Some("🧭".into()) }]);
    click(&mut picker, "icon-picker-clear");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    click(&mut picker, "icon-picker-symbols");
    picker.simulate_keystrokes("down down right enter");
    assert!(bar_text(&view, cx).is_none());
    view.read_with(cx, |w, _| assert_eq!(w.browser.space().custom_icon.as_deref(), Some("ion:boat")));
}

#[gpui::test]
fn command_shortcuts_replace_the_icon_popover_with_the_normal_command_window(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    dispatch_menu(cx, "Spaces", "Change Space Icon…");
    bar(&view, cx).simulate_keystrokes("cmd-t");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.command_bar.as_ref().unwrap().target), crate::window::Target::NewTab);
    assert!(bar_input_focused(&view, cx));
    assert!(bar(&view, cx).debug_bounds("command-bar").is_some());
    bar(&view, cx).simulate_keystrokes("escape");
    dispatch_menu(cx, "Spaces", "Change Space Icon…");
    bar(&view, cx).simulate_keystrokes("cmd-l");
    assert!(bar_input_focused(&view, cx));
    assert!(view.read_with(cx, |w, _| matches!(w.command_bar.as_ref().unwrap().target, crate::window::Target::Tab(_))));
}
fn assert_split_window_grows_to_fit_panes(cx: &mut TestAppContext, count: usize, vertical: bool) {
    let (view, cx, fake) = open_split_pages(cx, count);
    if vertical { run_named_command(&view, cx, "convert to vertical split"); }
    let before = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    fake.take_calls();
    cx.simulate_resize(gpui::size(gpui::px(600.), gpui::px(400.)));
    cx.run_until_parked();
    let grown = cx.update(|window, _| window.bounds().size);
    let minimum = crate::split_layout::minimum_extent(count) as f32;
    assert_eq!(if vertical { f32::from(grown.height) - 2. * crate::pages::CONTENT_INSET }
        else { f32::from(grown.width) - view.read_with(cx, |w, _| w.sidebar_width()) - crate::pages::CONTENT_INSET }, minimum);
    // TestWindow.resize records the native size. Deliver the corresponding
    // bounds event, as the real platform does, before inspecting page frames.
    cx.simulate_resize(grown);
    cx.run_until_parked();
    for id in &before.panes {
        let frame = cx.debug_bounds(test_selector(format!("split-pane-{}", id.get()))).unwrap();
        let extent = f32::from(if vertical { frame.size.height } else { frame.size.width });
        assert!((extent - 220.).abs() < 0.6, "pane {id:?}: {extent}");
    }
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        assert_eq!(split.panes, before.panes);
        assert_eq!(split.active, before.active);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn split_window_minimum_fits_two_horizontal_panes(cx: &mut TestAppContext) { assert_split_window_grows_to_fit_panes(cx, 2, false); }
#[gpui::test]
fn split_window_minimum_fits_three_horizontal_panes(cx: &mut TestAppContext) { assert_split_window_grows_to_fit_panes(cx, 3, false); }
#[gpui::test]
fn split_window_minimum_fits_four_horizontal_panes(cx: &mut TestAppContext) { assert_split_window_grows_to_fit_panes(cx, 4, false); }
#[gpui::test]
fn split_window_minimum_fits_two_vertical_panes(cx: &mut TestAppContext) { assert_split_window_grows_to_fit_panes(cx, 2, true); }
#[gpui::test]
fn split_window_minimum_fits_three_vertical_panes(cx: &mut TestAppContext) { assert_split_window_grows_to_fit_panes(cx, 3, true); }
#[gpui::test]
fn split_window_minimum_fits_four_vertical_panes(cx: &mut TestAppContext) { assert_split_window_grows_to_fit_panes(cx, 4, true); }

#[gpui::test]
fn theme_materials_preserve_command_bar_input_and_selection_in_light_and_dark(cx: &mut TestAppContext) {
    use browser_core::themes::{SpaceTheme, ThemeAppearance};
    let (view,cx)=open(cx);
    let other=view.read_with(cx,|w,_|w.browser.spaces.iter().find(|s|s.id!=w.browser.space().id).unwrap().id);
    cx.simulate_keystrokes("cmd-t");
    bar(&view,cx).simulate_input("new");
    bar(&view,cx).simulate_keystrokes("down");
    for appearance in [ThemeAppearance::Light,ThemeAppearance::Dark] {
        view.update(cx,|w,cx|{
            let mut theme=SpaceTheme::from_hue(270.);theme.appearance=appearance;
            let id=w.browser.space().id;
            let mut other_theme=theme.clone();
            other_theme.appearance=if appearance==ThemeAppearance::Light {ThemeAppearance::Dark} else {ThemeAppearance::Light};
            w.edit_profile(|b|{b.update_space_theme(id,theme);b.update_space_theme(other,other_theme);});cx.notify();
        });
        cx.run_until_parked();
        view.read_with(cx,|w,cx|{
            let input=w.command_bar.as_ref().unwrap();
            assert_eq!(input.input.read(cx).text(),"new");
            assert_eq!(input.selected,1);
            assert!(w.theme_tab_selection_for(w.browser.space().id).a<0.6);
            assert_eq!(w.theme_tab_selection_for(w.browser.space().id).l,1.);
            assert_ne!(w.theme_is_dark(),w.theme_is_dark_for(other));
            assert_ne!(w.theme_foreground_for(w.browser.space().id),w.theme_foreground_for(other));
            assert!(w.command_bar_selection().a<0.2);
            assert_eq!(w.command_bar_selection().s,0.);
            assert_eq!(w.command_bar_panel_fill(false).a,1.);
            assert!(w.command_bar_panel_fill(true).a<0.3);
        });
        assert!(bar(&view,cx).debug_bounds("suggestion-1").is_some());
    }
    bar(&view,cx).simulate_keystrokes("escape");cx.run_until_parked();
    assert!(view.read_with(cx,|w,_|w.command_bar.is_none()));
}
