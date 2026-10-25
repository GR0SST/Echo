use super::*;

#[gpui::test]
fn settings_follow_the_system_window_appearance_independently_of_space_appearance(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.edit_profile(|browser| {
            browser.settings.theme_appearance = browser_core::themes::ThemeAppearance::Dark;
        });
        cx.notify();
    });
    cx.simulate_keystrokes("cmd-,");
    let (settings, mut visual) = settings_window(cx);
    assert!(visual.update(|window, _| matches!(window.appearance(), gpui::WindowAppearance::Light)));
    for tab in [
        "Profiles",
        "Icon",
        "Links",
        "Shortcuts",
        "Advanced",
        "General",
    ] {
        click(&mut visual, test_selector(format!("settings-tab-{tab}")));
        assert!(settings.read_with(&visual, |settings, _| settings.uses_light_appearance()));
    }
    view.update(cx, |view, cx| {
        view.edit_profile(|browser| {
            browser.settings.theme_appearance = browser_core::themes::ThemeAppearance::Light;
        });
        cx.notify();
    });
    visual.run_until_parked();
    assert!(settings.read_with(&visual, |settings, _| settings.uses_light_appearance()));
    click(&mut visual, "settings-button-Previews Settings…");
    assert!(settings.read_with(&visual, |settings, _| settings.previews_open()));
    visual.simulate_keystrokes("escape");
    assert!(!settings.read_with(&visual, |settings, _| settings.previews_open()));
}
