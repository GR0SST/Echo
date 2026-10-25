use super::*;
use browser_core::settings::AppIcon;
#[gpui::test]
fn app_icon_choices_retain_one_selection_and_persist_without_changing_website_profiles(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let profiles = view.read_with(cx, |view, _| {
        view.browser
            .user_profiles()
            .map(|p| (p.id(), p.name().to_owned()))
            .collect::<Vec<_>>()
    });
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Icon");
    for (index, icon) in AppIcon::ALL.into_iter().enumerate() {
        let selector = test_selector(format!("settings-app-icon-{index}"));
        let bounds = settings.debug_bounds(selector).unwrap();
        assert_eq!(bounds.size.height, gpui::px(80.));
        click(&mut settings, selector);
        view.read_with(cx, |view, _| {
            view.profile.handle.read_settings(|browser| {
                assert_eq!(browser.settings.app_icon, icon);
                assert_eq!(
                    browser
                        .user_profiles()
                        .map(|p| (p.id(), p.name().to_owned()))
                        .collect::<Vec<_>>(),
                    profiles
                );
                let restored =
                    browser_core::session::from_json(&browser_core::session::to_json(browser))
                        .unwrap();
                assert_eq!(restored.settings.app_icon, icon);
            })
        });
        assert!(settings.debug_bounds("settings-current-icon").is_some());
    }
    click(&mut settings, "settings-app-icon-0");
    settings.simulate_keystrokes("tab space");
    settings.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view
            .profile
            .handle
            .read_settings(|browser| browser.settings.app_icon)),
        AppIcon::Candy
    );
}

#[gpui::test]
fn app_icon_svg_previews_keep_the_artwork_color_channels_and_alpha(cx: &mut TestAppContext) {
    cx.update(|cx| {
        for icon in AppIcon::ALL {
            let image = crate::app_icons::preview_image(icon, cx);
            assert!(image
                .as_bytes(0)
                .unwrap()
                .chunks_exact(4)
                .any(|pixel| pixel[3] == 255));
        }
        let paper = crate::app_icons::preview_image(AppIcon::Schoolbook, cx);
        let pixel = &paper.as_bytes(0).unwrap()
            [(10 * paper.size(0).width.0 as usize + paper.size(0).width.0 as usize / 2) * 4..][..4];
        assert!(
            pixel[2] > pixel[0].saturating_add(50),
            "The yellow artwork must retain red > blue in GPUI's BGRA buffer: {pixel:?}"
        );
        assert_eq!(pixel[3], 255);
        assert_eq!(&paper.as_bytes(0).unwrap()[..4], &[0, 0, 0, 0]);
    });
}
