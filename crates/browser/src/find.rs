//! A small GPUI child window above the page, leaving the page interactive.
use crate::text_input::{Changed, TextInput};
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use browser_core::TabId;
use gpui::{
    actions, div, hsla, point, prelude::*, px, size, AnyElement, App, Bounds, Context, Entity,
    Focusable, KeyBinding, Subscription, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions,
};

actions!(find, [FindPage, NextMatch, PreviousMatch, CloseFind]);
const CONTEXT: &str = "FindInPage";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", NextMatch, Some(CONTEXT)),
        KeyBinding::new("shift-enter", PreviousMatch, Some(CONTEXT)),
        KeyBinding::new("cmd-g", NextMatch, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-g", PreviousMatch, Some(CONTEXT)),
        KeyBinding::new("escape", CloseFind, Some(CONTEXT)),
        KeyBinding::new("cmd-f", FindPage, Some(CONTEXT)),
    ]);
}

pub(crate) struct FindBar {
    pub input: Entity<TextInput>,
    pub tab: TabId,
    pub overlay: Option<WindowHandle<Overlay>>,
    pub count: Option<i32>,
    pub active: i32,
    pub searching: bool,
    _changed: Subscription,
}

impl BrowserWindow {
    pub(crate) fn find_from_command(&mut self, cx: &mut Context<Self>) {
        self.close_command_bar(cx);
        let browser = cx.entity().downgrade();
        let main = self.main_window;
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = browser.update(cx, |this, cx| this.open_find(window, cx));
            });
        });
    }

    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_library_popup_focus(window, cx);
        self.close_user_profile_dialog(cx);
        self.close_tab_menu(cx);
        if self.focus_history_search(window, cx) { return; }
        if let Some(bar) = &self.find_bar {
            let input = bar.input.clone();
            let text = input.read(cx).text().to_string();
            input.update(cx, |input, cx| input.set_text(text, cx));
            if let Some(overlay) = bar.overlay {
                cx.defer(move |cx| {
                    let _ = overlay.update(cx, |_, window, cx| {
                        window.activate_window();
                        input.read(cx).focus_handle(cx).focus(window);
                    });
                });
            }
            return;
        }
        let Some(tab) = self.browser.space().active_tab else {
            return;
        };
        self.close_command_bar(cx);
        let input = cx.new(|cx| TextInput::new("Find in page...", cx).with_placeholder_opacity(0.6875));
        let changed = cx.subscribe(&input, |this, _, _: &Changed, cx| {
            this.search_page(false, true, cx)
        });
        self.find_bar = Some(FindBar {
            input: input.clone(),
            tab,
            overlay: None,
            count: None,
            active: 0,
            searching: false,
            _changed: changed,
        });
        let main = window.bounds();
        let width = px(300.0).min(main.size.width - px(40.0));
        let bounds = Bounds::new(
            point(
                main.origin.x + main.size.width - width - px(40.0),
                main.origin.y,
            ),
            size(width + px(40.0), px(72.0)),
        );
        let browser = cx.entity().downgrade();
        let parent = self.native_view;
        cx.defer(move |cx| {
            let wanted = browser
                .read_with(cx, |this, _| {
                    this.find_bar.as_ref().is_some_and(|bar| bar.input == input)
                })
                .unwrap_or(false);
            if !wanted {
                return;
            }
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: WindowKind::Normal,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    // A borderless padded HUD must not expose AppKit's frame rim.
                    // Attach afterwards: Find paints its own bounded shadow.
                    crate::native::prepare_toast_window(child);
                    crate::native::attach_overlay(parent, child, false);
                }
                input.read(cx).focus_handle(cx).focus(window);
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::Find, window, cx))
            }) {
                let _ = browser.update(cx, |this, _| {
                    if let Some(bar) = &mut this.find_bar {
                        bar.overlay = Some(overlay);
                    }
                });
            }
        });
        cx.notify();
    }

    pub(crate) fn close_find(&mut self, cx: &mut Context<Self>) {
        self.close_find_with_focus(crate::window::PopupFocus::Restore, cx);
    }

    pub(crate) fn close_find_with_focus(&mut self, focus: crate::window::PopupFocus, cx: &mut Context<Self>) {
        let Some(bar) = self.find_bar.take() else { return; };
        if let Some(pages) = &self.pages {
            pages.borrow_mut().stop_finding(bar.tab);
        }
        self.close_popup_overlay(bar.overlay, focus, false, cx);
        cx.notify();
    }

    pub(crate) fn find_next(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.search_page(true, forward, cx);
    }

    fn search_page(&mut self, next: bool, forward: bool, cx: &mut Context<Self>) {
        let Some(bar) = &mut self.find_bar else {
            return;
        };
        let text = bar.input.read(cx).text();
        if !next {
            bar.count = None;
            bar.active = 0;
        }
        bar.searching = !text.is_empty();
        if let Some(pages) = &self.pages {
            if text.is_empty() {
                pages.borrow_mut().stop_finding(bar.tab);
            } else {
                pages.borrow_mut().find(bar.tab, text, forward, next);
            }
        }
        cx.notify();
    }

    pub(crate) fn render_find_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(bar) = &self.find_bar else {
            return div().into_any_element();
        };
        let palette = crate::material::DialogPalette::new(self.theme_is_dark());
        let ink = if palette.dark { 1. } else { 0. };
        let has_query = !bar.input.read(cx).text().is_empty();
        let not_found = has_query && bar.count == Some(0) && !bar.searching;
        let status = match (bar.count, bar.active) {
            (Some(count), active) => format!("{}/{count}", active.max(0)),
            _ => "0/0".into(),
        };
        let control = |id: &'static str| div().id(id).debug_selector(move || id.into())
            .flex_none().w(px(26.)).h(px(30.)).flex().items_center().justify_center()
            .rounded(px(4.)).hover(|s| s.bg(hsla(0., 0., ink, 0.08)));
        div().size_full().p(px(20.)).child(div()
            .id("find-bar")
            .debug_selector(|| "find-bar".into())
            .key_context(CONTEXT)
            .size_full()
            .rounded(px(6.))
            .bg(if palette.dark { gpui::rgb(0x131c32).into() } else { palette.surface })
            .shadow(vec![gpui::BoxShadow {
                color: if palette.dark { gpui::rgba(0x131c32c9).into() } else { hsla(0., 0., 0., 0.22) }, offset: point(px(0.), px(3.)),
                blur_radius: px(4.8), spread_radius: px(0.),
            }])
            .text_color(hsla(0., 0., ink, 0.8))
            .text_size(px(13.)).line_height(px(16.)).font_weight(gpui::FontWeight(550.))
            .flex().items_center().pl(px(10.)).pr(px(3.))
            .on_action(cx.listener(|this, _: &NextMatch, _, cx| this.find_next(true, cx)))
            .on_action(cx.listener(|this, _: &PreviousMatch, _, cx| this.find_next(false, cx)))
            .on_action(cx.listener(|this, _: &CloseFind, _, cx| this.close_find(cx)))
            .on_action(cx.listener(|this, _: &FindPage, window, cx| this.open_find(window, cx)))
            .child(div().flex_none().mr(px(7.)).text_color(hsla(0., 0., ink, 0.55))
                .child(find_glyph(crate::icons::Icon::Search)))
            .child(div().flex_1().min_w_0().child(bar.input.clone()))
            .when(has_query && !not_found, |row| row
                .child(div().id("find-status").debug_selector(|| "find-status".into())
                    .w(px(25.)).flex_none().text_center().text_size(px(12.)).child(status))
                .child(control("find-previous")
                    .on_click(cx.listener(|this, _, _, cx| this.find_next(false, cx)))
                    .child(find_glyph(crate::icons::Icon::Up)))
                .child(control("find-next")
                    .on_click(cx.listener(|this, _, _, cx| this.find_next(true, cx)))
                    .child(find_glyph(crate::icons::Icon::Down))))
            .when(not_found, |row| row.child(div().id("find-not-found")
                .debug_selector(|| "find-not-found".into()).w(px(77.)).flex_none()
                .text_right().text_size(px(10.)).font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(gpui::rgb(0xdb1929)).child("NOT FOUND")))
            .child(control("find-close").ml(px(6.))
                .on_click(cx.listener(|this, _, _, cx| this.close_find(cx)))
                .child(find_glyph(crate::icons::Icon::Close))))
            .into_any_element()
    }
}

// The compact reference controls have a heavier stroke than a scaled toolbar
// icon. Keep the 14 px geometry independent of fonts and toolbar dimensions.
fn find_glyph(icon: crate::icons::Icon) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        use crate::icons::Icon;
        let color = window.text_style().color;
        let at = |x, y| bounds.origin + point(px(x), px(y));
        let lines: Vec<Vec<(f32, f32)>> = match icon {
            Icon::Up => vec![vec![(7., 12.), (7., 2.)], vec![(3., 6.), (7., 2.), (11., 6.)]],
            Icon::Down => vec![vec![(7., 2.), (7., 12.)], vec![(3., 8.), (7., 12.), (11., 8.)]],
            Icon::Close => vec![vec![(3., 3.), (11., 11.)], vec![(3., 11.), (11., 3.)]],
            Icon::Search => vec![vec![(9.5, 9.5), (12.5, 12.5)]],
            _ => unreachable!("Find uses only search, arrows and close"),
        };
        let stroke = if matches!(icon, Icon::Search) { 1.3 } else { 1.5 };
        if matches!(icon, Icon::Search) {
            let mut circle = gpui::PathBuilder::stroke(px(stroke));
            for index in 0..=48 {
                let angle = index as f32 / 48. * std::f32::consts::TAU;
                let p = at(6. + 4.5 * angle.cos(), 6. + 4.5 * angle.sin());
                if index == 0 { circle.move_to(p); } else { circle.line_to(p); }
            }
            if let Ok(path) = circle.build() { window.paint_path(path, color); }
        }
        for points in lines {
            let mut line = gpui::PathBuilder::stroke(px(stroke));
            line.move_to(at(points[0].0, points[0].1));
            for &(x, y) in &points[1..] { line.line_to(at(x, y)); }
            if let Ok(path) = line.build() { window.paint_path(path, color); }
            for (x, y) in points {
                let radius = px(stroke / 2.);
                let mut dot = gpui::fill(Bounds::new(at(x, y) - point(radius, radius), size(radius * 2., radius * 2.)), color);
                dot.corner_radii = radius.into();
                window.paint_quad(dot);
            }
        }
    }).size(px(14.)).flex_none().into_any_element()
}
