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
