use super::*;

#[gpui::test]
fn video_mini_player_header_opens_pip_for_the_retained_source_without_switching_tabs(
    cx: &mut TestAppContext,
) {
    let (view, visual, fake) = open_pages(cx);
    background_media(&view, visual, &fake);
    assert!(visual.debug_bounds("media-pip").is_none());
    let foreground = active_url(&view, visual);
    let mut video = playing_media();
    video.video = true;
    fake.send(
        visual,
        Event::MediaStateChanged {
            view: page(1),
            state: video,
        },
    );
    let center = visual.debug_bounds("mini-player").unwrap().center();
    visual.simulate_mouse_move(center, None, Modifiers::none());
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(300));
    visual.run_until_parked();
    fake.take_calls();
    click(visual, "media-pip");
    assert_eq!(active_url(&view, visual), foreground);
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::PictureInPicture(1, true)));
    assert!(!calls.iter().any(|call| matches!(
        call,
        Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::FocusPage(1)
    )));
}

#[gpui::test]
fn mini_player_pip_request_cannot_follow_a_same_url_document_replacement(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    background_media(&view, visual, &fake);
    let mut video = playing_media();
    video.video = true;
    fake.send(
        visual,
        Event::MediaStateChanged {
            view: page(1),
            state: video,
        },
    );
    let (source, identity, epoch) = view.read_with(visual, |view, _| {
        let source = view.mini_player_source().unwrap().0;
        let pages = view.pages.as_ref().unwrap().borrow();
        (
            source,
            pages.media_identity(source).unwrap(),
            pages.page_document_epoch(source).unwrap(),
        )
    });
    *fake.0.borrow_mut().document_epochs.entry(1).or_default() += 1;
    fake.take_calls();
    view.update(visual, |view, _| {
        assert!(!view
            .pages
            .as_ref()
            .unwrap()
            .borrow_mut()
            .open_media_pip(source, identity, epoch))
    });
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::PictureInPicture(..))));
}
