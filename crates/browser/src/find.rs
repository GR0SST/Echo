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
        let input = cx.new(|cx| TextInput::new("Find in Page…", cx));
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
        let width = px(420.0).min(main.size.width - px(32.0));
        let bounds = Bounds::new(
            point(
                main.origin.x + main.size.width - width - px(16.0),
                main.origin.y + px(16.0),
            ),
            size(width, px(48.0)),
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
                input.read(cx).focus_handle(cx).focus(window);
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::Find, cx))
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
        let Some(bar) = self.find_bar.take() else {
            return;
        };
        if let Some(pages) = &self.pages {
            pages.borrow_mut().stop_finding(bar.tab);
        }
        let main = self.main_window;
        let focus = self.focus_handle.clone();
        let pages = self.pages.clone();
        cx.defer(move |cx| {
            if let Some(overlay) = bar.overlay {
                let _ = overlay.update(cx, |_, window, _| window.remove_window());
            }
            let _ = main.update(cx, |_, window, _| {
                window.activate_window();
                focus.focus(window);
            });
            if let Some(pages) = pages {
                pages.borrow_mut().focus_shown();
            }
        });
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
        let status = if bar.input.read(cx).text().is_empty() {
            String::new()
        } else if bar.searching {
            "Searching…".into()
        } else {
            match bar.count {
                Some(0) => "No results found".into(),
                Some(count) if bar.active > 0 => format!("{} of {count}", bar.active),
                    Some(count) => format!("{count} matches"),
                None => String::new(),
            }
        };
        div()
            .id("find-bar")
            .debug_selector(|| "find-bar".into())
            .key_context(CONTEXT)
            .size_full()
            .rounded_lg()
            .bg(gpui::white())
            .border_1()
            .border_color(hsla(0.0, 0.0, 0.0, 0.15))
            .text_color(hsla(0.0, 0.0, 0.1, 1.0))
            .text_sm()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .on_action(cx.listener(|this, _: &NextMatch, _, cx| this.find_next(true, cx)))
            .on_action(cx.listener(|this, _: &PreviousMatch, _, cx| this.find_next(false, cx)))
            .on_action(cx.listener(|this, _: &CloseFind, _, cx| this.close_find(cx)))
            .on_action(cx.listener(|this, _: &FindPage, window, cx| this.open_find(window, cx)))
            .child(div().flex_1().min_w_0().child(bar.input.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(hsla(0.0, 0.0, 0.4, 1.0))
                    .child(status),
            )
            .child(
                div()
                    .id("find-previous")
                    .debug_selector(|| "find-previous".into())
                    .px_1()
                    .rounded_md()
                    .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.06)))
                    .on_click(cx.listener(|this, _, _, cx| this.find_next(false, cx)))
                    .child("↑"),
            )
            .child(
                div()
                    .id("find-next")
                    .debug_selector(|| "find-next".into())
                    .px_1()
                    .rounded_md()
                    .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.06)))
                    .on_click(cx.listener(|this, _, _, cx| this.find_next(true, cx)))
                    .child("↓"),
            )
            .child(
                div()
                    .id("find-close")
                    .debug_selector(|| "find-close".into())
                    .px_1()
                    .rounded_md()
                    .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.06)))
                    .on_click(cx.listener(|this, _, _, cx| this.close_find(cx)))
                    .child("×"),
            )
            .into_any_element()
    }
}
