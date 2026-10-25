//! Shift-Return searches without navigating or persisting a placeholder page.
use crate::window::{BrowserWindow, Suggestion, Target};
use browser_core::{max::InstantSnapshot, SpaceId};
use echo_ai::{
    client::{AiError, Cancellation, Client},
    ProviderConfig, WebLink,
};
use gpui::{div, hsla, prelude::*, px, AnyElement, App, BackgroundExecutor, Context, Task};
use std::sync::Arc;

pub(crate) trait Search: Send + Sync {
    fn start(
        &self,
        config: ProviderConfig,
        query: String,
        store: Arc<dyn echo_ai::credentials::CredentialStore>,
        cancel: Cancellation,
        executor: &BackgroundExecutor,
    ) -> Task<Result<Vec<WebLink>, AiError>>;
}
struct Direct;
impl Search for Direct {
    fn start(
        &self,
        config: ProviderConfig,
        query: String,
        store: Arc<dyn echo_ai::credentials::CredentialStore>,
        cancel: Cancellation,
        executor: &BackgroundExecutor,
    ) -> Task<Result<Vec<WebLink>, AiError>> {
        executor.spawn(async move { Client { store }.search_links(&config, &query, &cancel) })
    }
}
pub(crate) struct SearchService(pub Arc<dyn Search>);
impl gpui::Global for SearchService {}
pub(crate) struct Job {
    generation: u64,
    snapshot: InstantSnapshot,
    config: ProviderConfig,
    cancel: Cancellation,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl Job {
    fn current(&self, w: &BrowserWindow, cx: &App) -> bool {
        self.cancel.check().is_ok()
            && w.browser.instant_snapshot().as_ref() == Some(&self.snapshot)
            && w.profile.handle.read_settings(|b| {
                b.settings.max.provider == self.config
                    && b.settings.max.enabled(echo_ai::MaxFeature::InstantLinks)
            })
            && !crate::quit::is_quitting()
            && cx
                .windows()
                .iter()
                .any(|v| v.window_id() == w.main_window.window_id())
    }
}
impl BrowserWindow {
    /// False retains normal Return behavior for URLs, disabled features and
    /// rename/split/site-search forms; those are never submitted to a provider.
    pub(crate) fn start_instant_links(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(bar) = self
            .command_bar
            .as_ref()
            .filter(|b| b.target == Target::NewTab && b.site_search.is_none())
        else {
            return false;
        };
        let query = bar.input.read(cx).text().trim().to_owned();
        if !matches!(
            self.suggestions(cx).first(),
            Some(Suggestion::Load(
                browser_core::omnibox::Resolved::Search { .. }
            ))
        ) {
            return false;
        }
        let Some(snapshot) = self.browser.instant_snapshot() else {
            return false;
        };
        if query.is_empty() || query.len() > 8192 {
            return false;
        }
        self.max.instant = None;
        self.max.generation = self.max.generation.wrapping_add(1);
        let generation = self.max.generation;
        let config = self
            .profile
            .handle
            .read_settings(|b| b.settings.max.provider.clone());
        let cancel = Cancellation::default();
        let service = cx
            .try_global::<SearchService>()
            .map(|s| s.0.clone())
            .unwrap_or_else(|| Arc::new(Direct));
        let worker = service.start(
            config.clone(),
            query,
            crate::settings::max::store(cx),
            cancel.clone(),
            cx.background_executor(),
        );
        self.max.instant = Some(Job {
            generation,
            snapshot,
            config,
            cancel,
        });
        self.close_command_bar(cx);
        cx.spawn(async move |this, cx| {
            let answer = worker.await;
            let _ = this.update(cx, |this, cx| {
                let Some(job) = this
                    .max
                    .instant
                    .as_ref()
                    .filter(|j| j.generation == generation)
                else {
                    return;
                };
                let current = job.current(this, cx);
                let snapshot = job.snapshot.clone();
                this.max.instant = None;
                if current {
                    match answer {
                        Ok(links) => {
                            this.edit_browser(|b| {
                                b.apply_instant_links(
                                    &snapshot,
                                    links,
                                    std::time::SystemTime::now(),
                                )
                            });
                        }
                        Err(e) if e != AiError::Cancelled => this.show_toast(e.to_string(), cx),
                        _ => {}
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        true
    }
    pub(crate) fn render_instant_loading(
        &self,
        space: SpaceId,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let job = self.max.instant.as_ref()?;
        if job.snapshot.space != space || !job.current(self, cx) {
            return None;
        }
        let ink = if self.theme_uses_light_ink_for(space) {
            1.
        } else {
            0.
        };
        Some(
            div()
                .id("instant-links-loading")
                .debug_selector(|| "instant-links-loading".into())
                .flex()
                .items_center()
                .h(px(crate::window::SIDEBAR_ROW_HEIGHT))
                .flex_none()
                .px(px(9.))
                .gap(px(10.))
                .text_size(px(14.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(hsla(0., 0., ink, 0.85))
                .child(crate::icons::control_sized(crate::icons::Icon::Search, 16.))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child("Searching the web for you"),
                )
                .into_any_element(),
        )
    }
}
