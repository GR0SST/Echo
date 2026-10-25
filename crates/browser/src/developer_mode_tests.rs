use super::*;
use std::time::SystemTime;

#[gpui::test]
fn developer_mode_keeps_site_controls_open_and_follows_the_profile_and_host(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    visual.simulate_keystrokes("ctrl-d");
    assert!(view.read_with(visual, |v, _| v.developer_mode_enabled()));
    click(visual, "address-site-controls");
    let overlay = view.read_with(visual, |v, _| v.site_controls.as_ref().unwrap().overlay.unwrap());
    let mut panel = VisualTestContext::from_window(overlay.into(), &visual.cx);
    panel.run_until_parked();
    click(&mut panel, "site-developer-mode");
    assert!(!view.read_with(visual, |v, _| v.developer_mode_enabled()));
    assert!(view.read_with(visual, |v, _| v.site_controls.is_some()));
    panel.simulate_keystrokes("escape");
    visual.simulate_keystrokes("ctrl-d");
    let stored = view.read_with(visual, |v, _| browser_core::session::to_json(&v.browser));
    let restored = browser_core::session::from_json(&stored).unwrap();
    assert!(restored.active_user_profile().developer_sites.contains("www.chromium.org"));
    view.update(visual, |v, cx| {
        v.edit_browser(|b| { b.new_tab("https://other.example", SystemTime::now()); });
        cx.notify();
    });
    assert!(!view.read_with(visual, |v, _| v.developer_mode_enabled()));
    view.update(visual, |v, cx| {
        v.edit_browser(|b| { b.new_tab("https://www.chromium.org:8443/another", SystemTime::now()); });
        cx.notify();
    });
    assert!(view.read_with(visual, |v, _| v.developer_mode_enabled()));
}

#[gpui::test]
fn developer_mode_cannot_retarget_a_stale_site_panel(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    click(visual, "address-site-controls");
    let overlay = view.read_with(visual, |v, _| v.site_controls.as_ref().unwrap().overlay.unwrap());
    let mut panel = VisualTestContext::from_window(overlay.into(), &visual.cx);
    panel.run_until_parked();
    view.update(visual, |v, _| { v.edit_browser(|b| { b.new_tab("https://other.example", SystemTime::now()); }); });
    visual.run_until_parked();
    assert!(view.read_with(visual, |v, _| v.site_controls.is_none()));
    assert!(view.read_with(visual, |v, _| v.browser.active_user_profile().developer_sites.is_empty()));
}

#[gpui::test]
fn localhost_developer_default_can_be_disabled_and_restored_across_navigation(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    let ordinary_y = view.read_with(visual, |v, _| v.pages.as_ref().unwrap().borrow().surface_bounds()[0].y);
    let visit = |url: &str, visual: &mut VisualTestContext| {
        view.update(visual, |v, cx| {
            v.edit_browser(|b| { b.new_tab(url, SystemTime::now()); });
            cx.notify();
        });
        visual.run_until_parked();
    };
    visit("http://echo-parity.localhost:8000/", visual);
    assert!(visual.debug_bounds("developer-toolbar").is_some());
    visual.simulate_keystrokes("cmd-f");
    visual.run_until_parked();
    let overlay = view.read_with(visual, |v, _| v.find_bar.as_ref().unwrap().overlay.unwrap());
    let origin = visual.update(|window, _| window.bounds().origin.y);
    assert_eq!(overlay.update(visual, |_, window, _| window.bounds().origin.y).unwrap(), origin + gpui::px(40.));
    VisualTestContext::from_window(overlay.into(), &visual.cx).simulate_keystrokes("escape");
    visual.run_until_parked();
    visual.simulate_keystrokes("ctrl-d");
    visual.run_until_parked();
    assert!(!view.read_with(visual, |v, _| v.developer_mode_enabled()));
    // GPUI retains removed debug selectors; verify the restored page layout.
    visual.update(|window, cx| { let _ = window.draw(cx); });
    visual.run_until_parked();
    assert_eq!(view.read_with(visual, |v, _| v.pages.as_ref().unwrap().borrow().surface_bounds()[0].y), ordinary_y);
    visit("http://echo-parity.localhost:8001/revisit", visual);
    assert!(!view.read_with(visual, |v, _| v.developer_mode_enabled()));
    let restored = view.read_with(visual, |v, _| browser_core::session::from_json(&browser_core::session::to_json(&v.browser)).unwrap());
    assert!(!restored.active_user_profile().developer_mode_for_host("echo-parity.localhost"));
    assert!(restored.active_user_profile().developer_mode_for_host("other.localhost"));
    assert!(!restored.active_user_profile().developer_mode_for_host("localhost.example"));
    visual.simulate_keystrokes("ctrl-d");
    visual.run_until_parked();
    assert!(visual.debug_bounds("developer-toolbar").is_some());
    visit("https://example.org/", visual);
    assert!(!view.read_with(visual, |v, _| v.developer_mode_enabled()));
}
