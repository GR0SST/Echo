//! Per-document zoom badges above native page surfaces.
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use browser_core::TabId;
use gpui::{
    div, prelude::*, px, AnyElement, Bounds, Context, Pixels, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
};

pub(crate) struct PageZoom {
    pub overlay: Option<WindowHandle<Overlay>>,
    bounds: Bounds<Pixels>,
}

impl BrowserWindow {
    fn page_zoom_values(&self) -> Vec<(TabId, engine::Bounds, u16)> {
        if !self.page_content_visible()
            || self.command_bar.is_some()
            || self.site_controls.is_some()
        {
            return Vec::new();
        }
        let Some(pages) = &self.pages else {
            return Vec::new();
        };
        let Ok(pages) = pages.try_borrow() else {
            return Vec::new();
        };
        pages
            .surface_layout()
            .into_iter()
            .filter_map(|(tab, frame)| {
                let percent = pages.zoom_percent(tab);
                (percent != 100 && frame.width >= 76. && frame.height >= 58.)
                    .then_some((tab, frame, percent))
            })
            .collect()
    }

    pub(crate) fn sync_page_zoom(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self
            .pages
            .as_ref()
            .is_some_and(|pages| pages.try_borrow().is_err())
        {
            return;
        }
        let observations: Vec<_> = self
            .page_zoom_values()
            .iter()
            .map(|(tab, _, percent)| {
                (
                    self.browser.active_user_profile_id(),
                    *tab,
                    self.browser
                        .all_tabs()
                        .find(|entry| entry.id == *tab)
                        .map(|tab| tab.url.clone())
                        .unwrap_or_default(),
                    *percent,
                )
            })
            .collect();
        if self.page_zoom_refresh || observations != self.page_zoom_observed {
            self.page_zoom_refresh = false;
            self.page_zoom_observed = observations;
            self.page_zoom_generation += 1;
            if let Some(overlay) = self.page_zoom.take().and_then(|state| state.overlay) {
                cx.defer(move |cx| {
                    let _ = overlay.update(cx, |_, window, _| window.remove_window());
                });
            }
            if self.page_zoom_observed.is_empty() {
                return;
            }
            self.page_zoom = Some(PageZoom {
                overlay: None,
                bounds: window.bounds(),
            });
            let generation = self.page_zoom_generation;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(5))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.page_zoom_generation != generation {
                        return;
                    }
                    if let Some(overlay) = this.page_zoom.take().and_then(|state| state.overlay) {
                        cx.defer(move |cx| {
                            let _ = overlay.update(cx, |_, window, _| window.remove_window());
                        });
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        if self.page_zoom.is_none() {
            return;
        }
        let bounds = window.bounds();
        if self
            .page_zoom
            .as_ref()
            .is_some_and(|state| state.bounds == bounds && state.overlay.is_some())
        {
            return;
        }
        if let Some(overlay) = self.page_zoom.take().and_then(|state| state.overlay) {
            cx.defer(move |cx| {
                let _ = overlay.update(cx, |_, window, _| window.remove_window());
            });
        }
        self.page_zoom = Some(PageZoom {
            overlay: None,
            bounds,
        });
        let browser = cx.entity().downgrade();
        let parent = self.native_view;
        cx.defer(move |cx| {
            if !browser
                .read_with(cx, |this, _| {
                    this.page_zoom
                        .as_ref()
                        .is_some_and(|state| state.overlay.is_none() && state.bounds == bounds)
                        && !this.page_zoom_values().is_empty()
                })
                .unwrap_or(false)
            {
                return;
            }
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
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::PageZoom, cx))
            }) {
                let _ = browser.update(cx, |this, _| {
                    if let Some(state) = &mut this.page_zoom {
                        state.overlay = Some(overlay);
                    }
                });
            }
        });
    }

    pub(crate) fn render_page_zoom(&self) -> AnyElement {
        if self.page_zoom.is_none() {
            return div().into_any_element();
        }
        div()
            .size_full()
            .children(
                self.page_zoom_values()
                    .into_iter()
                    .map(|(tab, frame, percent)| {
                        div()
                            .id(gpui::SharedString::from(format!("page-zoom-{tab:?}")))
                            .debug_selector(|| "page-zoom".into())
                            .absolute()
                            .left(px((frame.x + frame.width - 68.) as f32))
                            .top(px(frame.y as f32 + 8.))
                            .w(px(60.))
                            .h(px(42.))
                            .rounded(px(13.))
                            .bg(gpui::linear_gradient(
                                180.,
                                gpui::linear_color_stop(gpui::rgb(0x3e2b65), 0.),
                                gpui::linear_color_stop(gpui::rgb(0x2f214e), 1.),
                            ))
                            .border_1()
                            .border_color(gpui::hsla(0., 0., 0., 0.25))
                            .shadow(vec![
                                gpui::BoxShadow {
                                    color: gpui::hsla(0., 0., 0., 0.17),
                                    offset: gpui::point(px(0.), px(10.)),
                                    blur_radius: px(15.),
                                    spread_radius: px(-3.),
                                },
                                gpui::BoxShadow {
                                    color: gpui::hsla(0., 0., 0., 0.1),
                                    offset: gpui::point(px(0.), px(4.)),
                                    blur_radius: px(6.),
                                    spread_radius: px(-4.),
                                },
                            ])
                            .flex()
                            .items_center()
                            .justify_center()
                            .font_family(".SystemUIFont")
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(gpui::hsla(0., 0., 1., 0.9))
                            .child(format!("{percent}%"))
                    }),
            )
            .into_any_element()
    }
}
