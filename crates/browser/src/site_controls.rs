//! The site controls panel opened from the address field, as the
//! reference's Site Control Center: page actions, extensions and the
//! connection's security, drawn over the page in a child window.
use crate::icons::Icon;
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use gpui::{
    actions, div, hsla, point, prelude::*, px, AnyElement, App, Bounds, Context, FocusHandle,
    KeyBinding, Pixels, Point, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions,
};
use std::{cell::Cell, rc::Rc};

const CONTEXT: &str = "SiteControls";
const PADDING: f32 = 9.0;
const TILE_WIDTH: f32 = 47.0;
const TILE_HEIGHT: f32 = 30.0;
const TILE_GAP: f32 = 9.0;
const COLUMNS: f32 = 4.0;
pub(crate) const WIDTH: f32 = PADDING * 2. + TILE_WIDTH * COLUMNS + TILE_GAP * (COLUMNS - 1.);
/// Space between the address field's button and the panel.
const ANCHOR_GAP: f32 = 14.0;
actions!(site_controls, [Dismiss]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", Dismiss, Some(CONTEXT))]);
}

pub(crate) struct SiteControls {
    position: Point<Pixels>,
    focus: FocusHandle,
    pub overlay: Option<WindowHandle<Overlay>>,
}

/// Where the address field's site controls button was last painted, in
/// its own window's coordinates.
pub(crate) type Anchor = Rc<Cell<Option<Bounds<Pixels>>>>;

impl BrowserWindow {
    pub(crate) fn toggle_site_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.site_controls.is_some() { self.close_site_controls(cx); return; }
        if self.browser.active_tab().is_none() { return; }
        self.close_user_profile_dialog(cx);
        self.close_tab_menu(cx);
        self.close_command_bar(cx);
        let anchor = self.site_controls_anchor.get().unwrap_or_default();
        let focus = cx.focus_handle();
        let origin = window.bounds().origin;
        let at = point(anchor.center().x - px(WIDTH / 2.), anchor.bottom() + px(ANCHOR_GAP));
        self.site_controls = Some(SiteControls { position: at, focus: focus.clone(), overlay: None });
        let main = self.main_window;
        let parent = self.native_view;
        let browser = cx.entity().downgrade();
        cx.notify();
        cx.defer(move |cx| {
            if !browser.read_with(cx, |this, _| this.site_controls.as_ref().is_some_and(|panel| panel.focus == focus)).unwrap_or(false) { return; }
            let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else { return; };
            let _ = browser.update(cx, |this, _| {
                if let Some(panel) = &mut this.site_controls {
                    let at = at + (origin - bounds.origin);
                    panel.position = point(at.x.clamp(px(8.), (bounds.size.width - px(WIDTH + 8.)).max(px(8.))), at.y.max(px(8.)));
                }
            });
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                focus.focus(window);
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) { crate::native::attach_overlay(parent, child, false); }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::SiteControls, cx))
            }) {
                let _ = browser.update(cx, |this, _| { if let Some(panel) = &mut this.site_controls { panel.overlay = Some(overlay); } });
            }
        });
    }

    pub(crate) fn close_site_controls(&mut self, cx: &mut Context<Self>) {
        self.close_site_controls_with_focus(crate::window::PopupFocus::Restore, cx);
    }

    pub(crate) fn close_site_controls_with_focus(&mut self, focus: crate::window::PopupFocus, cx: &mut Context<Self>) {
        let Some(panel) = self.site_controls.take() else { return; };
        self.close_popup_overlay(panel.overlay, focus, false, cx);
        cx.notify();
    }

    pub(crate) fn render_site_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(panel) = &self.site_controls else { return div().into_any_element(); };
        let secure = self.browser.active_tab().is_some_and(|tab| tab.url.starts_with("https://"));
        let tile = |id: &'static str| div().id(id).debug_selector(move || id.into())
            .flex_none().w(px(TILE_WIDTH)).h(px(TILE_HEIGHT)).rounded(px(8.)).flex().items_center().justify_center()
            .bg(hsla(0., 0., 1., 0.1)).hover(|style| style.bg(hsla(0., 0., 1., 0.18)));
        let heading = |text: &'static str| div().pt(px(12.)).pb(px(8.)).text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD).child(text);
        div().id("site-controls-backdrop").debug_selector(|| "site-controls-backdrop".into()).size_full()
            .key_context(CONTEXT).track_focus(&panel.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_site_controls(cx)))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.close_site_controls(cx)))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, _, _, cx| this.close_site_controls(cx)))
            .child(crate::panel::shell("site-controls")
                .absolute().left(panel.position.x).top(panel.position.y).w(px(WIDTH)).p(px(PADDING)).pt(px(10.))
                .flex().flex_col()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(div().flex().gap(px(TILE_GAP))
                    .child(tile("site-share").on_click(cx.listener(|this, _, _, cx| { this.close_site_controls(cx); this.copy_url(false, cx); }))
                        .child(crate::icons::control(Icon::Share)))
                    .child(tile("site-boost").child(crate::icons::control(Icon::Boost)))
                    .child(tile("site-capture").child(crate::icons::control(Icon::Capture)))
                    .child(tile("site-screenshot").child(crate::icons::control(Icon::Screenshot))))
                .child(heading("Extensions"))
                .child(div().flex().flex_wrap().gap(px(TILE_GAP))
                    .child(tile("site-add-extension").text_color(hsla(0., 0., 1., 0.7)).child(crate::icons::control(Icon::Plus))))
                .child(div().mt(px(14.)).flex().items_center().justify_between()
                    .child(div().id("site-security").debug_selector(|| "site-security".into())
                        .h(px(TILE_HEIGHT)).px(px(9.)).rounded(px(8.)).flex().items_center().gap(px(5.))
                        .bg(hsla(0., 0., 1., 0.1)).text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(crate::icons::control_sized(if secure { Icon::Lock } else { Icon::Unlock }, 14.))
                        .child(if secure { "Secure" } else { "Not Secure" }))
                    .child(div().id("site-more").debug_selector(|| "site-more".into())
                        .size(px(TILE_HEIGHT)).rounded_full().flex().items_center().justify_center()
                        .bg(hsla(0., 0., 1., 0.1)).hover(|style| style.bg(hsla(0., 0., 1., 0.18)))
                        .child(crate::icons::more()))))
            .into_any_element()
    }
}
