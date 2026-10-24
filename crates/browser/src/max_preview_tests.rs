use super::max_feature_tests::ProviderRequests;
use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
struct Documents(AtomicUsize);
impl crate::max_previews::Reader for Documents {
    fn read(
        &self,
        url: String,
        _: echo_ai::client::Cancellation,
        executor: &gpui::BackgroundExecutor,
    ) -> gpui::Task<Result<echo_ai::document::Document, echo_ai::document::DocumentError>> {
        self.0.fetch_add(1, Ordering::Relaxed);
        executor.spawn(async move {
            Ok(echo_ai::document::Document {
                url,
                title: "Type safety".into(),
                text: "Type safety prevents type errors.".into(),
            })
        })
    }
}
fn setup(
    view: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
) -> (Arc<Documents>, Arc<ProviderRequests>) {
    cx.update(|window, _| window.activate_window());
    let docs = Arc::new(Documents(AtomicUsize::new(0)));
    let requests = Arc::new(ProviderRequests::default());
    cx.update(|_, cx| {
        cx.set_global(crate::max_previews::ReaderService(docs.clone()));
        cx.set_global(crate::max::InferenceService(requests.clone()));
    });
    view.update(cx, |w, cx| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::FiveSecondPreviews, true);
            b.settings.max.provider = echo_ai::ProviderConfig {
                provider: echo_ai::Provider::OpenAiCompatible,
                model: "local-model".into(),
                base_url: "http://127.0.0.1:8129/v1".into(),
                api_key_id: "opaque-reference".into(),
                ..Default::default()
            };
        });
        cx.notify();
    });
    cx.run_until_parked();
    (docs, requests)
}
fn hover(fake: &FakeEngine, cx: &mut VisualTestContext, source: &str, token: u64) {
    fake.send(
        cx,
        Event::LinkPreviewHover {
            view: page(1),
            token,
            source_url: source.into(),
            hover: engine::LinkHover {
                active: true,
                dismiss: false,
                shift: true,
                url: "https://en.wikipedia.org/wiki/Type_safety".into(),
                x: 200.,
                y: 100.,
                width: 80.,
                height: 20.,
                viewport_width: 760.,
                viewport_height: 700.,
            },
        },
    );
}
fn answer(requests: &ProviderRequests) {
    requests.0.lock().unwrap()[0].1.complete(Ok(serde_json::json!({"title":"Understanding type safety in programming languages","points":[{"icon":"book","label":"Definition","text":"Prevents type errors in programming."},{"icon":"globe","label":"Types","text":"Static vs. dynamic type enforcement."},{"icon":"warning","label":"Importance","text":"Ensures program reliability and safety."},{"icon":"question","label":"Examples","text":"Languages like Java and C# are type-safe."}]}).to_string()));
}
#[gpui::test]
fn max_preview_requires_native_shift_and_current_observer_then_shares_display_text(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (docs, requests) = setup(&view, cx);
    let source = active_url(&view, cx).unwrap();
    let token = fake.0.borrow().preview_tokens[&1];
    assert_ne!(token, 0);
    hover(&fake, cx, &source, token);
    assert_eq!(
        docs.0.load(Ordering::Relaxed),
        0,
        "page-provided Shift alone cannot authorize a read"
    );
    fake.send(
        cx,
        Event::PreviewModifiers {
            view: page(1),
            shift: true,
        },
    );
    cx.run_until_parked();
    assert_eq!(docs.0.load(Ordering::Relaxed), 1);
    assert_eq!(requests.0.lock().unwrap().len(), 1);
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_some()));
    answer(&requests);
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| {
        w.max.preview.open.as_ref().unwrap().overlay.unwrap()
    });
    let mut popup = VisualTestContext::from_window(overlay.into(), &cx.cx);
    click(&mut popup, "max-preview-share");
    cx.run_until_parked();
    let menu = cx.update(|_, cx| {
        cx.windows()
            .into_iter()
            .find(|handle| {
                handle.window_id() != overlay.window_id()
                    && handle.window_id() != view.read(cx).main_window.window_id()
            })
            .unwrap()
    });
    let mut menu = VisualTestContext::from_window(menu, &cx.cx);
    click(&mut menu, "max-preview-action-0");
    cx.run_until_parked();
    let text = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(text.starts_with("Understanding type safety"));
    assert!(text.contains("Definition: Prevents type errors"));
    assert!(!text.contains("https://"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_none()));
}
#[gpui::test]
fn max_preview_cancels_after_source_navigation_and_discards_late_model_output(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (_, requests) = setup(&view, cx);
    let source = active_url(&view, cx).unwrap();
    let token = fake.0.borrow().preview_tokens[&1];
    fake.send(
        cx,
        Event::PreviewModifiers {
            view: page(1),
            shift: true,
        },
    );
    hover(&fake, cx, &source, token);
    cx.run_until_parked();
    assert_eq!(requests.0.lock().unwrap().len(), 1);
    fake.send(
        cx,
        Event::UrlChanged {
            view: page(1),
            url: "https://example.com/new-document".into(),
        },
    );
    cx.run_until_parked();
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_none()));
    answer(&requests);
    cx.run_until_parked();
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_none()));
}
#[gpui::test]
fn max_preview_disable_revokes_observer_and_old_hover_cannot_start_another_request(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (docs, requests) = setup(&view, cx);
    let source = active_url(&view, cx).unwrap();
    let token = fake.0.borrow().preview_tokens[&1];
    view.update(cx, |w, cx| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::FiveSecondPreviews, false)
        });
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(fake.0.borrow().preview_tokens[&1], 0);
    fake.send(
        cx,
        Event::PreviewModifiers {
            view: page(1),
            shift: true,
        },
    );
    hover(&fake, cx, &source, token);
    assert_eq!(docs.0.load(Ordering::Relaxed), 0);
    assert!(requests.0.lock().unwrap().is_empty());
}
#[gpui::test]
fn max_preview_shift_release_from_renderer_dismisses_without_a_native_key_callback(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (_, requests) = setup(&view, cx);
    let source = active_url(&view, cx).unwrap();
    let token = fake.0.borrow().preview_tokens[&1];
    fake.send(
        cx,
        Event::PreviewModifiers {
            view: page(1),
            shift: true,
        },
    );
    hover(&fake, cx, &source, token);
    cx.run_until_parked();
    assert_eq!(requests.0.lock().unwrap().len(), 1);
    fake.send(
        cx,
        Event::LinkPreviewHover {
            view: page(1),
            token,
            source_url: source,
            hover: engine::LinkHover {
                active: true,
                dismiss: false,
                shift: false,
                url: "https://en.wikipedia.org/wiki/Type_safety".into(),
                x: 200.,
                y: 100.,
                width: 80.,
                height: 20.,
                viewport_width: 760.,
                viewport_height: 700.,
            },
        },
    );
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_none()));
    answer(&requests);
    cx.run_until_parked();
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_none()));
}
#[gpui::test]
fn max_preview_menu_keyboard_has_child_focus_and_escape_dismisses_the_retained_panel(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (_, requests) = setup(&view, cx);
    let source = active_url(&view, cx).unwrap();
    let token = fake.0.borrow().preview_tokens[&1];
    fake.send(
        cx,
        Event::PreviewModifiers {
            view: page(1),
            shift: true,
        },
    );
    hover(&fake, cx, &source, token);
    cx.run_until_parked();
    answer(&requests);
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| {
        w.max.preview.open.as_ref().unwrap().overlay.unwrap()
    });
    let mut panel = VisualTestContext::from_window(overlay.into(), &cx.cx);
    click(&mut panel, "max-preview-share");
    cx.run_until_parked();
    let menu = cx.update(|_, cx| {
        cx.windows()
            .into_iter()
            .find(|handle| {
                handle.window_id() != overlay.window_id()
                    && handle.window_id() != view.read(cx).main_window.window_id()
            })
            .unwrap()
    });
    let mut menu = VisualTestContext::from_window(menu, &cx.cx);
    menu.simulate_keystrokes("down up enter");
    cx.run_until_parked();
    let text = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(text.starts_with("Understanding type safety"));
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_some()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_none()));
}
#[gpui::test]
fn max_preview_minimize_retires_children_without_restoring_focus_until_activation(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (_, requests) = setup(&view, cx);
    let source = active_url(&view, cx).unwrap();
    let token = fake.0.borrow().preview_tokens[&1];
    fake.send(
        cx,
        Event::PreviewModifiers {
            view: page(1),
            shift: true,
        },
    );
    hover(&fake, cx, &source, token);
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| {
        w.max.preview.open.as_ref().unwrap().overlay.unwrap()
    });
    overlay
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    fake.take_calls();
    view.update(cx, |w, cx| w.close_popups_for_window_operation(true, cx));
    cx.run_until_parked();
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_none()));
    assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
    assert!(!fake
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::FocusPage(_))));
    answer(&requests);
    cx.run_until_parked();
    view.read_with(cx, |w, _| assert!(w.max.preview.open.is_none()));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(fake.take_calls().contains(&Call::FocusPage(1)));
}
