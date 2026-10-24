use super::*;

fn activate(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
}

fn pasted(fake: &FakeEngine) -> Vec<Call> {
    fake.take_calls()
        .into_iter()
        .filter(|call| matches!(call, Call::PasteStyle(_) | Call::InspectorPasteStyle(_)))
        .collect()
}

#[gpui::test]
fn paste_style_gpui_menu_and_shortcut_replace_selection_as_one_plain_text_edit(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    activate(cx);
    for menu in [false, true] {
        cx.simulate_keystrokes("cmd-l");
        let original = bar_text(&view, cx).unwrap();
        let mut input = bar(&view, cx);
        input.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("é🦊\r\nline".into()))
        });
        if menu {
            dispatch_menu(&mut input, "Edit", "Paste and Match Style");
        } else {
            input.simulate_keystrokes("cmd-shift-v");
        }
        assert_eq!(bar_text(&view, cx).as_deref(), Some("é🦊  line"));
        input.simulate_keystrokes("cmd-z");
        assert_eq!(bar_text(&view, cx).as_deref(), Some(original.as_str()));
        input.simulate_keystrokes("cmd-shift-z");
        assert_eq!(bar_text(&view, cx).as_deref(), Some("é🦊  line"));
        assert!(pasted(&fake).is_empty());
        input.simulate_keystrokes("escape");
    }
}

#[gpui::test]
fn paste_style_native_menu_and_cef_shortcut_use_the_focused_split_pane(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    activate(cx);
    for id in [1, 2] {
        fake.send(cx, Event::Focused { view: page(id) });
        let before = view.read_with(cx, |view, _| view.session_json());
        fake.take_calls();
        dispatch_menu(cx, "Edit", "Paste and Match Style");
        assert_eq!(pasted(&fake), [Call::PasteStyle(id)]);
        fake.send(
            cx,
            fake.shortcut(Shortcut {
                cmd: true,
                shift: true,
                key: "v".into(),
                ..Default::default()
            }),
        );
        assert_eq!(pasted(&fake), [Call::PasteStyle(id)]);
        assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
    }
}

#[gpui::test]
fn paste_style_library_preview_is_allowed_but_spaces_and_ui_intent_are_not(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    activate(cx);
    let native = fake.0.borrow().focused_page.unwrap();
    cx.simulate_keystrokes("cmd-shift-j");
    click(cx, "downloads-search");
    cx.simulate_input("keep query");
    fake.send(cx, Event::Focused { view: native });
    fake.take_calls();
    dispatch_menu(cx, "Edit", "Paste and Match Style");
    assert_eq!(pasted(&fake), [Call::PasteStyle(raw(native))]);
    fake.send(
        cx,
        fake.shortcut(Shortcut {
            cmd: true,
            shift: true,
            key: "v".into(),
            ..Default::default()
        }),
    );
    assert_eq!(pasted(&fake), [Call::PasteStyle(raw(native))]);
    assert_eq!(
        view.read_with(cx, |view, _| view
            .sidebar_navigation
            .downloads
            .host
            .as_ref()
            .unwrap()
            .query()
            .to_owned()),
        "keep query"
    );
    click(cx, "library-nav-spaces");
    dispatch_menu(cx, "Edit", "Paste and Match Style");
    assert!(pasted(&fake).is_empty());
    // A GPUI editor consumes paste even if the previous native field was editable.
    cx.simulate_keystrokes("cmd-t");
    let mut input = bar(&view, cx);
    input.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("draft".into())));
    input.simulate_keystrokes("cmd-shift-v");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("draft"));
    assert!(pasted(&fake).is_empty());
}

#[gpui::test]
fn paste_style_inspector_targets_its_frontend_without_editing_inspected_page(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    activate(cx);
    accept_recording_inspector(&view, cx, &fake);
    cx.simulate_keystrokes("cmd-alt-i");
    cx.run_until_parked();
    let inspector = inspector_status(&view, cx).unwrap();
    inspector_event(
        &fake,
        cx,
        engine::InspectorEvent::Focused {
            inspector: inspector.id,
        },
    );
    fake.take_calls();
    dispatch_menu(cx, "Edit", "Paste and Match Style");
    assert_eq!(pasted(&fake), [Call::InspectorPasteStyle(inspector.id)]);
    fake.send(
        cx,
        Event::Focused {
            view: inspector.inspected,
        },
    );
    fake.take_calls();
    dispatch_menu(cx, "Edit", "Paste and Match Style");
    assert_eq!(pasted(&fake), [Call::PasteStyle(raw(inspector.inspected))]);
}

#[gpui::test]
fn paste_style_pending_effect_rejects_new_ui_intent_before_native_dispatch(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    activate(cx);
    fake.take_calls();
    view.update(cx, |view, cx| {
        view.paste_and_match_style(cx);
        view.pages.as_ref().unwrap().borrow_mut().request_ui_focus();
    });
    cx.run_until_parked();
    assert!(pasted(&fake).is_empty());
}

#[gpui::test]
fn paste_style_native_effect_is_reentrant_and_busy_retry_revalidates_focus(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    activate(cx);
    let pages = view.read_with(cx, |view, _| view.pages.as_ref().unwrap().clone());
    let weak = view.downgrade();
    let mut app = cx.update(|_, cx| cx.to_async());
    let called = Rc::new(std::cell::Cell::new(false));
    let output = called.clone();
    fake.0.borrow_mut().paste_hook = Some(Box::new(move || {
        weak.update(&mut app, |_, _| ())
            .expect("native edit must be reentrant");
        let mut pages = pages.try_borrow_mut().expect("Pages must be released");
        pages.request_ui_focus();
        output.set(true);
    }));
    dispatch_menu(cx, "Edit", "Paste and Match Style");
    assert!(called.get());
    assert_eq!(pasted(&fake).len(), 1);
    let native = fake.0.borrow().focused_page.unwrap();
    fake.send(cx, Event::Focused { view: native });
    fake.0.borrow_mut().paste_busy = 1;
    dispatch_menu(cx, "Edit", "Paste and Match Style");
    assert!(pasted(&fake).is_empty());
    cx.simulate_keystrokes("cmd-t");
    cx.executor().advance_clock(Duration::from_millis(8));
    cx.run_until_parked();
    assert!(pasted(&fake).is_empty());
}
