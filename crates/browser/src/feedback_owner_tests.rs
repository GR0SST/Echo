use super::*;
#[gpui::test]
fn lookup_feedback_preferences_follow_the_retained_normal_owner_after_its_window_closes(
    cx: &mut TestAppContext,
) {
    use browser_core::settings::Toggle;
    let (source, cx) = open(cx);
    let owner = source.read_with(cx, |view, _| view.profile.handle.clone());
    cx.simulate_keystrokes("cmd-alt-n");
    cx.run_until_parked();
    let (little, compact) = little_root(cx);
    owner.edit_settings(|browser| {
        browser.settings.set_enabled(Toggle::SoundEffects, false);
        browser.settings.set_enabled(Toggle::HapticFeedback, false);
    });
    for toggle in [Toggle::SoundEffects, Toggle::HapticFeedback] {
        assert!(
            !little.read_with(&compact, |view, cx| crate::feedback::enabled(
                view, toggle, cx
            ))
        );
    }
    source.update(cx, |view, cx| view.close_window(cx));
    cx.run_until_parked();
    owner.edit_settings(|browser| {
        browser.settings.set_enabled(Toggle::SoundEffects, true);
        browser.settings.set_enabled(Toggle::HapticFeedback, true);
    });
    for toggle in [Toggle::SoundEffects, Toggle::HapticFeedback] {
        assert!(
            little.read_with(&compact, |view, cx| crate::feedback::enabled(
                view, toggle, cx
            ))
        );
    }
}
#[gpui::test]
fn private_feedback_preferences_follow_the_normal_settings_owner(cx: &mut TestAppContext) {
    use browser_core::settings::Toggle;
    let (source, cx) = open(cx);
    // Production normal-window creation retains this owner before private entry.
    source.update(cx, |view, cx| view.retain_normal_source(cx));
    cx.simulate_keystrokes("cmd-shift-n");
    cx.run_until_parked();
    let private = cx.cx.update(|cx| {
        let roots: Vec<_> = cx
            .windows()
            .into_iter()
            .filter_map(|window| window.downcast::<BrowserWindow>())
            .filter_map(|window| window.root(cx).ok())
            .collect();
        roots
            .into_iter()
            .find(|view| view.read(cx).browser.is_private())
            .unwrap()
    });
    source.update(cx, |view, _| {
        view.edit_browser(|browser| {
            browser.settings.set_enabled(Toggle::SoundEffects, false);
            browser.settings.set_enabled(Toggle::HapticFeedback, false);
        })
    });
    for toggle in [Toggle::SoundEffects, Toggle::HapticFeedback] {
        assert!(!private.read_with(cx, |view, cx| crate::feedback::enabled(view, toggle, cx)));
    }
}
