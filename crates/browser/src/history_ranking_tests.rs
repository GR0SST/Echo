//! Typed local ranking follows the qualified Arc frequency cases.
use super::*;
use std::time::SystemTime;
fn local(view: &Entity<BrowserWindow>, cx: &VisualTestContext) -> Vec<(bool, String)> {
    view.read_with(cx, |view, cx| view.suggestions(cx).into_iter().filter_map(|row| match row {
        Suggestion::SwitchToTab { url, .. } => Some((true, url)),
        Suggestion::History { url, .. } => Some((false, url)),
        _ => None,
    }).collect())
}
#[gpui::test]
fn typed_history_prefers_revisits_after_favorites_pins_and_open_tabs_and_enter_loads_that_result(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 1);
    let now = SystemTime::now();
    let (expected, before) = view.update(cx, |view, _| view.edit_browser(|browser| {
        let profile = browser.active_user_profile_id();
        browser.active_user_profile_mut().settings.search.suggestions = false;
        for _ in 0..6 { browser.record_visit(profile, "https://rank.test/frequent", now); }
        browser.update_history_title(profile, "https://rank.test/frequent", "Rank fixture Frequent");
        browser.record_visit(profile, "https://rank.test/recent", now + Duration::from_secs(1));
        browser.update_history_title(profile, "https://rank.test/recent", "Rank fixture Recent");
        let favorite = browser.new_tab("https://rank.test/favorite", now);
        browser.set_title(favorite, "Rank fixture Favorite");
        browser.favorite_tab(favorite);
        for (name, visits) in [("first-pin", 9), ("second-pin", 1)] {
            let url = format!("https://rank.test/{name}");
            let tab = browser.new_tab(&url, now);
            browser.set_title(tab, format!("Rank fixture {name}"));
            browser.toggle_pin(tab);
            for _ in 0..visits { browser.record_visit(profile, &url, now); }
        }
        let open = browser.new_tab("https://rank.test/open", now);
        browser.set_title(open, "Rank fixture Open");
        browser.new_tab("https://control.test", now);
        let mut expected = vec![(true, "https://rank.test/favorite".to_owned())];
        expected.extend(browser.space().pinned.iter().filter(|tab| tab.url.starts_with("https://rank.test/")).map(|tab| (true, tab.url.clone())));
        expected.extend([(true, "https://rank.test/open".into()), (false, "https://rank.test/frequent".into()), (false, "https://rank.test/recent".into())]);
        (expected, browser.active_user_profile().history.iter().map(|entry| entry.url.clone()).collect::<Vec<_>>())
    }));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rank fixture");
    assert_eq!(local(&view, cx), expected);
    assert_eq!(view.read_with(cx, |view, _| view.browser.active_user_profile().history.iter().map(|entry| entry.url.clone()).collect::<Vec<_>>()), before);
    bar(&view, cx).simulate_keystrokes("down down down down down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://rank.test/frequent"));
}
#[gpui::test]
fn typing_ranks_open_today_by_frequency_while_empty_search_keeps_mru_and_selection_reuses_the_tab(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 1);
    let now = SystemTime::now();
    let (frequent, count) = view.update(cx, |view, _| view.edit_browser(|browser| {
        let profile = browser.active_user_profile_id();
        browser.active_user_profile_mut().settings.search.suggestions = false;
        let frequent = browser.new_tab("https://rank.test/frequent", now - Duration::from_secs(60));
        browser.set_title(frequent, "Rank fixture Frequent");
        for _ in 0..6 { browser.record_visit(profile, "https://rank.test/frequent", now); }
        let recent = browser.new_tab("https://rank.test/recent", now + Duration::from_secs(10));
        browser.set_title(recent, "Rank fixture Recent");
        browser.record_visit(profile, "https://rank.test/recent", now);
        browser.new_tab("https://control.test", now + Duration::from_secs(20));
        (frequent, browser.space().today.len())
    }));
    cx.simulate_keystrokes("cmd-t");
    let before: Vec<_> = local(&view, cx).into_iter().filter(|(_, url)| url.starts_with("https://rank.test/")).collect();
    assert_eq!(before, vec![(true, "https://rank.test/recent".into()), (true, "https://rank.test/frequent".into())]);
    bar(&view, cx).simulate_input("rank fixture");
    assert_eq!(local(&view, cx), vec![(true, "https://rank.test/frequent".into()), (true, "https://rank.test/recent".into())]);
    bar(&view, cx).simulate_keystrokes("down enter");
    assert!(view.read_with(cx, |view, _| view.browser.space().active_tab == Some(frequent) && view.browser.space().today.len() == count));
}
