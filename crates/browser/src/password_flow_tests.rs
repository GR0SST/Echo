use super::*;
use engine::{LoginCredential, PasswordEvent as P, PasswordToken};
const ORIGIN: &str = "http://127.0.0.1:8123";
fn token(n: u128) -> PasswordToken {
    PasswordToken::from_raw(n).unwrap()
}
fn send(f: &FakeEngine, cx: &mut VisualTestContext, view: ViewId, event: P) {
    f.send(cx, Event::Password { view, event });
}
fn begin(v: &Entity<BrowserWindow>, f: &FakeEngine, cx: &mut VisualTestContext) -> ViewId {
    let page = f.0.borrow().focused_page.expect("focused login page");
    send(
        f,
        cx,
        page,
        P::Document {
            token: token(1),
            origin: ORIGIN.into(),
            has_password_form: true,
        },
    );
    send(
        f,
        cx,
        page,
        P::Submitted {
            token: token(1),
            origin: ORIGIN.into(),
            credential: LoginCredential::new("echo-test-user", "synthetic-password-only").unwrap(),
        },
    );
    assert!(
        v.read_with(cx, |v, _| v.password_overlay()).is_none(),
        "Do not offer to save while the login form remains"
    );
    send(
        f,
        cx,
        page,
        P::Document {
            token: token(2),
            origin: ORIGIN.into(),
            has_password_form: false,
        },
    );
    page
}
#[gpui::test]
fn password_save_sheet_enter_saves_and_escape_declines(cx: &mut TestAppContext) {
    let f = FakeEngine::default();
    let (v, cx) = open_with(cx, Some(Box::new(f.clone())));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let page = begin(&v, &f, cx);
    let child = v
        .read_with(cx, |v, _| v.password_overlay())
        .expect("save sheet");
    let mut sheet = VisualTestContext::from_window(child.into(), &cx.cx);
    sheet.run_until_parked();
    assert_eq!(
        sheet.debug_bounds("password-save-sheet").unwrap().size,
        gpui::size(gpui::px(450.), gpui::px(273.))
    );
    sheet.simulate_keystrokes("enter");
    sheet.run_until_parked();
    v.update(&mut sheet, |v, cx| {
        assert_eq!(v.password_saved(ORIGIN, cx), (false, 1))
    });
    assert!(v.read_with(&sheet, |v, _| v.password_overlay()).is_none());
    send(
        &f,
        cx,
        page,
        P::Document {
            token: token(3),
            origin: ORIGIN.into(),
            has_password_form: true,
        },
    );
    f.take_calls();
    send(
        &f,
        cx,
        page,
        P::FillRequested {
            token: token(3),
            origin: ORIGIN.into(),
            username_hint: zeroize::Zeroizing::new(String::new()),
            anchor: None,
            field: 1,
        },
    );
    assert!(
        f.take_calls()
            .iter()
            .any(|c| matches!(c,Call::PasswordFill(_,t,o,_) if *t==token(3) && o==ORIGIN))
    );
    send(
        &f,
        cx,
        page,
        P::FillRequested {
            token: token(2),
            origin: ORIGIN.into(),
            username_hint: zeroize::Zeroizing::new(String::new()),
            anchor: None,
            field: 1,
        },
    );
    assert!(
        !f.take_calls()
            .iter()
            .any(|c| matches!(c, Call::PasswordFill(..)))
    );
}
#[gpui::test]
fn password_never_policy_and_escape_do_not_save(cx: &mut TestAppContext) {
    let f = FakeEngine::default();
    let (v, cx) = open_with(cx, Some(Box::new(f.clone())));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    begin(&v, &f, cx);
    let child = v.read_with(cx, |v, _| v.password_overlay()).unwrap();
    let mut sheet = VisualTestContext::from_window(child.into(), &cx.cx);
    sheet.simulate_keystrokes("escape");
    sheet.run_until_parked();
    v.update(&mut sheet, |v, cx| {
        assert_eq!(v.password_saved(ORIGIN, cx), (false, 0))
    });
    begin(&v, &f, cx);
    let child = v.read_with(cx, |v, _| v.password_overlay()).unwrap();
    let mut sheet = VisualTestContext::from_window(child.into(), &cx.cx);
    click(&mut sheet, "password-never");
    sheet.run_until_parked();
    v.update(&mut sheet, |v, cx| {
        assert_eq!(v.password_saved(ORIGIN, cx), (true, 0))
    });
    begin(&v, &f, cx);
    assert!(v.read_with(cx, |v, _| v.password_overlay()).is_none());
}
#[gpui::test]
fn password_new_document_invalidates_open_save_request(cx: &mut TestAppContext) {
    let f = FakeEngine::default();
    let (v, cx) = open_with(cx, Some(Box::new(f.clone())));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let page = begin(&v, &f, cx);
    assert!(v.read_with(cx, |v, _| v.password_overlay()).is_some());
    send(
        &f,
        cx,
        page,
        P::Document {
            token: token(3),
            origin: "https://other.invalid".into(),
            has_password_form: false,
        },
    );
    assert!(v.read_with(cx, |v, _| v.password_overlay()).is_none());
    v.update(cx, |v, cx| {
        assert_eq!(v.password_saved(ORIGIN, cx), (false, 0))
    });
}
#[gpui::test]
fn password_completion_waits_for_its_own_window_to_regain_activation(cx: &mut TestAppContext) {
    let f = FakeEngine::default();
    let (v, cx) = open_with(cx, Some(Box::new(f.clone())));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.deactivate_window();
    begin(&v, &f, cx);
    assert!(v.read_with(cx, |v, _| v.password_overlay()).is_none());
    assert!(!cx.update(|w, _| w.is_window_active()));
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v.password_overlay()).is_some());
}

#[gpui::test]
fn password_iframe_fill_uses_the_frame_origin_and_requires_cross_origin_selection(
    cx: &mut TestAppContext,
) {
    let f = FakeEngine::default();
    let (v, cx) = open_with(cx, Some(Box::new(f.clone())));
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    let page = f.0.borrow().focused_page.unwrap();
    v.update(cx, |v, cx| {
        let profile = v
            .browser
            .active_user_profile_id()
            .storage_key()
            .unwrap_or_else(|| "default".into());
        crate::password_flow::storage(cx)
            .save(
                &profile,
                ORIGIN,
                &LoginCredential::new("iframe-test", "synthetic-frame-password").unwrap(),
            )
            .unwrap();
    });
    send(
        &f,
        cx,
        page,
        P::Document {
            token: token(1),
            origin: ORIGIN.into(),
            has_password_form: false,
        },
    );
    let child = |n, origin: &str| P::FrameDocument {
        token: token(n),
        origin: origin.into(),
        frame: "child".into(),
        has_password_form: true,
    };
    let request = |n, anchor| P::FillRequested {
        token: token(n),
        origin: ORIGIN.into(),
        username_hint: zeroize::Zeroizing::new(String::new()),
        anchor,
        field: 1,
    };
    send(&f, cx, page, child(10, ORIGIN));
    f.take_calls();
    send(&f, cx, page, request(10, None));
    assert!(
        f.take_calls()
            .iter()
            .any(|c| matches!(c,Call::PasswordFill(_,t,o,_) if *t==token(10)&&o==ORIGIN))
    );
    send(
        &f,
        cx,
        page,
        P::Document {
            token: token(2),
            origin: "https://outer.invalid".into(),
            has_password_form: false,
        },
    );
    send(&f, cx, page, child(20, ORIGIN));
    f.take_calls();
    send(&f, cx, page, request(20, None));
    assert!(
        !f.take_calls()
            .iter()
            .any(|c| matches!(c, Call::PasswordFill(..))),
        "Cross-origin frames remain empty until an account is chosen"
    );
    send(&f, cx, page, request(20, Some([30., 40., 165., 35.])));
    // Another frame/document cannot dismiss this field's native account picker.
    send(&f, cx, page, P::Dismissed { token: token(2), origin: "https://outer.invalid".into() });
    let overlay = v
        .read_with(cx, |v, _| {
            v.passwords.accounts.as_ref().and_then(|a| a.overlay)
        })
        .unwrap();
    let mut picker = VisualTestContext::from_window(overlay.into(), &cx.cx);
    click(&mut picker, "password-account-0");
    picker.run_until_parked();
    assert!(
        f.take_calls()
            .iter()
            .any(|c| matches!(c,Call::PasswordFill(_,t,o,_) if *t==token(20)&&o==ORIGIN))
    );
    send(&f, cx, page, child(21, "https://other.invalid"));
    send(&f, cx, page, request(20, None));
    assert!(
        !f.take_calls()
            .iter()
            .any(|c| matches!(c, Call::PasswordFill(..)))
    );
    send(&f, cx, page, child(22, ORIGIN));
    send(
        &f,
        cx,
        page,
        P::Document {
            token: token(3),
            origin: "https://next.invalid".into(),
            has_password_form: false,
        },
    );
    send(&f, cx, page, request(22, None));
    assert!(
        !f.take_calls()
            .iter()
            .any(|c| matches!(c, Call::PasswordFill(..)))
    );
}
#[gpui::test]
fn password_account_dropdown_selects_the_requested_account_and_rejects_navigation(
    cx: &mut TestAppContext,
) {
    let f = FakeEngine::default();
    let (v, cx) = open_with(cx, Some(Box::new(f.clone())));
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    let page = f.0.borrow().focused_page.unwrap();
    v.update(cx, |v, cx| {
        let profile = v
            .browser
            .active_user_profile_id()
            .storage_key()
            .unwrap_or_else(|| "default".into());
        let store = crate::password_flow::storage(cx);
        let other = v
            .edit_browser(|b| b.create_user_profile("Other saved-login profile"))
            .unwrap();
        store
            .save(
                &other.storage_key().unwrap(),
                ORIGIN,
                &LoginCredential::new("foreign-profile-user", "synthetic-foreign-only").unwrap(),
            )
            .unwrap();
        for user in ["alpha", "beta"] {
            store
                .save(
                    &profile,
                    ORIGIN,
                    &LoginCredential::new(user, "synthetic-only").unwrap(),
                )
                .unwrap();
        }
    });
    send(
        &f,
        cx,
        page,
        P::Document {
            token: token(1),
            origin: ORIGIN.into(),
            has_password_form: true,
        },
    );
    let request = || P::FillRequested {
        token: token(1),
        origin: ORIGIN.into(),
        username_hint: zeroize::Zeroizing::new(String::new()),
        anchor: Some([30., 40., 165., 35.]),
        field: 1,
    };
    send(&f, cx, page, request());
    let child = v
        .read_with(cx, |v, _| {
            v.passwords.accounts.as_ref().and_then(|a| a.overlay)
        })
        .expect("accounts dropdown");
    let mut picker = VisualTestContext::from_window(child.into(), &cx.cx);
    picker.run_until_parked();
    assert_eq!(
        picker.debug_bounds("password-accounts").unwrap().size,
        gpui::size(gpui::px(217.), gpui::px(157.))
    );
    assert_eq!(
        v.read_with(&picker, |v, _| v
            .passwords
            .accounts
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|r| r.username().to_owned())
            .collect::<Vec<_>>()),
        ["alpha", "beta"]
    );
    f.take_calls();
    click(&mut picker, "password-account-1");
    picker.run_until_parked();
    assert!(f.take_calls().iter().any(
        |c| matches!(c,Call::PasswordFill(_,t,o,r) if *t==token(1)&&o==ORIGIN&&r.username()=="beta")
    ));
    assert!(v.read_with(&picker, |v, _| v.passwords.accounts.is_none()));
    cx.update(|w, _| w.activate_window());
    send(&f, cx, page, request());
    assert!(v.read_with(cx, |v, _| v.passwords.accounts.is_some()));
    f.take_calls();
    for key in ["down", "down", "enter"] {
        f.send(
            cx,
            Event::Shortcut {
                view: page,
                shortcut: engine::Shortcut {
                    key: key.into(),
                    ..Default::default()
                },
            },
        );
    }
    assert!(
        f.take_calls()
            .iter()
            .any(|c| matches!(c,Call::PasswordFill(_,_,_,r) if r.username()=="beta"))
    );
    cx.update(|w, _| w.activate_window());
    send(&f, cx, page, request());
    send(
        &f,
        cx,
        page,
        P::Document {
            token: token(2),
            origin: "https://next.invalid".into(),
            has_password_form: false,
        },
    );
    assert!(v.read_with(cx, |v, _| v.passwords.accounts.is_none()));
    f.take_calls();
    send(&f, cx, page, request());
    assert!(
        !f.take_calls()
            .iter()
            .any(|c| matches!(c, Call::PasswordFill(..)))
    );
}
