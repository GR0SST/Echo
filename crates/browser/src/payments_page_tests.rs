use super::*;

fn open_payments_for(v: &Entity<BrowserWindow>, cx: &mut VisualTestContext, profile: browser_core::UserProfileId) {
    v.update_in(cx, |v, window, cx| v.open_payments(profile, window, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("payments-page").is_some());
}

fn rows(v: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.run_until_parked();
    v.read_with(cx, |v, _| v.payment_rows())
}

fn type_into(cx: &mut VisualTestContext, field: &'static str, text: &str) {
    click(cx, field);
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input(text);
}

fn next_year() -> u16 {
    chrono::Datelike::year(&chrono::Local::now()) as u16 + 1
}

/// Picks February of next year in the form's expiration pickers.
fn pick_expiration(cx: &mut VisualTestContext) {
    click(cx, "payments-month");
    click(cx, "payments-month-2");
    click(cx, "payments-year");
    let year: &'static str = Box::leak(format!("payments-year-{}", next_year()).into_boxed_str());
    click(cx, year);
}

fn add_card(cx: &mut VisualTestContext, number: &str) {
    click(cx, "payments-add");
    cx.simulate_input(number);
    pick_expiration(cx);
    type_into(cx, "payments-field-name", "Echo Test");
    click(cx, "payments-editor-save");
    cx.run_until_parked();
}

#[gpui::test]
fn payments_settings_entry_opens_the_selected_profile(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let (default, other) = v.update(cx, |v, _| {
        let a = v.browser.active_user_profile_id();
        let b = v.edit_browser(|b| b.create_user_profile("Payments fixture profile")).unwrap();
        (a, b)
    });
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Profiles");
    click(&mut settings, "settings-profile-Payments fixture profile");
    click(&mut settings, "settings-credit-cards");
    cx.run_until_parked();
    assert_eq!(active_url(&v, cx), Some(format!("{}?profile={}", crate::payments_page::URL, serde_json::to_string(&other).unwrap())));
    assert!(cx.debug_bounds("payments-page").is_some());
    assert!(cx.debug_bounds("settings-nav").is_some(), "Payments renders inside the settings frame");
    assert_eq!(crate::settings_shell::Section::of(crate::payments_page::URL), Some(crate::settings_shell::Section::Autofill));
    assert_eq!(v.read_with(cx, |v, _| v.browser.active_user_profile_id()), default);
    assert!(cx.debug_bounds("payments-empty").is_some());
}

#[gpui::test]
fn payments_add_validates_lists_edits_and_removes_cards(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let profile = v.read_with(cx, |v, _| v.browser.active_user_profile_id());
    open_payments_for(&v, cx, profile);
    // A number failing the Luhn check keeps the form open with an error.
    click(cx, "payments-add");
    cx.simulate_input("5555 5555 5555 4245");
    click(cx, "payments-editor-save");
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v.payment_editor_open() && v.payment_editor_error()));
    assert!(cx.debug_bounds("payments-editor-error").is_some());
    type_into(cx, "payments-field-number", "5555 5555 5555 4444");
    pick_expiration(cx);
    type_into(cx, "payments-field-cvc", "123");
    click(cx, "payments-editor-save");
    cx.run_until_parked();
    assert!(!v.read_with(cx, |v, _| v.payment_editor_open()));
    add_card(cx, "4111 1111 1111 1111");
    let yy = next_year() % 100;
    let listed = rows(&v, cx);
    assert_eq!(listed, vec![format!("Mastercard •• 4444 · 02/{yy:02}"), format!("Visa •• 1111 · 02/{yy:02}")]);
    assert!(listed.iter().all(|r| !r.contains("4111 1111") && !r.contains("411111")), "Only the last four digits show");
    // Edit the Visa card's nickname.
    let index = listed.iter().position(|r| r.starts_with("Visa")).unwrap();
    let more: &'static str = Box::leak(format!("payments-more-{index}").into_boxed_str());
    click(cx, more);
    click(cx, "payments-menu-Edit");
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v.payment_editor_open()));
    type_into(cx, "payments-field-nickname", "Travel");
    click(cx, "payments-editor-save");
    let listed = rows(&v, cx);
    assert!(listed.iter().any(|r| r.starts_with("Travel •• 1111 · 02/")), "{listed:?}");
    // Remove it.
    let index = listed.iter().position(|r| r.starts_with("Travel")).unwrap();
    let more: &'static str = Box::leak(format!("payments-more-{index}").into_boxed_str());
    click(cx, more);
    click(cx, "payments-menu-Remove");
    let listed = rows(&v, cx);
    assert!(!listed.iter().any(|r| r.contains("1111")), "{listed:?}");
    let session = v.read_with(cx, |v, _| v.session_json());
    assert!(!session.contains("4111111111111111") && !session.contains("5555555555554444"));
}

#[gpui::test]
fn payments_toggles_persist_and_control_the_cvc_field(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let profile = v.read_with(cx, |v, _| v.browser.active_user_profile_id());
    open_payments_for(&v, cx, profile);
    let read = |v: &Entity<BrowserWindow>, cx: &mut VisualTestContext| {
        v.read_with(cx, |v, _| v.profile.handle.read_settings(|b| b.user_profile(profile).unwrap().settings.payments.clone()))
    };
    assert_eq!(read(&v, cx), browser_core::payments::PaymentSettings::default());
    for toggle in ["payments-toggle-save-and-fill", "payments-toggle-verify", "payments-toggle-save-cvc", "payments-toggle-sites-check"] {
        click(cx, toggle);
    }
    let settings = read(&v, cx);
    assert!(!settings.save_and_fill && settings.verify_before_fill && !settings.save_security_codes && !settings.sites_can_check);
    // GPUI keeps debug bounds of earlier frames, so check the field's absence
    // before it has ever been drawn.
    click(cx, "payments-add");
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v.payment_editor_open()));
    assert!(cx.debug_bounds("payments-field-number").is_some());
    assert!(cx.debug_bounds("payments-field-cvc").is_none(), "No CVC field when Save security codes is off");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!v.read_with(cx, |v, _| v.payment_editor_open()));
    click(cx, "payments-toggle-save-cvc");
    click(cx, "payments-add");
    assert!(cx.debug_bounds("payments-field-cvc").is_some(), "CVC shows while Save security codes is on");
    cx.simulate_keystrokes("escape");
    click(cx, "payments-toggle-save-cvc");
    // The preferences belong to the profile and survive a session round trip.
    let session = v.read_with(cx, |v, _| v.session_json());
    let restored = browser_core::session::from_json(&session).unwrap();
    assert!(!restored.user_profile(profile).unwrap().settings.payments.save_security_codes);
}

#[gpui::test]
fn payments_cards_are_isolated_per_profile(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let (first, second) = v.update(cx, |v, _| {
        let a = v.browser.active_user_profile_id();
        let b = v.edit_browser(|b| b.create_user_profile("Second payments profile")).unwrap();
        (a, b)
    });
    open_payments_for(&v, cx, second);
    add_card(cx, "378282246310005");
    assert_eq!(rows(&v, cx).len(), 1);
    assert!(rows(&v, cx)[0].starts_with("American Express •• 0005"));
    open_payments_for(&v, cx, first);
    assert!(rows(&v, cx).is_empty());
    assert!(cx.debug_bounds("payments-empty").is_some());
    open_payments_for(&v, cx, second);
    assert_eq!(rows(&v, cx).len(), 1);
}
