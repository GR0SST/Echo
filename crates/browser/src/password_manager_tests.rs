use super::*;
const ORIGIN: &str = "http://127.0.0.1:53849";
fn manager(v: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    cx.update(|w, _| w.activate_window());
    cx.simulate_keystrokes("cmd-t");
    bar(v, cx).simulate_input("Manage Passwords");
    bar(v, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("password-manager-page").is_some());
}
fn add(v: &Entity<BrowserWindow>, cx: &mut VisualTestContext, user: &str) {
    click(cx, "password-add");
    cx.simulate_input(ORIGIN);
    cx.simulate_keystrokes("tab");
    cx.simulate_input(user);
    cx.simulate_keystrokes("tab");
    cx.simulate_input("synthetic-manager-test-password");
    cx.simulate_keystrokes("tab tab");
    cx.simulate_input("line1");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("line2");
    assert!(
        v.read_with(cx, |v, _| v.password_manager_snapshot().1),
        "Return in Note must insert a newline"
    );
    cx.simulate_keystrokes("tab tab enter");
    cx.run_until_parked();
    assert!(
        !v.read_with(cx, |v, _| v.password_manager_snapshot().1),
        "Save did not close its own editor"
    );
}
#[gpui::test]
fn password_manager_add_edit_copy_delete_and_undo_share_the_profile_vault(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    manager(&v, cx);
    add(&v, cx, "echo-test-user");
    assert_eq!(
        v.read_with(cx, |v, _| v.password_manager_snapshot()),
        (1, false, false)
    );
    assert!(v.read_with(cx, |v, _| v.password_manager_notes("line1\nline2")));
    click(cx, "password-row-0");
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v.password_manager_snapshot().2));
    click(cx, "password-edit-0");
    cx.simulate_input("changed-user");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.password_manager_snapshot()),
        (1, false, true)
    );
    click(cx, "password-copy-user-0");
    cx.run_until_parked();
    assert!(cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .is_some_and(|t| t == "changed-user"));
    click(cx, "password-copy-password-0");
    cx.run_until_parked();
    assert!(cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .is_some_and(|t| t == "synthetic-manager-test-password"));
    click(cx, "password-delete-0");
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.password_manager_snapshot()),
        (0, false, false)
    );
    click(cx, "password-undo");
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.password_manager_snapshot()),
        (1, false, false)
    );
    assert!(v.read_with(cx, |v, _| v.password_manager_notes("line1\nline2")));
    let session = v.read_with(cx, |v, _| v.session_json());
    assert!(!session.contains("synthetic-manager-test-password"));
    assert!(!session.contains("line1"));
}
#[gpui::test]
fn password_manager_cancel_and_duplicate_add_cannot_replace_a_saved_password(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx);
    manager(&v, cx);
    click(cx, "password-add");
    cx.simulate_input(ORIGIN);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.password_manager_snapshot()),
        (0, false, false)
    );
    add(&v, cx, "echo-test-user");
    click(cx, "password-add");
    cx.simulate_input(ORIGIN);
    cx.simulate_keystrokes("tab");
    cx.simulate_input("echo-test-user");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("must-not-replace");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.password_manager_snapshot()),
        (1, true, false)
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.password_manager_snapshot()),
        (1, false, false)
    );
}
#[gpui::test]
fn password_manager_settings_entry_owns_the_selected_unused_profile(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let (default, other) = v.update(cx, |v, _| {
        let a = v.browser.active_user_profile_id();
        let b = v
            .edit_browser(|b| b.create_user_profile("Password fixture profile"))
            .unwrap();
        (a, b)
    });
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-profile-Password fixture profile");
    click(&mut settings, "settings-passwords");
    cx.run_until_parked();
    assert!(active_url(&v, cx).is_some_and(|u| u.contains(&format!(
        "profile={}",
        serde_json::to_string(&other).unwrap()
    ))));
    add(&v, cx, "other-profile-user");
    assert_eq!(v.read_with(cx, |v, _| v.password_manager_snapshot()).0, 1);
    assert_eq!(
        v.read_with(cx, |v, _| v.browser.active_user_profile_id()),
        default
    );
    manager(&v, cx);
    assert_eq!(v.read_with(cx, |v, _| v.password_manager_snapshot()).0, 0);
}
#[gpui::test]
fn password_manager_deleted_owner_cannot_save_into_the_default_profile(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let other = v.update(cx, |v, _| {
        v.edit_browser(|b| b.create_user_profile("Disposable password owner"))
            .unwrap()
    });
    v.update_in(cx, |v, window, cx| {
        v.open_password_manager(other, window, cx)
    });
    cx.run_until_parked();
    click(cx, "password-add");
    cx.simulate_input(ORIGIN);
    cx.simulate_keystrokes("tab");
    cx.simulate_input("stale-test-user");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("stale-test-password");
    v.update(cx, |v, _| {
        v.edit_browser(|b| b.delete_user_profile(other)).unwrap()
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    manager(&v, cx);
    assert_eq!(v.read_with(cx, |v, _| v.password_manager_snapshot()).0, 0);
}
#[gpui::test]
fn password_manager_updates_another_window_for_the_same_profile(cx: &mut TestAppContext) {
    let (first, first_cx) = open(cx);
    manager(&first, first_cx);
    let (second, mut second_cx) = another_profile_window(&first, first_cx);
    manager(&second, &mut second_cx);
    first_cx.update(|w, _| w.activate_window());
    add(&first, first_cx, "shared-profile-user");
    second_cx.run_until_parked();
    assert_eq!(
        second
            .read_with(&second_cx, |v, _| v.password_manager_snapshot())
            .0,
        1
    );
}
#[gpui::test]
fn password_manager_old_filter_does_not_edit_a_replacement_profiles_route(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    manager(&view, cx);
    let old = view.read_with(cx, |v, _| v.password_manager_filter());
    let other = view.update(cx, |v, _| {
        v.edit_browser(|b| b.create_user_profile("Replacement password owner"))
            .unwrap()
    });
    view.update_in(cx, |v, w, cx| v.open_password_manager(other, w, cx));
    cx.run_until_parked();
    // An IME completion for the removed field may arrive after the profile changes.
    old.update(cx, |input, cx| input.set_text("previous-profile-query", cx));
    cx.run_until_parked();
    assert!(!active_url(&view, cx)
        .unwrap()
        .contains("previous-profile-query"));
    let current = view.read_with(cx, |v, _| v.password_manager_filter());
    assert!(current.read_with(cx, |i, _| i.text().is_empty()));
}
#[gpui::test]
fn password_manager_credentials_are_available_to_the_login_autofill_service(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let original = view.read_with(cx, |v, _| v.browser.space().active_tab.unwrap());
    manager(&view, cx);
    add(&view, cx, "shared-autofill-user");
    view.update(cx, |v, _| {
        v.edit_browser(|b| {
            b.set_url(original, ORIGIN);
            b.activate_tab(original, std::time::SystemTime::now());
        })
    });
    cx.run_until_parked();
    let native = fake.0.borrow().focused_page.unwrap();
    let token = engine::PasswordToken::from_raw(601).unwrap();
    fake.send(
        cx,
        Event::Password {
            view: native,
            event: engine::PasswordEvent::Document {
                token,
                origin: ORIGIN.into(),
                has_password_form: true,
            },
        },
    );
    fake.take_calls();
    fake.send(
        cx,
        Event::Password {
            view: native,
            event: engine::PasswordEvent::FillRequested {
                token,
                origin: ORIGIN.into(),
                username_hint: zeroize::Zeroizing::new(String::new()),
                anchor: None,
            field:1,
            },
        },
    );
    assert!(fake.take_calls().iter().any(|c|matches!(c,Call::PasswordFill(_,_,origin,credential) if origin==ORIGIN && credential.username()=="shared-autofill-user" && credential.password()=="synthetic-manager-test-password")));
}

#[gpui::test]
fn password_manager_appearance_changes_preserve_editor_input_and_focus(cx:&mut TestAppContext){
    let(v,cx)=open(cx);manager(&v,cx);click(cx,"password-add");cx.simulate_input("https://light-manager.test");cx.simulate_keystrokes("tab");cx.simulate_input("light-test-user");
    for mode in [browser_core::themes::ThemeAppearance::Light,browser_core::themes::ThemeAppearance::Dark] {
        v.update(cx,|v,_|v.edit_profile(|b|b.settings.theme_appearance=mode));cx.run_until_parked();
        assert!(cx.debug_bounds("password-editor").is_some());assert!(v.read_with(cx,|v,_|v.password_manager_snapshot().1));
    }
    cx.simulate_keystrokes("tab");cx.simulate_input("synthetic-light-test-only");cx.simulate_keystrokes("enter");cx.run_until_parked();assert_eq!(v.read_with(cx,|v,_|v.password_manager_snapshot().0),1);
}
