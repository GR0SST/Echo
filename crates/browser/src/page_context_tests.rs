//! CEF menu choices enter the real window as document-bound user input.
use super::*;
use engine::page_context::{Action, OpenTarget, Request};
fn choose(
    fake: &FakeEngine,
    view: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
    action: Action,
) {
    let url = active_url(view, cx).unwrap();
    fake.send(
        cx,
        Event::PageContext {
            view: page(1),
            request: Request {
                page_url: url,
                action,
            },
        },
    );
}
#[gpui::test]
fn page_context_copy_preserves_the_exact_target_and_rejects_a_navigated_document(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let source = active_url(&view, cx).unwrap();
    let link = "https://example.com/path?utm_source=context&token=a%2Fb#section";
    fake.take_calls();
    choose(&fake, &view, cx, Action::Copy(link.into()));
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), link);
    assert_eq!(active_url(&view, cx).as_deref(), Some(source.as_str()));
    assert!(fake.take_calls().is_empty());
    fake.send(
        cx,
        Event::UrlChanged {
            view: page(1),
            url: "https://later.example/".into(),
        },
    );
    fake.send(
        cx,
        Event::PageContext {
            view: page(1),
            request: Request {
                page_url: source,
                action: Action::Copy("stale".into()),
            },
        },
    );
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), link);
}
#[gpui::test]
fn page_context_background_open_keeps_the_original_page_selected(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = active_url(&view, cx);
    let count = view.read_with(cx, |view, _| view.browser.space().today.len());
    choose(
        &fake,
        &view,
        cx,
        Action::Open {
            url: "https://background.example/".into(),
            target: OpenTarget::BackgroundTab,
        },
    );
    assert_eq!(active_url(&view, cx), before);
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.space().today.len(), count + 1);
        assert!(view
            .browser
            .space()
            .today
            .iter()
            .any(|tab| tab.url == "https://background.example/"));
        assert!(view.toast.is_none());
    });
}
#[gpui::test]
fn page_context_split_copies_a_pin_and_preserves_its_original_document(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let id = browser.active_tab().unwrap().id;
            browser.toggle_pin(id);
            id
        })
    });
    cx.run_until_parked();
    fake.take_calls();
    choose(
        &fake,
        &view,
        cx,
        Action::Open {
            url: "https://split.example/".into(),
            target: OpenTarget::Split,
        },
    );
    view.read_with(cx, |view, _| {
        assert!(view
            .browser
            .space()
            .pinned
            .iter()
            .any(|tab| tab.id == original));
        assert!(view.browser.split_for(original).is_none());
        let split = view.browser.active_split().unwrap();
        assert_eq!(split.panes.len(), 2);
        assert!(!split.panes.contains(&original));
        assert_eq!(
            view.browser.active_tab().unwrap().url,
            "https://split.example/"
        );
        assert!(view.split_draft.is_none());
        assert!(view.command_bar.is_none());
    });
    assert!(!fake.take_calls().contains(&Call::Close(1)));
}
#[gpui::test]
fn page_context_new_window_inherits_the_profile_without_changing_source_selection(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    view.update(cx, |view, cx| {
        view.save_session_to(
            std::env::temp_dir().join(format!(
                "echo-context-window-{}-{:?}.json",
                std::process::id(),
                cx.entity_id()
            )),
            cx,
        )
    });
    let before = active_url(&view, cx);
    let profile = view.read_with(cx, |view, _| view.browser.active_user_profile_id());
    choose(
        &fake,
        &view,
        cx,
        Action::Open {
            url: "https://window.example/".into(),
            target: OpenTarget::Window,
        },
    );
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), before);
    let others: Vec<_> = cx.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|handle| handle.downcast::<BrowserWindow>())
            .filter_map(|handle| handle.root(cx).ok())
            .filter(|other| other != &view)
            .collect()
    });
    assert_eq!(others.len(), 1);
    others[0].read_with(cx, |destination, _| {
        assert_eq!(
            destination.browser.active_tab().unwrap().url,
            "https://window.example/"
        );
        assert_eq!(destination.browser.active_user_profile_id(), profile);
        assert_eq!(
            destination
                .browser
                .window_selection()
                .selected_tabs()
                .count(),
            1
        );
        assert!(!destination.browser.is_private());
    });
}
#[gpui::test]
fn page_context_retired_profile_generation_cannot_open_a_link(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let source = active_url(&view, cx).unwrap();
    view.update(cx, |view, _| {
        view.edit_profile(|browser| {
            let profile = browser.create_user_profile("Context replacement").unwrap();
            browser
                .assign_space_user_profile(browser.space().id, profile)
                .unwrap();
        })
    });
    cx.run_until_parked();
    let count = view.read_with(cx, |view, _| view.browser.space().today.len());
    fake.send(
        cx,
        Event::PageContext {
            view: page(1),
            request: Request {
                page_url: source,
                action: Action::Open {
                    url: "https://stale.example/".into(),
                    target: OpenTarget::BackgroundTab,
                },
            },
        },
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.space().today.len()),
        count
    );
}

#[gpui::test]
fn page_context_image_copy_rejects_superseded_requests_navigation_and_a_later_url_copy(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let source = active_url(&view, cx).unwrap();
    let png = include_bytes!("../assets/echo-icon.png").to_vec();
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "Known fixture sentinel".into(),
    ));
    view.update(cx, |view, cx| view.copy_url_as_quote(cx));
    let quote = fake
        .take_calls()
        .iter()
        .find_map(|call| {
            if let Call::Quote(_, request) = call {
                Some(*request)
            } else {
                None
            }
        })
        .unwrap();
    for url in [
        "https://image.example/one.png",
        "https://image.example/two.png",
    ] {
        choose(&fake, &view, cx, Action::CopyImage(url.into()));
    }
    fake.send(
        cx,
        Event::QuoteSelection {
            view: page(1),
            request: quote,
            text: "Superseded quote".into(),
            source_url: source.clone(),
        },
    );
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("Known fixture sentinel")
    );
    let requests = fake.0.borrow().context_images.clone();
    assert_eq!(requests.len(), 2);
    assert!(requests
        .iter()
        .all(|(id, _, page_url, _)| *id == 1 && page_url == &source));
    fake.send(
        cx,
        Event::ContextImage {
            view: page(1),
            request: requests[0].1,
            page_url: source.clone(),
            png: png.clone(),
        },
    );
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("Known fixture sentinel")
    );
    fake.send(
        cx,
        Event::ContextImage {
            view: page(1),
            request: requests[1].1,
            page_url: source.clone(),
            png: png.clone(),
        },
    );
    assert_eq!(
        cx.read_from_clipboard(),
        Some(gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            png.clone()
        )))
    );
    choose(
        &fake,
        &view,
        cx,
        Action::CopyImage("https://image.example/three.png".into()),
    );
    let token = fake.0.borrow().context_images.last().unwrap().1;
    cx.simulate_keystrokes("cmd-shift-c");
    cx.run_until_parked();
    let copied = cx.read_from_clipboard().unwrap();
    assert!(copied.text().is_some());
    fake.send(
        cx,
        Event::ContextImage {
            view: page(1),
            request: token,
            page_url: source.clone(),
            png: png.clone(),
        },
    );
    assert_eq!(cx.read_from_clipboard(), Some(copied.clone()));
    choose(
        &fake,
        &view,
        cx,
        Action::CopyImage("https://image.example/four.png".into()),
    );
    let token = fake.0.borrow().context_images.last().unwrap().1;
    fake.send(
        cx,
        Event::UrlChanged {
            view: page(1),
            url: "https://later.example/".into(),
        },
    );
    fake.send(
        cx,
        Event::ContextImage {
            view: page(1),
            request: token,
            page_url: source,
            png,
        },
    );
    assert_eq!(cx.read_from_clipboard(), Some(copied));
}

#[gpui::test]
fn page_context_little_open_keeps_the_normal_tabs_and_uses_the_source_profile(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let before = active_url(&view, cx);
    let (profile, count) = view.read_with(cx, |view, _| {
        (
            view.browser.active_user_profile_id(),
            view.browser.all_tabs().count(),
        )
    });
    choose(
        &fake,
        &view,
        cx,
        Action::Open {
            url: "https://little.example/".into(),
            target: OpenTarget::Little,
        },
    );
    cx.run_until_parked();
    let other = cx.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|handle| handle.downcast::<BrowserWindow>())
            .filter_map(|handle| handle.root(cx).ok())
            .find(|other| other != &view)
            .unwrap()
    });
    other.read_with(cx, |other, _| {
        assert!(other.transient.is_some());
        assert!(other.browser.is_blank());
        assert_eq!(other.browser.active_user_profile_id(), profile);
        assert_eq!(
            other.browser.active_tab().unwrap().url,
            "https://little.example/"
        );
    });
    assert_eq!(active_url(&view, cx), before);
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.all_tabs().count()),
        count
    );
}
#[gpui::test]
fn page_context_incognito_open_keeps_the_normal_model_out_of_the_private_window(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let before = active_url(&view, cx);
    let count = view.read_with(cx, |view, _| view.browser.all_tabs().count());
    choose(
        &fake,
        &view,
        cx,
        Action::Open {
            url: "https://private.example/".into(),
            target: OpenTarget::Incognito,
        },
    );
    cx.run_until_parked();
    let other = cx.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|handle| handle.downcast::<BrowserWindow>())
            .filter_map(|handle| handle.root(cx).ok())
            .find(|other| other != &view)
            .unwrap()
    });
    other.read_with(cx, |other, _| {
        assert!(other.browser.is_private());
        assert!(!other.browser.is_blank());
        assert_eq!(other.browser.all_tabs().count(), 1);
        assert_eq!(
            other.browser.active_tab().unwrap().url,
            "https://private.example/"
        );
        assert!(other.command_bar.is_none());
    });
    assert_eq!(active_url(&view, cx), before);
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.all_tabs().count()),
        count
    );
}
