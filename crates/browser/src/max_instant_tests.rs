use super::*;
use echo_ai::{
    client::{AiError, Cancellation},
    credentials::CredentialStore,
    ProviderConfig, WebLink,
};
use std::{
    sync::{Arc, Mutex},
    task::{Poll, Waker},
};
#[derive(Default)]
struct Reply(Mutex<(Option<Result<Vec<WebLink>, AiError>>, Option<Waker>)>);
impl Reply {
    fn complete(&self, value: Result<Vec<WebLink>, AiError>) {
        let mut s = self.0.lock().unwrap();
        s.0 = Some(value);
        if let Some(w) = s.1.take() {
            w.wake()
        }
    }
}
struct Pending(Arc<Reply>);
impl std::future::Future for Pending {
    type Output = Result<Vec<WebLink>, AiError>;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let mut s = self.0 .0.lock().unwrap();
        if let Some(v) = s.0.take() {
            Poll::Ready(v)
        } else {
            s.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
#[derive(Default)]
struct Searches(Mutex<Vec<(String, Arc<Reply>)>>);
impl crate::max_instant::Search for Searches {
    fn start(
        &self,
        config: ProviderConfig,
        query: String,
        _: Arc<dyn CredentialStore>,
        _: Cancellation,
        executor: &gpui::BackgroundExecutor,
    ) -> gpui::Task<Result<Vec<WebLink>, AiError>> {
        assert_eq!(config.model, "local-model");
        let reply = Arc::new(Reply::default());
        self.0.lock().unwrap().push((query, reply.clone()));
        executor.spawn(Pending(reply))
    }
}
fn setup(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Arc<Searches> {
    let service = Arc::new(Searches::default());
    cx.update(|_, cx| cx.set_global(crate::max_instant::SearchService(service.clone())));
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::InstantLinks, true);
            b.settings.max.provider = ProviderConfig {
                model: "local-model".into(),
                api_key_id: "opaque".into(),
                ..Default::default()
            };
        })
    });
    service
}
fn links() -> Vec<WebLink> {
    (1..=10)
        .map(|n| WebLink {
            title: format!("Result {n}"),
            url: format!("https://rust-lang.org/result/{n}"),
        })
        .collect()
}
fn submit(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("cmd-t");
    bar(view, cx).simulate_input("Rust language official documentation overview");
    bar(view, cx).simulate_keystrokes("shift-enter");
    cx.run_until_parked();
}
#[gpui::test]
fn instant_shift_return_keeps_source_visible_then_publishes_ten_ordered_tabs(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let service = setup(&view, cx);
    let source = active_url(&view, cx);
    let before = view.read_with(cx, |w, _| {
        w.browser
            .space()
            .today
            .iter()
            .map(|t| (t.id, t.url.clone()))
            .collect::<Vec<_>>()
    });
    fake.take_calls();
    submit(&view, cx);
    assert_eq!(active_url(&view, cx), source);
    assert_eq!(
        service.0.lock().unwrap()[0].0,
        "Rust language official documentation overview"
    );
    view.read_with(cx, |w, _| {
        assert!(w.command_bar.is_none());
        assert_eq!(w.browser.space().today.len(), 3);
    });
    assert!(cx.debug_bounds("instant-links-loading").is_some());
    assert!(!fake
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::Create(..) | Call::Navigate(..) | Call::Close(..))));
    service.0.lock().unwrap()[0].1.complete(Ok(links()));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert!(w.max.instant.is_none());
        assert_eq!(w.browser.space().today.len(), 13);
        assert_eq!(w.browser.active_tab().unwrap().title, "Result 1");
        assert_eq!(w.browser.space().today[9].title, "Result 10");
        assert_eq!(
            w.browser.space().today[10..]
                .iter()
                .map(|t| (t.id, t.url.clone()))
                .collect::<Vec<_>>(),
            before
        );
    });
}
#[gpui::test]
fn instant_stale_answer_cannot_navigate_after_user_selection_changes(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let service = setup(&view, cx);
    submit(&view, cx);
    view.update(cx, |w, _| {
        w.edit_browser(|b| {
            b.new_tab("https://example.org/new", std::time::SystemTime::UNIX_EPOCH);
        })
    });
    service.0.lock().unwrap()[0].1.complete(Ok(links()));
    cx.run_until_parked();
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://example.org/new")
    );
    view.read_with(cx, |w, _| assert_eq!(w.browser.space().today.len(), 4));
}

#[gpui::test]
fn instant_disable_or_provider_change_discards_the_pending_search(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let service = setup(&view, cx);
    let source = active_url(&view, cx);
    submit(&view, cx);
    view.update(cx, |w, _| {
        w.edit_profile(|b| b.settings.max.provider.model = "other-model".into())
    });
    service.0.lock().unwrap()[0].1.complete(Ok(links()));
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), source);
    view.read_with(cx, |w, _| {
        assert!(w.max.instant.is_none());
        assert_eq!(w.browser.space().today.len(), 3);
    });
    view.update(cx, |w, _| {
        w.edit_profile(|b| b.settings.max.provider.model = "local-model".into())
    });
    submit(&view, cx);
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::InstantLinks, false)
        })
    });
    service.0.lock().unwrap()[1].1.complete(Ok(links()));
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), source);
    view.read_with(cx, |w, _| assert_eq!(w.browser.space().today.len(), 3));
}
#[gpui::test]
fn disabled_instant_links_and_url_input_never_call_the_search_provider(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let service = setup(&view, cx);
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::InstantLinks, false)
        })
    });
    submit(&view, cx);
    assert!(service.0.lock().unwrap().is_empty());
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::InstantLinks, true)
        })
    });
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://rust-lang.org/");
    bar(&view, cx).simulate_keystrokes("shift-enter");
    cx.run_until_parked();
    assert!(service.0.lock().unwrap().is_empty());
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://rust-lang.org/")
    );
}
