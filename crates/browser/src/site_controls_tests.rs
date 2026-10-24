use super::*;

fn install_site_info(
    view: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
    fake: &FakeEngine,
    secure: bool,
) -> String {
    let url = active_url(view, cx).unwrap();
    let origin = browser_core::site_settings::SiteSettings::origin(&url).unwrap();
    fake.0.borrow_mut().site_infos.insert(
        1,
        engine::SiteInfo {
            origin: origin.clone(),
            host: crate::window::address_host(&origin),
            secure,
            certificate: None,
            zoom_percent: 100,
            permissions: engine::PermissionKind::ALL
                .into_iter()
                .map(|kind| engine::SitePermission {
                    kind,
                    value: engine::PermissionValue::Ask,
                })
                .collect(),
        },
    );
    origin
}
fn panel(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    let overlay = view.read_with(cx, |view, _| {
        view.site_controls.as_ref().unwrap().overlay.unwrap()
    });
    let panel = VisualTestContext::from_window(overlay.into(), &cx.cx);
    panel.run_until_parked();
    panel
}

#[gpui::test]
fn site_security_uses_engine_verification_and_more_menu_keeps_its_own_dismissal(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "address-site-controls");
    let mut controls = panel(&view, cx);
    assert!(controls.debug_bounds("site-security-insecure").is_some());
    controls.simulate_keystrokes("escape");
    cx.run_until_parked();
    install_site_info(&view, cx, &fake, true);
    click(cx, "address-site-controls");
    let mut controls = panel(&view, cx);
    assert!(controls.debug_bounds("site-security-secure").is_some());
    assert_eq!(
        f32::from(controls.debug_bounds("site-controls").unwrap().size.width),
        236.
    );
    click(&mut controls, "site-more");
    assert_eq!(
        f32::from(controls.debug_bounds("site-more-menu").unwrap().size.width),
        162.
    );
    controls.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.site_controls.is_some()));
    controls.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.site_controls.is_none()));
}

#[gpui::test]
fn site_permission_toggle_uses_the_profile_origin_and_updates_the_live_engine(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let origin = install_site_info(&view, cx, &fake, true);
    fake.0
        .borrow_mut()
        .site_infos
        .get_mut(&1)
        .unwrap()
        .permissions
        .iter_mut()
        .find(|setting| setting.kind == engine::PermissionKind::Camera)
        .unwrap()
        .value = engine::PermissionValue::Block;
    view.update(cx, |view, cx| {
        view.edit_browser(|model| {
            model
                .active_user_profile_mut()
                .site_settings
                .record_permission(&origin, browser_core::site_settings::Permission::Camera);
        });
        cx.notify();
    });
    click(cx, "address-site-controls");
    let mut controls = panel(&view, cx);
    fake.take_calls();
    click(&mut controls, "site-permission-Camera");
    assert!(fake.take_calls().contains(&Call::SitePermission(
        1,
        origin.clone(),
        engine::PermissionKind::Camera,
        engine::PermissionValue::Allow
    )));
    assert_eq!(
        fake.0.borrow().site_infos[&1]
            .permissions
            .iter()
            .find(|setting| setting.kind == engine::PermissionKind::Camera)
            .unwrap()
            .value,
        engine::PermissionValue::Allow
    );
    // The request history records controls, not a separate source of grants.
    assert_eq!(
        view.read_with(cx, |view, _| view
            .browser
            .active_user_profile()
            .site_settings
            .permissions(&origin)
            .count()),
        1
    );
}

#[gpui::test]
fn site_clear_cookies_targets_the_opened_origin_and_shortcuts_return_to_the_main_window(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let origin = install_site_info(&view, cx, &fake, true);
    click(cx, "address-site-controls");
    let mut controls = panel(&view, cx);
    fake.take_calls();
    click(&mut controls, "site-more");
    controls.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(fake.take_calls().contains(&Call::ClearSiteData(
        1,
        origin,
        engine::ClearSiteData::Cookies
    )));
    assert!(view.read_with(cx, |view, _| view.site_controls.is_none()));
    click(cx, "address-site-controls");
    let mut controls = panel(&view, cx);
    controls.simulate_keystrokes("cmd-l");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.site_controls.is_none()));
    assert_eq!(bar_text(&view, cx), active_url(&view, cx));
    bar(&view, cx).simulate_keystrokes("escape");
    click(cx, "address-site-controls");
    let mut controls = panel(&view, cx);
    controls.simulate_keystrokes("cmd-shift-d");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.toolbar_shown()
        && view.site_controls.is_none()));
}

#[gpui::test]
fn stale_site_controls_do_not_apply_a_clear_to_a_new_document_origin(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    install_site_info(&view, cx, &fake, true);
    click(cx, "address-site-controls");
    let mut controls = panel(&view, cx);
    click(&mut controls, "site-more");
    fake.take_calls();
    // The renderer has navigated before its address event reaches the model.
    fake.0.borrow_mut().site_infos.get_mut(&1).unwrap().origin = "https://other.example".into();
    click(&mut controls, "site-more-action-1");
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::ClearSiteData(..))));
    view.update(cx, |view, cx| {
        let tab = view.browser.active_tab().unwrap().id;
        view.edit_browser(|model| model.set_url(tab, "https://other.example"));
        cx.notify();
    });
    controls.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.site_controls.is_none()));
}

fn permission_request(
    view: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
    id: u64,
    kind: engine::PermissionKind,
) -> engine::PermissionRequest {
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let origin =
        browser_core::site_settings::SiteSettings::origin(&active_url(view, cx).unwrap()).unwrap();
    engine::PermissionRequest {
        id: engine::PermissionRequestId::from_raw(id),
        origin: origin.clone(),
        top_origin: origin,
        permissions: vec![kind],
    }
}

#[gpui::test]
fn site_permission_prompt_resolves_explicit_choices_and_escape_leaves_it_open(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    for (id, kind, button, decision) in [
        (
            10,
            engine::PermissionKind::Camera,
            "site-permission-not-now",
            engine::PermissionDecision::Block,
        ),
        (
            11,
            engine::PermissionKind::Microphone,
            "site-permission-allow",
            engine::PermissionDecision::Allow,
        ),
    ] {
        let request = permission_request(&view, cx, id, kind);
        fake.send(
            cx,
            Event::PermissionRequested {
                view: page(1),
                request: request.clone(),
            },
        );
        let mut prompt = panel(&view, cx);
        assert_eq!(
            f32::from(
                prompt
                    .debug_bounds("site-permission-prompt")
                    .unwrap()
                    .size
                    .width
            ),
            236.
        );
        fake.take_calls();
        click(&mut prompt, button);
        cx.run_until_parked();
        assert_eq!(
            fake.take_calls()
                .iter()
                .filter(|call| matches!(call, Call::PermissionResponse(..)))
                .cloned()
                .collect::<Vec<_>>(),
            vec![Call::PermissionResponse(1, request.id, decision)]
        );
        assert!(view.read_with(cx, |view, _| view.site_controls.is_none()));
    }
    let request = permission_request(&view, cx, 12, engine::PermissionKind::Location);
    fake.send(
        cx,
        Event::PermissionRequested {
            view: page(1),
            request: request.clone(),
        },
    );
    let mut prompt = panel(&view, cx);
    fake.take_calls();
    prompt.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::PermissionResponse(..))));
    assert!(view.read_with(cx, |view, _| view.site_controls.is_some()));
    click(&mut prompt, "site-permission-not-now");
    cx.run_until_parked();
    assert!(fake.take_calls().contains(&Call::PermissionResponse(
        1,
        request.id,
        engine::PermissionDecision::Block
    )));
}

#[gpui::test]
fn permission_prompts_reject_stale_origin_and_cannot_survive_engine_cancellation(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let mut stale = permission_request(&view, cx, 20, engine::PermissionKind::Camera);
    stale.top_origin = "https://other.example".into();
    fake.send(
        cx,
        Event::PermissionRequested {
            view: page(1),
            request: stale.clone(),
        },
    );
    assert!(view.read_with(cx, |view, _| view.site_controls.is_none()));
    assert!(fake.take_calls().contains(&Call::PermissionResponse(
        1,
        stale.id,
        engine::PermissionDecision::Dismiss
    )));
    let request = permission_request(&view, cx, 21, engine::PermissionKind::Notifications);
    fake.send(
        cx,
        Event::PermissionRequested {
            view: page(1),
            request: request.clone(),
        },
    );
    let _prompt = panel(&view, cx);
    fake.take_calls();
    fake.send(
        cx,
        Event::PermissionDismissed {
            view: page(1),
            request: request.id,
        },
    );
    assert!(view.read_with(cx, |view, _| view.site_controls.is_none()));
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::PermissionResponse(..))));
}

#[gpui::test]
fn permission_prompt_navigation_and_command_bar_shortcuts_cannot_allow_the_old_page(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let request = permission_request(&view, cx, 30, engine::PermissionKind::Camera);
    fake.send(
        cx,
        Event::PermissionRequested {
            view: page(1),
            request: request.clone(),
        },
    );
    let mut prompt = panel(&view, cx);
    fake.take_calls();
    prompt.simulate_keystrokes("cmd-l");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.command_bar.is_some()));
    assert!(fake.take_calls().contains(&Call::PermissionResponse(
        1,
        request.id,
        engine::PermissionDecision::Dismiss
    )));
    view.update(cx, |view, cx| view.close_command_bar(cx));
    cx.run_until_parked();
    let request = permission_request(&view, cx, 31, engine::PermissionKind::Camera);
    fake.send(
        cx,
        Event::PermissionRequested {
            view: page(1),
            request: request.clone(),
        },
    );
    let _prompt = panel(&view, cx);
    fake.take_calls();
    fake.send(
        cx,
        Event::UrlChanged {
            view: page(1),
            url: "https://other.example/new".into(),
        },
    );
    assert!(view.read_with(cx, |view, _| view.site_controls.is_none()));
    assert!(!fake.take_calls().contains(&Call::PermissionResponse(
        1,
        request.id,
        engine::PermissionDecision::Allow
    )));
}

#[gpui::test]
fn a_background_window_cannot_steal_focus_with_a_permission_prompt(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let request = permission_request(&view, cx, 50, engine::PermissionKind::Camera);
    cx.deactivate_window();
    fake.send(
        cx,
        Event::PermissionRequested {
            view: page(1),
            request: request.clone(),
        },
    );
    assert!(view.read_with(cx, |view, _| view.site_controls.is_none()));
    assert!(fake.take_calls().contains(&Call::PermissionResponse(
        1,
        request.id,
        engine::PermissionDecision::Dismiss
    )));
}

#[gpui::test]
fn an_old_permission_button_cannot_allow_a_replacement_or_changed_origin(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let old = permission_request(&view, cx, 60, engine::PermissionKind::Camera);
    fake.send(
        cx,
        Event::PermissionRequested {
            view: page(1),
            request: old.clone(),
        },
    );
    let _old_panel = panel(&view, cx);
    fake.send(
        cx,
        Event::PermissionDismissed {
            view: page(1),
            request: old.id,
        },
    );
    let current = permission_request(&view, cx, 61, engine::PermissionKind::Microphone);
    fake.send(
        cx,
        Event::PermissionRequested {
            view: page(1),
            request: current.clone(),
        },
    );
    let _current_panel = panel(&view, cx);
    fake.take_calls();
    // A queued click from the retired child carries its original request ID.
    view.update(cx, |view, cx| {
        view.resolve_site_permission(old.id, engine::PermissionDecision::Allow, cx)
    });
    cx.run_until_parked();
    assert!(fake.take_calls().is_empty());
    assert!(view.read_with(cx, |view, _| view.site_controls.is_some()));
    // A navigation may update the model before the child's next render.
    view.update(cx, |view, cx| {
        view.edit_browser(|model| {
            model.set_url(model.active_tab().unwrap().id, "https://other.example")
        });
        view.resolve_site_permission(current.id, engine::PermissionDecision::Allow, cx);
    });
    cx.run_until_parked();
    assert!(fake.take_calls().contains(&Call::PermissionResponse(
        1,
        current.id,
        engine::PermissionDecision::Dismiss
    )));
}

#[gpui::test]
fn an_unanswered_permission_is_not_misreported_as_a_stored_block(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    install_site_info(&view, cx, &fake, true);
    let request = permission_request(&view, cx, 70, engine::PermissionKind::Camera);
    fake.send(
        cx,
        Event::PermissionRequested {
            view: page(1),
            request: request.clone(),
        },
    );
    let _prompt = panel(&view, cx);
    fake.send(
        cx,
        Event::PermissionDismissed {
            view: page(1),
            request: request.id,
        },
    );
    click(cx, "address-site-controls");
    let mut controls = panel(&view, cx);
    assert!(controls.debug_bounds("site-permission-Camera").is_none());
}

#[gpui::test]
fn site_share_opens_system_services_for_the_full_url_and_keeps_the_panel(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    visual.simulate_keystrokes("cmd-l");
    bar(&view, visual).simulate_input("https://share.example/path?query=exact#fragment");
    bar(&view, visual).simulate_keystrokes("enter");
    visual.run_until_parked();
    let url = active_url(&view, visual).unwrap();
    assert_eq!(url, "https://share.example/path?query=exact#fragment");
    click(visual, "address-site-controls");
    let mut controls = panel(&view, visual);
    click(&mut controls, "site-share");
    view.read_with(visual, |view, _| {
        assert_eq!(view.share_picker.as_ref().unwrap().urls, vec![url]);
        assert!(view.site_controls.is_some());
    });
    controls.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.site_controls.is_none()));
}

#[gpui::test]
fn site_share_uses_the_current_url_after_navigation_within_the_same_origin(
    cx: &mut TestAppContext,
) {
    let (view, visual, _) = open_pages(cx);
    click(visual, "address-site-controls");
    let mut controls = panel(&view, visual);
    let bounds = controls.debug_bounds("site-share").unwrap();
    controls.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    controls.run_until_parked();
    let url = view.update(visual, |view, _| {
        let origin = browser_core::site_settings::SiteSettings::origin(
            &view.browser.active_tab().unwrap().url,
        )
        .unwrap();
        let url = format!("{origin}/changed-before-menu?exact=true#part");
        view.edit_browser(|model| model.set_url(model.active_tab().unwrap().id, &url));
        url
    });
    controls.simulate_click(bounds.center(), Modifiers::none());
    controls.run_until_parked();
    visual.run_until_parked();
    view.read_with(visual, |view, _| {
        assert_eq!(view.share_picker.as_ref().unwrap().urls, vec![url])
    });
}
