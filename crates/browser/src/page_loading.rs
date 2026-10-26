//! Arc's compact loading pulse and delayed indeterminate bar, above the page card.
use crate::{pages::PageIdentity, window::BrowserWindow};
use browser_core::TabId;
use engine::Bounds as PageBounds;
use gpui::{div, hsla, prelude::*, px, AnyElement, Context, WeakEntity, Window};
use std::{collections::HashMap, time::Instant};

const PULSE_PERIOD: f32 = 1.6;
const STALL_DELAY: f32 = 3.2;
const EXPANSION_TIME: f32 = 0.8;
const SWEEP_PERIOD: f32 = 1.1;
const FINISH_TIME: f32 = 0.2;

struct Indicator {
    identity: PageIdentity,
    frame: PageBounds,
    started: Instant,
    finished: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct LoadingIndicators {
    entries: HashMap<TabId, Indicator>,
}
impl LoadingIndicators {
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn update(&mut self, snapshots: Vec<(PageIdentity, PageBounds, bool)>, now: Instant) {
        self.entries.retain(|_, indicator| {
            snapshots
                .iter()
                .any(|(identity, _, _)| *identity == indicator.identity)
        });
        for (identity, frame, loading) in snapshots {
            if loading {
                let indicator = self.entries.entry(identity.tab).or_insert(Indicator {
                    identity,
                    frame,
                    started: now,
                    finished: None,
                });
                if indicator.finished.take().is_some() {
                    indicator.started = now;
                }
                indicator.frame = frame;
            } else if let Some(indicator) = self.entries.get_mut(&identity.tab) {
                indicator.frame = frame;
                indicator.finished.get_or_insert(now);
            }
        }
        self.entries.retain(|_, indicator| {
            indicator.finished.is_none_or(|finished| {
                now.saturating_duration_since(finished).as_secs_f32() < FINISH_TIME
            })
        });
    }
}

/// The indicators in their own view: their per-frame animation redraws only
/// this view. Animating through the browser window would re-render its whole
/// sidebar every frame, which with hundreds of tabs saturates the UI thread.
pub(crate) struct LoadingView {
    pub(crate) browser: WeakEntity<BrowserWindow>,
    /// Set for a frame requested only by this animation.
    pub(crate) tick: std::rc::Rc<std::cell::Cell<bool>>,
}
impl Render for LoadingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (element, animating) = self
            .browser
            .update(cx, |browser, cx| {
                browser.sync_page_loading(window, cx);
                (browser.render_page_loading(cx), !browser.loading_indicators.is_empty())
            })
            .unwrap_or_else(|_| (div().into_any_element(), false));
        if animating {
            let tick = self.tick.clone();
            cx.on_next_frame(window, move |_, _, cx| {
                tick.set(true);
                cx.notify();
            });
        }
        div().absolute().inset_0().child(element)
    }
}

impl BrowserWindow {
    pub(crate) fn sync_page_loading(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let snapshots = if self.page_content_visible() && self.transient.is_none() {
            let Some(pages) = &self.pages else {
                self.loading_indicators = Default::default();
                return;
            };
            let Ok(pages) = pages.try_borrow() else {
                return;
            };
            let Some(snapshots) = pages.loading_layout() else {
                return;
            };
            snapshots
        } else {
            Vec::new()
        };
        let _ = window;
        self.loading_indicators
            .update(snapshots, cx.background_executor().now());
    }

    pub(crate) fn render_page_loading(&self, cx: &Context<Self>) -> AnyElement {
        let now = cx.background_executor().now();
        div()
            .absolute()
            .inset_0()
            .children(self.loading_indicators.entries.values().map(|indicator| {
                let elapsed = now
                    .saturating_duration_since(indicator.started)
                    .as_secs_f32();
                let pulse = (std::f32::consts::PI * elapsed / PULSE_PERIOD)
                    .sin()
                    .powi(2);
                let expansion = ((elapsed - STALL_DELAY) / EXPANSION_TIME).clamp(0., 1.);
                let expansion = 1. - (1. - expansion).powi(3);
                let fade = indicator.finished.map_or(1., |finished| {
                    1. - (now.saturating_duration_since(finished).as_secs_f32() / FINISH_TIME)
                        .clamp(0., 1.)
                });
                let compact = 42. + 6. * pulse;
                let width = (compact + (128. - compact) * expansion) * fade + 42. * (1. - fade);
                let sweep = ((elapsed - STALL_DELAY).max(0.) % SWEEP_PERIOD) / SWEEP_PERIOD;
                let frame = indicator.frame;
                div()
                    .id(gpui::SharedString::from(format!(
                        "page-loading-{:?}",
                        indicator.identity.tab
                    )))
                    .debug_selector(|| "page-loading".into())
                    .absolute()
                    .left(px((frame.x + frame.width / 2.) as f32 - width / 2.))
                    .top(px((frame.y as f32 - 7.).max(0.)))
                    .w(px(width))
                    .h(px(4.))
                    .rounded(px(2.))
                    .overflow_hidden()
                    .opacity(fade)
                    .bg(hsla(
                        0.,
                        0.,
                        250. / 255.,
                        0.22 * expansion + 0.75 * pulse * (1. - expansion),
                    ))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left(px(sweep * 384. - 128.))
                            .w(px(128.))
                            .h_full()
                            .bg(hsla(0., 0., 250. / 255., 0.75 * expansion)),
                    )
            }))
            .into_any_element()
    }
}
