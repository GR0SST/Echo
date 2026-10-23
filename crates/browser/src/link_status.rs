//! Passive link targets above native page surfaces.
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use browser_core::TabId;
use engine::ViewId;
use gpui::{div, hsla, prelude::*, px, AnyElement, Bounds, Context, Pixels, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions};
use std::time::Duration;

pub(crate) struct LinkStatus {
    pub tab: TabId,
    pub text: String,
    pub expanded: bool,
    pub overlay: Option<WindowHandle<Overlay>>,
    bounds: Bounds<Pixels>,
}

impl BrowserWindow {
    pub(crate) fn update_link_status(&mut self, view: ViewId, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.pages.as_ref().and_then(|pages| {
            let pages = pages.borrow();
            pages.tab_for(view).filter(|tab| pages.is_visible(*tab))
        }) else { return; };
        if text.is_empty() {
            if self.link_status.as_ref().is_some_and(|status| status.tab == tab) { self.close_link_status(cx); }
            return;
        }
        if self.command_bar.is_some() || self.tab_menu.is_some() || self.site_controls.is_some() || self.new_menu.is_some() { return; }
        if self.link_status.as_ref().is_some_and(|status| status.tab == tab && status.text == text) { return; }
        self.link_status_generation += 1;
        let generation = self.link_status_generation;
        let overlay = self.link_status.as_mut().and_then(|status| status.overlay.take());
        self.link_status = Some(LinkStatus { tab, text, expanded: false, overlay, bounds: window.bounds() });
        self.ensure_link_status_overlay(cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            let _ = this.update(cx, |this, cx| {
                if this.link_status_generation == generation {
                    if let Some(status) = &mut this.link_status { status.expanded = true; }
                    cx.notify();
                }
            });
        }).detach();
        cx.notify();
    }

    fn ensure_link_status_overlay(&self, cx: &mut Context<Self>) {
        let Some(status) = &self.link_status else { return; };
        if status.overlay.is_some() { return; }
        let bounds = status.bounds;
        let generation = self.link_status_generation;
        let browser = cx.entity().downgrade();
        let parent = self.native_view;
        cx.defer(move |cx| {
            if !browser.read_with(cx, |this, _| this.link_status.as_ref().is_some_and(|status|
                status.overlay.is_none() && status.bounds == bounds && this.link_status_generation == generation)).unwrap_or(false) { return; }
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None, focus: false,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                    crate::native::set_overlay_passthrough(child);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::LinkStatus, cx))
            }) {
                let _ = browser.update(cx, |this, _| {
                    if let Some(status) = &mut this.link_status { status.overlay = Some(overlay); }
                });
            }
        });
    }

    pub(crate) fn close_link_status(&mut self, cx: &mut Context<Self>) {
        self.link_status_generation += 1;
        if let Some(status) = self.link_status.take() {
            if let Some(overlay) = status.overlay {
                cx.defer(move |cx| { let _ = overlay.update(cx, |_, window, _| window.remove_window()); });
            }
            cx.notify();
        }
    }

    pub(crate) fn sync_link_status(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(status) = &self.link_status else { return; };
        let valid = if let Some(pages) = &self.pages {
            let Ok(pages) = pages.try_borrow() else { return; };
            pages.is_visible(status.tab) && pages.state(status.tab).status == status.text
        } else { false };
        if !valid { self.close_link_status(cx); return; }
        let bounds = window.bounds();
        if status.bounds.size != bounds.size {
            let status = self.link_status.as_mut().unwrap();
            if let Some(overlay) = status.overlay.take() {
                cx.defer(move |cx| { let _ = overlay.update(cx, |_, window, _| window.remove_window()); });
            }
            status.bounds = bounds;
            self.ensure_link_status_overlay(cx);
        }
    }

    pub(crate) fn render_link_status(&self, window: &Window) -> AnyElement {
        let Some(status) = &self.link_status else { return div().into_any_element(); };
        let Some(frame) = self.pages.as_ref().and_then(|pages| pages.borrow().surface_layout().into_iter()
            .find_map(|(tab, frame)| (tab == status.tab).then_some(frame))) else { return div().into_any_element(); };
        let available = (frame.width - 16.).max(0.) as f32;
        let width = if status.expanded { available } else { available.min(360.) };
        let text = crate::status_url::elide(&status.text, (width - 16.).max(0.), |text| {
            f32::from(window.text_system().shape_line(text.to_owned().into(), px(12.), &[gpui::TextRun {
                len: text.len(), font: gpui::font(".SystemUIFont"), color: gpui::black(),
                background_color: None, underline: None, strikethrough: None,
            }], None).width)
        });
        div().size_full().child(div().id("link-status").debug_selector(|| "link-status".into())
            .absolute().left(px(frame.x as f32 + 8.)).top(px((frame.y + frame.height - 32.).max(frame.y) as f32))
            .max_w(px(width)).h(px(24.)).px_2().flex().items_center().rounded(px(6.))
            .bg(hsla(self.browser.space().hue / 360., 0.28, 0.84, 0.98))
            .text_color(hsla(0., 0., 0.1, 0.75)).font_family(".SystemUIFont").text_size(px(12.))
            .child(div().min_w_0().truncate().child(text))).into_any_element()
    }
}
