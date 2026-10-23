//! Passive, short-lived notifications above native page views.
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use gpui::{
    div, hsla, point, prelude::*, px, size, AnyElement, Bounds, ClipboardItem, Context,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::time::Duration;

pub(crate) struct Toast {
    pub message: String,
    pub overlay: Option<WindowHandle<Overlay>>,
}

impl BrowserWindow {
    pub(crate) fn copy_url(&mut self, markdown: bool, cx: &mut Context<Self>) {
        let Some(id) = self.browser.space().active_tab else { return; };
        self.copy_tab_url(id, markdown, cx);
    }

    pub(crate) fn copy_tab_url(&mut self, id: browser_core::TabId, markdown: bool, cx: &mut Context<Self>) {
        let Some(tab) = self.browser.all_tabs().find(|tab| tab.id == id) else { return; };
        let clean = browser_core::links::clean_url(&tab.url);
        let message = if markdown {
            "Copied URL as Markdown"
        } else if clean != tab.url {
            "Copied a clean link without trackers"
        } else {
            "Copied URL"
        };
        let text = if markdown {
            browser_core::links::markdown_link(&tab.title, &tab.url)
        } else {
            clean
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.show_toast(message.into(), cx);
    }

    pub(crate) fn show_toast(&mut self, message: String, cx: &mut Context<Self>) {
        self.close_toast(cx);
        self.toast_generation += 1;
        let generation = self.toast_generation;
        self.toast = Some(Toast {
            message,
            overlay: None,
        });
        let main = self.main_window;
        let browser = cx.entity().downgrade();
        let parent = self.native_view;
        cx.defer(move |cx| {
            if !browser
                .read_with(cx, |this, _| {
                    this.toast.is_some() && this.toast_generation == generation
                })
                .unwrap_or(false)
            {
                return;
            }
            let Ok(main) = main.update(cx, |_, window, _| window.bounds()) else {
                return;
            };
            let width = px(360.0).min(main.size.width - px(32.0));
            let bounds = Bounds::new(
                point(
                    main.origin.x + (main.size.width - width) / 2.0,
                    main.origin.y + main.size.height - px(68.0),
                ),
                size(width, px(40.0)),
            );
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                focus: false,
                kind: WindowKind::Normal,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::Toast, cx))
            }) {
                let _ = browser.update(cx, |this, _| {
                    if let Some(toast) = &mut this.toast {
                        toast.overlay = Some(overlay);
                    }
                });
            }
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |this, cx| {
                if this.toast_generation == generation {
                    this.close_toast(cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn close_toast(&mut self, cx: &mut Context<Self>) {
        if let Some(toast) = self.toast.take() {
            if let Some(overlay) = toast.overlay {
                cx.defer(move |cx| {
                    let _ = overlay.update(cx, |_, window, _| window.remove_window());
                });
            }
            cx.notify();
        }
    }

    pub(crate) fn render_toast(&self) -> AnyElement {
        let Some(toast) = &self.toast else {
            return div().into_any_element();
        };
        div()
            .id("toast")
            .debug_selector(|| "toast".into())
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .rounded_lg()
            .bg(hsla(0.0, 0.0, 0.12, 0.96))
            .text_color(gpui::white())
            .text_sm()
            .child(crate::icons::control(crate::icons::Icon::Check))
            .child(toast.message.clone())
            .into_any_element()
    }
}
