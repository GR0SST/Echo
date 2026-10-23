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

/// Keep both native children strictly within the owner pane, with a GPUI gap.
/// The default matches the observed right dock (one third of content width).
pub(crate) fn frames(frame: Bounds) -> Option<(Bounds, Bounds, Bounds)> {
    if ![frame.x, frame.y, frame.width, frame.height]
        .iter()
        .all(|v| v.is_finite())
        || frame.width < 3.
        || frame.height < 1.
    {
        return None;
    }
    let dock_width = (frame.width / 3.).floor().max(1.);
    let page_width = frame.width - dock_width - 1.;
    Some((
        Bounds {
            width: page_width,
            ..frame
        },
        Bounds {
            x: frame.x + page_width,
            width: 1.,
            ..frame
        },
        Bounds {
            x: frame.x + page_width + 1.,
            width: dock_width,
            ..frame
        },
    ))
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
        frames(frame)?;
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
        let (_, _, dock_frame) = frames(frame).unwrap();
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
        frames(*store.layouts.get(&self.host)?.get(&dock.tab)?).map(|(_, divider, _)| divider)
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
            .and_then(frames);
        let Some((page_frame, _, frontend)) = layout.filter(|_| valid) else {
            self.close_devtools();
            return Some(dock.host);
        };
        self.engine.set_bounds(dock.view, page_frame);
        self.pages.get_mut(&dock.tab).unwrap().frame = page_frame;
        self.engine.set_inspector_frame(dock.id, frontend);
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
        for width in [3., 31., 899.5, 1200.] {
            let frame = Bounds {
                x: 248.,
                y: 8.,
                width,
                height: 700.,
            };
            let (page, divider, frontend) = frames(frame).unwrap();
            assert_eq!(page.x, frame.x);
            assert_eq!(page.x + page.width, divider.x);
            assert_eq!(divider.x + divider.width, frontend.x);
            assert_eq!(frontend.x + frontend.width, frame.x + width);
            assert_eq!(frontend.width, (width / 3.).floor());
            assert!(page.width >= 1. && frontend.width >= 1.);
        }
        assert!(
            frames(Bounds {
                width: 2.,
                height: 700.,
                ..Bounds::default()
            })
            .is_none()
        );
        assert!(
            frames(Bounds {
                width: f64::NAN,
                height: 700.,
                ..Bounds::default()
            })
            .is_none()
        );
    }
}
