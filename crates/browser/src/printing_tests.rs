use super::*;

fn active(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.is_window_active()));
}
fn dispatch(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    for _ in 0..5 {
        cx.executor().advance_clock(Duration::from_millis(8));
        cx.run_until_parked();
    }
}
fn printed(fake: &FakeEngine) -> Vec<u64> {
    fake.take_calls()
        .into_iter()
        .filter_map(|call| {
            if let Call::Print(id) = call {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}

#[gpui::test]
fn print_shortcut_menu_and_command_use_the_same_page_and_preserve_model(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    let native = raw(fake.0.borrow().focused_page.unwrap());
    let before = view.read_with(cx, |view, _| view.session_json());
    for entry in 0..3 {
        fake.take_calls();
        match entry {
            0 => cx.simulate_keystrokes("cmd-p"),
            1 => dispatch_menu(cx, "File", "Print"),
            _ => run_named_command(&view, cx, "print page"),
        }
        dispatch(cx);
        assert_eq!(printed(&fake), [native]);
        assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
    }
}

#[gpui::test]
fn native_print_effect_runs_after_all_application_window_and_pages_borrows_end(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    let (pages, tab) = view.read_with(cx, |view, _| {
        (
            view.pages.as_ref().unwrap().clone(),
            view.browser.active_tab().unwrap().id,
        )
    });
    let weak = view.downgrade();
    let mut app = cx.update(|_, cx| cx.to_async());
    let called = Rc::new(std::cell::Cell::new(false));
    let output = called.clone();
    fake.0.borrow_mut().print_hook = Some(Box::new(move || {
        weak.update(&mut app, |_, _| ())
            .expect("window must be reentrant during native print");
        let mut pages = pages
            .try_borrow_mut()
            .expect("Pages must not remain borrowed");
        pages.reload(tab); // Also reenters the actual PageStore/engine borrow.
        output.set(true);
    }));
    view.update(cx, |view, cx| {
        view.print_page(cx);
        assert!(!called.get(), "native printing must not run inline");
    });
    dispatch(cx);
    assert!(called.get());
    assert_eq!(printed(&fake).len(), 1);
}

#[gpui::test]
fn busy_print_retains_target_but_rejects_selection_changes_before_dispatch(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    let native = raw(fake.0.borrow().focused_page.unwrap());
    fake.0.borrow_mut().print_busy = 2;
    cx.simulate_keystrokes("cmd-p");
    dispatch(cx);
    assert_eq!(printed(&fake), [native]);
    view.update(cx, |view, cx| {
        view.print_page(cx);
        view.run_command(crate::window::Command::NextTab, cx);
    });
    dispatch(cx);
    assert!(printed(&fake).is_empty());
}

#[gpui::test]
fn print_from_cef_and_library_preview_uses_registered_shortcut_without_new_page(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    let native = raw(fake.0.borrow().focused_page.unwrap());
    let shortcut = crate::window::page_shortcuts()
        .into_iter()
        .find(|key| key.cmd && !key.shift && !key.alt && !key.ctrl && key.key == "p")
        .unwrap();
    fake.send(cx, fake.shortcut(shortcut));
    dispatch(cx);
    assert_eq!(printed(&fake), [native]);
    cx.simulate_keystrokes("cmd-shift-j");
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| view.print_available()),
        "native preview is printable after layout"
    );
    assert!(
        cx.update(|window, cx| window.is_action_available(&crate::window::PrintPage, cx)),
        "Print action must update with preview visibility"
    );
    cx.simulate_keystrokes("cmd-p");
    dispatch(cx);
    assert_eq!(printed(&fake), [native]);
    click(cx, "library-nav-spaces");
    assert!(cx.update(|window, cx| window.is_action_available(&crate::window::PrintPage, cx)));
    view.update(cx, |view, cx| view.print_page(cx));
    dispatch(cx);
    assert_eq!(printed(&fake), [native]);
}

#[gpui::test]
fn print_targets_each_focused_split_pane_from_its_actual_header(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    active(cx);
    let tabs = view.read_with(cx, |view, _| {
        view.browser
            .related_tabs(view.browser.active_tab().unwrap().id)
    });
    assert_eq!(tabs.len(), 2);
    for tab in tabs {
        let selector: &'static str =
            Box::leak(format!("pane-header-{}", tab.get()).into_boxed_str());
        click(cx, selector);
        let target = fake.0.borrow().focused_page.unwrap();
        assert_eq!(
            view.read_with(cx, |view, _| view
                .pages
                .as_ref()
                .unwrap()
                .borrow()
                .identity_for(target)
                .unwrap()
                .tab),
            tab
        );
        fake.take_calls();
        cx.simulate_keystrokes("cmd-p");
        dispatch(cx);
        assert_eq!(printed(&fake), [raw(target)]);
    }
}

#[gpui::test]
fn queued_print_cannot_follow_a_reassigned_profile(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    view.update(cx, |view, cx| {
        view.print_page(cx);
        let space = view.browser.space().id;
        view.edit_profile(|browser| {
            let other = browser.create_user_profile("Print replacement").unwrap();
            browser.assign_space_user_profile(space, other).unwrap();
        });
    });
    dispatch(cx);
    assert!(printed(&fake).is_empty());
}

#[gpui::test]
fn newer_command_bar_intent_cancels_a_queued_print(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    view.update(cx, |view, cx| view.print_page(cx));
    cx.simulate_keystrokes("cmd-t");
    dispatch(cx);
    assert!(view.read_with(cx, |view, _| view.command_bar.is_some()));
    assert!(printed(&fake).is_empty());
}

#[gpui::test]
fn closing_the_root_cancels_print_even_if_its_entity_is_retained(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    let root = view.read_with(cx, |view, _| view.main_window.window_id());
    view.update(cx, |view, cx| {
        view.print_page(cx);
        view.close_window(cx);
    });
    dispatch(cx);
    assert!(!cx
        .cx
        .update(|cx| cx.windows().iter().any(|window| window.window_id() == root)));
    assert!(printed(&fake).is_empty());
}

#[gpui::test]
fn ui_only_windows_do_not_offer_a_nonfunctional_print_action(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    assert!(!cx.update(|window, cx| window.is_action_available(&crate::window::PrintPage, cx)));
    view.update(cx, |view, cx| view.print_page(cx));
    dispatch(cx);
    assert!(!view.read_with(cx, |view, _| view.print_available()));
}

#[gpui::test]
fn print_waits_for_the_owned_popup_to_return_native_activation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    let native = raw(fake.0.borrow().focused_page.unwrap());
    run_named_command(&view, cx, "print page");
    // Model the real AppKit gap: the overlay has been removed and activation
    // requested, but the parent key notification has not arrived yet.
    cx.deactivate_window();
    dispatch(cx);
    assert!(printed(&fake).is_empty());
    active(cx);
    dispatch(cx);
    assert_eq!(printed(&fake), [native]);
}

#[gpui::test]
fn print_activation_timeout_cannot_fire_on_a_later_window_activation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    active(cx);
    run_named_command(&view, cx, "print page");
    cx.deactivate_window();
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert!(printed(&fake).is_empty());
    active(cx);
    dispatch(cx);
    assert!(printed(&fake).is_empty());
    // An inactive request with no popup handoff receives no grace period.
    cx.deactivate_window();
    view.update(cx, |view, cx| {
        view.run_command(crate::window::Command::PrintPage, cx)
    });
    dispatch(cx);
    active(cx);
    dispatch(cx);
    assert!(printed(&fake).is_empty());
}
