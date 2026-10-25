use super::*;
use engine::{PermissionKind as K, PermissionValue as V};
use std::time::SystemTime;
fn install(
    view: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
    fake: &FakeEngine,
) -> (crate::pages::StorageSelection, String) {
    let profile = view.read_with(cx, |view, _| view.browser.active_user_profile_id());
    let storage = crate::pages::StorageSelection::for_profile(profile);
    let origin =
        browser_core::site_settings::SiteSettings::origin(&active_url(view, cx).unwrap()).unwrap();
    fake.0.borrow_mut().site_infos.insert(
        1,
        engine::SiteInfo {
            origin: origin.clone(),
            host: "example.com".into(),
            secure: true,
            certificate: None,
            zoom_percent: 100,
            permissions: vec![],
        },
    );
    fake.0.borrow().profile_settings.borrow_mut().insert(
        (storage.clone(), origin.clone()),
        K::ALL
            .into_iter()
            .map(|kind| engine::SiteSetting {
                kind,
                value: V::Ask,
                default_value: V::Ask,
                uses_default: Some(true),
            })
            .collect(),
    );
    (storage, origin)
}
#[gpui::test]
fn site_details_opens_from_last_more_row_in_peek_and_edits_its_canonical_profile(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (storage, origin) = install(&view, cx, &fake);
    let original = active_url(&view, cx);
    click(cx, "address-site-controls");
    let overlay = view.read_with(cx, |v, _| {
        v.site_controls.as_ref().unwrap().overlay.unwrap()
    });
    let mut controls = VisualTestContext::from_window(overlay.into(), &cx.cx);
    controls.run_until_parked();
    click(&mut controls, "site-more");
    controls.simulate_keystrokes("up enter");
    cx.run_until_parked();
    let (_, mut page) = peek_window(&view, cx);
    page.update(|window, _| window.refresh());
    page.run_until_parked();
    assert!(page.debug_bounds("site-settings-page").is_some());
    // Merely opening Settings must not reclaim a deliberate source activation.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.is_window_active()));
    page.update(|window, _| window.activate_window());
    click(&mut page, "site-setting-control-Camera");
    page.simulate_keystrokes("up enter");
    assert_eq!(
        fake.0.borrow().profile_settings.borrow()[&(storage.clone(), origin.clone())]
            .iter()
            .find(|s| s.kind == K::Camera)
            .unwrap()
            .value,
        V::Block
    );
    let profile = view.read_with(cx, |v, _| v.browser.active_user_profile_id());
    assert!(
        view.read_with(cx, |v, _| v.profile.handle.read_settings(|b| b
            .user_profile(profile)
            .unwrap()
            .site_settings
            .permissions(&origin)
            .any(|p| p == browser_core::site_settings::Permission::Camera)))
    );
    click(&mut page, "site-settings-reset");
    click(&mut page, "site-reset-cancel");
    assert_eq!(
        fake.0.borrow().profile_settings.borrow()[&(storage.clone(), origin.clone())]
            .iter()
            .find(|s| s.kind == K::Camera)
            .unwrap()
            .value,
        V::Block
    );
    click(&mut page, "site-settings-reset");
    click(&mut page, "site-reset-confirm");
    assert!(
        fake.0.borrow().profile_settings.borrow()[&(storage, origin.clone())]
            .iter()
            .all(|s| s.uses_default == Some(true))
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.profile.handle.read_settings(|b| b
            .user_profile(profile)
            .unwrap()
            .site_settings
            .permissions(&origin)
            .count())),
        0
    );
    fake.take_calls();
    click(&mut page, "site-settings-delete");
    click(&mut page, "site-reset-cancel");
    assert!(!fake
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::ClearSiteData(..))));
    let child = fake.0.borrow().forks.last().unwrap().clone();
    child.take_calls();
    click(&mut page, "site-settings-delete");
    click(&mut page, "site-delete-confirm");
    assert!(fake.take_calls().iter().any(|c| matches!(c, Call::ClearSiteData(_, site, engine::ClearSiteData::Cookies) if site == &origin)));
    assert!(child.take_calls().contains(&Call::FocusUi));
    // A late CEF focus notification from the refreshed, covered source must
    // not leave the visible settings document without keyboard input.
    cx.update(|window, _| window.activate_window());
    fake.send(
        cx,
        Event::Focused {
            view: ViewId::from_raw(1),
        },
    );
    cx.run_until_parked();
    page.run_until_parked();
    assert!(page.update(|window, _| window.is_window_active()));
    page.simulate_keystrokes("escape");
    page.executor().advance_clock(Duration::from_millis(220));
    page.run_until_parked();
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), original);
    assert!(view.read_with(cx, |v, _| v.peek_child.is_none()));
}
#[gpui::test]
fn an_expired_site_data_clear_does_not_reclaim_source_activation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    install(&view, cx, &fake);
    view.update(cx, |view, cx| view.open_all_site_settings(cx));
    cx.run_until_parked();
    let (_, mut page) = peek_window(&view, cx);
    page.update(|window, _| window.refresh());
    page.run_until_parked();
    click(&mut page, "site-settings-delete");
    click(&mut page, "site-delete-confirm");
    page.executor().advance_clock(Duration::from_secs(7));
    page.run_until_parked();
    cx.run_until_parked();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.is_window_active()));
    assert!(!page.update(|window, _| window.is_window_active()));
}
#[gpui::test]
fn unavailable_profile_settings_cannot_write_through_a_stale_dropdown(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let (storage, origin) = install(&view, cx, &fake);
    view.update(cx, |view, cx| view.open_all_site_settings(cx));
    cx.run_until_parked();
    let (_, mut page) = peek_window(&view, cx);
    page.update(|window, _| window.refresh());
    page.run_until_parked();
    click(&mut page, "site-setting-control-Camera");
    fake.0
        .borrow()
        .profile_settings
        .borrow_mut()
        .remove(&(storage, origin));
    let child = fake.0.borrow().forks.last().unwrap().clone();
    child.take_calls();
    click(&mut page, "site-setting-choice-Allow");
    assert!(!child
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::ProfileSiteSetting(..))));
}
#[gpui::test]
fn named_site_settings_cannot_write_a_deleted_profile_or_fall_back_to_default(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let id = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let id = browser.create_user_profile("Work").unwrap();
            let space = browser.space().id;
            browser.assign_space_user_profile(space, id).unwrap();
            browser.new_tab("https://settings.example", SystemTime::now());
            id
        })
    });
    cx.run_until_parked();
    let (storage, origin) = install(&view, cx, &fake);
    assert!(matches!(storage, crate::pages::StorageSelection::Named(_)));
    let mut other =
        fake.0.borrow().profile_settings.borrow()[&(storage.clone(), origin.clone())].clone();
    for row in &mut other {
        row.value = V::Allow;
        row.default_value = V::Allow;
    }
    fake.0.borrow().profile_settings.borrow_mut().insert(
        (crate::pages::StorageSelection::Default, origin.clone()),
        other,
    );
    view.update(cx, |view, cx| view.open_all_site_settings(cx));
    cx.run_until_parked();
    let (_, mut page) = peek_window(&view, cx);
    page.update(|window, _| window.refresh());
    page.run_until_parked();
    click(&mut page, "site-setting-control-Camera");
    click(&mut page, "site-setting-choice-Block");
    assert_eq!(
        fake.0.borrow().profile_settings.borrow()[&(storage, origin.clone())]
            .iter()
            .find(|s| s.kind == K::Camera)
            .unwrap()
            .value,
        V::Block
    );
    assert!(fake.0.borrow().profile_settings.borrow()
        [&(crate::pages::StorageSelection::Default, origin)]
        .iter()
        .all(|s| s.value == V::Allow));
    click(&mut page, "site-setting-control-Camera");
    view.update(cx, |view, _| {
        view.profile.handle.edit_settings(|browser| {
            let space = browser.space().id;
            browser
                .assign_space_user_profile(space, browser.default_user_profile_id())
                .unwrap();
            browser.delete_user_profile(id).unwrap();
        })
    });
    let child = fake.0.borrow().forks.last().unwrap().clone();
    child.take_calls();
    click(&mut page, "site-setting-choice-Allow");
    assert!(!child
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::ProfileSiteSetting(..))));
}
#[gpui::test]
fn queued_site_settings_creation_cannot_retarget_a_reassigned_space(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    install(&view, cx, &fake);
    view.update(cx, |view, cx| {
        view.open_all_site_settings(cx);
        view.edit_browser(|browser| {
            let id = browser.create_user_profile("Other").unwrap();
            let space = browser.space().id;
            browser.assign_space_user_profile(space, id).unwrap();
        });
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.peek_child.is_none()));
    assert!(fake.0.borrow().forks.is_empty());
}
