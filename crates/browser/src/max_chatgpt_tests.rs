use super::*;
#[gpui::test]
fn max_chatgpt_command_scopes_on_tab_prefills_website_and_backspace_unscopes(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::ChatGptCommandBar, true)
        })
    });
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("ChatGPT");
    assert!(bar(&view, cx).debug_bounds("chatgpt-tab-hint").is_some());
    bar(&view, cx).simulate_keystrokes("tab");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    view.read_with(cx, |w, _| {
        assert_eq!(
            w.command_bar.as_ref().unwrap().site_search,
            Some(browser_core::omnibox::SiteSearch::ChatGpt)
        )
    });
    assert!(bar(&view, cx).debug_bounds("site-search-pill").is_some());
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).len()) >= 2);
    bar(&view, cx).simulate_keystrokes("backspace");
    view.read_with(cx, |w, _| {
        assert!(w.command_bar.as_ref().unwrap().site_search.is_none())
    });
    bar(&view, cx).simulate_input("ChatGPT");
    bar(&view, cx).simulate_keystrokes("tab");
    bar(&view, cx).simulate_input("What is the Rust programming language?");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 1);
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://chat.openai.com/?q=What%20is%20the%20Rust%20programming%20language?")
    );
}
#[gpui::test]
fn disabled_chatgpt_feature_cannot_scope_or_submit_a_stale_scope(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("ChatGPT");
    bar(&view, cx).simulate_keystrokes("tab");
    view.read_with(cx, |w, _| {
        assert!(w.command_bar.as_ref().unwrap().site_search.is_none())
    });
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::ChatGptCommandBar, true)
        })
    });
    bar(&view, cx).simulate_keystrokes("tab");
    bar(&view, cx).simulate_input("A question");
    let before = active_url(&view, cx);
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::ChatGptCommandBar, false)
        })
    });
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx), before);
    view.read_with(cx, |w, cx| assert!(w.suggestions(cx).is_empty()));
}

#[gpui::test]
fn chatgpt_keyword_in_a_rename_form_stays_a_tab_name(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::ChatGptCommandBar, true)
        })
    });
    let before = active_url(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |w, cx| {
            let id = w.browser.active_tab().unwrap().id;
            w.open_command_bar(crate::window::Target::RenameTab(id), "", window, cx);
        });
    });
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("ChatGPT");
    assert!(bar(&view, cx).debug_bounds("chatgpt-tab-hint").is_none());
    bar(&view, cx).simulate_keystrokes("tab");
    view.read_with(cx, |w, _| {
        assert!(w.command_bar.as_ref().unwrap().site_search.is_none())
    });
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx), before);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_tab().unwrap().title, "ChatGPT")
    });
}
fn enable_generic_chatgpt(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |w, cx| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::ChatGptCommandBar, true)
        });
        w.edit_browser(|b| b.active_user_profile_mut().settings.search.suggestions = false);
        cx.notify();
    });
}
#[gpui::test]
fn chatgpt_ordinary_query_offers_the_captioned_row_and_keyboard_handoff(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    enable_generic_chatgpt(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("Explain Rust lifetimes");
    view.read_with(cx, |w, cx| {
        let suggestions = w.suggestions(cx);
        assert_eq!(suggestions.len(), 2);
        assert!(matches!(&suggestions[1], Suggestion::AskChatGpt { query } if query == "Explain Rust lifetimes"));
    });
    assert!(bar(&view, cx).debug_bounds("ask-chatgpt").is_some());
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://chat.openai.com/?q=Explain%20Rust%20lifetimes")
    );
}
#[gpui::test]
fn chatgpt_ordinary_query_click_uses_the_same_website_handoff(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    enable_generic_chatgpt(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("Explain Rust lifetimes");
    click(&mut bar(&view, cx), "ask-chatgpt");
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://chat.openai.com/?q=Explain%20Rust%20lifetimes")
    );
}
#[gpui::test]
fn chatgpt_ordinary_query_row_respects_feature_changes_and_does_not_replace_url_navigation(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    enable_generic_chatgpt(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://rust-lang.org/");
    view.read_with(cx, |w, cx| {
        assert!(!w
            .suggestions(cx)
            .iter()
            .any(|s| matches!(s, Suggestion::AskChatGpt { .. })))
    });
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("Explain Rust lifetimes");
    view.read_with(cx, |w, cx| {
        assert!(w
            .suggestions(cx)
            .iter()
            .any(|s| matches!(s, Suggestion::AskChatGpt { .. })))
    });
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::ChatGptCommandBar, false)
        })
    });
    view.read_with(cx, |w, cx| {
        assert!(!w
            .suggestions(cx)
            .iter()
            .any(|s| matches!(s, Suggestion::AskChatGpt { .. })))
    });
    let before = active_url(&view, cx);
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(active_url(&view, cx), before);
}
