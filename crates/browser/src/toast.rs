//! Passive, short-lived notifications above native page views.
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use gpui::{
    div, hsla, point, prelude::*, px, size, AnyElement, Bounds, ClipboardItem, Context,
    AnimationExt, FontWeight, TextRun, linear_gradient, linear_color_stop, rgb,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::time::{Duration, Instant};

pub(crate) struct Toast {
    pub message: String,
    pub overlay: Option<WindowHandle<Overlay>>,
    pub(crate) copied_link: bool,
    undo_hint: bool,
    started: Instant,
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
            "Copied Current URL"
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
            copied_link: message.starts_with("Copied"),
            undo_hint: message == "Cleared Tabs! Use ⌘Z to undo.",
            message,
            overlay: None,
            started: Instant::now(),
        });
        let main = self.main_window;
        let browser = cx.entity().downgrade();
        let parent = self.native_view;
        let message = self.toast.as_ref().unwrap().message.clone();
        let copied_link = self.toast.as_ref().unwrap().copied_link;
        let undo_hint = self.toast.as_ref().unwrap().undo_hint;
        cx.defer(move |cx| {
            if !browser
                .read_with(cx, |this, _| {
                    this.toast.is_some() && this.toast_generation == generation
                })
                .unwrap_or(false)
            {
                return;
            }
            let Ok((main, text_width)) = main.update(cx, |_, window, _| {
                let mut font = gpui::font(".SystemUIFont");
                font.weight = FontWeight::SEMIBOLD;
                let width = window.text_system().shape_line(message.clone().into(), px(14.), &[TextRun {
                    len: message.len(), font, color: gpui::white(),
                    background_color: None, underline: None, strikethrough: None,
                }], None).width;
                (window.bounds(), width)
            }) else {
                return;
            };
            // Arc's copy toast is content-sized (203 × 48 for the English
            // current-URL message), 16 px from the right and 19 px from the top.
            let width = (text_width.ceil() + px(if copied_link { 68. } else if undo_hint { 41. } else { 24. })).min(main.size.width - px(32.));
            let bounds = Bounds::new(
                point(
                    main.origin.x + main.size.width - width - px(if copied_link { 16. } else { 4. }),
                    main.origin.y + px(if copied_link { 19. } else { 4. }),
                ),
                size(width, px(if copied_link || undo_hint { 48. } else { 42. })),
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
                    crate::native::set_overlay_passthrough(child);
                    crate::native::set_overlay_shadow(child);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::Toast, cx))
            }) {
                let retained = browser.update(cx, |this, _| {
                    if this.toast_generation != generation { return false; }
                    if let Some(toast) = &mut this.toast {
                        toast.overlay = Some(overlay);
                        return true;
                    }
                    false
                }).unwrap_or(false);
                if !retained {
                    let _ = overlay.update(cx, |_, window, _| window.remove_window());
                }
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
        let message = toast.message.clone();
        let copied_link = toast.copied_link;
        let undo_hint = toast.undo_hint;
        let elapsed = toast.started.elapsed();
        let duration = Duration::from_millis(200);
        let surface = move |scale: f32| div()
            .id("toast")
            .debug_selector(|| "toast".into())
            .w(gpui::relative(scale))
            .h(px(if copied_link || undo_hint { 48. } else { 42. } * scale))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8. * scale))
            .px(px(12. * scale))
            .rounded(px(12. * scale))
            .border_1()
            .border_color(hsla(0.76, 0.5, 0.4, 0.25))
            .bg(linear_gradient(180.,
                linear_color_stop(rgb(0x28154e), 0.),
                linear_color_stop(rgb(0x201140), 1.)))
            .text_color(gpui::white())
            .font_family(".SystemUIFont")
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(14. * scale))
            .line_height(px(18. * scale))
            .when(!undo_hint, |this| this.child(div().flex_1().min_w_0().truncate().child(message.clone())))
            .when(undo_hint, |this| this.child(div().flex().items_center().gap(px(4. * scale))
                .child("Cleared Tabs! Use")
                .child(div().debug_selector(|| "toast-undo-shortcut".into()).px(px(6. * scale)).h(px(28. * scale))
                    .flex().items_center().rounded(px(4. * scale)).bg(hsla(0., 0., 1., 0.1)).child("⌘Z"))
                .child("to undo.")))
            .when(copied_link, |this| this.child(
                div().w(px(32. * scale)).h(px(28. * scale)).flex_none().rounded(px(6. * scale))
                    .bg(hsla(0., 0., 1., 0.2)).flex().items_center().justify_center()
                    .child(crate::icons::control_sized(crate::icons::Icon::Share, 18. * scale))));
        let root = div().size_full().flex().items_center().justify_center();
        if crate::native::prefers_reduced_motion() || elapsed >= duration {
            return root.child(surface(1.)).into_any_element();
        }
        let started = toast.started;
        root.with_animation(gpui::SharedString::from(format!("toast-enter-{}", self.toast_generation)), gpui::Animation::new(duration - elapsed), move |root, delta| {
            let t = if delta >= 1. { 1. } else { (started.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.) };
            let t = 1. - (1. - t).powi(3);
            root.opacity(t).child(surface(0.75 + 0.25 * t))
        }).into_any_element()
    }

    /// Arc confirms pinning only while its destination is hidden.
    pub(crate) fn toggle_tab_pin(&mut self, id: browser_core::TabId, cx: &mut Context<Self>) {
        if self.browser.is_blank() { return; }
        let before = self.browser.all_tabs().find(|tab| tab.id == id).map(|tab| tab.pinned_url.is_some());
        let Some(before) = before else { return; };
        self.edit_browser(|browser| browser.toggle_pin(id));
        let changed = self.browser.all_tabs().any(|tab| tab.id == id && tab.pinned_url.is_some() != before);
        if changed && !self.sidebar_visible {
            let space = &self.browser.space().name;
            self.show_toast(format!("{} Tab {} “{space}”", if before { "Un-Pinned" } else { "Pinned" }, if before { "from" } else { "to" }), cx);
        }
        cx.notify();
    }
}
