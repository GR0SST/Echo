//! Behavioral regressions from the second native parity audit.
use super::*;

#[gpui::test]
fn current_tab_is_history_and_inactive_tab_is_switchable(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    let current = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("second page");
    view.read_with(cx, |w, cx| {
        let rows = w.suggestions(cx);
        assert!(!rows.iter().any(|row| matches!(row, Suggestion::SwitchToTab { id, .. } if *id == current)));
        assert!(rows.iter().any(|row| matches!(row, Suggestion::History { url, .. } if url == "https://example.com/two")));
    });
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://unrelated.example");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("second page");
    view.read_with(cx, |w, cx| {
        let rows = w.suggestions(cx);
        assert!(rows.iter().any(|row| matches!(row, Suggestion::SwitchToTab { id, .. } if *id == current)));
        assert!(!rows.iter().any(|row| matches!(row, Suggestion::History { url, .. } if url == "https://example.com/two")));
    });
    fake.take_calls();
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), current);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..))));
}
