//! The held sidebar row is painted in a passive child window over the whole
//! browser window: native pages and the floating sidebar's own window would
//! otherwise clip it where it leaves the sidebar.
use crate::{tab_drag::TabDrag, window::{BrowserWindow, Overlay, OverlayKind}};
use gpui::{div, prelude::*, px, AnyElement, Bounds, Context, Pixels, Point, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions};

/// Room for a row hanging over the window edge before a tear-off takes over.
const GHOST_MARGIN: f32 = 280.;

pub(crate) struct DragGhost {
    drag: TabDrag,
    /// The pointer in browser-window coordinates.
    pointer: Option<Point<Pixels>>,
    bounds: Bounds<Pixels>,
    overlay: Option<WindowHandle<Overlay>>,
}

impl DragGhost {
    pub(crate) fn drag(&self) -> &TabDrag { &self.drag }
    pub(crate) fn overlay_handle(&self) -> Option<WindowHandle<Overlay>> { self.overlay }
}

impl BrowserWindow {
    pub(crate) fn start_drag_ghost(&mut self, drag: &TabDrag, cx: &mut Context<Self>) {
        self.close_drag_ghost(cx);
        self.drag_ghost = Some(DragGhost { drag: drag.clone(), pointer: None, bounds: self.window_bounds, overlay: None });
        self.ensure_drag_ghost_overlay(cx);
    }

    /// Every tab drag move, from the browser window or the floating sidebar.
    pub(crate) fn on_tab_drag_move(&mut self, drag: &TabDrag, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.update_split_drop(drag, position, cx);
        self.update_tear_off(drag, position, cx);
        let bounds = self.window_bounds;
        if let Some(ghost) = &mut self.drag_ghost {
            ghost.pointer = Some(position);
            if ghost.bounds != bounds {
                ghost.bounds = bounds;
                if let Some(overlay) = ghost.overlay.take() { cx.defer(move |cx| { let _ = overlay.update(cx, |_, window, _| window.remove_window()); }); }
            }
        }
        self.ensure_drag_ghost_overlay(cx);
        cx.notify();
    }

    fn ensure_drag_ghost_overlay(&mut self, cx: &mut Context<Self>) {
        let Some(ghost) = &self.drag_ghost else { return; };
        if ghost.overlay.is_some() { return; }
        let bounds = ghost.bounds;
        let browser = cx.entity().downgrade();
        let parent = self.native_view;
        cx.defer(move |cx| {
            if !browser.read_with(cx, |this, _| this.drag_ghost.as_ref().is_some_and(|ghost| ghost.overlay.is_none() && ghost.bounds == bounds)).unwrap_or(false) { return; }
            let margin = gpui::point(px(GHOST_MARGIN), px(GHOST_MARGIN));
            let expanded = Bounds::new(bounds.origin - margin, bounds.size + gpui::size(px(2. * GHOST_MARGIN), px(2. * GHOST_MARGIN)));
            let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(expanded)), titlebar: None, focus: false,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default() };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                    crate::native::set_overlay_passthrough(child);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::DragGhost, window, cx))
            }) {
                let keep = browser.update(cx, |this, cx| match &mut this.drag_ghost {
                    Some(ghost) if ghost.overlay.is_none() && ghost.bounds == bounds => { ghost.overlay = Some(overlay); cx.notify(); true }
                    _ => false,
                }).unwrap_or(false);
                if !keep { let _ = overlay.update(cx, |_, window, _| window.remove_window()); }
            }
        });
    }

    pub(crate) fn close_drag_ghost(&mut self, cx: &mut Context<Self>) {
        if let Some(ghost) = self.drag_ghost.take() {
            if let Some(overlay) = ghost.overlay { cx.defer(move |cx| { let _ = overlay.update(cx, |_, window, _| window.remove_window()); }); }
            cx.notify();
        }
    }

    /// Once the passive window exists it alone paints the row.
    pub(crate) fn drag_ghost_shown(&self) -> bool {
        // Until the first move it has no position; GPUI's own row stays.
        self.drag_ghost.as_ref().is_some_and(|ghost| ghost.overlay.is_some() && ghost.pointer.is_some())
    }

    pub(crate) fn render_drag_ghost(&self, _window: &mut Window) -> AnyElement {
        let Some(ghost) = &self.drag_ghost else { return div().into_any_element(); };
        // The page card and the tear-off picture replace the row.
        let Some(pointer) = ghost.pointer.filter(|_| self.split_drop.is_none() && self.tear_off.is_none()) else { return div().into_any_element(); };
        let origin = pointer - ghost.drag.grab_point() + gpui::point(px(GHOST_MARGIN), px(GHOST_MARGIN));
        div().size_full().child(div().debug_selector(|| "tab-drag-ghost".into()).absolute().left(origin.x).top(origin.y)
            .child(ghost.drag.ghost_element()))
            .into_any_element()
    }
}
