use super::*;

fn video() -> engine::MediaState {
    let mut state = playing_media();
    state.video = true;
    state
}

fn leave_split(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, url: &str) {
    cx.simulate_keystrokes("cmd-t");
    bar(view, cx).simulate_input(url);
    bar(view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn focusing_another_visible_split_pane_does_not_open_or_close_video_pip(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.send(
        cx,
        Event::MediaStateChanged {
            view: page(1),
            state: video(),
        },
    );
    fake.take_calls();
    click(cx, "pane-header-2");
    click(cx, "pane-header-3");
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::PictureInPicture(..))));
    click(cx, "pane-header-2");
    let mut manual = video();
    manual.picture_in_picture = true;
    fake.send(
        cx,
        Event::MediaStateChanged {
            view: page(1),
            state: manual,
        },
    );
    fake.take_calls();
    click(cx, "pane-header-3");
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::PictureInPicture(..))));
    assert_eq!(
        view.read_with(cx, |view, _| view
            .pages
            .as_ref()
            .unwrap()
            .borrow()
            .surface_layout()
            .len()),
        2
    );
}

#[gpui::test]
fn leaving_and_returning_to_a_split_tracks_its_unique_nonfocused_playing_video(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.send(
        cx,
        Event::MediaStateChanged {
            view: page(1),
            state: video(),
        },
    );
    click(cx, "pane-header-3");
    let focused = view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id);
    let panes = view.read_with(cx, |view, _| {
        view.browser.active_split().unwrap().panes.clone()
    });
    fake.take_calls();
    leave_split(&view, cx, "https://outside.example/first");
    assert!(fake.take_calls().contains(&Call::PictureInPicture(1, true)));
    let mut pip = video();
    pip.picture_in_picture = true;
    fake.send(
        cx,
        Event::MediaStateChanged {
            view: page(1),
            state: pip,
        },
    );
    fake.take_calls();
    click(cx, "tab-2");
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id),
        focused
    );
    assert_eq!(
        view.read_with(cx, |view, _| view
            .browser
            .active_split()
            .unwrap()
            .panes
            .clone()),
        panes
    );
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::PictureInPicture(1, false)));
    assert!(!calls.iter().any(|call| matches!(
        call,
        Call::Create(..) | Call::Close(..) | Call::Navigate(..)
    )));
    // The same Split path respects the saved preference.
    fake.send(
        cx,
        Event::MediaStateChanged {
            view: page(1),
            state: video(),
        },
    );
    view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            browser.settings.set_enabled(
                browser_core::settings::Toggle::AutomaticPictureInPicture,
                false,
            )
        })
    });
    fake.take_calls();
    leave_split(&view, cx, "https://outside.example/disabled");
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::PictureInPicture(..))));
}

#[gpui::test]
fn split_pip_does_not_choose_between_ambiguous_siblings_but_keeps_the_focused_source(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    for id in [1, 2] {
        fake.send(
            cx,
            Event::MediaStateChanged {
                view: page(id),
                state: video(),
            },
        );
    }
    let last = view.read_with(cx, |view, _| *view.browser.active_split().unwrap().panes.last().unwrap());
    click(cx, test_selector(format!("pane-header-{}", last.get())));
    fake.take_calls();
    leave_split(&view, cx, "https://outside.example/ambiguous");
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::PictureInPicture(..))));
    click(cx, "tab-2");
    click(cx, "pane-header-2");
    fake.take_calls();
    leave_split(&view, cx, "https://outside.example/focused");
    let calls = fake.take_calls();
    assert_eq!(
        calls
            .iter()
            .filter_map(|call| if let Call::PictureInPicture(id, enabled) = call {
                Some((*id, *enabled))
            } else {
                None
            })
            .collect::<Vec<_>>(),
        [(1, true)]
    );
}
