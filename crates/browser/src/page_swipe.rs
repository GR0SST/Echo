//! Passive history feedback above the native document surface.
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use browser_core::TabId;
use gpui::{
    div, prelude::*, px, AnyElement, Bounds, Context, Pixels, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowOptions,
};
use std::time::{Duration, Instant};
pub(crate) struct PageSwipeFeedback {
    tab: TabId,
    state: engine::PageSwipe,
    ending: Option<(Instant, f32)>,
    pub overlay: Option<WindowHandle<Overlay>>,
    bounds: Bounds<Pixels>,
    generation: u64,
}
impl BrowserWindow {
    pub(crate) fn update_page_swipe(
        &mut self,
        tab: TabId,
        state: Option<engine::PageSwipe>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .pages
            .as_ref()
            .is_some_and(|p| p.borrow().is_visible(tab))
        {
            return;
        }
        if let Some(state) = state {
            if !state.progress.is_finite() {
                return;
            }
            if let Some(old) = self.page_swipe.as_mut().filter(|old| old.tab == tab) {
                old.state = state;
                old.ending = None;
                old.generation += 1;
            } else {
                self.close_page_swipe(cx);
                self.page_swipe = Some(PageSwipeFeedback {
                    tab,
                    state,
                    ending: None,
                    overlay: None,
                    bounds: window.bounds(),
                    generation: 0,
                });
            }
        } else if let Some(old) = self.page_swipe.as_mut().filter(|old| old.tab == tab) {
            if crate::native::prefers_reduced_motion() {
                self.close_page_swipe(cx);
                return;
            }
            old.generation += 1;
            let generation = old.generation;
            old.ending = Some((cx.background_executor().now(), old.state.progress));
            cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                let keep = this.update(cx, |this, cx| {
                    let Some(state) = this
                        .page_swipe
                        .as_ref()
                        .filter(|s| s.generation == generation)
                    else {
                        return false;
                    };
                    if state.ending.is_none() {
                        return false;
                    }
                    if state.ending.is_some_and(|(at, _)| {
                        cx.background_executor().now().saturating_duration_since(at)
                            >= Duration::from_millis(450)
                    }) {
                        this.close_page_swipe(cx);
                        return false;
                    }
                    cx.notify();
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            })
            .detach();
        }
        cx.notify();
    }
    pub(crate) fn close_page_swipe(&mut self, cx: &mut Context<Self>) {
        if let Some(overlay) = self.page_swipe.take().and_then(|s| s.overlay) {
            cx.defer(move |cx| {
                let _ = overlay.update(cx, |_, window, _| window.remove_window());
            });
        }
    }
    pub(crate) fn sync_page_swipe(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(state) = self.page_swipe.as_ref() else {
            return;
        };
        if self.command_bar.is_some()
            || !self
                .pages
                .as_ref()
                .is_some_and(|p| p.borrow().is_visible(state.tab))
        {
            self.close_page_swipe(cx);
            return;
        }
        if state.bounds != window.bounds() {
            self.close_page_swipe(cx);
            return;
        }
        if state.overlay.is_some() {
            return;
        }
        let bounds = window.bounds();
        let parent = self.native_view;
        let browser = cx.entity().downgrade();
        cx.defer(move |cx| {
            if !browser
                .read_with(cx, |this, _| {
                    this.page_swipe
                        .as_ref()
                        .is_some_and(|s| s.overlay.is_none())
                })
                .unwrap_or(false)
            {
                return;
            }
            if let Ok(overlay) = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    focus: false,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                |window, cx| {
                    if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                        crate::native::attach_overlay(parent, child, false);
                        crate::native::set_overlay_passthrough(child);
                    }
                    cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::PageSwipe, window, cx))
                },
            ) {
                let _ = browser.update(cx, |this, cx| {
                    if let Some(state) = this.page_swipe.as_mut() {
                        state.overlay = Some(overlay);
                    }
                    cx.notify();
                });
            }
        });
    }
    pub(crate) fn render_page_swipe(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = self.page_swipe.as_ref() else {
            return div().into_any_element();
        };
        let frame = self.pages.as_ref().and_then(|p| {
            p.borrow()
                .surface_layout()
                .into_iter()
                .find(|(tab, _)| *tab == state.tab)
                .map(|(_, frame)| frame)
        });
        let Some(frame) = frame else {
            return div().into_any_element();
        };
        let progress = state
            .ending
            .map_or(state.state.progress, |(at, from)| {
                crate::window::pager_spring(
                    from,
                    0.,
                    0.,
                    crate::window::PAGER_RETURN_FREQUENCY,
                    cx.background_executor()
                        .now()
                        .saturating_duration_since(at)
                        .as_secs_f32(),
                )
                .0
            })
            .clamp(0., 1.);
        let x = if state.state.back {
            frame.x - 32. + progress as f64 * 56.
        } else {
            frame.x + frame.width - progress as f64 * 56.
        };
        div()
            .absolute()
            .left(px(frame.x as f32))
            .top(px(frame.y as f32))
            .w(px(frame.width as f32))
            .h(px(frame.height as f32))
            .overflow_hidden()
            .child(
                div()
                    .debug_selector(|| "page-swipe-arrow".into())
                    .absolute()
                    .left(px((x - frame.x) as f32))
                    .top(px((frame.height / 2. - 16.) as f32))
                    .size(px(32.))
                    .rounded_full()
                    .shadow_md()
                    .bg(gpui::rgb(0xf2f2f2))
                    .text_color(gpui::rgb(0x333333))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(crate::icons::control_sized(
                        if state.state.back {
                            crate::icons::Icon::Back
                        } else {
                            crate::icons::Icon::Forward
                        },
                        18.,
                    )),
            )
            .into_any_element()
    }
}
