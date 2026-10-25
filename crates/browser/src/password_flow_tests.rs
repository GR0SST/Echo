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
    cx.update(|window,_|window.activate_window());cx.run_until_parked();
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
        },
    );
    assert!(f
        .take_calls()
        .iter()
        .any(|c| matches!(c,Call::PasswordFill(_,t,o,_) if *t==token(3) && o==ORIGIN)));
    send(
        &f,
        cx,
        page,
        P::FillRequested {
            token: token(2),
            origin: ORIGIN.into(),
            username_hint: zeroize::Zeroizing::new(String::new()),
        },
    );
    assert!(!f
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::PasswordFill(..))));
}
#[gpui::test]
fn password_never_policy_and_escape_do_not_save(cx: &mut TestAppContext) {
    let f = FakeEngine::default();
    let (v, cx) = open_with(cx, Some(Box::new(f.clone())));
    cx.update(|window,_|window.activate_window());cx.run_until_parked();
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
    cx.update(|window,_|window.activate_window());cx.run_until_parked();
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
    cx.update(|window,_|window.activate_window());cx.run_until_parked();
    cx.deactivate_window();
    begin(&v, &f, cx);
    assert!(v.read_with(cx, |v, _| v.password_overlay()).is_none());
    assert!(!cx.update(|w, _| w.is_window_active()));
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v.password_overlay()).is_some());
}
