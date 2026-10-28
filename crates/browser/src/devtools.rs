//! Product DevTools commands and native dock ownership. The diagnostic probe
//! remains independent until Chromium's authenticated frontend bridge is enabled.
use super::{PageStore, Pages};
use crate::window::BrowserWindow;
use browser_core::TabId;
use engine::{Bounds, InspectorDock, InspectorId, InspectorPanel, InspectorStatus, ViewId};
use gpui::{AppContext as _, Context, Styled as _};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Elements,
    Console,
    Network,
}

pub(crate) enum Input {
    Focus(TabId),
    Shortcut(engine::Shortcut),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Close,
    Open(InspectorPanel, bool),
}

fn operation(command: Command, open: bool, selected: Option<InspectorPanel>) -> Operation {
    match command {
        Command::Toggle if open => Operation::Close,
        Command::Console if open && selected == Some(InspectorPanel::Console) => Operation::Close,
        Command::Network if open && selected == Some(InspectorPanel::Network) => Operation::Close,
        Command::Console => Operation::Open(InspectorPanel::Console, false),
        Command::Network => Operation::Open(InspectorPanel::Network, false),
        Command::Elements => Operation::Open(InspectorPanel::Elements, true),
        Command::Toggle => Operation::Open(InspectorPanel::Elements, false),
    }
}

#[derive(Clone, Copy)]
pub(super) struct Dock {
    pub id: InspectorId,
    pub tab: TabId,
    pub view: ViewId,
    pub host: u64,
    pub side: InspectorDock,
    /// The frontend's reserved page rectangle, relative to the pane.
    pub page_bounds: Option<[u32; 4]>,
    /// Last native layout (pane, page, shown); unchanged layouts are skipped.
    pub applied: Option<(Bounds, Bounds, bool)>,
}

/// Docked, the original frontend fills the pane and lays out its own
/// splitter (resizing, dock side, close button) around a reserved rectangle;
/// the inspected page is placed there, above it. Until the frontend reports
/// it, the default third keeps the frontend visible beside the page.
pub(crate) fn page_frame(pane: Bounds, side: InspectorDock, reserved: Option<[u32; 4]>) -> Bounds {
    if side == InspectorDock::Undocked { return pane; }
    if let Some([x, y, width, height]) = reserved {
        let x = f64::from(x).min(pane.width);
        let y = f64::from(y).min(pane.height);
        let page = Bounds { x: pane.x + x, y: pane.y + y, width: f64::from(width).min(pane.width - x).max(1.), height: f64::from(height).min(pane.height - y).max(1.) };
        return reveal_resizer(pane, page);
    }
    let (width, height) = ((pane.width * 2. / 3.).floor(), (pane.height * 2. / 3.).floor());
    match side {
        InspectorDock::Left => Bounds { x: pane.x + pane.width - width, width, ..pane },
        InspectorDock::Bottom => Bounds { height, ..pane },
        _ => Bounds { width, ..pane },
    }
}

/// The frontend's splitter straddles the page edge; the page sits above the
/// frontend, so pull its joined edges back to keep the whole splitter usable.
const RESIZER_HALF: f64 = 4.;
fn reveal_resizer(pane: Bounds, page: Bounds) -> Bounds {
    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
    let left = if near(page.x, pane.x) { 0. } else { RESIZER_HALF };
    let top = if near(page.y, pane.y) { 0. } else { RESIZER_HALF };
    let right = if near(page.x + page.width, pane.x + pane.width) { 0. } else { RESIZER_HALF };
    let bottom = if near(page.y + page.height, pane.y + pane.height) { 0. } else { RESIZER_HALF };
    Bounds { x: page.x + left, y: page.y + top, width: (page.width - left - right).max(1.), height: (page.height - top - bottom).max(1.) }
}

/// The page keeps rounded only the corners it shares with its pane
/// (top-left, top-right, bottom-left, bottom-right).
pub(crate) fn page_corners(pane: Bounds, page: Bounds) -> [bool; 4] {
    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
    let (left, top) = (near(page.x, pane.x), near(page.y, pane.y));
    let right = near(page.x + page.width, pane.x + pane.width);
    let bottom = near(page.y + page.height, pane.y + pane.height);
    [left && top, right && top, left && bottom, right && bottom]
}

impl BrowserWindow {
    pub(crate) fn devtools_available(&self) -> bool {
        if !self.page_content_visible() { return false; }
        let Some(tab) = self
            .browser
            .active_tab()
            .filter(|tab| !browser_core::omnibox::is_internal_url(&tab.url))
        else {
            return false;
        };
        self.pages
            .as_ref()
            .is_some_and(|pages| pages.borrow().devtools_available(tab.id))
    }

    pub(crate) fn run_devtools(&mut self, command: Command, cx: &mut Context<Self>) {
        if !self.devtools_available() {
            return;
        }
        let tab = self.browser.active_tab().unwrap().id;
        self.close_user_profile_dialog(cx);
        self.close_command_bar(cx);
        self.close_find(cx);
        // Closing an overlay restores its root responder asynchronously. Focus
        // the frontend after that restoration, using the current private/public
        // Pages engine, and abandon stale commands if selection changed.
        let entity = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = entity.update(cx, |this, cx| {
                if !this.page_content_visible()
                    || this.browser.active_tab().is_none_or(|active| active.id != tab)
                    || this.user_profile_dialog.is_some()
                    || this.command_bar.is_some()
                    || this.find_bar.is_some()
                {
                    return;
                }
                if let Some(pages) = &this.pages {
                    let mut pages = pages.borrow_mut();
                    pages.sync(&this.browser, this.page_content_visible());
                    if let Err(error) = pages.run_devtools(tab, command) {
                        eprintln!("[devtools] {error}");
                    }
                }
                cx.notify();
            });
        });
    }
}

impl Pages {
    /// Resolve the focused split pane's actual owner, never a model-only tab or
    /// another window's live view. A fake native parent is sufficient in tests.
    fn devtools_target(&self, tab: TabId) -> Option<(ViewId, *mut std::ffi::c_void, Bounds)> {
        let store = self.store.borrow();
        if store.inspector_probe.is_some()
            || !store.engine.supports_inspector()
            || store.active_host != self.host
            || self.wanted_focus != Some(tab)
            || self.shown != Some(tab)
            || !self.visible.contains(&tab)
        {
            return None;
        }
        let page = store.pages.get(&tab).filter(|page| {
            page.owner == self.host
                && page.visible
                && !browser_core::omnibox::is_internal_url(&page.url)
        })?;
        let parent = store
            .hosts
            .get(&self.host)
            .copied()
            .filter(|parent| !parent.is_null())?;
        let frame = *store.layouts.get(&self.host)?.get(&tab)?;
        (frame.width >= 3. && frame.height >= 3.).then_some((page.view, parent, frame))
    }

    pub fn devtools_available(&self, tab: TabId) -> bool {
        self.devtools_target(tab).is_some()
    }

    /// Each pane (tab) owns its own DevTools; toggling one never closes another.
    pub(crate) fn run_devtools(&mut self, tab: TabId, command: Command) -> Result<(), String> {
        let (view, parent, frame) = self
            .devtools_target(tab)
            .ok_or("No focused owned live page")?;
        let mut store = self.store.borrow_mut();
        // The probe and product frontend must never share transport ownership.
        if store.inspector_probe.is_some() {
            return Err("Close the diagnostic inspector first".into());
        }
        let current = store
            .devtools
            .get(&tab)
            .filter(|dock| dock.host == self.host && dock.view == view)
            .and_then(|dock| {
                store
                    .engine
                    .inspector_status(dock.id)
                    .filter(|status| status.inspected == dock.view)
            });
        let operation = operation(
            command,
            current.is_some(),
            current.as_ref().and_then(|status| status.selected_panel),
        );
        if operation == Operation::Close {
            store.close_devtools(tab);
            drop(store);
            self.page_has_focus = false;
            self.apply();
            return Ok(());
        }
        if store.devtools.get(&tab).is_some_and(|dock| dock.view != view || dock.host != self.host) {
            store.close_devtools(tab);
        }
        let Operation::Open(panel, inspect) = operation else {
            unreachable!()
        };
        // An open frontend is reused (the engine switches its panel).
        let id = store.engine.open_inspector(view, parent, frame, panel, inspect)?;
        // An engine must not attach a frontend to a different inspected page.
        if !store
            .engine
            .inspector_status(id)
            .is_some_and(|status| status.inspected == view)
        {
            store.engine.close_inspector(id);
            store.devtools.remove(&tab);
            return Err("Inspector owner did not match the focused page".into());
        }
        if !store.devtools.contains_key(&tab) {
            let side = store.devtools_side;
            store.engine.set_inspector_dock(id, side);
            store.engine.place_inspector_below_page(id);
            store.devtools.insert(tab, Dock { id, tab, view, host: self.host, side, page_bounds: None, applied: None });
        }
        store.devtools_restore.remove(&self.host);
        store.mirror_revision = store.mirror_revision.wrapping_add(1);
        store.layout_devtools();
        store.engine.focus_inspector(id);
        self.devtools_focused = true;
        Ok(())
    }

    /// The focused pane's DevTools, else any open in this window.
    pub fn devtools_status(&self) -> Option<InspectorStatus> {
        let store = self.store.borrow();
        let mut docks: Vec<_> = store.devtools.values().filter(|dock| dock.host == self.host).copied().collect();
        docks.sort_by_key(|dock| (Some(dock.tab) != self.wanted_focus, dock.id.raw()));
        let dock = docks.first()?;
        store
            .engine
            .inspector_status(dock.id)
            .filter(|status| status.inspected == dock.view)
    }

    /// Consume ordered native input without turning frontend focus into page
    /// focus. Revalidate here because earlier events can close or move its owner.
    pub(crate) fn record_inspector_input(
        &mut self,
        event: &engine::InspectorEvent,
    ) -> Option<Input> {
        let dock = self.store.borrow().inspector_input_owner(event)?;
        if dock.host != self.host {
            return None;
        }
        match event {
            engine::InspectorEvent::Focused { .. } if self.page_focus => {
                self.devtools_focused = true;
                self.shown = Some(dock.tab);
                self.wanted_focus = Some(dock.tab);
                self.page_has_focus = false;
                Some(Input::Focus(dock.tab))
            }
            engine::InspectorEvent::Shortcut { shortcut, .. } => {
                Some(Input::Shortcut(shortcut.clone()))
            }
            _ => None,
        }
    }
}

/// A separate DevTools window owned by the page's browser window.
pub(crate) struct DevToolsWindow {
    pages: std::rc::Weak<std::cell::RefCell<Pages>>,
    id: InspectorId,
    attached: Option<Bounds>,
}

impl gpui::Render for DevToolsWindow {
    fn render(&mut self, window: &mut gpui::Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        let size = window.viewport_size();
        let frame = Bounds { x: 0., y: 0., width: f64::from(f32::from(size.width)), height: f64::from(f32::from(size.height)) };
        if self.attached != Some(frame) {
            if let (Some(pages), Some(view)) = (self.pages.upgrade(), crate::native::view_of(window)) {
                if let Ok(pages) = pages.try_borrow() {
                    if pages.show_devtools_window(self.id, view, frame, self.attached.is_none()) { self.attached = Some(frame); }
                }
            }
        }
        gpui::div().size_full().bg(gpui::rgb(0x282828))
    }
}

impl BrowserWindow {
    /// Open a window for each DevTools the frontend undocked, and return
    /// re-docked or closed ones to their page.
    /// Called from `render`: opening or closing a window while drawing
    /// corrupts GPUI's element arena, so the change is deferred.
    pub(crate) fn sync_devtools_windows(&mut self, cx: &mut Context<Self>) {
        let Some(pages) = self.pages.as_ref() else { return; };
        let Ok(undocked) = pages.try_borrow().map(|pages| pages.undocked_devtools()) else { return; };
        let changed = undocked.len() != self.devtools_windows.len()
            || undocked.iter().any(|(id, _)| !self.devtools_windows.contains_key(id));
        if changed {
            let this = cx.entity().downgrade();
            cx.defer(move |cx| { let _ = this.update(cx, |this, cx| this.apply_devtools_windows(cx)); });
        }
    }

    fn apply_devtools_windows(&mut self, cx: &mut Context<Self>) {
        let Some(pages) = self.pages.clone() else { return; };
        let Ok(undocked) = pages.try_borrow().map(|pages| pages.undocked_devtools()) else { return; };
        let stale: Vec<_> = self.devtools_windows.keys().filter(|id| !undocked.iter().any(|(open, _)| open == *id)).copied().collect();
        for id in stale {
            if let Some(handle) = self.devtools_windows.remove(&id) {
                if let Ok(mut pages) = pages.try_borrow_mut() { pages.redock_devtools(id); }
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
        }
        for (id, title) in undocked {
            if self.devtools_windows.contains_key(&id) { continue; }
            let weak = Rc::downgrade(&pages);
            let bounds = gpui::Bounds::centered(None, gpui::size(gpui::px(960.), gpui::px(640.)), cx);
            let closing = Rc::downgrade(&pages);
            let Ok(handle) = cx.open_window(gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions { title: Some(format!("DevTools - {title}").into()), ..Default::default() }),
                ..Default::default()
            }, move |window, cx| {
                window.on_window_should_close(cx, move |_, _| {
                    if let Some(pages) = closing.upgrade() { if let Ok(mut pages) = pages.try_borrow_mut() { pages.close_devtools_by_id(id); } }
                    true
                });
                cx.new(|_| DevToolsWindow { pages: weak, id, attached: None })
            }) else { continue; };
            self.devtools_windows.insert(id, handle);
        }
    }
}

impl Pages {
    /// Undocked DevTools of this window with their page URL (for the title).
    pub(crate) fn undocked_devtools(&self) -> Vec<(InspectorId, String)> {
        let store = self.store.borrow();
        store.devtools.values().filter(|dock| dock.host == self.host && dock.side == InspectorDock::Undocked)
            .map(|dock| (dock.id, store.pages.get(&dock.tab).map(|page| page.url.clone()).unwrap_or_default())).collect()
    }

    /// Move (first) or resize an undocked frontend inside its own window.
    pub(crate) fn show_devtools_window(&self, id: InspectorId, view: *mut std::ffi::c_void, frame: Bounds, attach: bool) -> bool {
        let Ok(mut store) = self.store.try_borrow_mut() else { return false; };
        if !store.devtools.values().any(|dock| dock.id == id && dock.side == InspectorDock::Undocked) { return false; }
        if attach {
            if !store.engine.move_inspector(id, view, frame) { return false; }
            store.engine.set_inspector_corners(id, 0., [true; 4]);
        }
        store.engine.set_inspector_frame(id, frame);
        store.engine.set_inspector_visible(id, true);
        true
    }

    /// Bring a re-docked frontend back beneath its page in this window.
    pub(crate) fn redock_devtools(&mut self, id: InspectorId) {
        let mut store = self.store.borrow_mut();
        let Some(dock) = store.devtools.values().copied().find(|dock| dock.id == id) else { return; };
        let Some(parent) = store.hosts.get(&dock.host).copied().filter(|parent| !parent.is_null()) else { return; };
        let pane = store.layouts.get(&dock.host).and_then(|layout| layout.get(&dock.tab)).copied().unwrap_or_default();
        if store.engine.move_inspector(id, parent, pane) {
            store.engine.place_inspector_below_page(id);
        }
        if let Some(entry) = store.devtools.get_mut(&dock.tab) { entry.applied = None; }
        store.layout_devtools();
    }

    pub(crate) fn close_devtools_by_id(&mut self, id: InspectorId) {
        let mut store = self.store.borrow_mut();
        if let Some(tab) = store.devtools.values().find(|dock| dock.id == id).map(|dock| dock.tab) { store.close_devtools(tab); }
        drop(store);
        self.apply();
    }
}

impl PageStore {
    pub(super) fn handle_devtools_events(&mut self, host: u64, allow_focus: bool) {
        // Other windows must not consume events belonging to the active host.
        if self.active_host != host {
            return;
        }
        for event in self.engine.take_inspector_events() {
            if self.inspector_probe.is_some() {
                eprintln!("[inspector-event] {event:?}");
            }
            match event {
                engine::InspectorEvent::CloseRequested { inspector } => {
                    if let Some(dock) = self.inspector_event_owner(inspector) { self.close_devtools(dock.tab); }
                }
                engine::InspectorEvent::FocusRequested { inspector }
                    if self.inspector_event_owner(inspector).is_some() && allow_focus =>
                {
                    self.engine.focus_inspector(inspector);
                }
                engine::InspectorEvent::DockRequested { inspector, dock: side } => {
                    if let Some(dock) = self.devtools.values_mut().find(|dock| dock.id == inspector) {
                        dock.side = side;
                        dock.page_bounds = None;
                        // The next DevTools opens where this one was put.
                        self.devtools_side = side;
                        self.mirror_revision = self.mirror_revision.wrapping_add(1);
                    }
                }
                engine::InspectorEvent::PageBounds { inspector, bounds } => {
                    if let Some(dock) = self.devtools.values_mut().find(|dock| dock.id == inspector) {
                        if dock.page_bounds != Some(bounds) {
                            dock.page_bounds = Some(bounds);
                            self.mirror_revision = self.mirror_revision.wrapping_add(1);
                        }
                    }
                }
                // Inspector title, URL and focus must never become tab metadata.
                _ => {}
            }
        }
        self.layout_devtools();
    }

    pub(super) fn inspector_input_owner(&self, event: &engine::InspectorEvent) -> Option<Dock> {
        match event {
            engine::InspectorEvent::Focused { inspector }
            | engine::InspectorEvent::Shortcut { inspector, .. } => {
                self.inspector_event_owner(*inspector)
            }
            _ => None,
        }
    }

    pub(super) fn reconcile_devtools(
        &mut self,
        host: u64,
        wanted: Option<TabId>,
        restore_focus: bool,
    ) -> (bool, bool) {
        self.layout_devtools();
        if !self.devtools_restore.remove(&host) {
            return (false, false);
        }
        let view = wanted
            .and_then(|tab| self.pages.get(&tab))
            .filter(|page| {
                page.owner == host && page.visible && self.active_host == host && restore_focus
            })
            .map(|page| page.view);
        if let Some(view) = view {
            self.engine.focus_page(view);
        }
        (true, view.is_some())
    }

    pub(super) fn close_devtools(&mut self, tab: TabId) {
        let Some(dock) = self.devtools.remove(&tab) else {
            return;
        };
        self.engine.close_inspector(dock.id);
        self.engine.set_open_corners(dock.view, [true; 4]);
        self.devtools_restore.insert(dock.host);
        self.mirror_revision = self.mirror_revision.wrapping_add(1);
        if let Some(page) = self
            .pages
            .get_mut(&dock.tab)
            .filter(|page| page.owner == dock.host && page.view == dock.view)
        {
            if let Some(frame) = self
                .layouts
                .get(&dock.host)
                .and_then(|layout| layout.get(&dock.tab))
                .copied()
            {
                self.engine.set_bounds(dock.view, frame);
                page.frame = frame;
            }
        }
    }

    pub(super) fn close_host_devtools(&mut self, host: u64) {
        let tabs: Vec<_> = self.devtools.values().filter(|dock| dock.host == host).map(|dock| dock.tab).collect();
        for tab in tabs { self.close_devtools(tab); }
    }

    /// A docked frontend fills its pane beneath the page; a hidden pane hides
    /// its DevTools without closing it. Never reload or recreate the page.
    pub(super) fn layout_devtools(&mut self) {
        let docks: Vec<Dock> = self.devtools.values().copied().collect();
        for dock in docks {
            let alive = self.hosts.contains_key(&dock.host)
                && self.pages.get(&dock.tab).is_some_and(|page| page.owner == dock.host && page.view == dock.view)
                && self.engine.inspector_status(dock.id).is_some_and(|status| {
                    status.inspected == dock.view
                        && !matches!(status.phase, engine::InspectorPhase::Detached | engine::InspectorPhase::LoadFailed)
                });
            if !alive {
                self.close_devtools(dock.tab);
                continue;
            }
            let shown = self.active_host == dock.host && self.pages.get(&dock.tab).is_some_and(|page| page.visible);
            let pane = self.layouts.get(&dock.host).and_then(|layout| layout.get(&dock.tab)).copied();
            let page = pane.map(|pane| page_frame(pane, dock.side, dock.page_bounds));
            let applied = pane.zip(page).map(|(pane, page)| (pane, page, shown));
            if applied.is_some() && applied == dock.applied { continue; }
            if let Some(entry) = self.devtools.get_mut(&dock.tab) { entry.applied = applied; }
            let (Some(pane), Some(page), true) = (pane, page, shown) else {
                // A separate DevTools window stays up while its tab is hidden.
                self.engine.set_inspector_visible(dock.id, dock.side == InspectorDock::Undocked);
                continue;
            };
            self.engine.set_bounds(dock.view, page);
            self.engine.set_open_corners(dock.view, page_corners(pane, page));
            self.pages.get_mut(&dock.tab).unwrap().frame = page;
            if dock.side != InspectorDock::Undocked {
                self.engine.set_inspector_frame(dock.id, pane);
                self.engine.set_inspector_corners(dock.id, f64::from(crate::pages::CONTENT_CORNER_RADIUS), [true; 4]);
            }
            self.engine.set_inspector_visible(dock.id, true);
        }
    }

    pub(super) fn inspector_event_owner(&self, id: InspectorId) -> Option<Dock> {
        self.devtools.values().copied().find(|dock| {
            dock.id == id
                && dock.host == self.active_host
                && self.pages.get(&dock.tab).is_some_and(|page| {
                    page.owner == dock.host && page.view == dock.view && (page.visible || dock.side == InspectorDock::Undocked)
                })
                && self
                    .engine
                    .inspector_status(id)
                    .is_some_and(|status| status.inspected == dock.view)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed_panel_controls_toggling_and_inspection_never_closes() {
        for selected in [
            None,
            Some(InspectorPanel::Elements),
            Some(InspectorPanel::Console),
            Some(InspectorPanel::Network),
        ] {
            assert_eq!(
                operation(Command::Toggle, false, selected),
                Operation::Open(InspectorPanel::Elements, false)
            );
            assert_eq!(operation(Command::Toggle, true, selected), Operation::Close);
            assert_eq!(
                operation(Command::Elements, true, selected),
                Operation::Open(InspectorPanel::Elements, true)
            );
            assert_eq!(
                operation(Command::Console, true, selected),
                if selected == Some(InspectorPanel::Console) {
                    Operation::Close
                } else {
                    Operation::Open(InspectorPanel::Console, false)
                }
            );
            assert_eq!(
                operation(Command::Network, true, selected),
                if selected == Some(InspectorPanel::Network) {
                    Operation::Close
                } else {
                    Operation::Open(InspectorPanel::Network, false)
                }
            );
        }
    }
}
