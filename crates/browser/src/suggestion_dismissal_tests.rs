use super::*;
use browser_core::omnibox::{Resolved, SearchEngine};

fn inject(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, engine: SearchEngine) {
    view.update(cx, |view, cx| {
        view.edit_browser(|browser| {
            browser.active_user_profile_mut().settings.search.engine = engine;
            browser
                .active_user_profile_mut()
                .settings
                .search
                .suggestions = true;
        });
        let bar = view.command_bar.as_mut().unwrap();
        bar.remote_task = None;
        bar.remote_engine = Some(engine);
        bar.remote = vec!["quokka reference book".into(), "quokka habitats".into()];
        cx.notify();
    });
    cx.run_until_parked();
}

fn remote(view: &Entity<BrowserWindow>, cx: &VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, cx| {
        view.suggestions(cx)
            .into_iter()
            .filter_map(|row| match row {
                Suggestion::Remote {
                    resolved: Resolved::Search { query, .. },
                    ..
                } => Some(query),
                _ => None,
            })
            .collect()
    })
}

#[gpui::test]
fn deleting_a_remote_row_preserves_local_history_typed_search_and_document(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            browser
                .active_user_profile_mut()
                .history
                .push(browser_core::history::HistoryEntry {
                    url: "https://history.example/quokka".into(),
                    title: "quokka local record".into(),
                    visited_at: std::time::SystemTime::UNIX_EPOCH,
                    visit_count: 7,
                });
        })
    });
    let history = view.read_with(cx, |view, _| {
        serde_json::to_string(&view.browser.active_user_profile().history).unwrap()
    });
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("quokka");
    inject(&view, cx, SearchEngine::Google);
    let index = view.read_with(cx, |view, cx| view.suggestions(cx).iter().position(|row|
        matches!(row, Suggestion::Remote { resolved: Resolved::Search { query, .. }, .. } if query == "quokka reference book")).unwrap());
    fake.take_calls();
    click(
        &mut bar(&view, cx),
        test_selector(format!("delete-suggestion-{index}")),
    );
    assert_eq!(remote(&view, cx), ["quokka habitats"]);
    view.read_with(cx, |view, cx| {
        assert_eq!(serde_json::to_string(&view.browser.active_user_profile().history).unwrap(), history);
        assert!(view.suggestions(cx).iter().any(|row| matches!(row, Suggestion::Load(Resolved::Search { query, .. }) if query == "quokka")));
        assert_eq!(view.command_bar.as_ref().unwrap().input.read(cx).text(), "quokka");
    });
    assert!(bar_input_focused(&view, cx));
    assert!(!fake.take_calls().iter().any(|call| matches!(
        call,
        Call::Create(..) | Call::Close(..) | Call::Navigate(..)
    )));
    // The provider can return it again in a later popup: suppression persists.
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("quokka");
    inject(&view, cx, SearchEngine::Google);
    assert_eq!(remote(&view, cx), ["quokka habitats"]);
    inject(&view, cx, SearchEngine::Bing);
    assert_eq!(remote(&view, cx).len(), 2);
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("quokka reference book");
    inject(&view, cx, SearchEngine::Google);
    assert!(view.read_with(cx, |view, cx| view.suggestions(cx).iter().any(|row|
        matches!(row, Suggestion::Load(Resolved::Search { query, .. }) if query == "quokka reference book"))));
}

#[gpui::test]
fn stale_remote_delete_cannot_cross_popup_request_provider_or_profile(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("quokka");
    inject(&view, cx, SearchEngine::Google);
    let (owner, input, generation) = view.read_with(cx, |view, _| {
        let bar = view.command_bar.as_ref().unwrap();
        (
            view.browser.active_user_profile_id(),
            bar.input.clone(),
            bar.remote_generation,
        )
    });
    // A new request in the same popup revokes the captured request generation.
    bar(&view, cx).simulate_input(" changed");
    inject(&view, cx, SearchEngine::Google);
    view.update(cx, |view, cx| {
        view.dismiss_remote_suggestion(
            owner,
            &input,
            generation,
            SearchEngine::Google,
            "quokka reference book",
            cx,
        )
    });
    assert_eq!(remote(&view, cx).len(), 2);
    let current = view.read_with(cx, |view, _| {
        view.command_bar.as_ref().unwrap().remote_generation
    });
    inject(&view, cx, SearchEngine::Bing);
    view.update(cx, |view, cx| {
        view.dismiss_remote_suggestion(
            owner,
            &input,
            current,
            SearchEngine::Google,
            "quokka reference book",
            cx,
        )
    });
    assert_eq!(remote(&view, cx).len(), 2);
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("quokka");
    inject(&view, cx, SearchEngine::Google);
    view.update(cx, |view, cx| {
        view.dismiss_remote_suggestion(
            owner,
            &input,
            current,
            SearchEngine::Google,
            "quokka reference book",
            cx,
        )
    });
    assert_eq!(remote(&view, cx).len(), 2);
    let (fresh_input, fresh_generation) = view.read_with(cx, |view, _| {
        let bar = view.command_bar.as_ref().unwrap();
        (bar.input.clone(), bar.remote_generation)
    });
    view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let profile = browser.create_user_profile("Synthetic Work").unwrap();
            let space = browser.add_space("Synthetic Work", 20.);
            browser.assign_space_user_profile(space, profile).unwrap();
            browser.switch_space(browser.spaces.len() - 1);
        })
    });
    view.update(cx, |view, cx| {
        view.dismiss_remote_suggestion(
            owner,
            &fresh_input,
            fresh_generation,
            SearchEngine::Google,
            "quokka reference book",
            cx,
        )
    });
    view.read_with(cx, |view, _| {
        assert!(!view
            .browser
            .user_profile(owner)
            .unwrap()
            .settings
            .search
            .suggestion_dismissed(SearchEngine::Google, "quokka reference book"))
    });
}
