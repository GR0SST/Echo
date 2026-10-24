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
