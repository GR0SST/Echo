//! Keeps the engine's web views in sync with the browser model: one view per
//! opened page, with the active tab or Split View laid out over the content card.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;

use browser_core::{Browser, TabId};
use engine::{Bounds, Engine, Event, ViewId};

/// What the window needs from a web engine. Implemented by Chromium in the
/// app and by a recording fake in tests.
pub trait WebEngine {
    fn create(&mut self, url: &str, bounds: Bounds) -> Option<ViewId>;
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
    fn create(&mut self, url: &str, bounds: Bounds) -> Option<ViewId> {
        self.engine
            .create_view(self.parent, url, bounds, f64::from(CONTENT_CORNER_RADIUS))
    }
    fn set_bounds(&mut self, view: ViewId, bounds: Bounds) {
        self.engine.set_frame(view, bounds)
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
    view: ViewId,
    /// The URL the engine is showing or was last told to load.
    url: String,
    state: PageState,
    muted: bool,
}

#[derive(Clone)]
struct PageIntent {
    tab: TabId,
    url: String,
    muted: bool,
    weight: f32,
    internal: bool,
}

pub struct Pages {
    engine: Box<dyn WebEngine>,
    pages: HashMap<TabId, Page>,
    wanted: Vec<PageIntent>,
    wanted_focus: Option<TabId>,
    axis: browser_core::splits::Axis,
    split: bool,
    visible: HashSet<TabId>,
    shown: Option<TabId>,
    bounds: Option<Bounds>,
}

impl Pages {
    pub fn new(engine: Box<dyn WebEngine>) -> Self {
        Self {
            engine,
            pages: HashMap::new(),
            wanted: Vec::new(),
            wanted_focus: None,
            axis: browser_core::splits::Axis::Horizontal,
            split: false,
            visible: HashSet::new(),
            shown: None,
            bounds: None,
        }
    }

    pub fn state(&self, tab: TabId) -> PageState {
        self.pages
            .get(&tab)
            .map(|page| page.state.clone())
            .unwrap_or_default()
    }

    pub fn tab_for(&self, view: ViewId) -> Option<TabId> {
        self.pages
            .iter()
            .find_map(|(tab, page)| (page.view == view).then_some(*tab))
    }

    /// Reconciles views with the model. `visible` is false while UI covers
    /// the page (the command bar), since native views draw above the UI.
    pub fn sync(&mut self, browser: &Browser, visible: bool) {
        let open: Vec<TabId> = browser.all_tabs().filter(|tab| !browser_core::omnibox::is_internal_url(&tab.url)).map(|tab| tab.id).collect();
        let closed: Vec<TabId> = self
            .pages
            .keys()
            .filter(|tab| !open.contains(tab))
            .copied()
            .collect();
        for tab in closed {
            let page = self.pages.remove(&tab).unwrap();
            self.engine.close(page.view);
            self.visible.remove(&tab);
            if self.shown == Some(tab) {
                self.shown = None;
                self.engine.focus_ui();
            }
        }

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
        self.apply();
        self.sync_audio(browser);
    }

    /// Applies mute intent even when an occluded window is not rendering.
    pub fn sync_audio(&mut self, browser: &Browser) {
        for intent in &mut self.wanted {
            if let Some(tab) = browser.all_tabs().find(|tab| tab.id == intent.tab) { intent.muted = tab.muted; }
        }
        // Muting a background tab must not activate or load it.
        for tab in browser.all_tabs() {
            if let Some(page) = self.pages.get_mut(&tab.id) {
                if page.muted != tab.muted || self.engine.audio_muted(page.view).is_some_and(|actual| actual != tab.muted) {
                    self.engine.set_audio_muted(page.view, tab.muted);
                    page.muted = tab.muted;
                }
            }
        }
    }

    /// Unloads a tab's page, e.g. a closed pinned tab that stays in the
    /// sidebar. It loads again when the tab is next shown.
    pub fn discard(&mut self, tab: TabId) {
        let Some(page) = self.pages.remove(&tab) else {
            return;
        };
        self.engine.close(page.view);
        self.visible.remove(&tab);
        if self.shown == Some(tab) {
            self.shown = None;
            self.engine.focus_ui();
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
        let frames = self.pane_bounds();
        let wanted: Vec<_> = self.wanted.iter().zip(frames).filter(|(intent, frame)| !intent.internal && frame.page.width >= 1. && frame.page.height >= 1.)
            .map(|(intent, frame)| (intent.clone(), frame.page)).collect();
        let ids: HashSet<_> = wanted.iter().map(|(intent, _)| intent.tab).collect();
        let hidden: Vec<_> = self.visible.difference(&ids).copied().collect();
        for tab in hidden {
            if let Some(page) = self.pages.get_mut(&tab) {
                page.state.status.clear();
                self.engine.set_visible(page.view, false);
            }
            self.visible.remove(&tab);
        }
        let focused = self.wanted_focus.filter(|id| ids.contains(id));
        if self.shown.is_some() && (focused.is_none() || !self.shown.is_some_and(|id| ids.contains(&id))) {
            self.shown = None;
            self.engine.focus_ui();
        }
        let mut created = false;
        for (intent, bounds) in wanted {
            let PageIntent { tab, url, muted, .. } = intent;
            match self.pages.get_mut(&tab) {
                Some(page) => {
                    if page.muted != muted {
                        self.engine.set_audio_muted(page.view, muted);
                        page.muted = muted;
                    }
                    if page.url != url {
                        page.state.status.clear();
                        self.engine.navigate(page.view, &url);
                        page.url = url;
                    }
                    self.engine.set_bounds(page.view, bounds);
                }
                None => {
                    let Some(view) = self.engine.create(&url, bounds) else { continue; };
                    created = true;
                    if muted { self.engine.set_audio_muted(view, true); }
                    self.pages.insert(tab, Page { view, url, state: PageState::default(), muted });
                }
            }
            self.engine.set_top_corners_rounded(self.pages[&tab].view, !self.split);
            if self.visible.insert(tab) { self.engine.set_visible(self.pages[&tab].view, true); }
        }
        // Creating a native browser may take the responder even when the
        // intended active pane has not changed. Reassert it after all creates.
        if focused != self.shown || created {
            if let Some((tab, view)) = focused.and_then(|id| self.view(id).map(|view| (id, view))) {
                self.engine.focus_page(view);
                self.shown = Some(tab);
            } else if self.shown.take().is_some() || created {
                self.engine.focus_ui();
            }
        }
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
        self.wanted.iter().zip(self.pane_bounds()).filter(|(intent, _)| self.visible.contains(&intent.tab)).map(|(intent, frame)| (intent.tab, frame.page)).collect()
    }

    pub fn is_visible(&self, tab: TabId) -> bool { self.visible.contains(&tab) }

    /// Transfers the native responder to GPUI without hiding the page.
    pub fn focus_ui(&mut self) {
        self.engine.focus_ui();
    }

    /// Gives keyboard focus back to the page on screen.
    pub fn focus_shown(&mut self) {
        if let Some(view) = self.shown.and_then(|tab| self.view(tab)) {
            self.engine.focus_page(view);
        }
    }

    pub fn audio_muted(&self, tab: TabId) -> Option<bool> {
        self.view(tab).and_then(|view| self.engine.audio_muted(view))
    }

    pub fn shown(&self) -> Option<TabId> {
        self.shown
    }

    fn view(&self, tab: TabId) -> Option<ViewId> {
        self.pages.get(&tab).map(|page| page.view)
    }

    pub fn go_back(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) {
            self.engine.go_back(view);
        }
    }

    pub fn go_forward(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) {
            self.engine.go_forward(view);
        }
    }

    pub fn undo(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.engine.undo(view); }
    }

    pub fn redo(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.engine.redo(view); }
    }

    pub fn reload(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) {
            self.engine.reload(view);
        }
    }

    pub fn force_reload(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.engine.force_reload(view); }
    }

    pub fn stop_loading(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.engine.stop_loading(view); }
    }

    pub fn zoom(&mut self, tab: TabId, direction: i32) {
        if let Some(view) = self.view(tab) { self.engine.zoom(view, direction); }
    }

    pub fn find(&mut self, tab: TabId, text: &str, forward: bool, next: bool) {
        if let Some(view) = self.view(tab) { self.engine.find(view, text, forward, next); }
    }

    pub fn stop_finding(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.engine.stop_finding(view); }
    }

    pub fn pump_fn(&self) -> fn() {
        self.engine.pump_fn()
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        self.engine.take_events()
    }

    /// Records what the engine reported so it is not re-sent by `sync`.
    pub fn record(&mut self, event: &Event) {
        match event {
            Event::EditFocusChanged { view, editable } => {
                if let Some(page) = self.pages.values_mut().find(|page| page.view == *view) { page.state.editable = *editable; }
            }
            Event::Focused { view } => {
                if let Some(tab) = self.tab_for(*view).filter(|tab| self.visible.contains(tab)) { self.shown = Some(tab); }
            }
            Event::UrlChanged { view, url } => {
                if let Some(page) = self.pages.values_mut().find(|page| page.view == *view) {
                    page.url = url.clone();
                    page.state.status.clear();
                }
            }
            Event::StatusChanged { view, text } => {
                // Hidden pages can still deliver callbacks queued before they
                // were hidden. Never revive their old hover state.
                if let Some(tab) = self.tab_for(*view).filter(|tab| self.visible.contains(tab)) {
                    self.pages.get_mut(&tab).unwrap().state.status = text.clone();
                }
            }
            Event::LoadingChanged {
                view,
                loading,
                can_go_back,
                can_go_forward,
            } => {
                if let Some(page) = self.pages.values_mut().find(|page| page.view == *view) {
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
