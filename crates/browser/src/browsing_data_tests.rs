use super::*;
use std::time::SystemTime;
fn data(
    v: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
    profile: browser_core::UserProfileId,
) {
    v.update_in(cx, |v, w, cx| v.open_browsing_data(profile, w, cx));
    cx.run_until_parked();
    click(cx, "delete-browsing-data-row");
    cx.run_until_parked();
    assert!(cx.debug_bounds("data-dialog").is_some());
}
#[gpui::test]
fn browsing_data_cancel_ranges_categories_and_other_data_do_not_delete(cx: &mut TestAppContext) {
    let (v, cx, fake) = open_pages(cx);
    let p = v.read_with(cx, |v, _| v.browser.active_user_profile_id());
    v.update(cx, |v, _| {
        v.edit_browser(|b| b.record_visit(p, "https://history.test", SystemTime::now()))
    });
    data(&v, cx, p);
    click(cx, "data-range-4");
    click(cx, "data-more-range-1");
    assert_eq!(
        v.read_with(cx, |v, _| v.browsing_data.range),
        browser_core::browsing_data::BrowsingDataRange::AllTime
    );
    click(cx, "data-show-more");
    assert_eq!(
        v.read_with(cx, |v, _| v.browsing_data.mask),
        7,
        "Show more changed categories"
    );
    assert!(cx.debug_bounds("data-category-6").is_some());
    click(cx, "data-category-0");
    assert_eq!(v.read_with(cx, |v, _| v.browsing_data.mask), 6);
    click(cx, "data-category-1");
    assert_eq!(v.read_with(cx, |v, _| v.browsing_data.mask), 4);
    click(cx, "data-category-2");
    assert_eq!(v.read_with(cx, |v, _| v.browsing_data.mask), 0);
    click(cx, "data-delete");
    cx.run_until_parked();
    assert!(!fake
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::ClearBrowsingData(..))));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    click(cx, "delete-browsing-data-row");
    assert_eq!(
        v.read_with(cx, |v, _| v.browsing_data.mask),
        7,
        "Cancel must discard draft categories"
    );
    click(cx, "data-manage");
    cx.run_until_parked();
    click(cx, "data-other-done");
    assert!(
        !v.read_with(cx, |v, _| v.browsing_data.manage),
        "Done did not close Other data"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v
        .browser
        .active_tab()
        .unwrap()
        .url
        .starts_with(crate::browsing_data::PRIVACY)));
    assert_eq!(
        v.read_with(cx, |v, _| v.browser.user_profile(p).unwrap().history.len()),
        1
    );
}
#[gpui::test]
fn browsing_data_accepted_deletion_retains_unused_owner_across_navigation(cx: &mut TestAppContext) {
    let (v, cx, fake) = open_pages(cx);
    let (p, other) = v.update(cx, |v, _| {
        v.edit_browser(|b| {
            let p = b.active_user_profile_id();
            let other = b.create_user_profile("Data owner").unwrap();
            for id in [p, other] {
                b.record_visit(id, "https://history.test", SystemTime::now());
            }
            (p, other)
        })
    });
    let reply = Rc::new(RefCell::new(None));
    fake.0.borrow_mut().data_clear_reply = Some(reply.clone());
    data(&v, cx, other);
    click(cx, "data-range-4");
    click(cx, "data-more-range-1");
    assert_eq!(
        v.read_with(cx, |v, _| v.browsing_data.range),
        browser_core::browsing_data::BrowsingDataRange::AllTime
    );
    click(cx, "data-delete");
    cx.run_until_parked();
    assert!(fake.take_calls().iter().any(|c|matches!(c,Call::ClearBrowsingData(storage,7,-1) if *storage==crate::pages::StorageSelection::for_profile(other))));
    v.update(cx, |v, _| {
        v.edit_browser(|b| b.new_tab("https://other-navigation.test", SystemTime::now()))
    });
    *reply.borrow_mut() = Some(Ok(()));
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.profile.handle.read_settings(|b| b
            .user_profile(other)
            .unwrap()
            .history
            .len())),
        0
    );
    assert_eq!(
        v.read_with(cx, |v, _| v.browser.user_profile(p).unwrap().history.len()),
        1
    );
}
#[gpui::test]
fn browsing_data_native_failure_does_not_publish_false_local_success(cx: &mut TestAppContext) {
    let (v, cx, fake) = open_pages(cx);
    let p = v.read_with(cx, |v, _| v.browser.active_user_profile_id());
    v.update(cx, |v, _| {
        v.edit_browser(|b| b.record_visit(p, "https://retained.test", SystemTime::now()))
    });
    fake.0.borrow_mut().data_clear_reply = Some(Rc::new(RefCell::new(Some(Err(
        "synthetic native failure".into(),
    )))));
    data(&v, cx, p);
    click(cx, "data-delete");
    cx.run_until_parked();
    assert!(cx.debug_bounds("data-dialog").is_some());
    assert_eq!(
        v.read_with(cx, |v, _| v.browser.user_profile(p).unwrap().history.len()),
        1
    );
}

#[gpui::test]
fn browsing_data_direct_route_and_following_navigation_do_not_reuse_dialog_enter(
    cx: &mut TestAppContext,
) {
    let (v, cx, _) = open_pages(cx);
    cx.update(|w, _| w.activate_window());
    cx.simulate_keystrokes("cmd-t");
    bar(&v, cx).simulate_input("browser://settings/clearBrowserData?profile=1");
    bar(&v, cx).simulate_keystrokes("enter");
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v
        .browser
        .active_tab()
        .unwrap()
        .url
        .starts_with(crate::browsing_data::CLEAR)));
    click(cx, "data-cancel");
    cx.simulate_keystrokes("cmd-t");
    bar(&v, cx).simulate_input("https://after-data.test");
    bar(&v, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.browser.active_tab().unwrap().url.clone()),
        "https://after-data.test"
    );
}

#[gpui::test]
fn browsing_data_completed_deletion_returns_keyboard_to_privacy_page(cx: &mut TestAppContext) {
    let (v, cx, _) = open_pages(cx);
    let profile = v.read_with(cx, |v, _| v.browser.active_user_profile_id());
    cx.update(|w, _| w.activate_window());
    data(&v, cx, profile);
    click(cx, "data-category-0");
    click(cx, "data-delete");
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v
        .browser
        .active_tab()
        .unwrap()
        .url
        .starts_with(crate::browsing_data::PRIVACY)));
    cx.simulate_keystrokes("cmd-t");
    assert!(
        v.read_with(cx, |v, _| v.command_bar.is_some()),
        "Deletion left focus in the removed dialog"
    );
    bar(&v, cx).simulate_input("https://after-delete.test");
    bar(&v, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.browser.active_tab().unwrap().url.clone()),
        "https://after-delete.test"
    );
}

#[gpui::test]
fn browsing_data_settings_entry_keeps_the_selected_unused_profile(cx: &mut TestAppContext) {
    let (v, cx, _) = open_pages(cx);
    let other = v.update(cx, |v, _| {
        v.edit_browser(|b| b.create_user_profile("Clear fixture profile"))
            .unwrap()
    });
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-profile-Clear fixture profile");
    click(&mut settings, "settings-clear-browsing-data");
    cx.run_until_parked();
    assert!(active_url(&v, cx).is_some_and(|u| u
        == format!(
            "{}?profile={}",
            crate::browsing_data::PRIVACY,
            serde_json::to_string(&other).unwrap()
        )));
    assert_ne!(
        v.read_with(cx, |v, _| v.browser.active_user_profile_id()),
        other
    );
}

#[gpui::test]
fn browsing_data_range_menu_outside_click_only_dismisses_the_menu(cx: &mut TestAppContext) {
    let (v, cx, _) = open_pages(cx);
    let profile = v.read_with(cx, |v, _| v.browser.active_user_profile_id());
    data(&v, cx, profile);
    click(cx, "data-range-4");
    let bounds = cx.debug_bounds("data-category-0").unwrap();
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.browsing_data.mask),
        7,
        "Outside click reached the checkbox behind the menu"
    );
    click(cx, "data-category-0");
    assert_eq!(
        v.read_with(cx, |v, _| v.browsing_data.mask),
        6,
        "Outside click did not dismiss the menu"
    );
}
