use super::*;

fn active(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
}
fn failed(
    view: &Entity<BrowserWindow>,
    cx: &VisualTestContext,
    tab: browser_core::TabId,
) -> Option<i32> {
    view.read_with(cx, |v, _| {
        v.pages.as_ref().unwrap().borrow().state(tab).crashed
    })
}

#[gpui::test]
fn crashed_tab_reload_keeps_identity_url_and_recovers_the_same_native_page(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    let tab = view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id);
    let before = view.read_with(cx, |v, _| v.session_json());
    fake.take_calls();
    fake.send(
        cx,
        Event::RendererStateChanged {
            view: page(1),
            error: Some(11),
        },
    );
    assert_eq!(failed(&view, cx, tab), Some(11));
    assert_eq!(view.read_with(cx, |v, _| v.session_json()), before);
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Visible(1, false)), "{calls:?}");
    assert!(!calls
        .iter()
        .any(|c| matches!(c, Call::Close(_) | Call::Navigate(..) | Call::Create(..))));
    // A queued pre-crash load callback cannot dismiss the recovery surface.
    fake.send(
        cx,
        Event::LoadingChanged {
            view: page(1),
            loading: true,
            can_go_back: true,
            can_go_forward: false,
        },
    );
    assert_eq!(failed(&view, cx, tab), Some(11));
    click(
        cx,
        test_selector(format!("reload-crashed-tab-{}", tab.get())),
    );
    assert!(fake.take_calls().contains(&Call::Reload(1)));
    assert_eq!(failed(&view, cx, tab), Some(11));
    fake.send(
        cx,
        Event::RendererStateChanged {
            view: page(1),
            error: None,
        },
    );
    assert_eq!(failed(&view, cx, tab), None);
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Visible(1, true)), "{calls:?}");
    assert!(!calls
        .iter()
        .any(|c| matches!(c, Call::Close(_) | Call::Navigate(..) | Call::Create(..))));
    assert_eq!(
        view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id),
        tab
    );
}

#[gpui::test]
fn crashed_page_cannot_steal_command_bar_input_and_keyboard_reload_still_works(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("keep typing");
    fake.take_calls();
    fake.send(
        cx,
        Event::RendererStateChanged {
            view: page(1),
            error: Some(9),
        },
    );
    assert!(!fake
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::FocusPage(_) | Call::FocusUi)));
    bar(&view, cx).simulate_input(" here");
    view.read_with(cx, |v, cx| {
        assert_eq!(
            v.command_bar.as_ref().unwrap().input.read(cx).text(),
            "keep typing here"
        )
    });
    bar(&view, cx).simulate_keystrokes("escape");
    cx.run_until_parked();
    fake.take_calls();
    cx.simulate_keystrokes("cmd-r");
    cx.run_until_parked();
    assert!(fake.take_calls().contains(&Call::Reload(1)));
}

#[gpui::test]
fn split_crash_hides_only_failed_pane_and_its_reload_keeps_the_group(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    let first = view.read_with(cx, |v, _| v.browser.active_tab().unwrap().id);
    let other = view.update(cx, |v, _| {
        v.edit_browser(|b| {
            let other = b.new_tab("https://healthy.example", std::time::SystemTime::now());
            b.add_split(
                first,
                other,
                browser_core::splits::Placement::Right,
                std::time::SystemTime::now(),
            )
            .unwrap();
            other
        })
    });
    cx.run_until_parked();
    fake.take_calls();
    fake.send(
        cx,
        Event::RendererStateChanged {
            view: page(1),
            error: Some(11),
        },
    );
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Visible(1, false)), "{calls:?}");
    assert!(!calls.contains(&Call::Visible(2, false)), "{calls:?}");
    assert_eq!(failed(&view, cx, first), Some(11));
    assert_eq!(failed(&view, cx, other), None);
    click(
        cx,
        test_selector(format!("reload-crashed-tab-{}", first.get())),
    );
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Reload(1)), "{calls:?}");
    assert!(!calls.contains(&Call::Reload(2)));
    view.read_with(cx, |v, _| {
        assert_eq!(v.browser.active_split().unwrap().panes, vec![first, other])
    });
}
