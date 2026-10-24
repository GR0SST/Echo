//! Passive, short-lived notifications above native page views.
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use gpui::{
    div, hsla, point, prelude::*, px, size, AnyElement, Bounds, ClipboardItem, Context,
    FontWeight, TextRun, linear_gradient, linear_color_stop, rgb,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::time::{Duration, Instant};

pub(crate) struct Toast {
    pub message: String,
    pub overlay: Option<WindowHandle<Overlay>>,
    pub(crate) copied_link: bool,
    undo_hint: bool,
    generation: u64,
    started: Instant,
    dismissed: Option<(Instant, f32)>,
    motion: Option<gpui::Task<()>>,
}

// Arc 1.170.0, measured from arc-copy.mov (variable-frame-rate PTS):
// spring ζ=0.694, ω=15.42; dismissal starts 4.986 s after presentation
// and shrinks/fades about the top center over 200 ms.
pub(crate) const TOAST_LIFETIME: Duration = Duration::from_secs(5);
const ENTER_SETTLED: Duration = Duration::from_millis(650);
const EXIT: Duration = Duration::from_millis(200);
const FRAME: Duration = Duration::from_millis(16);
const PAD_X: f32 = 12.;
const PAD_Y: f32 = 8.;

impl Toast {
    fn progress(&self, now: Instant, reduced: bool) -> (f32, f32, bool) {
        let elapsed = now.saturating_duration_since(self.started);
        let exit = self.dismissed.map(|(at, from)| (now.saturating_duration_since(at), from))
            .or_else(|| (elapsed >= TOAST_LIFETIME).then(|| (elapsed - TOAST_LIFETIME, 1.)));
        if let Some((elapsed, from)) = exit {
            if reduced || elapsed >= EXIT { return (0., 0., true); }
            let t = (elapsed.as_secs_f32() / EXIT.as_secs_f32()).clamp(0., 1.);
            let remaining = 1. - t * t * (3. - 2. * t);
            return (from * remaining, remaining, false);
        }
        if reduced || elapsed >= ENTER_SETTLED { return (1., 1., false); }
        let t = elapsed.as_secs_f32();
        let damping: f32 = 0.694;
        let frequency: f32 = 15.42;
        let damped = (1. - damping * damping).sqrt();
        let scale = 1. - (-damping * frequency * t).exp()
            * ((frequency * damped * t).cos() + damping / damped * (frequency * damped * t).sin());
        (scale.max(0.), scale.clamp(0., 1.), false)
    }
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
        self.retire_toast(cx);
        self.toast_generation += 1;
        let generation = self.toast_generation;
        self.toast = Some(Toast {
            copied_link: message.starts_with("Copied"),
            undo_hint: message == "Cleared Tabs! Use ⌘Z to undo.",
            message,
            overlay: None,
            generation,
            started: cx.background_executor().now(),
            dismissed: None,
            motion: None,
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
                    main.origin.x + main.size.width - width - px(if copied_link { 16. } else { 4. }) - px(PAD_X),
                    main.origin.y + px(if copied_link { 19. } else { 4. }) - px(PAD_Y),
                ),
                size(width + px(PAD_X * 2.), px(if copied_link || undo_hint { 48. } else { 42. }) + px(PAD_Y * 2.)),
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
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::Toast(generation), cx))
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
        self.drive_toast(generation, cx);
        cx.notify();
    }

    fn toast_by_generation(&self, generation: u64) -> Option<&Toast> {
        self.toast.as_ref().filter(|toast| toast.generation == generation)
            .or_else(|| self.retiring_toasts.iter().find(|toast| toast.generation == generation))
    }

    fn drive_toast(&mut self, generation: u64, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| loop {
            let now = cx.background_executor().now();
            let next = this.update(cx, |this, cx| {
                let toast = this.toast_by_generation(generation)?;
                let reduced = crate::native::prefers_reduced_motion();
                let elapsed = now.saturating_duration_since(toast.started);
                if toast.progress(now, reduced).2 {
                    this.remove_toast(generation, cx);
                    return None;
                }
                let next = if toast.dismissed.is_some() || elapsed < ENTER_SETTLED || elapsed >= TOAST_LIFETIME {
                    FRAME
                } else { TOAST_LIFETIME - elapsed };
                Some((next, toast.overlay))
            }).ok().flatten();
            let Some((next, overlay)) = next else { break; };
            if let Some(overlay) = overlay {
                let _ = overlay.update(cx, |_, _, cx| cx.notify());
            }
            cx.background_executor().timer(next).await;
        });
        if let Some(toast) = self.toast.as_mut().filter(|toast| toast.generation == generation) {
            toast.motion = Some(task);
        } else if let Some(toast) = self.retiring_toasts.iter_mut().find(|toast| toast.generation == generation) {
            toast.motion = Some(task);
        }
    }

    fn retire_toast(&mut self, cx: &mut Context<Self>) {
        let Some(mut toast) = self.toast.take() else { return; };
        let now = cx.background_executor().now();
        let from = toast.progress(now, crate::native::prefers_reduced_motion()).0;
        toast.motion.take();
        toast.dismissed = Some((now, from));
        let generation = toast.generation;
        self.retiring_toasts.push(toast);
        self.drive_toast(generation, cx);
    }

    fn remove_toast(&mut self, generation: u64, cx: &mut Context<Self>) {
        let toast = if self.toast.as_ref().is_some_and(|toast| toast.generation == generation) {
            self.toast.take()
        } else {
            self.retiring_toasts.iter().position(|toast| toast.generation == generation)
                .map(|index| self.retiring_toasts.remove(index))
        };
        if let Some(overlay) = toast.and_then(|toast| toast.overlay) {
            cx.defer(move |cx| { let _ = overlay.update(cx, |_, window, _| window.remove_window()); });
        }
        cx.notify();
    }

    pub(crate) fn render_toast(&self, generation: u64, width: f32, now: Instant) -> AnyElement {
        let Some(toast) = self.toast_by_generation(generation) else {
            return div().into_any_element();
        };
        let message = toast.message.clone();
        let copied_link = toast.copied_link;
        let undo_hint = toast.undo_hint;
        let (scale, opacity, _) = toast.progress(now, crate::native::prefers_reduced_motion());
        if scale < 0.001 { return div().size_full().into_any_element(); }
        let surface = div()
            .id("toast")
            .debug_selector(|| "toast".into())
            .w(px((width - PAD_X * 2.) * scale))
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
        div().size_full().flex().items_start().justify_center().pt(px(PAD_Y))
            .opacity(opacity).child(surface).into_any_element()
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
