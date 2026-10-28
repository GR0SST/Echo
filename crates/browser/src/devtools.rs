//! Product DevTools commands and native dock ownership. The diagnostic probe
//! remains independent until Chromium's authenticated frontend bridge is enabled.
use super::{PageStore, Pages};
use crate::window::BrowserWindow;
use browser_core::TabId;
use engine::{Bounds, InspectorId, InspectorPanel, InspectorStatus, ViewId};
use gpui::Context;

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
}

/// The GPUI gap between the page card and the DevTools card, as in Arc.
pub(crate) const DIVIDER_WIDTH: f64 = crate::pages::CONTENT_INSET as f64;
/// Neither card shrinks below this while the divider is dragged.
const MIN_PANE: f64 = 160.;

/// Keep both native children strictly within the owner pane, with a GPUI gap.
/// Without a dragged width the dock takes the observed third of the content.
pub(crate) fn frames(frame: Bounds, dock_width: Option<f64>) -> Option<(Bounds, Bounds, Bounds)> {
    if ![frame.x, frame.y, frame.width, frame.height]
        .iter()
        .all(|v| v.is_finite())
        || frame.width < DIVIDER_WIDTH + 2.
        || frame.height < 1.
    {
        return None;
    }
    let available = frame.width - DIVIDER_WIDTH;
    let min = MIN_PANE.min(available / 2.).max(1.);
    let dock_width = dock_width
        .filter(|width| width.is_finite())
        .unwrap_or(frame.width / 3.)
        .clamp(min, available - min)
        .floor();
    let page_width = available - dock_width;
    Some((
        Bounds {
            width: page_width,
            ..frame
        },
        Bounds {
            x: frame.x + page_width,
            width: DIVIDER_WIDTH,
            ..frame
        },
        Bounds {
            x: frame.x + page_width + DIVIDER_WIDTH,
            width: dock_width,
            ..frame
        },
    ))
}

/// Arc's opaque divider gap with its small round grip centered.
pub(crate) fn paint_divider(window: &mut gpui::Window, divider: Bounds, dark: bool) {
    use gpui::{point, px, size};
    const GRIP: f32 = 5.;
    let gap = gpui::Bounds::new(point(px(divider.x as f32), px(divider.y as f32)), size(px(divider.width as f32), px(divider.height as f32)));
    window.paint_quad(gpui::fill(gap, gpui::hsla(0., 0., if dark { 0.13 } else { 0.91 }, 1.)));
    let center = gap.center();
    window.paint_quad(gpui::fill(gpui::Bounds::new(center - point(px(GRIP / 2.), px(GRIP / 2.)), size(px(GRIP), px(GRIP))), gpui::hsla(0., 0., 0.5, 0.25))
        .corner_radii(px(GRIP / 2.)));
}

impl BrowserWindow {
    /// The divider gap is GPUI's own surface between the two native cards, so
    /// it owns the resize cursor and drag; window listeners keep the drag
    /// while the pointer crosses either native view.
    pub(crate) fn render_devtools_divider(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        use gpui::{prelude::*, px};
        let divider = self.pages.as_ref()?.try_borrow().ok()?.devtools_divider()?;
        let owner = cx.entity().downgrade();
        let tracking = self.devtools_resizing.then(|| gpui::canvas(|_, _, _| (), move |_, _, window, _| {
            let moving = owner.clone();
            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
                if phase != gpui::DispatchPhase::Capture { return; }
                let _ = moving.update(cx, |this, cx| {
                    if !event.dragging() { this.devtools_resizing = false; cx.notify(); return; }
                    // One native resize per frame: pointer events outpace
                    // Chromium, which kept resizing after the release.
                    let scheduled = this.devtools_resize_target.replace(f64::from(f32::from(event.position.x))).is_some();
                    if !scheduled {
                        cx.on_next_frame(window, |this, _, cx| this.apply_devtools_resize(cx));
                    }
                });
            });
            let ending = owner.clone();
            window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                if phase != gpui::DispatchPhase::Capture || event.button != gpui::MouseButton::Left { return; }
                let _ = ending.update(cx, |this, cx| {
                    this.devtools_resize_target = Some(f64::from(f32::from(event.position.x)));
                    this.apply_devtools_resize(cx);
                    this.devtools_resizing = false;
                    cx.notify();
                });
            });
        }).absolute().size_0());
        Some(gpui::div()
            .id("devtools-divider")
            .debug_selector(|| "devtools-divider".into())
            .absolute()
            .left(px(divider.x as f32))
            .top(px(divider.y as f32))
            .w(px(divider.width as f32))
            .h(px(divider.height as f32))
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .capture_any_mouse_down(cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                if event.button != gpui::MouseButton::Left { return; }
                window.prevent_default();
                cx.stop_propagation();
                this.devtools_resizing = true;
                cx.notify();
            }))
            .children(tracking)
            .into_any_element())
    }

    fn apply_devtools_resize(&mut self, cx: &mut Context<Self>) {
        let Some(x) = self.devtools_resize_target.take() else { return; };
        if let Some(pages) = &self.pages { pages.borrow_mut().resize_devtools(x); }
        cx.notify();
    }

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
        frames(frame, store.devtools_width)?;
        Some((page.view, parent, frame))
    }

    pub fn devtools_available(&self, tab: TabId) -> bool {
        self.devtools_target(tab).is_some()
    }

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
            .filter(|dock| {
                dock.host == self.host && (dock.view == view || command == Command::Toggle)
            })
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
            store.close_devtools();
            drop(store);
            self.page_has_focus = false;
            self.apply();
            return Ok(());
        }
        if store
            .devtools
            .is_some_and(|dock| dock.view != view || dock.host != self.host)
        {
            store.close_devtools();
        }
        let Operation::Open(panel, inspect) = operation else {
            unreachable!()
        };
        let (_, _, dock_frame) = frames(frame, store.devtools_width).unwrap();
        let id = store
            .engine
            .open_inspector(view, parent, dock_frame, panel, inspect)?;
        // An engine must not attach a frontend to a different inspected page.
        if !store
            .engine
            .inspector_status(id)
            .is_some_and(|status| status.inspected == view)
        {
            store.engine.close_inspector(id);
            return Err("Inspector owner did not match the focused page".into());
        }
        store.devtools_restore.remove(&self.host);
        store.devtools = Some(Dock {
            id,
            tab,
            view,
            host: self.host,
        });
        store.mirror_revision = store.mirror_revision.wrapping_add(1);
        store.layout_devtools();
        store.engine.focus_inspector(id);
        self.devtools_focused = true;
        Ok(())
    }

    pub fn devtools_status(&self) -> Option<InspectorStatus> {
        let store = self.store.borrow();
        let dock = store.devtools.filter(|dock| dock.host == self.host)?;
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

    pub(crate) fn devtools_divider(&self) -> Option<Bounds> {
        let store = self.store.borrow();
        let dock = store.devtools.filter(|dock| dock.host == self.host)?;
        frames(*store.layouts.get(&self.host)?.get(&dock.tab)?, store.devtools_width).map(|(_, divider, _)| divider)
    }

    /// Drag the divider so its center follows `x`; the width persists for
    /// every later dock in this session.
    pub(crate) fn resize_devtools(&mut self, x: f64) {
        let mut store = self.store.borrow_mut();
        let Some(dock) = store.devtools.filter(|dock| dock.host == self.host) else { return; };
        let Some(frame) = store.layouts.get(&self.host).and_then(|layout| layout.get(&dock.tab)).copied() else { return; };
        let width = frame.x + frame.width - x - DIVIDER_WIDTH / 2.;
        let Some((_, _, frontend)) = frames(frame, Some(width)) else { return; };
        if store.devtools_width == Some(frontend.width) { return; }
        store.devtools_width = Some(frontend.width);
        store.mirror_revision = store.mirror_revision.wrapping_add(1);
        store.layout_devtools();
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
                engine::InspectorEvent::CloseRequested { inspector }
                    if self.inspector_event_owner(inspector).is_some() =>
                {
                    self.close_devtools();
                }
                engine::InspectorEvent::FocusRequested { inspector }
                    if self.inspector_event_owner(inspector).is_some() && allow_focus =>
                {
                    self.engine.focus_inspector(inspector);
                }
                // Inspector title, URL and focus must never become tab metadata.
                _ => {}
            }
        }
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

    pub(super) fn close_devtools(&mut self) {
        let Some(dock) = self.devtools.take() else {
            return;
        };
        self.engine.close_inspector(dock.id);
        self.engine.set_side_corners_rounded(dock.view, true, true);
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

    /// Returns the closed host so its Pages can restore focus, unless an overlay
    /// currently owns the responder. Never reload or recreate the inspected page.
    pub(super) fn layout_devtools(&mut self) -> Option<u64> {
        let dock = self.devtools?;
        let valid = self.active_host == dock.host
            && self.hosts.contains_key(&dock.host)
            && self.pages.get(&dock.tab).is_some_and(|page| {
                page.owner == dock.host && page.view == dock.view && page.visible
            })
            && self.engine.inspector_status(dock.id).is_some_and(|status| {
                status.inspected == dock.view
                    && !matches!(
                        status.phase,
                        engine::InspectorPhase::Detached | engine::InspectorPhase::LoadFailed
                    )
            });
        let layout = self
            .layouts
            .get(&dock.host)
            .and_then(|layout| layout.get(&dock.tab))
            .copied()
            .and_then(|frame| frames(frame, self.devtools_width));
        let Some((page_frame, _, frontend)) = layout.filter(|_| valid) else {
            self.close_devtools();
            return Some(dock.host);
        };
        self.engine.set_bounds(dock.view, page_frame);
        self.engine.set_side_corners_rounded(dock.view, true, false);
        self.pages.get_mut(&dock.tab).unwrap().frame = page_frame;
        self.engine.set_inspector_frame(dock.id, frontend);
        self.engine.set_inspector_corners(dock.id, f64::from(crate::pages::CONTENT_CORNER_RADIUS));
        self.engine.set_inspector_visible(dock.id, true);
        None
    }

    pub(super) fn inspector_event_owner(&self, id: InspectorId) -> Option<Dock> {
        self.devtools.filter(|dock| {
            dock.id == id
                && dock.host == self.active_host
                && self.pages.get(&dock.tab).is_some_and(|page| {
                    page.owner == dock.host && page.view == dock.view && page.visible
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
    #[test]
    fn right_third_native_frames_leave_only_the_gpui_divider_uncovered() {
        for width in [DIVIDER_WIDTH + 2., 31., 899.5, 1200.] {
            let frame = Bounds { x: 248., y: 8., width, height: 700. };
            for dock in [None, Some(0.), Some(250.), Some(1e6)] {
                let (page, divider, frontend) = frames(frame, dock).unwrap();
                assert_eq!(page.x, frame.x);
                assert_eq!(page.x + page.width, divider.x);
                assert_eq!(divider.width, DIVIDER_WIDTH);
                assert_eq!(divider.x + divider.width, frontend.x);
                assert_eq!(frontend.x + frontend.width, frame.x + width);
                assert!(page.width >= 1. && frontend.width >= 1.);
            }
        }
        let frame = Bounds { x: 0., y: 0., width: 1200., height: 700. };
        assert_eq!(frames(frame, None).unwrap().2.width, 400.);
        assert_eq!(frames(frame, Some(250.)).unwrap().2.width, 250.);
        // Neither card collapses while dragging.
        assert_eq!(frames(frame, Some(0.)).unwrap().2.width, MIN_PANE);
        assert_eq!(frames(frame, Some(1e6)).unwrap().0.width, MIN_PANE);
        assert!(frames(Bounds { width: 2., height: 700., ..Bounds::default() }, None).is_none());
        assert!(frames(Bounds { width: f64::NAN, height: 700., ..Bounds::default() }, None).is_none());
    }
}
