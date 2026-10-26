use super::*;
use browser_core::settings::Toggle;
const NAME: &str = "Echo Profile Reference 20261023";

#[gpui::test]
fn profile_settings_rename_keeps_storage_pages_spaces_and_other_preferences(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (id, spaces) = view.read_with(cx, |view, _| {
        (
            view.browser.default_user_profile_id(),
            view.browser
                .spaces
                .iter()
                .map(|space| (space.id, space.user_profile_id))
                .collect::<Vec<_>>(),
        )
    });
    let old_storage = id.storage_key();
    fake.take_calls();
    cx.simulate_keystrokes("cmd-,");
    let (preferences, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-rename-profile");
    assert_eq!(
        settings
            .debug_bounds("settings-profile-sheet")
            .unwrap()
            .size,
        gpui::size(gpui::px(496.), gpui::px(256.))
    );
    settings.simulate_input("Renamed profile");
    assert_eq!(
        settings.update(|window, cx| preferences.read(cx).editor_debug(window, cx)),
        ("Renamed profile".into(), true, false)
    );
    settings.simulate_keystrokes("enter");
    settings.run_until_parked();
    assert!(!preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    view.read_with(cx, |view, _| {
        view.profile.handle.read_settings(|browser| {
            assert_eq!(browser.user_profile(id).unwrap().name(), "Renamed profile");
            assert_eq!(id.storage_key(), old_storage);
            assert_eq!(
                browser
                    .spaces
                    .iter()
                    .map(|space| (space.id, space.user_profile_id))
                    .collect::<Vec<_>>(),
                spaces
            );
        })
    });
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::Close(_) | Call::Navigate(..))));
}
#[gpui::test]
fn profile_settings_delete_only_unused_profiles_and_require_the_exact_captured_name(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let id = view.update(cx, |view, _| {
        view.edit_browser(|browser| browser.create_user_profile(NAME).unwrap())
    });
    cx.simulate_keystrokes("cmd-,");
    let (preferences, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-delete-profile");
    assert!(!preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    click(
        &mut settings,
        test_selector(format!("settings-profile-{NAME}")),
    );
    click(&mut settings, "settings-delete-profile");
    assert_eq!(
        settings
            .debug_bounds("settings-profile-sheet")
            .unwrap()
            .size,
        gpui::size(gpui::px(450.), gpui::px(350.))
    );
    settings.simulate_input("echo profile reference 20261023");
    settings.simulate_keystrokes("enter");
    assert!(preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    settings.simulate_keystrokes("cmd-a");
    settings.simulate_input(NAME);
    assert_eq!(
        settings.update(|window, cx| preferences.read(cx).editor_debug(window, cx)),
        (NAME.into(), true, false)
    );
    settings.simulate_keystrokes("enter");
    settings.run_until_parked();
    assert!(!preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    assert!(view.read_with(cx, |view, _| view
        .profile
        .handle
        .read_settings(|browser| browser.user_profile(id).is_none())));
    let next = view.update(cx, |view, _| {
        view.edit_browser(|browser| browser.create_user_profile(NAME).unwrap())
    });
    assert_ne!(next.storage_key(), id.storage_key());
}
#[gpui::test]
fn profile_settings_assigned_profile_cannot_be_deleted_and_cancel_does_not_rename(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let id = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let id = browser.create_user_profile("Assigned profile").unwrap();
            browser
                .assign_space_user_profile(browser.spaces[1].id, id)
                .unwrap();
            id
        })
    });
    cx.simulate_keystrokes("cmd-,");
    let (preferences, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-profile-Assigned profile");
    click(&mut settings, "settings-delete-profile");
    assert!(!preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    click(&mut settings, "settings-rename-profile");
    settings.simulate_input("Cancelled name");
    settings.simulate_keystrokes("escape");
    assert!(!preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    assert_eq!(
        view.read_with(cx, |view, _| view.profile.handle.read_settings(|browser| {
            browser.user_profile(id).unwrap().name().to_owned()
        })),
        "Assigned profile"
    );
}
#[gpui::test]
fn profile_settings_stale_delete_and_rename_cannot_overwrite_an_external_change(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let id = view.update(cx, |view, _| {
        view.edit_browser(|browser| browser.create_user_profile(NAME).unwrap())
    });
    cx.simulate_keystrokes("cmd-,");
    let (preferences, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(
        &mut settings,
        test_selector(format!("settings-profile-{NAME}")),
    );
    click(&mut settings, "settings-delete-profile");
    settings.simulate_input(NAME);
    view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            browser
                .assign_space_user_profile(browser.spaces[1].id, id)
                .unwrap();
        })
    });
    settings.run_until_parked();
    assert!(!preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    assert!(view.read_with(cx, |view, _| view
        .profile
        .handle
        .read_settings(|browser| browser.user_profile(id).is_some())));
    click(&mut settings, "settings-rename-profile");
    settings.simulate_input("Stale name");
    view.update(cx, |view, _| {
        view.edit_browser(|browser| browser.rename_user_profile(id, "Newer name").unwrap())
    });
    settings.run_until_parked();
    assert!(!preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    assert_eq!(
        view.read_with(cx, |view, _| view.profile.handle.read_settings(|browser| {
            browser.user_profile(id).unwrap().name().to_owned()
        })),
        "Newer name"
    );
}
#[gpui::test]
fn profile_settings_sheet_keeps_keyboard_focus_inside_and_blocks_background_changes(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-,");
    let (preferences, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-rename-profile");
    settings.simulate_keystrokes("tab");
    settings.simulate_keystrokes("enter");
    assert!(!preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    click(&mut settings, "settings-tab-General");
    let enabled = view.read_with(cx, |view, _| {
        view.profile
            .handle
            .read_settings(|browser| browser.settings.enabled(Toggle::WarnBeforeQuitting))
    });
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-rename-profile");
    // The dimmed tab remains behind the sheet; its click cannot leave Profiles.
    click(&mut settings, "settings-tab-General");
    assert!(preferences.read_with(&settings, |view, _| view.profile_editor_open()));
    assert_eq!(
        preferences.read_with(&settings, |view, _| view.tab),
        crate::settings::Tab::Profiles
    );
    settings.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(cx, |view, _| view.profile.handle.read_settings(|browser| {
            browser.settings.enabled(Toggle::WarnBeforeQuitting)
        })),
        enabled
    );
}

#[gpui::test]
fn profile_reclamation_uses_loaded_retirements_and_never_an_unsaved_live_deletion(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let fixture = tempfile::tempdir().unwrap();
    let handle = view.read_with(cx, |view, _| view.profile.handle.clone());
    let id = handle.edit_settings(|browser| browser.create_user_profile("Retirement fixture").unwrap());
    handle.edit_settings(|browser| browser.delete_user_profile(id).unwrap());
    cx.update(|_, cx| handle.save_session_to(fixture.path().join("session.json"), cx));
    assert!(cx.read(|cx| handle.reclaimable_profile_storage_keys(cx)).is_empty());
}

#[gpui::test]
fn restored_shared_page_constructor_reclaims_only_explicitly_loaded_retirements(cx: &mut TestAppContext) {
    cx.update(init);
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("session.json");
    let mut browser = browser_core::Browser::new();
    let id = browser.create_user_profile("Retired fixture").unwrap();
    browser.delete_user_profile(id).unwrap();
    browser_core::session::save(&path, &browser_core::session::to_json(&browser)).unwrap();
    let profile = cx.update(|cx| crate::new_window::source_from_session(&path, cx).unwrap().profile);
    let fake = FakeEngine::default();
    let engine = fake.clone();
    let (_, visual) = cx.add_window_view(|window, cx| {
        BrowserWindow::with_profile_pages(profile, None, Some(crate::pages::Pages::new(Box::new(engine))), window, cx)
    });
    visual.run_until_parked();
    assert_eq!(fake.0.borrow().retired_storage, vec![id.storage_key().unwrap()]);
}
