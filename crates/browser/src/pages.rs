//! Keeps the engine's web views in sync with the browser model: one view per
//! opened page, with the active tab or Split View laid out over the content card.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::time::{Duration, Instant};
use std::{cell::RefCell, rc::Rc, sync::Arc};

use browser_core::{Browser, TabId};
use engine::{Bounds, Engine, Event, ViewId};

/// What the window needs from a web engine. Implemented by Chromium in the
/// app and by a recording fake in tests.
pub trait WebEngine {
    fn open_inspector_probe(&mut self, _view: ViewId, _parent: *mut c_void, _bounds: Bounds) -> Result<engine::InspectorId, String> { Err("Inspector probe is unsupported".into()) }
    fn inspector_status(&self, _id: engine::InspectorId) -> Option<engine::InspectorStatus> { None }
    fn set_inspector_frame(&mut self, _id: engine::InspectorId, _bounds: Bounds) {}
    fn set_inspector_visible(&mut self, _id: engine::InspectorId, _visible: bool) {}
    fn send_inspector_probe_message(&mut self, _id: engine::InspectorId, _message: &str) -> bool { false }
    fn take_inspector_events(&mut self) -> Vec<engine::InspectorEvent> { Vec::new() }
    fn close_inspector(&mut self, _id: engine::InspectorId) {}
    fn create(&mut self, url: &str, bounds: Bounds) -> Option<ViewId>;
    /// Whether transfers should wait briefly for an owner-viewport photograph.
    fn supports_snapshots(&self) -> bool { false }
    fn request_snapshot(&mut self, _view: ViewId, _request: u64) -> bool { false }
    fn take_snapshots(&mut self) -> Vec<engine::PageSnapshot> { Vec::new() }
    fn create_in_host(&mut self, _parent: *mut c_void, url: &str, bounds: Bounds) -> Option<ViewId> { self.create(url, bounds) }
    fn focus_ui_in_host(&mut self, _parent: *mut c_void) { self.focus_ui(); }
    /// Transfer an existing native page. Engines without native hosting reject it.
    fn move_view(&mut self, _view: ViewId, _parent: *mut c_void, _bounds: Bounds) -> bool { false }
    fn set_bounds(&mut self, view: ViewId, bounds: Bounds);
    fn set_top_corners_rounded(&mut self, view: ViewId, rounded: bool);
    fn set_visible(&mut self, view: ViewId, visible: bool);
    /// Keyboard focus to the page.
    fn focus_page(&mut self, view: ViewId);
    /// Keyboard focus to the browser UI (e.g. the command bar).
    fn focus_ui(&mut self);
    fn navigate(&mut self, view: ViewId, url: &str);
    fn go_back(&mut self, view: ViewId);
    fn go_forward(&mut self, view: ViewId);
    fn undo(&mut self, view: ViewId);
    fn redo(&mut self, view: ViewId);
    fn reload(&mut self, view: ViewId);
    fn force_reload(&mut self, view: ViewId);
    fn stop_loading(&mut self, view: ViewId);
    fn set_audio_muted(&mut self, view: ViewId, muted: bool);
    fn audio_muted(&self, view: ViewId) -> Option<bool>;
    fn zoom(&mut self, view: ViewId, direction: i32);
    fn find(&mut self, view: ViewId, text: &str, forward: bool, next: bool);
    fn stop_finding(&mut self, view: ViewId);
    fn close(&mut self, view: ViewId);
    /// Runs engine work. Takes no `self`: the engine may dispatch native UI
    /// events from inside it, which re-enter the window and its pages.
    fn pump_fn(&self) -> fn();
    fn take_events(&mut self) -> Vec<Event>;
}

/// Chromium views hosted in a native parent view (`NSView*` on macOS).
pub struct Chromium {
    pub engine: Engine,
    pub parent: *mut c_void,
}

/// macOS window corner radius (measured on macOS 26+).
pub const WINDOW_CORNER_RADIUS: f32 = 16.0;
/// Gap between the window edge and the content card.
pub const CONTENT_INSET: f32 = 8.0;
/// Concentric with the window corner: inner radius = outer radius - inset.
pub const CONTENT_CORNER_RADIUS: f32 = WINDOW_CORNER_RADIUS - CONTENT_INSET;

impl WebEngine for Chromium {
    fn open_inspector_probe(&mut self, view: ViewId, parent: *mut c_void, bounds: Bounds) -> Result<engine::InspectorId, String> {
        self.engine.open_inspector_probe(view, parent, bounds, engine::InspectorPanel::Elements)
    }
    fn inspector_status(&self, id: engine::InspectorId) -> Option<engine::InspectorStatus> { self.engine.inspector_status(id) }
    fn set_inspector_frame(&mut self, id: engine::InspectorId, bounds: Bounds) { self.engine.set_inspector_frame(id, bounds); }
    fn set_inspector_visible(&mut self, id: engine::InspectorId, visible: bool) { self.engine.set_inspector_visible(id, visible); }
    fn send_inspector_probe_message(&mut self, id: engine::InspectorId, message: &str) -> bool { self.engine.send_inspector_probe_message(id, message) }
    fn take_inspector_events(&mut self) -> Vec<engine::InspectorEvent> { self.engine.take_inspector_events() }
    fn close_inspector(&mut self, id: engine::InspectorId) { self.engine.close_inspector(id); }
    fn supports_snapshots(&self) -> bool { true }
    fn request_snapshot(&mut self, view: ViewId, request: u64) -> bool { self.engine.request_snapshot(view, request) }
    fn take_snapshots(&mut self) -> Vec<engine::PageSnapshot> { self.engine.take_snapshots() }
    fn create_in_host(&mut self, parent: *mut c_void, url: &str, bounds: Bounds) -> Option<ViewId> {
        let parent = if parent.is_null() { self.parent } else { parent };
        self.engine.create_view(parent, url, bounds, f64::from(CONTENT_CORNER_RADIUS))
    }
    fn focus_ui_in_host(&mut self, parent: *mut c_void) { self.engine.focus_parent(if parent.is_null() { self.parent } else { parent }); }
    fn create(&mut self, url: &str, bounds: Bounds) -> Option<ViewId> {
        self.engine
            .create_view(self.parent, url, bounds, f64::from(CONTENT_CORNER_RADIUS))
    }
    fn set_bounds(&mut self, view: ViewId, bounds: Bounds) {
        self.engine.set_frame(view, bounds)
    }
    fn move_view(&mut self, view: ViewId, parent: *mut c_void, bounds: Bounds) -> bool {
        self.engine.move_view(view, parent, bounds)
    }
    fn set_top_corners_rounded(&mut self, view: ViewId, rounded: bool) {
        self.engine.set_top_corners_rounded(view, rounded)
    }
    fn set_visible(&mut self, view: ViewId, visible: bool) {
        self.engine.set_visible(view, visible)
    }
    fn focus_page(&mut self, view: ViewId) {
        self.engine.focus(view)
    }
    fn focus_ui(&mut self) {
        self.engine.focus_parent(self.parent)
    }
    fn navigate(&mut self, view: ViewId, url: &str) {
        self.engine.navigate(view, url)
    }
    fn go_back(&mut self, view: ViewId) {
        self.engine.go_back(view)
    }
    fn go_forward(&mut self, view: ViewId) {
        self.engine.go_forward(view)
    }
    fn undo(&mut self, view: ViewId) { self.engine.undo(view) }
    fn redo(&mut self, view: ViewId) { self.engine.redo(view) }
    fn reload(&mut self, view: ViewId) {
        self.engine.reload(view)
    }
    fn force_reload(&mut self, view: ViewId) { self.engine.force_reload(view) }
    fn stop_loading(&mut self, view: ViewId) { self.engine.stop_loading(view) }
    fn set_audio_muted(&mut self, view: ViewId, muted: bool) { self.engine.set_audio_muted(view, muted) }
    fn audio_muted(&self, view: ViewId) -> Option<bool> { self.engine.audio_muted(view) }
    fn zoom(&mut self, view: ViewId, direction: i32) { self.engine.zoom(view, direction) }
    fn find(&mut self, view: ViewId, text: &str, forward: bool, next: bool) { self.engine.find(view, text, forward, next) }
    fn stop_finding(&mut self, view: ViewId) { self.engine.stop_finding(view) }
    fn close(&mut self, view: ViewId) {
        self.engine.close(view)
    }
    fn pump_fn(&self) -> fn() {
        Engine::pump
    }
    fn take_events(&mut self) -> Vec<Event> {
        self.engine.take_events()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageState {
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub status: String,
    pub editable: bool,
}

struct Page {
    owner: u64,
    frame: Bounds,
    visible: bool,
    view: ViewId,
    /// The URL the engine is showing or was last told to load.
    url: String,
    state: PageState,
    muted: bool,
    snapshot: Option<(u64, Arc<Vec<u8>>)>,
    snapshot_pending: Option<u64>,
    snapshot_origin: Option<(u64, Bounds)>,
    snapshot_good_origin: Option<(u64, Bounds)>,
}

#[derive(Clone)]
struct PageIntent {
    tab: TabId,
    url: String,
    muted: bool,
    weight: f32,
    internal: bool,
}

/// A claim belongs to one activation/selection and one unchanged source viewport.
/// Its request ID also acts as the generation: cancelled IDs are never reused.
struct Handoff {
    source: u64,
    target: u64,
    view: ViewId,
    frame: Bounds,
    request: u64,
    deadline: Instant,
    ready: bool,
}

const HANDOFF_TIMEOUT: Duration = Duration::from_millis(250);

/// Temporary native diagnostic dock; never part of the browser model or session.
#[derive(Clone, Copy)]
struct InspectorProbe {
    id: engine::InspectorId,
    tab: TabId,
    view: ViewId,
    host: u64,
}

struct PageStore {
    inspector_probe: Option<InspectorProbe>,
    next_inspector_eval: u64,
    engine: Box<dyn WebEngine>,
    pages: HashMap<TabId, Page>,
    hosts: HashMap<u64, *mut c_void>,
    next_host: u64,
    active_host: u64,
    events: Vec<Event>,
    next_snapshot: u64,
    handoffs: HashMap<TabId, Handoff>,
    layouts: HashMap<u64, HashMap<TabId, Bounds>>,
    frozen: HashMap<(u64, TabId), Arc<Vec<u8>>>,
    mirror_revision: u64,
}

pub struct Pages {
    store: Rc<RefCell<PageStore>>,
    host: u64,
    parent: *mut c_void,
    page_focus: bool,
    page_has_focus: bool,
    preserve_ui_focus: bool,
    wanted: Vec<PageIntent>,
    wanted_focus: Option<TabId>,
    axis: browser_core::splits::Axis,
    split: bool,
    visible: HashSet<TabId>,
    shown: Option<TabId>,
    bounds: Option<Bounds>,
    seen_revision: u64,
}

impl Pages {
    pub fn new(engine: Box<dyn WebEngine>) -> Self {
        Self {
            store: Rc::new(RefCell::new(PageStore { inspector_probe: None, next_inspector_eval: 0, engine, pages: HashMap::new(), hosts: HashMap::from([(1, std::ptr::null_mut())]), next_host: 1, active_host: 1,
                events: Vec::new(), next_snapshot: 0, handoffs: HashMap::new(), layouts: HashMap::new(), frozen: HashMap::new(), mirror_revision: 0 })),
            host: 1,
            parent: std::ptr::null_mut(),
            page_focus: true,
            page_has_focus: false,
            preserve_ui_focus: false,
            wanted: Vec::new(),
            wanted_focus: None,
            axis: browser_core::splits::Axis::Horizontal,
            split: false,
            visible: HashSet::new(),
            shown: None,
            bounds: None,
            seen_revision: 0,
        }
    }

    /// Creates an independent layout over the same live pages and engine.
    /// The new host cannot take another window's page until explicitly activated.
    pub fn for_host(&self, parent: *mut c_void) -> Self {
        let mut store = self.store.borrow_mut();
        store.next_host += 1;
        let host = store.next_host;
        store.hosts.insert(host, parent);
        Self { store: self.store.clone(), host, parent, page_focus: true, page_has_focus: false, preserve_ui_focus: false, wanted: Vec::new(), wanted_focus: None,
            axis: browser_core::splits::Axis::Horizontal, split: false, visible: HashSet::new(), shown: None, bounds: None, seen_revision: 0 }
    }

    pub fn set_parent(&mut self, parent: *mut c_void) {
        self.parent = parent;
        self.store.borrow_mut().hosts.insert(self.host, parent);
    }

    /// Called when a browser root becomes active, before reconciling its layout.
    pub fn activate_host(&mut self, browser: &Browser) {
        {
            let mut store = self.store.borrow_mut();
            if store.active_host != self.host { self.shown = None; }
            store.active_host = self.host;
            let cancelled: Vec<_> = store.handoffs.iter().filter(|(_, claim)| claim.target != self.host).map(|(tab, _)| *tab).collect();
            for tab in cancelled { store.cancel_handoff(tab); }
        }
        // Rebuild intent from the freshly projected model before transferring:
        // the previous window may have navigated while this host was inactive.
        self.preserve_ui_focus = !self.page_focus;
        self.sync(browser, true);
        self.preserve_ui_focus = false;
    }

    /// Call before destroying the native parent. Unselected live pages are
    /// parked, hidden, in another surviving host until selected there.
    pub fn release_host(&mut self) {
        let mut store = self.store.borrow_mut();
        if store.inspector_probe.is_some_and(|probe| probe.host == self.host) { store.close_inspector_probe(); }
        if store.hosts.remove(&self.host).is_none() { return; }
        store.layouts.remove(&self.host);
        store.frozen.retain(|(host, _), _| *host != self.host);
        let cancelled: Vec<_> = store.handoffs.iter().filter(|(_, claim)| claim.source == self.host || claim.target == self.host).map(|(tab, _)| *tab).collect();
        for tab in cancelled { store.cancel_handoff(tab); }
        store.mirror_revision = store.mirror_revision.wrapping_add(1);
        let destination = store.hosts.iter().filter(|(_, parent)| !parent.is_null())
            .min_by_key(|(host, _)| **host).map(|(host, parent)| (*host, *parent));
        let owned: Vec<_> = store.pages.iter().filter(|(_, page)| page.owner == self.host).map(|(tab, _)| *tab).collect();
        for tab in owned {
            let PageStore { engine, pages, .. } = &mut *store;
            let page = pages.get_mut(&tab).unwrap();
            if let Some((host, _)) = destination.filter(|(_, parent)| engine.move_view(page.view, *parent, page.frame)) {
                engine.set_visible(page.view, false);
                page.visible = false;
                page.state.status.clear();
                page.owner = host;
            } else {
                // A rejected native transfer must never leave a child attached
                // to a window that is about to be destroyed.
                engine.close(page.view);
                pages.remove(&tab);
            }
        }
        if store.active_host == self.host { store.active_host = destination.map_or(0, |(host, _)| host); }
        self.visible.clear();
        self.shown = None;
    }

    pub fn state(&self, tab: TabId) -> PageState {
        self.store.borrow().pages
            .get(&tab)
            .map(|page| page.state.clone())
            .unwrap_or_default()
    }

    pub fn tab_for(&self, view: ViewId) -> Option<TabId> {
        self.store.borrow().pages
            .iter()
            .find_map(|(tab, page)| (page.view == view).then_some(*tab))
    }

    /// Reconciles views with the model. `visible` is false while UI covers
    /// the page (the command bar), since native views draw above the UI.
    pub fn sync(&mut self, browser: &Browser, visible: bool) {
        {
            let mut store = self.store.borrow_mut();
            let PageStore { engine, pages, .. } = &mut *store;
            let open: Vec<TabId> = browser.all_tabs().filter(|tab| !browser_core::omnibox::is_internal_url(&tab.url)).map(|tab| tab.id).collect();
            let closed: Vec<TabId> = pages
                .keys()
                .filter(|tab| !open.contains(tab))
                .copied()
                .collect();
            for tab in closed {
                let page = pages.remove(&tab).unwrap();
                engine.close(page.view);
                self.visible.remove(&tab);
                if self.shown == Some(tab) {
                    self.shown = None;
                    if page.owner == self.host { engine.focus_ui_in_host(self.parent); }
                }
            }
        }

        self.store.borrow_mut().prune();
        self.wanted.clear();
        self.wanted_focus = None;
        self.split = false;
        if visible {
            if let Some(active) = browser.active_tab() {
                self.wanted_focus = (!browser_core::omnibox::is_internal_url(&active.url)).then_some(active.id);
                if let Some(split) = browser.active_split() {
                    self.split = true;
                    self.axis = split.axis;
                    for (index, id) in split.panes.iter().enumerate() {
                        if let Some(tab) = browser.all_tabs().find(|tab| tab.id == *id) {
                            self.wanted.push(PageIntent { tab: *id, url: tab.url.clone(), muted: tab.muted,
                                weight: split.weights[index], internal: browser_core::omnibox::is_internal_url(&tab.url) });
                        }
                    }
                } else if self.wanted_focus.is_some() {
                    self.wanted.push(PageIntent { tab: active.id, url: active.url.clone(), muted: active.muted, weight: 1., internal: false });
                }
            }
        }
        self.apply_with_navigation(true);
        self.sync_audio(browser);
    }

    /// Applies mute intent even when an occluded window is not rendering.
    pub fn sync_audio(&mut self, browser: &Browser) {
        let mut store = self.store.borrow_mut();
        let PageStore { engine, pages, .. } = &mut *store;
        for intent in &mut self.wanted {
            if let Some(tab) = browser.all_tabs().find(|tab| tab.id == intent.tab) { intent.muted = tab.muted; }
        }
        // Muting a background tab must not activate or load it.
        for tab in browser.all_tabs() {
            if let Some(page) = pages.get_mut(&tab.id) {
                if page.muted != tab.muted || engine.audio_muted(page.view).is_some_and(|actual| actual != tab.muted) {
                    engine.set_audio_muted(page.view, tab.muted);
                    page.muted = tab.muted;
                }
            }
        }
    }

    /// Unloads a tab's page, e.g. a closed pinned tab that stays in the
    /// sidebar. It loads again when the tab is next shown.
    pub fn discard(&mut self, tab: TabId) {
        let mut store = self.store.borrow_mut();
        let PageStore { engine, pages, .. } = &mut *store;
        let Some(page) = pages.remove(&tab) else {
            return;
        };
        engine.close(page.view);
        store.prune();
        self.visible.remove(&tab);
        if self.shown == Some(tab) {
            self.shown = None;
            store.engine.focus_ui_in_host(self.parent);
        }
    }

    /// Called from layout with the content card's bounds.
    pub fn set_bounds(&mut self, bounds: Bounds) {
        if self.bounds == Some(bounds) {
            return;
        }
        self.bounds = Some(bounds);
        self.apply();
    }

    fn apply(&mut self) {
        self.apply_with_navigation(false);
    }

    // Only sync supplies fresh model intent. Capture completion and geometry
    // updates must never navigate using an inactive window's cached URL.
    fn apply_with_navigation(&mut self, navigate: bool) {
        let frames = self.pane_bounds();
        let wanted: Vec<_> = self.wanted.iter().zip(frames).filter(|(intent, frame)| !intent.internal && frame.page.width >= 1. && frame.page.height >= 1.)
            .map(|(intent, frame)| (intent.clone(), frame.page)).collect();
        let mut store = self.store.borrow_mut();
        let active = store.active_host == self.host;
        let layout: HashMap<_, _> = wanted.iter().map(|(intent, bounds)| (intent.tab, *bounds)).collect();
        if store.layouts.get(&self.host) != Some(&layout) {
            store.layouts.insert(self.host, layout);
            store.mirror_revision = store.mirror_revision.wrapping_add(1);
        }
        store.prune();
        // Decide before borrowing individual pages. Starting capture never pumps
        // CEF, and the native parent is unchanged until ready or the deadline.
        let waiting: HashSet<_> = wanted.iter().filter_map(|(intent, _)| {
            (active && !self.parent.is_null() && !store.prepare_handoff(intent.tab, self.host)).then_some(intent.tab)
        }).collect();
        let PageStore { engine, pages, .. } = &mut *store;
        let ids: HashSet<_> = wanted.iter().map(|(intent, _)| intent.tab).collect();
        let hidden: Vec<_> = self.visible.difference(&ids).copied().collect();
        for tab in hidden {
            if let Some(page) = pages.get_mut(&tab).filter(|page| page.owner == self.host) {
                page.state.status.clear();
                engine.set_visible(page.view, false);
                page.visible = false;
            }
            self.visible.remove(&tab);
        }
        let focused = self.wanted_focus.filter(|id| ids.contains(id));
        if self.shown.is_some() && (focused.is_none() || !self.shown.is_some_and(|id| ids.contains(&id))) {
            self.shown = None;
            if active { engine.focus_ui_in_host(self.parent); }
        }
        let mut created = false;
        let mut moved = false;
        for (intent, bounds) in wanted {
            let PageIntent { tab, url, muted, .. } = intent;
            match pages.get_mut(&tab) {
                Some(page) => {
                    if page.owner != self.host {
                        if waiting.contains(&tab) || !active || self.parent.is_null() || !engine.move_view(page.view, self.parent, bounds) { self.visible.remove(&tab); continue; }
                        page.owner = self.host;
                        moved = true;
                        self.visible.remove(&tab);
                        created = true;
                    }
                    if page.muted != muted {
                        engine.set_audio_muted(page.view, muted);
                        page.muted = muted;
                    }
                    if navigate && page.url != url {
                        page.state.status.clear();
                        engine.navigate(page.view, &url);
                        page.url = url;
                    }
                    engine.set_bounds(page.view, bounds);
                    page.frame = bounds;
                }
                None => {
                    if !active { continue; }
                    let Some(view) = engine.create_in_host(self.parent, &url, bounds) else { continue; };
                    created = true;
                    if muted { engine.set_audio_muted(view, true); }
                    pages.insert(tab, Page { owner: self.host, frame: bounds, visible: false, view, url, state: PageState::default(), muted, snapshot: None, snapshot_pending: None, snapshot_origin: None, snapshot_good_origin: None });
                }
            }
            engine.set_top_corners_rounded(pages[&tab].view, !self.split);
            let page = pages.get_mut(&tab).unwrap();
            if !page.visible { engine.set_visible(page.view, true); page.visible = true; created = true; }
            self.visible.insert(tab);
        }
        // Creating a native browser may take the responder even when the
        // intended active pane has not changed. Reassert it after all creates.
        if active && self.page_focus && !self.preserve_ui_focus && (focused != self.shown || created || !self.page_has_focus) {
            if let Some((tab, view)) = focused.and_then(|id| pages.get(&id).filter(|page| page.owner == self.host).map(|page| (id, page.view))) {
                engine.focus_page(view);
                self.shown = Some(tab);
                self.page_focus = true;
                self.page_has_focus = true;
            } else if self.shown.take().is_some() || created {
                if active { engine.focus_ui_in_host(self.parent); }
            }
        }
        if active && (!self.page_focus || self.preserve_ui_focus) {
            self.page_has_focus = false;
            self.shown = focused.filter(|tab| pages.get(tab).is_some_and(|page| page.owner == self.host));
            if created { engine.focus_ui_in_host(self.parent); }
        }
        if moved { store.mirror_revision = store.mirror_revision.wrapping_add(1); }
        // Ownership changes remove target photographs and reveal source ones.
        let completed: Vec<_> = store.handoffs.iter().filter(|(tab, claim)| store.pages.get(*tab).is_some_and(|page| page.owner == claim.target)).map(|(tab, _)| *tab).collect();
        for tab in completed {
            store.cancel_handoff(tab);
            store.mirror_revision = store.mirror_revision.wrapping_add(1);
        }
        store.layout_inspector_probe();
        self.seen_revision = store.mirror_revision;
    }

    fn pane_bounds(&self) -> Vec<crate::split_layout::PaneBounds> {
        let Some(bounds) = self.bounds else { return Vec::new(); };
        let weights: Vec<_> = self.wanted.iter().map(|intent| intent.weight).collect();
        crate::split_layout::layout(bounds, self.axis, &weights, self.split)
    }

    pub fn surface_bounds(&self) -> Vec<Bounds> {
        self.surface_layout().into_iter().map(|(_, bounds)| bounds).collect()
    }

    pub fn surface_layout(&self) -> Vec<(TabId, Bounds)> {
        self.wanted.iter().zip(self.pane_bounds()).filter(|(intent, _)| self.is_visible(intent.tab)).map(|(intent, frame)| (intent.tab, frame.page)).collect()
    }

    /// Frozen owner-viewport images, positioned in this host's logical layout.
    /// Images never follow later captures in another host.
    pub fn mirrors(&self) -> Vec<(TabId, Bounds, Arc<Vec<u8>>)> {
        let store = self.store.borrow();
        self.wanted.iter().zip(self.pane_bounds()).filter_map(|(intent, frame)| {
            let page = store.pages.get(&intent.tab)?;
            if page.owner == self.host || intent.internal { return None; }
            store.frozen.get(&(self.host, intent.tab)).map(|png| (intent.tab, frame.page, png.clone()))
        }).collect()
    }

    pub fn mirror_revision(&self) -> u64 { self.store.borrow().mirror_revision }

    pub fn is_visible(&self, tab: TabId) -> bool { self.visible.contains(&tab) && self.view(tab).is_some() }

    /// Transfers the native responder to GPUI without hiding the page.
    pub fn focus_ui(&mut self) {
        self.request_ui_focus();
        self.store.borrow_mut().engine.focus_ui_in_host(self.parent);
    }

    /// An overlay owns its own native responder; remember its intent without
    /// pulling focus back into the underlying root window.
    pub fn request_ui_focus(&mut self) {
        self.page_focus = false;
        self.page_has_focus = false;
    }

    /// Records explicit navigation/selection intent without changing the native
    /// responder before layout or an asynchronous handoff is ready.
    pub fn request_page_focus(&mut self) { self.page_focus = true; self.page_has_focus = false; }

    /// Gives keyboard focus back to the page on screen.
    pub fn focus_shown(&mut self) {
        self.page_focus = true;
        if let Some((tab, view)) = self.wanted_focus.and_then(|tab| self.view(tab).map(|view| (tab, view))) {
            self.store.borrow_mut().engine.focus_page(view);
            self.shown = Some(tab);
            self.page_has_focus = true;
        }
    }

    /// Transfers one loaded page, retaining its native view ID and cached state.
    pub fn move_to_host(&mut self, tab: TabId, parent: *mut c_void, bounds: Bounds) -> bool {
        if parent.is_null() { return false; }
        let Some(view) = self.view(tab) else { return false; };
        self.store.borrow_mut().engine.move_view(view, parent, bounds)
    }

    pub fn audio_muted(&self, tab: TabId) -> Option<bool> {
        self.view(tab).and_then(|view| self.store.borrow_mut().engine.audio_muted(view))
    }

    pub fn shown(&self) -> Option<TabId> {
        self.shown
    }

    fn view(&self, tab: TabId) -> Option<ViewId> {
        self.store.borrow().pages.get(&tab).filter(|page| page.owner == self.host).map(|page| page.view)
    }

    pub fn go_back(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) {
            self.store.borrow_mut().engine.go_back(view);
        }
    }

    pub fn go_forward(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) {
            self.store.borrow_mut().engine.go_forward(view);
        }
    }

    pub fn undo(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.undo(view); }
    }

    pub fn redo(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.redo(view); }
    }

    pub fn reload(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) {
            self.store.borrow_mut().engine.reload(view);
        }
    }

    pub fn force_reload(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.force_reload(view); }
    }

    pub fn stop_loading(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.stop_loading(view); }
    }

    pub fn zoom(&mut self, tab: TabId, direction: i32) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.zoom(view, direction); }
    }

    pub fn find(&mut self, tab: TabId, text: &str, forward: bool, next: bool) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.find(view, text, forward, next); }
    }

    pub fn stop_finding(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.stop_finding(view); }
    }

    /// Captures the existing document; concurrent requests for a tab coalesce.
    pub fn request_snapshot(&mut self, tab: TabId) -> Option<u64> {
        let mut store = self.store.borrow_mut();
        let page = store.pages.get(&tab)?;
        if let Some(request) = page.snapshot_pending { return Some(request); }
        let view = page.view;
        let origin = (page.owner, page.frame);
        store.next_snapshot = store.next_snapshot.checked_add(1)?;
        let request = store.next_snapshot;
        if !store.engine.request_snapshot(view, request) { return None; }
        let page = store.pages.get_mut(&tab)?;
        page.snapshot_pending = Some(request);
        page.snapshot_origin = Some(origin);
        Some(request)
    }

    /// A previous good image remains available while another capture is pending.
    pub fn snapshot(&self, tab: TabId) -> Option<(u64, Arc<Vec<u8>>)> {
        self.store.borrow().pages.get(&tab)?.snapshot.clone()
    }

    pub fn snapshot_pending(&self, tab: TabId, request: u64) -> bool {
        self.store.borrow().pages.get(&tab).is_some_and(|page| page.snapshot_pending == Some(request))
    }

    /// Native-only diagnostic entry point: use this host's actual focused,
    /// visible page, never a mirror or an arbitrary model tab.
    pub fn open_inspector_probe(&mut self) -> Result<engine::InspectorStatus, String> {
        let mut store = self.store.borrow_mut();
        let tab = self.shown.filter(|tab| Some(*tab) == self.wanted_focus && self.visible.contains(tab))
            .ok_or("No focused visible page")?;
        let page = store.pages.get(&tab).filter(|page| page.owner == self.host && page.visible)
            .ok_or("Focused page is not owned by this host")?;
        if store.active_host != self.host { return Err("Inspector host is inactive".into()); }
        let view = page.view;
        let parent = store.hosts.get(&page.owner).copied().filter(|parent| !parent.is_null())
            .ok_or("Focused page has no native GPUI parent")?;
        let frame = store.layouts.get(&self.host).and_then(|layout| layout.get(&tab)).copied()
            .ok_or("Focused page has no current layout")?;
        if frame.width < 2. || frame.height < 1. { return Err("Page is too small for diagnostic dock".into()); }
        if store.inspector_probe.is_some_and(|probe| probe.view != view || probe.host != self.host) { store.close_inspector_probe(); }
        let (_, dock) = inspector_probe_frames(frame);
        let id = store.engine.open_inspector_probe(view, parent, dock)?;
        store.inspector_probe = Some(InspectorProbe { id, tab, view, host: self.host });
        store.layout_inspector_probe();
        store.engine.inspector_status(id).ok_or("Inspector closed during creation".into())
    }

    pub fn inspector_probe_status(&self) -> Option<engine::InspectorStatus> {
        let store = self.store.borrow();
        let probe = store.inspector_probe.filter(|probe| probe.host == self.host)?;
        store.engine.inspector_status(probe.id)
    }

    /// Fixed trusted expression only. It assigns a document-lifetime UUID on
    /// first evaluation so reopen can prove that the inspected page survived.
    /// Submission does not wait: a following snapshot runs concurrently.
    pub fn eval_inspector_probe(&mut self) -> Result<u64, String> {
        let mut store = self.store.borrow_mut();
        let probe = store.inspector_probe.filter(|probe| probe.host == self.host)
            .ok_or("No inspector probe in this host")?;
        store.next_inspector_eval = store.next_inspector_eval.checked_add(1)
            .filter(|id| *id <= 9_007_199_254_740_991).ok_or("Inspector request IDs exhausted")?;
        let id = store.next_inspector_eval;
        let message = serde_json::json!({ "id": id, "method": "Runtime.evaluate", "params": {
            "expression": "JSON.stringify({documentURL:document.URL,uuid:(globalThis.__echoInspectorProbeUUID ??= Array.from(crypto.getRandomValues(new Uint8Array(16)), n => n.toString(16).padStart(2, '0')).join(''))})",
            "returnByValue": true
        }}).to_string();
        if !store.engine.send_inspector_probe_message(probe.id, &message) { return Err("Inspector evaluation rejected".into()); }
        Ok(id)
    }

    pub fn close_inspector_probe(&mut self) {
        let closed = {
            let mut store = self.store.borrow_mut();
            if store.inspector_probe.is_some_and(|probe| probe.host == self.host) {
                store.close_inspector_probe();
                true
            } else { false }
        };
        if closed {
            // Inspector focus events are intentionally separate from tab
            // events. Its responder may have replaced the inspected page's.
            self.page_has_focus = false;
            self.apply();
        }
    }

    pub fn pump_fn(&self) -> fn() {
        self.store.borrow_mut().engine.pump_fn()
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        let mut store = self.store.borrow_mut();
        store.drain_snapshots();
        // Every host polls, even without a browser event. A source may drain the
        // result first; shared revision wakes the target on its following poll.
        let reapply = store.mirror_revision != self.seen_revision
            || store.handoffs.values().any(|claim| claim.target == self.host);
        drop(store);
        if reapply {
            // A deferred transfer must retain the UI/page focus decision that
            // activation made, rather than stealing focus from an open input.
            self.preserve_ui_focus = !self.page_focus;
            self.apply();
            self.preserve_ui_focus = false;
        }
        let mut store = self.store.borrow_mut();
        let fresh = store.engine.take_events();
        // take_events processes inspector load callbacks first. This runs after
        // the regular pump, with no borrow retained across pumping or awaiting.
        for event in store.engine.take_inspector_events() { eprintln!("[inspector-event] {event:?}"); }
        store.layout_inspector_probe();
        store.events.extend(fresh);
        let queued = std::mem::take(&mut store.events);
        let mut delivered = Vec::new();
        for event in queued {
            let owner = match &event {
                Event::Shortcut(_) => Some(store.active_host),
                Event::OpenInNewTab { opener, .. } => store.pages.values().find(|page| page.view == *opener).map(|page| page.owner),
                Event::TitleChanged { view, .. } | Event::Favicon { view, .. } | Event::Focused { view }
                | Event::EditFocusChanged { view, .. } | Event::FindResult { view, .. } | Event::UrlChanged { view, .. }
                | Event::StatusChanged { view, .. } | Event::LoadingChanged { view, .. } =>
                    store.pages.values().find(|page| page.view == *view).map(|page| page.owner),
            };
            if owner == Some(self.host) { delivered.push(event); }
            else if owner.is_some_and(|owner| store.hosts.contains_key(&owner)) { store.events.push(event); }
        }
        delivered
    }

    /// Records what the engine reported so it is not re-sent by `sync`.
    pub fn record(&mut self, event: &Event) {
        let mut store = self.store.borrow_mut();
        let PageStore { pages, .. } = &mut *store;
        match event {
            Event::EditFocusChanged { view, editable } => {
                if let Some(page) = pages.values_mut().find(|page| page.view == *view) { page.state.editable = *editable; }
            }
            Event::Focused { view } => {
                if let Some(tab) = pages.iter().find_map(|(tab, page)| (page.view == *view && page.owner == self.host).then_some(*tab)).filter(|tab| self.visible.contains(tab)) { self.shown = Some(tab); self.page_focus = true; self.page_has_focus = true; }
            }
            Event::UrlChanged { view, url } => {
                if let Some(page) = pages.values_mut().find(|page| page.view == *view) {
                    page.url = url.clone();
                    page.state.status.clear();
                }
            }
            Event::StatusChanged { view, text } => {
                // Hidden pages can still deliver callbacks queued before they
                // were hidden. Never revive their old hover state.
                if let Some(tab) = pages.iter().find_map(|(tab, page)| (page.view == *view && page.owner == self.host).then_some(*tab)).filter(|tab| self.visible.contains(tab)) {
                    pages.get_mut(&tab).unwrap().state.status = text.clone();
                }
            }
            Event::LoadingChanged {
                view,
                loading,
                can_go_back,
                can_go_forward,
            } => {
                if let Some(page) = pages.values_mut().find(|page| page.view == *view) {
                    page.state.loading = *loading;
                    page.state.can_go_back = *can_go_back;
                    page.state.can_go_forward = *can_go_forward;
                    if *loading { page.state.status.clear(); }
                }
            }
            Event::TitleChanged { .. } | Event::Favicon { .. } | Event::OpenInNewTab { .. } | Event::Shortcut(_) | Event::FindResult { .. } => {}
        }
    }
}

fn inspector_probe_frames(frame: Bounds) -> (Bounds, Bounds) {
    let width = (frame.width / 2.).floor();
    (Bounds { width, ..frame }, Bounds { x: frame.x + width, width: frame.width - width, ..frame })
}

impl PageStore {
    fn close_inspector_probe(&mut self) {
        let Some(probe) = self.inspector_probe.take() else { return; };
        self.engine.close_inspector(probe.id);
        if let Some(page) = self.pages.get_mut(&probe.tab).filter(|page| page.view == probe.view && page.owner == probe.host) {
            if let Some(frame) = self.layouts.get(&probe.host).and_then(|layout| layout.get(&probe.tab)).copied() {
                self.engine.set_bounds(page.view, frame);
                page.frame = frame;
            }
        }
    }

    fn layout_inspector_probe(&mut self) {
        let Some(probe) = self.inspector_probe else { return; };
        let frame = self.layouts.get(&probe.host).and_then(|layout| layout.get(&probe.tab)).copied();
        let valid = self.active_host == probe.host && self.hosts.contains_key(&probe.host)
            && self.pages.get(&probe.tab).is_some_and(|page| page.view == probe.view && page.owner == probe.host && page.visible)
            && self.engine.inspector_status(probe.id).is_some();
        let Some(frame) = frame.filter(|frame| valid && frame.width >= 2. && frame.height >= 1.) else {
            self.close_inspector_probe();
            return;
        };
        let (page_frame, dock) = inspector_probe_frames(frame);
        self.engine.set_bounds(probe.view, page_frame);
        self.pages.get_mut(&probe.tab).unwrap().frame = page_frame;
        self.engine.set_inspector_frame(probe.id, dock);
        self.engine.set_inspector_visible(probe.id, true);
    }

    fn cancel_handoff(&mut self, tab: TabId) {
        if let Some(claim) = self.handoffs.remove(&tab) {
            if let Some(page) = self.pages.get_mut(&tab).filter(|page| page.snapshot_pending == Some(claim.request)) {
                page.snapshot_pending = None;
                page.snapshot_origin = None;
            }
        }
    }

    fn prune(&mut self) {
        let cancelled: Vec<_> = self.handoffs.iter().filter(|(tab, claim)| {
            self.active_host != claim.target
                || !self.hosts.contains_key(&claim.source)
                || !self.layouts.get(&claim.target).is_some_and(|layout| layout.contains_key(*tab))
                || !self.pages.get(*tab).is_some_and(|page| page.owner == claim.source && page.view == claim.view && page.frame == claim.frame)
        }).map(|(tab, _)| *tab).collect();
        for tab in cancelled { self.cancel_handoff(tab); }
        let before = self.frozen.len();
        self.frozen.retain(|(host, tab), _| self.hosts.contains_key(host) && self.pages.contains_key(tab)
            && self.layouts.get(host).is_some_and(|layout| layout.contains_key(tab)));
        if before != self.frozen.len() { self.mirror_revision = self.mirror_revision.wrapping_add(1); }
    }

    /// True means the existing view may now be reparented.
    fn prepare_handoff(&mut self, tab: TabId, target: u64) -> bool {
        // The temporary dock belongs to its original GPUI parent. Restore the
        // full viewport before snapshotting or transferring the inspected view.
        if self.inspector_probe.is_some_and(|probe| probe.tab == tab && probe.host != target) { self.close_inspector_probe(); }
        let Some(page) = self.pages.get(&tab) else { return true; };
        if page.owner == target || !self.engine.supports_snapshots() { return true; }
        if let Some(claim) = self.handoffs.get(&tab) {
            if claim.target == target {
                if claim.ready || Instant::now() >= claim.deadline {
                    self.freeze_cached(tab);
                    return true;
                }
                return false;
            }
        }
        let (source, view, frame) = (page.owner, page.view, page.frame);
        let existing = page.snapshot_pending.filter(|_| page.snapshot_origin == Some((source, frame)));
        let request = if let Some(request) = existing { request } else {
            let Some(request) = self.next_snapshot.checked_add(1) else { return true; };
            self.next_snapshot = request;
            if !self.engine.request_snapshot(view, request) { self.freeze_cached(tab); return true; }
            let page = self.pages.get_mut(&tab).unwrap();
            page.snapshot_pending = Some(request);
            page.snapshot_origin = Some((source, frame));
            request
        };
        self.handoffs.insert(tab, Handoff { source, target, view, frame, request,
            deadline: Instant::now() + HANDOFF_TIMEOUT, ready: false });
        false
    }

    fn freeze_cached(&mut self, tab: TabId) {
        let Some(page) = self.pages.get(&tab) else { return; };
        if page.snapshot_good_origin != Some((page.owner, page.frame))
            || !self.layouts.get(&page.owner).is_some_and(|layout| layout.contains_key(&tab)) { return; }
        if let Some((_, png)) = &page.snapshot {
            let key = (page.owner, tab);
            if !self.frozen.get(&key).is_some_and(|old| Arc::ptr_eq(old, png)) {
                self.frozen.insert(key, png.clone());
                self.mirror_revision = self.mirror_revision.wrapping_add(1);
            }
        }
    }

    fn drain_snapshots(&mut self) {
        self.prune();
        for result in self.engine.take_snapshots() {
            let Some(tab) = self.pages.iter().find_map(|(tab, page)|
                (page.view == result.view && page.snapshot_pending == Some(result.request)).then_some(*tab)) else { continue; };
            let page = self.pages.get_mut(&tab).unwrap();
            page.snapshot_pending = None;
            let origin = page.snapshot_origin.take();
            let image = result.png.map(Arc::new);
            // Preserve the diagnostic cache API independently of handoff images.
            let timely = self.handoffs.get(&tab).is_none_or(|claim| Instant::now() < claim.deadline);
            if let Some(png) = &image {
                if timely {
                    page.snapshot = Some((result.request, png.clone()));
                    page.snapshot_good_origin = origin;
                }
            }
            if let Some(claim) = self.handoffs.get_mut(&tab).filter(|claim|
                claim.request == result.request && claim.view == result.view
                    && claim.source == page.owner && claim.frame == page.frame
                    && origin == Some((claim.source, claim.frame))) {
                // Commit the image only when prepare_handoff revalidates this
                // claim immediately before reparenting. Merely completing a
                // cancelled/obsolete claim must not replace a host photograph.
                claim.ready = true;
                self.mirror_revision = self.mirror_revision.wrapping_add(1);
            }
        }
    }
}

impl Drop for Pages {
    fn drop(&mut self) { self.release_host(); }
}
