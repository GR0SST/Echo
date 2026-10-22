//! Keeps the engine's web views in sync with the browser model: one view per
//! opened tab, only the active one visible, laid out over the content card.

use std::collections::HashMap;
use std::ffi::c_void;

use browser_core::{Browser, TabId};
use engine::{Bounds, Engine, Event, ViewId};

/// What the window needs from a web engine. Implemented by Chromium in the
/// app and by a recording fake in tests.
pub trait WebEngine {
    fn create(&mut self, url: &str, bounds: Bounds) -> Option<ViewId>;
    fn set_bounds(&mut self, view: ViewId, bounds: Bounds);
    fn set_visible(&mut self, view: ViewId, visible: bool);
    /// Keyboard focus to the page.
    fn focus_page(&mut self, view: ViewId);
    /// Keyboard focus to the browser UI (e.g. the command bar).
    fn focus_ui(&mut self);
    fn navigate(&mut self, view: ViewId, url: &str);
    fn go_back(&mut self, view: ViewId);
    fn go_forward(&mut self, view: ViewId);
    fn reload(&mut self, view: ViewId);
    fn force_reload(&mut self, view: ViewId);
    fn stop_loading(&mut self, view: ViewId);
    fn zoom(&mut self, view: ViewId, direction: i32);
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
    fn reload(&mut self, view: ViewId) {
        self.engine.reload(view)
    }
    fn force_reload(&mut self, view: ViewId) { self.engine.force_reload(view) }
    fn stop_loading(&mut self, view: ViewId) { self.engine.stop_loading(view) }
    fn zoom(&mut self, view: ViewId, direction: i32) { self.engine.zoom(view, direction) }
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
}

struct Page {
    view: ViewId,
    /// The URL the engine is showing or was last told to load.
    url: String,
    state: PageState,
}

pub struct Pages {
    engine: Box<dyn WebEngine>,
    pages: HashMap<TabId, Page>,
    /// Tab whose page should be on screen, with the URL it should show.
    wanted: Option<(TabId, String)>,
    shown: Option<TabId>,
    bounds: Option<Bounds>,
}

impl Pages {
    pub fn new(engine: Box<dyn WebEngine>) -> Self {
        Self {
            engine,
            pages: HashMap::new(),
            wanted: None,
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
        let open: Vec<TabId> = browser.all_tabs().map(|tab| tab.id).collect();
        let closed: Vec<TabId> = self
            .pages
            .keys()
            .filter(|tab| !open.contains(tab))
            .copied()
            .collect();
        for tab in closed {
            let page = self.pages.remove(&tab).unwrap();
            self.engine.close(page.view);
            if self.shown == Some(tab) {
                self.shown = None;
            }
        }

        self.wanted = browser
            .active_tab()
            .filter(|_| visible)
            .map(|tab| (tab.id, tab.url.clone()));
        self.apply();
    }

    /// Unloads a tab's page, e.g. a closed pinned tab that stays in the
    /// sidebar. It loads again when the tab is next shown.
    pub fn discard(&mut self, tab: TabId) {
        let Some(page) = self.pages.remove(&tab) else {
            return;
        };
        self.engine.close(page.view);
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
        let wanted = self.wanted.clone();
        let wanted_tab = wanted.as_ref().map(|(tab, _)| *tab);

        if self.shown.is_some() && self.shown != wanted_tab {
            let shown = self.shown.take().unwrap();
            if let Some(page) = self.pages.get(&shown) {
                self.engine.set_visible(page.view, false);
            }
            self.engine.focus_ui();
        }

        let (Some((tab, url)), Some(bounds)) = (wanted, self.bounds) else {
            return;
        };
        match self.pages.get_mut(&tab) {
            Some(page) => {
                if page.url != url {
                    self.engine.navigate(page.view, &url);
                    page.url = url;
                }
                self.engine.set_bounds(page.view, bounds);
            }
            None => {
                let Some(view) = self.engine.create(&url, bounds) else {
                    return;
                };
                self.pages.insert(
                    tab,
                    Page {
                        view,
                        url,
                        state: PageState::default(),
                    },
                );
            }
        }
        if self.shown != Some(tab) {
            let view = self.pages[&tab].view;
            self.engine.set_visible(view, true);
            self.engine.focus_page(view);
            self.shown = Some(tab);
        }
    }

    /// Gives keyboard focus back to the page on screen.
    pub fn focus_shown(&mut self) {
        if let Some(view) = self.shown.and_then(|tab| self.view(tab)) {
            self.engine.focus_page(view);
        }
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

    pub fn pump_fn(&self) -> fn() {
        self.engine.pump_fn()
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        self.engine.take_events()
    }

    /// Records what the engine reported so it is not re-sent by `sync`.
    pub fn record(&mut self, event: &Event) {
        match event {
            Event::UrlChanged { view, url } => {
                if let Some(page) = self.pages.values_mut().find(|page| page.view == *view) {
                    page.url = url.clone();
                }
            }
            Event::LoadingChanged {
                view,
                loading,
                can_go_back,
                can_go_forward,
            } => {
                if let Some(page) = self.pages.values_mut().find(|page| page.view == *view) {
                    page.state = PageState {
                        loading: *loading,
                        can_go_back: *can_go_back,
                        can_go_forward: *can_go_forward,
                    };
                }
            }
            Event::TitleChanged { .. } | Event::OpenInNewTab { .. } | Event::Shortcut(_) => {}
        }
    }
}
