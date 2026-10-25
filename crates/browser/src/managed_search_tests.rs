use super::*;
use browser_core::omnibox::search_settings::ShortcutMode;
use gpui::px;
fn search_page(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    cx.simulate_resize(gpui::size(px(1100.), px(1200.)));
    cx.simulate_keystrokes("cmd-t");
    bar(view, cx).simulate_input("Search Settings");
    bar(view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("search-settings-page").is_some());
}
#[gpui::test]
fn managed_search_editor_adds_a_custom_default_and_command_bar_uses_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    search_page(&view, cx);
    click(cx, "search-add");
    assert!(
        cx.debug_bounds("search-editor").is_some(),
        "Add click did not open its editor: {:?}",
        cx.debug_bounds("search-add")
    );
    cx.simulate_input("Docs");
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    cx.simulate_input("docs");
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    cx.simulate_input("https://docs.test/search?q=%s");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |v, cx| v.search_editor_debug(cx))
            .is_none(),
        "Editor: {:?}",
        view.read_with(cx, |v, cx| v.search_editor_debug(cx))
    );
    let id = view.read_with(cx, |v, _| {
        let p = &v.browser.active_user_profile().settings.search;
        p.provider_for_shortcut("docs")
            .unwrap_or_else(|| {
                panic!(
                    "Custom search missing; url={:?}; providers={:?}",
                    v.browser.active_tab().map(|t| &t.url),
                    p.providers
                )
            })
            .id
    });
    click(cx, test_selector(format!("search-more-{id}")));
    click(cx, "search-menu-Make default");
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.browser
                .active_user_profile()
                .settings
                .search
                .default_name(),
            "Docs"
        )
    });
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rust & λ");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://docs.test/search?q=rust+%26+%CE%BB")
    );
    let serialized = view.read_with(cx, |v, _| browser_core::session::to_json(&v.browser));
    let restored = browser_core::session::from_json(&serialized).unwrap();
    assert_eq!(
        restored
            .active_user_profile()
            .settings
            .search
            .default_name(),
        "Docs"
    );
}
#[gpui::test]
fn managed_search_tab_mode_and_deactivation_control_live_command_bar_scope(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let id = view.update(cx, |v, _| {
        v.edit_profile(|b| {
            let p = &mut b.active_user_profile_mut().settings.search;
            p.shortcut_mode = ShortcutMode::Tab;
            p.add_provider("Papers", "papers", "https://papers.test/find/%s")
                .unwrap()
        })
    });
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("papers ");
    assert!(view.read_with(cx, |v, _| v
        .command_bar
        .as_ref()
        .unwrap()
        .site_search
        .is_none()));
    bar(&view, cx).simulate_keystrokes("backspace tab");
    assert!(view.read_with(cx, |v, _| v
        .command_bar
        .as_ref()
        .unwrap()
        .site_search
        .is_some()));
    bar(&view, cx).simulate_input("two words");
    view.read_with(cx, |v, cx| {
        assert_eq!(
            match &v.suggestions(cx)[0] {
                crate::window::Suggestion::Load(url) => url.url(),
                _ => panic!("Expected site search"),
            },
            "https://papers.test/find/two%20words"
        )
    });
    view.update(cx, |v, _| {
        v.edit_profile(|b| {
            b.active_user_profile_mut()
                .settings
                .search
                .set_provider_active(id, false)
                .unwrap();
        })
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v
        .command_bar
        .as_ref()
        .unwrap()
        .site_search
        .is_none()));
}
#[gpui::test]
fn managed_search_editor_cancellation_and_deleted_target_cannot_change_another_profile(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    search_page(&view, cx);
    click(cx, "search-add");
    assert!(cx.debug_bounds("search-editor").is_some());
    cx.simulate_input("Cancelled");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(cx, |v, _| v
            .browser
            .active_user_profile()
            .settings
            .search
            .providers
            .len()),
        7
    );
    let owner = view.update(cx, |v, _| {
        v.edit_browser(|b| b.create_user_profile("Other search profile").unwrap())
    });
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_search_settings(owner, window, cx)));
    cx.run_until_parked();
    click(cx, "search-add");
    assert!(cx.debug_bounds("search-editor").is_some());
    cx.simulate_input("Stale");
    view.update(cx, |v, _| {
        v.edit_browser(|b| b.delete_user_profile(owner).unwrap())
    });
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |v, cx| v.search_editor_debug(cx))
            .is_none(),
        "Editor: {:?}",
        view.read_with(cx, |v, cx| v.search_editor_debug(cx))
    );
    assert_eq!(
        view.read_with(cx, |v, _| v
            .browser
            .active_user_profile()
            .settings
            .search
            .providers
            .len()),
        7
    );
}

#[gpui::test]
fn managed_search_page_scrolls_from_the_keyboard_and_returns_focus_after_cancel(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    search_page(&view, cx);
    cx.simulate_resize(gpui::size(px(1012.), px(760.)));
    cx.simulate_keystrokes("pagedown");
    cx.run_until_parked();
    let add = cx.debug_bounds("search-add").unwrap();
    assert!(
        add.origin.y > px(56.) && add.bottom() < px(750.),
        "Add must be visible after Page Down: {add:?}"
    );
    click(cx, "search-add");
    assert!(view
        .read_with(cx, |v, cx| v.search_editor_debug(cx))
        .is_some());
    cx.simulate_keystrokes("escape pageup");
    cx.run_until_parked();
    assert!(view
        .read_with(cx, |v, cx| v.search_editor_debug(cx))
        .is_none());
    assert!(cx.debug_bounds("search-mode-tab").unwrap().origin.y > px(56.));
}

#[gpui::test]
fn managed_search_scope_cannot_rebind_to_a_different_profiles_matching_provider_id(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    view.update(cx, |v, _| {
        v.edit_profile(|b| {
            b.active_user_profile_mut()
                .settings
                .search
                .add_provider("Papers", "papers", "https://papers.test/%s")
                .unwrap();
        })
    });
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("papers");
    bar(&view, cx).simulate_keystrokes("tab");
    bar(&view, cx).simulate_input("private query");
    view.update(cx, |v, cx| {
        v.edit_browser(|b| {
            let owner = b.create_user_profile("Another search owner").unwrap();
            b.user_profile_mut(owner)
                .unwrap()
                .settings
                .search
                .add_provider("Other", "other", "https://other.test/%s")
                .unwrap();
            b.assign_space_user_profile(b.space().id, owner).unwrap();
        });
        assert!(
            v.suggestions(cx).is_empty(),
            "An existing scope must not acquire the other profile's same-numbered provider"
        );
    });
}
