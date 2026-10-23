//! Profile integration tests: real GPUI windows consume independent projections.
use super::*;
use crate::window::Suggestion;
use gpui::{TestAppContext, VisualTestContext};

const NOW: SystemTime = SystemTime::UNIX_EPOCH;

fn open(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext) {
    cx.update(crate::init);
    let (view, visual) = cx.add_window_view(|window, cx| {
        BrowserWindow::new(Browser::new(), None, window, cx)
    });
    visual.run_until_parked();
    (view, visual)
}

fn add_window(handle: ProfileHandle, visual: &mut VisualTestContext) -> Entity<BrowserWindow> {
    visual.cx.update(|cx| {
        let window = cx.open_window(gpui::WindowOptions::default(), |window, cx| cx.new(|cx| {
            BrowserWindow::with_profile(handle, None, None, window, cx)
        })).unwrap();
        window.root(cx).unwrap()
    })
}

fn blank(normal: &Entity<BrowserWindow>, visual: &mut VisualTestContext) -> Entity<BrowserWindow> {
    let source = normal.read_with(visual, |view, _| view.profile.handle.clone());
    let handle = visual.cx.update(|cx| ProfileHandle::new_with_shared_history(Browser::new_blank(), &source, cx));
    add_window(handle, visual)
}

fn urls(view: &Entity<BrowserWindow>, visual: &VisualTestContext) -> Vec<String> {
    view.read_with(visual, |view, _| view.browser.history.iter().map(|entry| entry.url.clone()).collect())
}

#[gpui::test]
fn normal_history_refreshes_blank_projection_and_command_bar(cx: &mut TestAppContext) {
    let (normal, visual) = open(cx);
    normal.update(visual, |view, _| view.edit_profile(|browser| browser.record_visit("https://normal.example/existing", NOW)));
    let blank = blank(&normal, visual);
    assert_eq!(urls(&blank, visual), vec!["https://normal.example/existing"]);
    normal.update(visual, |view, _| view.edit_profile(|browser| {
        browser.record_visit("https://normal.example/probe", NOW);
        browser.update_history_title("https://normal.example/probe", "Blank Probe normal");
    }));
    visual.run_until_parked();
    assert_eq!(urls(&blank, visual), vec!["https://normal.example/probe", "https://normal.example/existing"]);
    let blank_window = blank.read_with(visual, |view, _| view.main_window);
    let mut blank_visual = VisualTestContext::from_window(blank_window, &visual.cx);
    blank_visual.update(|window, cx| blank.update(cx, |view, _| view.focus(window)));
    blank_visual.simulate_keystrokes("cmd-t");
    let overlay = blank.read_with(&blank_visual, |view, _| view.command_bar.as_ref().unwrap().overlay.unwrap());
    let mut bar = VisualTestContext::from_window(overlay.into(), &blank_visual);
    bar.simulate_input("Blank Probe");
    blank.read_with(&bar, |view, cx| assert!(view.suggestions(cx).iter().any(|entry|
        matches!(entry, Suggestion::History { title, url } if title == "Blank Probe normal" && url == "https://normal.example/probe"))));
}

#[gpui::test]
fn blank_history_visits_titles_deletion_and_clear_reach_normal_and_other_blanks(cx: &mut TestAppContext) {
    let (normal, visual) = open(cx);
    let a = blank(&normal, visual);
    let b = blank(&normal, visual);
    a.update(visual, |view, _| view.edit_profile(|browser| {
        browser.record_visit("https://blank.example/one", NOW);
        browser.update_history_title("https://blank.example/one", "Blank Probe one");
        browser.record_visit("https://blank.example/two", NOW);
    }));
    visual.run_until_parked();
    for view in [&normal, &b] {
        assert_eq!(urls(view, visual), vec!["https://blank.example/two", "https://blank.example/one"]);
        view.read_with(visual, |view, _| assert_eq!(view.browser.history[1].title, "Blank Probe one"));
    }
    b.update(visual, |view, _| view.edit_browser(|browser| browser.delete_history_entry("https://blank.example/one")));
    visual.run_until_parked();
    assert_eq!(urls(&normal, visual), vec!["https://blank.example/two"]);
    a.update(visual, |view, _| view.edit_profile(|browser| browser.history.clear()));
    visual.run_until_parked();
    for view in [&normal, &a, &b] { assert!(urls(view, visual).is_empty()); }
}

#[gpui::test]
fn stale_blank_edits_sync_authoritative_history_before_modification(cx: &mut TestAppContext) {
    let (normal, visual) = open(cx);
    let a = blank(&normal, visual);
    let b = blank(&normal, visual);
    // No executor drain between edits: every window projection may still be stale.
    visual.cx.update(|cx| {
        normal.update(cx, |view, _| view.edit_profile(|browser| browser.record_visit("https://normal.example/recent", NOW)));
        a.update(cx, |view, _| view.edit_browser(|browser| browser.new_tab("https://blank.example/local", NOW)));
        b.update(cx, |view, _| view.edit_profile(|browser| browser.record_visit("https://blank.example/recent", NOW)));
        normal.update(cx, |view, _| view.edit_profile(|browser| browser.record_visit("https://normal.example/newest", NOW)));
        a.update(cx, |view, _| view.edit_profile(|browser| browser.delete_history_entry("https://normal.example/recent")));
    });
    visual.run_until_parked();
    for view in [&normal, &a, &b] {
        assert_eq!(urls(view, visual), vec!["https://normal.example/newest", "https://blank.example/recent"]);
    }
    visual.cx.update(|cx| {
        b.update(cx, |view, _| view.edit_browser(|browser| browser.history.clear()));
        a.update(cx, |view, _| view.edit_browser(|browser| browser.new_tab("https://blank.example/after-clear", NOW)));
    });
    visual.run_until_parked();
    for view in [&normal, &a, &b] { assert!(urls(view, visual).is_empty()); }
}

#[gpui::test]
fn shared_history_never_shares_tab_ids_archive_or_selection(cx: &mut TestAppContext) {
    let (normal, visual) = open(cx);
    let a = blank(&normal, visual);
    let b = blank(&normal, visual);
    let normal_tab = normal.update(visual, |view, _| view.edit_browser(|browser| browser.new_tab("https://normal.example/tab", NOW)));
    let a_tab = a.update(visual, |view, _| view.edit_browser(|browser| browser.new_tab("https://a.example/tab", NOW)));
    let b_tab = b.update(visual, |view, _| view.edit_browser(|browser| browser.new_tab("https://b.example/tab", NOW)));
    assert_eq!(normal_tab, a_tab);
    assert_eq!(a_tab, b_tab);
    a.update(visual, |view, _| view.edit_browser(|browser| {
        browser.close_tab(a_tab, NOW).unwrap();
        browser.record_visit("https://a.example/tab", NOW);
    }));
    visual.run_until_parked();
    for (view, url) in [(&normal, "https://normal.example/tab"), (&b, "https://b.example/tab")] {
        view.read_with(visual, |view, _| {
            assert_eq!(view.browser.active_tab().unwrap().url, url);
            assert_eq!(view.browser.all_tabs().count(), 1);
            assert!(view.browser.archive.is_empty());
        });
    }
    normal.update(visual, |view, _| view.edit_browser(|browser| browser.history.clear()));
    visual.run_until_parked();
    a.read_with(visual, |view, _| {
        assert!(view.browser.active_tab().is_none());
        assert_eq!(view.browser.archive[0].tab.url, "https://a.example/tab");
    });
    a.update(visual, |view, _| view.edit_browser(|browser| browser.restore_last_archived(NOW)));
    visual.run_until_parked();
    a.read_with(visual, |view, _| assert_eq!(view.browser.active_tab().unwrap().url, "https://a.example/tab"));
    b.read_with(visual, |view, _| assert_eq!(view.browser.active_tab().unwrap().url, "https://b.example/tab"));
}

#[gpui::test]
fn private_history_is_isolated_and_only_normal_profile_serializes_blank_visits(cx: &mut TestAppContext) {
    let (normal, visual) = open(cx);
    let blank = blank(&normal, visual);
    let private_handle = visual.cx.update(|cx| ProfileHandle::new(Browser::new_private(), cx));
    let private = add_window(private_handle, visual);
    private.update(visual, |view, _| view.edit_profile(|browser| browser.record_visit("https://private.example/source", NOW)));
    blank.update(visual, |view, _| view.edit_profile(|browser| browser.record_visit("https://blank.example/from-private", NOW)));
    visual.run_until_parked();
    assert!(urls(&private, visual).is_empty());
    assert_eq!(urls(&normal, visual), vec!["https://blank.example/from-private"]);
    normal.read_with(visual, |view, _| {
        let json = view.profile.session_json(&None);
        let saved = browser_core::session::from_json(&json).unwrap();
        assert_eq!(saved.history[0].url, "https://blank.example/from-private");
        assert!(saved.all_tabs().next().is_none());
    });
    let path = std::env::temp_dir().join(format!("echo-blank-history-{}.json", std::process::id()));
    blank.update(visual, |view, cx| view.save_session_to(path.clone(), cx));
    blank.read_with(visual, |view, cx| assert!(view.profile.handle.owner.read(cx).path.is_none()));
}

#[gpui::test]
fn normal_session_writer_persists_blank_history_without_blank_tabs(cx: &mut TestAppContext) {
    let (normal, visual) = open(cx);
    let blank = blank(&normal, visual);
    let path = std::env::temp_dir().join(format!("echo-blank-writer-{}-{:?}.json", std::process::id(), normal.entity_id()));
    let _ = std::fs::remove_file(&path);
    normal.update(visual, |view, cx| view.save_session_to(path.clone(), cx));
    blank.update(visual, |view, _| view.edit_browser(|browser| {
        browser.new_tab("https://blank.example/local-tab", NOW);
        browser.record_visit("https://blank.example/saved-history", NOW);
    }));
    visual.executor().advance_clock(Duration::from_secs(2));
    visual.run_until_parked();
    let saved = browser_core::session::load(&path).unwrap().unwrap();
    assert_eq!(saved.history[0].url, "https://blank.example/saved-history");
    assert!(saved.all_tabs().next().is_none());
    std::fs::remove_file(path).unwrap();
}

#[gpui::test]
fn history_is_shared_across_one_profiles_spaces_but_isolated_from_other_profiles(cx: &mut TestAppContext) {
    let (normal_a, visual) = open(cx);
    let handle_b = visual.cx.update(|cx| ProfileHandle::new(Browser::new(), cx));
    let normal_b = add_window(handle_b, visual);
    let blank_a = blank(&normal_a, visual);
    let blank_b = blank(&normal_b, visual);
    normal_a.update(visual, |view, _| view.edit_browser(|browser| {
        browser.record_visit("https://profile-a.example/first-space", NOW);
        browser.add_space("Second Space", 120.0);
        browser.active_space = 1;
        browser.record_visit("https://profile-a.example/second-space", NOW);
    }));
    normal_b.update(visual, |view, _| view.edit_profile(|browser| {
        browser.record_visit("https://profile-b.example/normal", NOW);
    }));
    blank_a.update(visual, |view, _| view.edit_profile(|browser| browser.record_visit("https://profile-a.example/blank", NOW)));
    blank_b.update(visual, |view, _| view.edit_profile(|browser| browser.record_visit("https://profile-b.example/blank", NOW)));
    visual.run_until_parked();
    for view in [&normal_a, &blank_a] {
        assert_eq!(urls(view, visual), vec!["https://profile-a.example/blank", "https://profile-a.example/second-space", "https://profile-a.example/first-space"]);
    }
    for view in [&normal_b, &blank_b] {
        assert_eq!(urls(view, visual), vec!["https://profile-b.example/blank", "https://profile-b.example/normal"]);
    }
    normal_a.update(visual, |view, _| view.edit_browser(|browser| browser.active_space = 0));
    visual.run_until_parked();
    normal_a.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_space, 0);
        assert_eq!(view.browser.history.len(), 3);
    });
    blank_a.update(visual, |view, _| view.edit_browser(|browser| browser.history.clear()));
    visual.run_until_parked();
    for view in [&normal_a, &blank_a] { assert!(urls(view, visual).is_empty()); }
    for view in [&normal_b, &blank_b] { assert_eq!(urls(view, visual).len(), 2); }
}

#[gpui::test]
fn blank_alone_retains_normal_session_writer_after_all_normal_windows_close(cx: &mut TestAppContext) {
    let (normal, visual) = open(cx);
    let path = std::env::temp_dir().join(format!("echo-blank-retained-writer-{}-{:?}.json", std::process::id(), normal.entity_id()));
    let _ = std::fs::remove_file(&path);
    normal.update(visual, |view, cx| {
        view.edit_browser(|browser| browser.new_tab("https://normal.example/retained-tab", NOW));
        view.save_session_to(path.clone(), cx);
    });
    let source = normal.read_with(visual, |view, _| view.profile.handle.clone());
    let source_owner = source.owner.downgrade();
    let blank_handle = visual.cx.update(|cx| ProfileHandle::new_with_shared_history(Browser::new_blank(), &source, cx));
    let blank = add_window(blank_handle, visual);
    drop(source);
    visual.executor().advance_clock(Duration::from_secs(2));
    visual.run_until_parked();
    assert!(browser_core::session::load(&path).unwrap().unwrap().history.is_empty());

    let normal_window = normal.downgrade();
    visual.update(|window, _| window.remove_window());
    drop(normal);
    visual.cx.update(|cx| {
        // Do not let a retained application global mask a missing Blank owner.
        if cx.try_global::<crate::new_window::NormalProfile>().is_some() {
            drop(cx.remove_global::<crate::new_window::NormalProfile>());
        }
    });
    visual.run_until_parked();
    assert!(normal_window.upgrade().is_none());
    assert!(source_owner.upgrade().is_some());
    visual.cx.update(|cx| {
        assert!(cx.try_global::<crate::new_window::NormalProfile>().is_none());
        assert_eq!(cx.windows().len(), 1);
    });

    // This visit happens after the normal window's release/flush. Only the
    // writer retained through Blank can persist it on the next timer tick.
    blank.update(visual, |view, _| view.edit_browser(|browser| {
        browser.new_tab("https://blank.example/unsaved-local-tab", NOW);
        browser.record_visit("https://blank.example/after-normal-close", NOW);
        browser.update_history_title("https://blank.example/after-normal-close", "Blank retained writer");
    }));
    visual.executor().advance_clock(Duration::from_secs(2));
    visual.run_until_parked();
    let saved = browser_core::session::load(&path).unwrap().unwrap();
    assert_eq!(saved.history.len(), 1);
    assert_eq!(saved.history[0].url, "https://blank.example/after-normal-close");
    assert_eq!(saved.history[0].title, "Blank retained writer");
    assert_eq!(saved.all_tabs().map(|tab| tab.url.as_str()).collect::<Vec<_>>(), vec!["https://normal.example/retained-tab"]);
    assert!(saved.archive.is_empty());
    blank.read_with(visual, |view, _| assert_eq!(view.browser.active_tab().unwrap().url, "https://blank.example/unsaved-local-tab"));
    std::fs::remove_file(path).unwrap();
}
