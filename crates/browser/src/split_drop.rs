//! A passive themed cover clips the live document while a tab is held at an edge.
use crate::{tab_drag::TabDrag, window::{BrowserWindow, Overlay, OverlayKind}};
use browser_core::{omnibox, splits::{Axis, Placement}, TabId};
use gpui::{div, prelude::*, px, AnyElement, Bounds, Context, Pixels, Point, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions};
use std::time::SystemTime;

pub(crate) struct SplitDropPreview {
    pub source: TabId,
    pub anchor: TabId,
    pub placement: Placement,
    pub cover: Bounds<Pixels>,
    pub outline: Bounds<Pixels>,
    pub overlay: Option<WindowHandle<Overlay>>,
    bounds: Bounds<Pixels>,
}

impl BrowserWindow {
    fn split_drop_target(&self, drag: &TabDrag, position: Point<Pixels>, window: &Window) -> Option<SplitDropPreview> {
        if !self.page_content_visible() || self.command_bar.is_some() || !self.accepts_tab_drag(drag)
            || self.browser.split_for(drag.id).is_some() { return None; }
        let anchor = self.browser.space().active_tab?;
        if self.browser.user_profile_id_for_tab(anchor) != self.browser.user_profile_id_for_tab(drag.id)
            || self.browser.all_tabs().find(|t| t.id == drag.id).is_none_or(|t| omnibox::is_internal_url(&t.url)) { return None; }
        if !self.can_add_split(anchor) || self.browser.related_tabs(anchor).contains(&drag.id) { return None; }
        let viewport = window.viewport_size();
        let left = self.sidebar_width() * self.sidebar_amount() + crate::pages::CONTENT_INSET * (1. - self.sidebar_amount());
        let card = engine::Bounds { x: left.into(), y: crate::pages::CONTENT_INSET.into(),
            width: (f32::from(viewport.width) - left - crate::pages::CONTENT_INSET).max(0.).into(),
            height: (f32::from(viewport.height) - 2. * crate::pages::CONTENT_INSET).max(0.).into() };
        let split = self.browser.active_split();
        let weights = split.map_or(&[1.][..], |split| split.weights.as_slice());
        let axis = split.map_or(Axis::Horizontal, |split| split.axis);
        let panes = crate::split_layout::layout(card, axis, weights, split.is_some());
        let index = panes.iter().position(|pane| {
            let p = pane.frame;
            f64::from(f32::from(position.x)) >= p.x && f64::from(f32::from(position.x)) < p.x+p.width
                && f64::from(f32::from(position.y)) >= p.y && f64::from(f32::from(position.y)) < p.y+p.height
        })?;
        let frame = panes[index].frame;
        let x = (f64::from(f32::from(position.x)) - frame.x) / frame.width;
        let y = (f64::from(f32::from(position.y)) - frame.y) / frame.height;
        let (placement, horizontal) = if split.is_some() && axis == Axis::Vertical {
            if y < 0.25 { (Placement::Above, false) } else if y > 0.75 { (Placement::Below, false) } else { return None; }
        } else if x < 0.25 { (Placement::Left, true) } else if x > 0.75 { (Placement::Right, true) }
        else if split.is_none() && y < 0.25 { (Placement::Above, false) }
        else if split.is_none() && y > 0.75 { (Placement::Below, false) } else { return None; };
        let before = matches!(placement, Placement::Left | Placement::Above);
        let half = if horizontal { frame.width / 2. } else { frame.height / 2. };
        let gap = crate::split_layout::GAP / 2.;
        let cover = engine::Bounds {
            x: frame.x + if horizontal && !before { half-gap } else { 0. },
            y: frame.y + if !horizontal && !before { half-gap } else { 0. },
            width: if horizontal { half+gap } else { frame.width },
            height: if horizontal { frame.height } else { half+gap },
        };
        let outline = engine::Bounds {
            x: frame.x + if horizontal && !before { half+gap } else { 0. },
            y: frame.y + if !horizontal && !before { half+gap } else { 0. },
            width: if horizontal { half-gap } else { frame.width },
            height: if horizontal { frame.height } else { half-gap },
        };
        let convert = |r: engine::Bounds| Bounds::new(gpui::point(px(r.x as f32),px(r.y as f32)),gpui::size(px(r.width as f32),px(r.height as f32)));
        Some(SplitDropPreview { source: drag.id, anchor: split.map_or(anchor, |s| s.panes[index]), placement,
            cover: convert(cover), outline: convert(outline), overlay: None, bounds: window.bounds() })
    }

    pub(crate) fn update_split_drop(&mut self, drag: &TabDrag, position: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let Some(mut next) = self.split_drop_target(drag, position, window) else { self.close_split_drop(cx); return; };
        if self.split_drop.as_ref().is_some_and(|old| old.source == next.source && old.anchor == next.anchor
            && old.placement == next.placement && old.outline == next.outline && old.bounds == next.bounds) { return; }
        next.overlay = self.split_drop.as_mut().and_then(|old| old.overlay.take());
        self.split_drop = Some(next);
        self.split_drop_generation += 1;
        self.ensure_split_drop_overlay(cx);
        cx.notify();
    }

    fn ensure_split_drop_overlay(&self, cx: &mut Context<Self>) {
        let Some(preview) = &self.split_drop else { return; };
        if preview.overlay.is_some() { return; }
        let bounds = preview.bounds;
        let generation = self.split_drop_generation;
        let browser = cx.entity().downgrade();
        let parent = self.native_view;
        cx.defer(move |cx| {
            if !browser.read_with(cx, |this, _| this.split_drop.as_ref().is_some_and(|p| p.overlay.is_none() && this.split_drop_generation == generation)).unwrap_or(false) { return; }
            let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None, focus: false,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default() };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                    crate::native::set_overlay_passthrough(child);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::SplitDrop, cx))
            }) {
                let _ = browser.update(cx, |this, _| { if let Some(p) = &mut this.split_drop { p.overlay = Some(overlay); } });
            }
        });
    }

    pub(crate) fn close_split_drop(&mut self, cx: &mut Context<Self>) {
        self.split_drop_generation += 1;
        if let Some(preview) = self.split_drop.take() {
            if let Some(overlay) = preview.overlay {
                cx.defer(move |cx| { let _ = overlay.update(cx, |_, window, _| window.remove_window()); });
            }
            cx.notify();
        }
    }

    pub(crate) fn drop_tab_into_split(&mut self, drag: &TabDrag, position: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let target = self.split_drop_target(drag, position, window);
        self.close_split_drop(cx);
        let Some(target) = target else { return; };
        let now = SystemTime::now();
        let protected = |this: &Self, id| this.browser.is_favorite(id) || this.browser.spaces.iter().any(|s| s.pinned.iter().any(|t| t.id == id));
        let anchor = if protected(self, target.anchor) { self.edit_browser(|b| b.copy_item_to_today(target.anchor, now)).unwrap() } else { target.anchor };
        let other = if protected(self, drag.id) { self.edit_browser(|b| b.copy_item_to_today(drag.id, now)).unwrap() } else { drag.id };
        self.edit_browser(|b| b.add_split(anchor, other, target.placement, now)).expect("validated drag split");
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, self.page_content_visible()); }
        cx.notify();
    }

    pub(crate) fn render_split_drop(&self, window: &Window) -> AnyElement {
        let Some(preview) = &self.split_drop else { return div().into_any_element(); };
        div().size_full()
            .child(div().absolute().left(preview.cover.origin.x).top(preview.cover.origin.y)
                .w(preview.cover.size.width).h(preview.cover.size.height)
                .child(self.theme_region(preview.cover, window.viewport_size())))
            .child(div().debug_selector(|| "split-drop-outline".into()).absolute()
                .left(preview.outline.origin.x).top(preview.outline.origin.y).w(preview.outline.size.width).h(preview.outline.size.height)
                .rounded(px(crate::pages::CONTENT_CORNER_RADIUS)).border_2().border_color(gpui::hsla(0.,0.,1.,0.55)))
            .into_any_element()
    }
}
