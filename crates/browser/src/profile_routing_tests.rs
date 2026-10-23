//! Ownership regressions at the GPUI/model and recording-engine boundaries.
use super::*;
use browser_core::Browser;
use crate::pages::{Pages, StorageSelection};
use std::time::SystemTime;

const NOW: SystemTime = SystemTime::UNIX_EPOCH;
const URL: &str = "https://profiles.example/shared-url";
fn frame() -> Bounds { Bounds { x: 0., y: 0., width: 900., height: 600. } }
fn png() -> Vec<u8> { [b"\x89PNG\r\n\x1a\n".as_slice(), &[1]].concat() }

#[test]
fn pages_keep_one_namespace_and_recreate_changed_storage_with_no_late_snapshot() {
    let fake = FakeEngine::default();
    { let mut state = fake.0.borrow_mut(); state.capture_supported = true; state.inspector_supported = true; state.inspector_accept = true; }
    let mut browser = Browser::new();
    let default = browser.active_user_profile_id();
    let tab = browser.new_tab(URL, NOW);
    let mut pages = Pages::new(Box::new(fake.clone()));
    pages.set_parent(1usize as *mut _);
    pages.set_bounds(frame());
    pages.sync(&browser, true);
    let original = pages.identity_for(page(1)).unwrap();
    assert_eq!(fake.0.borrow().storage[&1], StorageSelection::Default);
    let request = pages.request_snapshot(tab).unwrap();
    pages.run_devtools(tab, crate::devtools::Command::Toggle).unwrap();
    let inspector = pages.devtools_status().unwrap().id;
    let named = browser.create_user_profile("Work").unwrap();
    browser.assign_space_user_profile(browser.space().id, named).unwrap();
    fake.take_calls();
    pages.sync(&browser, true);
    let replacement = pages.identity_for(page(2)).unwrap();
    assert_eq!(replacement.tab, tab);
    assert_eq!(replacement.profile, named);
    assert_ne!(replacement.generation, original.generation);
    assert!(!pages.accepts_identity(original));
    assert_eq!(fake.0.borrow().storage[&2], StorageSelection::for_profile(named));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)), "{calls:?}");
    assert!(calls.contains(&Call::InspectorClose(inspector)), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Navigate(..))), "{calls:?}");
    fake.0.borrow_mut().snapshots.push(engine::PageSnapshot { view: page(1), request, png: Some(png()) });
    fake.0.borrow_mut().events.push(Event::UrlChanged { view: page(1), url: "https://stale.example".into() });
    assert!(pages.take_events().is_empty());
    assert!(pages.snapshot(tab).is_none());
    assert!(pages.devtools_status().is_none());
    assert_eq!(browser.user_profile(default).unwrap().history.len(), 0);
}

#[test]
fn same_profile_favorites_use_one_live_view_across_spaces_and_window_hosts() {
    let fake = FakeEngine::default();
    let mut browser = Browser::new();
    let favorite = browser.new_tab(URL, NOW);
    browser.favorite_tab(favorite);
    browser.add_space("Shared", 120.);
    let mut a = Pages::new(Box::new(fake.clone()));
    a.set_parent(1usize as *mut _);
    a.set_bounds(frame());
    a.sync(&browser, true);
    let original = a.identity_for(page(1)).unwrap();
    browser.switch_space(1);
    browser.activate_tab(favorite, NOW);
    a.sync(&browser, true);
    let mut b = a.for_host(2usize as *mut _);
    b.set_bounds(frame());
    fake.take_calls();
    b.activate_host(&browser);
    let calls = fake.take_calls();
    assert_eq!(b.identity_for(page(1)), Some(original));
    assert!(calls.iter().any(|call| matches!(call, Call::MoveHost(1, 2, _))), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))), "{calls:?}");
    assert_eq!(browser.all_tabs().filter(|tab| tab.id == favorite).count(), 1);
}

#[gpui::test]
fn different_profiles_isolate_equal_urls_and_background_metadata(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    let (default, named, default_tab, named_tab) = view.update(visual, |view, _| view.edit_browser(|browser| {
        let default = browser.active_user_profile_id();
        let default_tab = browser.active_tab().unwrap().id;
        browser.set_url(default_tab, URL);
        browser.favorite_tab(default_tab);
        let named = browser.create_user_profile("Work").unwrap();
        browser.assign_space_user_profile(browser.spaces[1].id, named).unwrap();
        browser.switch_space(1);
        let named_tab = browser.new_tab(URL, NOW);
        browser.favorite_tab(named_tab);
        (default, named, default_tab, named_tab)
    }));
    visual.run_until_parked();
    fake.send(visual, Event::UrlChanged { view: page(1), url: URL.into() });
    fake.send(visual, Event::TitleChanged { view: page(1), title: "Default background page".into() });
    fake.send(visual, Event::Favicon { view: page(1), page_url: URL.into(), png: png() });
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_user_profile_id(), named);
        assert_eq!(view.browser.active_tab().unwrap().id, named_tab);
        let source = view.browser.user_profile(default).unwrap();
        assert_eq!(source.favorites[0].id, default_tab);
        assert_eq!(source.history[0].title, "Default background page");
        assert_eq!(view.browser.favicon(default, URL), Some(png().as_slice()));
        assert!(view.browser.favicon(named, URL).is_none());
        assert!(view.browser.user_profile(named).unwrap().history.is_empty());
        assert_eq!(view.browser.active_user_profile().favorites[0].id, named_tab);
    });
    assert_eq!(fake.0.borrow().storage[&1], StorageSelection::Default);
    assert!(fake.0.borrow().storage.values().any(|storage| *storage == StorageSelection::for_profile(named)));
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("Default background page");
    view.read_with(visual, |view, cx| assert!(!view.suggestions(cx).iter().any(|row| matches!(row, Suggestion::SwitchToTab { id, .. } if *id == default_tab))));
}

#[gpui::test]
fn callbacks_queued_behind_shortcut_keep_generation_and_do_not_edit_reassigned_tab(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    let tab = view.read_with(visual, |view, _| view.browser.active_tab().unwrap().id);
    visual.update(|window, cx| view.update(cx, |view, cx| {
        view.apply_events(vec![
            Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "t".into(), ..Default::default() } },
            Event::TitleChanged { view: page(1), title: "Stale queued title".into() },
            Event::UrlChanged { view: page(1), url: "https://stale.example/queued".into() },
            Event::Favicon { view: page(1), page_url: URL.into(), png: png() },
        ], window, cx);
        view.edit_browser(|browser| {
            let named = browser.create_user_profile("Replacement").unwrap();
            browser.assign_space_user_profile(browser.space().id, named).unwrap();
        });
    }));
    visual.run_until_parked();
    view.read_with(visual, |view, _| {
        let tab = view.browser.all_tabs().find(|entry| entry.id == tab).unwrap();
        assert_ne!(tab.page_title.as_deref(), Some("Stale queued title"));
        assert_ne!(tab.url, "https://stale.example/queued");
        assert!(view.browser.user_profiles().all(|profile| profile.history.is_empty() && profile.favicon_cache.is_empty()));
    });
}

#[gpui::test]
fn shared_profile_spaces_share_history_and_favorites_in_the_real_sidebar(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    let favorite = view.update(visual, |view, _| view.edit_browser(|browser| {
        let tab = browser.active_tab().unwrap().id;
        browser.favorite_tab(tab);
        let profile = browser.active_user_profile_id();
        browser.record_visit(profile, "https://shared.example/history", NOW);
        browser.update_history_title(profile, "https://shared.example/history", "Shared history needle");
        tab
    }));
    visual.run_until_parked();
    visual.simulate_keystrokes("ctrl-2");
    visual.executor().advance_clock(Duration::from_secs(1));
    visual.run_until_parked();
    assert!(visual.debug_bounds(test_selector(format!("favorite-{}", favorite.get()))).is_some());
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("Shared history needle");
    view.read_with(visual, |view, cx| {
        assert_eq!(view.browser.active_space, 1);
        assert!(view.browser.is_favorite(favorite));
        assert!(view.suggestions(cx).iter().any(|row| matches!(row, Suggestion::History { title, .. } if title == "Shared history needle")));
    });
}

#[gpui::test]
fn history_clear_confirmation_cannot_delete_the_reassigned_profiles_history(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    let (source, destination) = view.update(visual, |view, _| view.edit_browser(|browser| {
        let source = browser.active_user_profile_id();
        let destination = browser.create_user_profile("Destination").unwrap();
        browser.record_visit(source, "https://source.example/keep", NOW);
        browser.record_visit(destination, "https://destination.example/keep", NOW);
        (source, destination)
    }));
    visual.simulate_keystrokes("cmd-y");
    click(visual, "clear-history");
    assert!(visual.debug_bounds("history-clear-confirm").is_some());
    let history_tab = view.read_with(visual, |view, _| view.browser.active_tab().unwrap().id);
    // A stale confirmation callback arrives after reassignment but before any
    // frame can replace its painted button. It must invalidate, not target B.
    view.update(visual, |view, cx| {
        view.edit_browser(|browser| browser.assign_space_user_profile(browser.space().id, destination).unwrap());
        view.confirm_history_clear(source, cx);
    });
    visual.run_until_parked();
    assert!(visual.debug_bounds("history-clear-confirm").is_none());
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, history_tab);
        assert_eq!(view.browser.user_profile(source).unwrap().history.len(), 1);
        assert_eq!(view.browser.user_profile(destination).unwrap().history.len(), 1);
    });
    // Even an old A button callback arriving after a fresh B confirmation
    // must not consume B's request. The newly painted B button remains valid.
    click(visual, "clear-history");
    view.update(visual, |view, cx| view.confirm_history_clear(source, cx));
    assert_eq!(view.read_with(visual, |view, _| view.browser.user_profile(destination).unwrap().history.len()), 1);
    click(visual, "history-clear-confirm");
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.user_profile(source).unwrap().history.len(), 1);
        assert!(view.browser.user_profile(destination).unwrap().history.is_empty());
    });
}

#[gpui::test]
fn deferred_popup_restore_refreshes_other_hosts_reassignment_before_creating_any_page(cx: &mut TestAppContext) {
    let (a, visual, fake) = open_pages(cx);
    let tab = a.read_with(visual, |view, _| view.browser.active_tab().unwrap().id);
    let (b, _) = another_live_profile_window(&a, visual);
    let destination = b.update(visual, |view, _| view.edit_browser(|browser| browser.create_user_profile("Destination").unwrap()));
    a.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    visual.simulate_keystrokes("cmd-t");
    fake.take_calls();
    // Do not drain foreground notifications between A scheduling restoration
    // and B changing canonical ownership of the same Space.
    visual.cx.update(|cx| {
        a.update(cx, |view, cx| view.close_command_bar(cx));
        b.update(cx, |view, _| view.edit_browser(|browser| browser.assign_space_user_profile(browser.space().id, destination).unwrap()));
    });
    visual.run_until_parked();
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)), "{calls:?}");
    let recreated: Vec<_> = calls.iter().filter_map(|call| match call { Call::Create(id, _) => Some(*id), _ => None }).collect();
    assert!(!recreated.is_empty(), "{calls:?}");
    for id in recreated {
        assert_eq!(fake.0.borrow().storage[&id], StorageSelection::for_profile(destination));
    }
    a.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_user_profile_id(), destination);
        assert_eq!(view.browser.active_tab().unwrap().id, tab);
        assert!(view.pages.as_ref().unwrap().borrow().identity_for(page(1)).is_none());
    });
}

#[gpui::test]
fn deferred_page_cmd_w_is_rejected_after_another_host_retires_its_generation(cx: &mut TestAppContext) {
    let (a, visual, fake) = open_pages(cx);
    let tab = a.read_with(visual, |view, _| view.browser.active_tab().unwrap().id);
    let (b, _) = another_live_profile_window(&a, visual);
    let destination = b.update(visual, |view, _| view.edit_browser(|browser| browser.create_user_profile("Destination").unwrap()));
    a.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    fake.take_calls();
    visual.update(|window, cx| {
        a.update(cx, |view, cx| view.apply_events(vec![Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } }], window, cx));
        // The native shortcut has been accepted and deferred, but has not yet
        // dispatched a GPUI action. B retires its exact page generation now.
        b.update(cx, |view, _| view.edit_browser(|browser| browser.assign_space_user_profile(browser.space().id, destination).unwrap()));
    });
    visual.run_until_parked();
    a.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_user_profile_id(), destination);
        assert_eq!(view.browser.active_tab().unwrap().id, tab);
        assert!(view.browser.all_tabs().any(|entry| entry.id == tab));
        assert!(view.browser.archive.is_empty());
    });
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(id) if *id > 1)), "{calls:?}");
    assert!(fake.0.borrow().storage.iter().filter(|(id, _)| **id > 1).all(|(_, selection)| *selection == StorageSelection::for_profile(destination)));
}

#[gpui::test]
fn inactive_host_retained_shortcut_keeps_original_view_and_generation_after_recreation(cx: &mut TestAppContext) {
    let (x, visual, fake) = open_pages(cx);
    let original = x.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(page(1)).unwrap());
    let (y, _) = another_live_profile_window(&x, visual);
    let destination = y.update(visual, |view, _| view.edit_browser(|browser| browser.create_user_profile("Destination").unwrap()));
    x.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    fake.take_calls();
    // Y drains CEF first while X owns the callback's page. Keep every step in
    // one foreground update so no automatic host pump can shortcut the race.
    visual.update(|window, cx| {
        fake.0.borrow_mut().events.push(Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } });
        y.update(cx, |view, _| assert!(view.pages.as_ref().unwrap().borrow_mut().take_events().is_empty()));
        // The event has left CEF and is retained inside the shared PageStore.
        assert!(fake.0.borrow().events.is_empty());
        y.update(cx, |view, _| view.edit_browser(|browser| browser.assign_space_user_profile(browser.space().id, destination).unwrap()));
        x.update(cx, |view, cx| {
            view.refresh_profile_projection();
            let events = {
                let mut pages = view.pages.as_ref().unwrap().borrow_mut();
                pages.sync(&view.browser, true);
                assert!(!pages.accepts_identity(original));
                assert_eq!(pages.identity_for(page(2)).unwrap().profile, destination);
                let events = pages.take_events();
                assert!(events.is_empty(), "retained shortcut must remain bound to its retired source: {events:?}");
                events
            };
            view.apply_events(events, window, cx);
        });
    });
    visual.run_until_parked();
    x.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, original.tab);
        assert_eq!(view.browser.active_user_profile_id(), destination);
        assert!(view.browser.archive.is_empty());
    });
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)), "{calls:?}");
    assert!(!calls.contains(&Call::Close(2)), "replacement must survive: {calls:?}");
}

#[gpui::test]
fn command_bar_history_delete_rejects_painted_owner_after_other_host_reassignment(cx: &mut TestAppContext) {
    let (a, visual, _) = open_pages(cx);
    let url = "https://painted-history.example/same";
    let (source, destination) = a.update(visual, |view, _| view.edit_browser(|browser| {
        let source = browser.active_user_profile_id();
        let destination = browser.create_user_profile("Destination").unwrap();
        for owner in [source, destination] {
            browser.record_visit(owner, url, NOW);
            browser.update_history_title(owner, url, "Painted history needle");
        }
        (source, destination)
    }));
    let (b, _) = another_profile_window(&a, visual);
    visual.simulate_keystrokes("cmd-t");
    bar(&a, visual).simulate_input("Painted history needle");
    let painted_owner = a.read_with(visual, |view, _| view.browser.active_user_profile_id());
    assert_eq!(painted_owner, source);
    assert!(bar(&a, visual).debug_bounds("delete-suggestion-1").is_some());
    // Execute the previously painted callback after canonical ownership changes,
    // before A's normal observation/render queue can replace the old row.
    visual.cx.update(|cx| {
        b.update(cx, |view, _| view.edit_browser(|browser| browser.assign_space_user_profile(browser.space().id, destination).unwrap()));
        a.update(cx, |view, cx| view.delete_command_history_entry(painted_owner, url, cx));
    });
    visual.run_until_parked();
    a.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_user_profile_id(), destination);
        assert_eq!(view.browser.user_profile(source).unwrap().history.len(), 1);
        assert_eq!(view.browser.user_profile(destination).unwrap().history.len(), 1);
    });
    // A newly rendered destination row still deletes its own matching URL.
    click(&mut bar(&a, visual), "delete-suggestion-1");
    a.read_with(visual, |view, _| {
        assert_eq!(view.browser.user_profile(source).unwrap().history.len(), 1);
        assert!(view.browser.user_profile(destination).unwrap().history.is_empty());
    });
}

#[gpui::test]
fn retained_shortcut_does_not_migrate_to_surviving_host_but_metadata_keeps_fifo(cx: &mut TestAppContext) {
    let (x, visual, fake) = open_pages(cx);
    let original = x.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(page(1)).unwrap());
    let (y, _) = another_live_profile_window(&x, visual);
    let unrelated = y.update(visual, |view, _| view.edit_browser(|browser| browser.new_tab("https://surviving-host.example/unrelated", NOW)));
    visual.run_until_parked();
    x.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    fake.take_calls();
    let first = Event::TitleChanged { view: page(1), title: "Before source close".into() };
    let last = Event::TitleChanged { view: page(1), title: "After retained input".into() };
    visual.update(|window, cx| {
        fake.0.borrow_mut().events.extend([
            first.clone(),
            Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } },
            last.clone(),
        ]);
        y.update(cx, |view, _| assert!(view.pages.as_ref().unwrap().borrow_mut().take_events().is_empty()));
        assert!(fake.0.borrow().events.is_empty());
        // Closing X parks the same live document in Y, without advancing its
        // document generation. Input must not acquire that surviving host.
        x.update(cx, |view, cx| view.close_window(cx));
        y.update(cx, |view, cx| {
            let events = {
                let mut pages = view.pages.as_ref().unwrap().borrow_mut();
                assert_eq!(pages.identity_for(page(1)), Some(original));
                pages.take_events()
            };
            assert_eq!(events, vec![first, last], "metadata follows the live document; input belongs to closed X");
            view.apply_events(events, window, cx);
        });
    });
    visual.run_until_parked();
    y.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, unrelated);
        assert!(view.browser.all_tabs().any(|tab| tab.id == unrelated));
        assert_eq!(view.browser.all_tabs().find(|tab| tab.id == original.tab).unwrap().page_title.as_deref(), Some("After retained input"));
        assert!(view.browser.archive.is_empty());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Close(_))));
}

#[gpui::test]
fn deferred_shortcut_is_rejected_when_its_page_moves_while_source_host_remains_alive(cx: &mut TestAppContext) {
    let (x, visual, fake) = open_pages(cx);
    let original = x.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(page(1)).unwrap());
    let (y, _) = another_live_profile_window(&x, visual);
    x.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    let mut unrelated = None;
    fake.take_calls();
    visual.update(|window, cx| {
        x.update(cx, |view, cx| view.apply_events(vec![Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } }], window, cx));
        y.update(cx, |view, _| {
            view.edit_browser(|browser| browser.activate_tab(original.tab, NOW));
            view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser);
            assert_eq!(view.pages.as_ref().unwrap().borrow().identity_for(page(1)), Some(original));
        });
        unrelated = Some(x.update(cx, |view, _| view.edit_browser(|browser| browser.new_tab("https://source-host.example/unrelated", NOW))));
    });
    visual.run_until_parked();
    let unrelated = unrelated.unwrap();
    x.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, unrelated);
        assert!(view.browser.all_tabs().any(|tab| tab.id == unrelated));
        assert!(view.browser.all_tabs().any(|tab| tab.id == original.tab));
        assert!(view.browser.archive.is_empty());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Close(_))));
}

#[test]
fn input_attachment_does_not_revive_when_the_same_live_page_returns_to_its_original_host() {
    let fake = FakeEngine::default();
    let mut browser = Browser::new();
    browser.new_tab(URL, NOW);
    let mut x = Pages::new(Box::new(fake));
    x.set_parent(1usize as *mut _);
    x.set_bounds(frame());
    x.sync(&browser, true);
    let identity = x.identity_for(page(1)).unwrap();
    let origin = x.capture_event(Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } }).unwrap().input_origin.unwrap();
    let mut y = x.for_host(2usize as *mut _);
    y.set_bounds(frame());
    y.activate_host(&browser);
    assert!(!x.accepts_input_origin(origin));
    assert_eq!(y.identity_for(page(1)), Some(identity));
    x.activate_host(&browser);
    assert_eq!(x.identity_for(page(1)), Some(identity));
    assert!(!x.accepts_input_origin(origin), "old input cannot revive after a round trip");
}

#[gpui::test]
fn raw_engine_shortcut_is_captured_before_source_host_closes_without_any_prior_drain(cx: &mut TestAppContext) {
    let (x, visual, fake) = open_pages(cx);
    let original = x.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(page(1)).unwrap());
    let (y, _) = another_live_profile_window(&x, visual);
    let unrelated = y.update(visual, |view, _| view.edit_browser(|browser| browser.new_tab("https://survivor.example/raw-input", NOW)));
    visual.run_until_parked();
    x.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    fake.take_calls();
    visual.update(|window, cx| {
        fake.0.borrow_mut().events.push(Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } });
        assert_eq!(fake.0.borrow().events.len(), 1);
        // No Pages.take_events or host pump has converted the raw callback.
        x.update(cx, |view, cx| view.close_window(cx));
        assert!(fake.0.borrow().events.is_empty(), "release must capture raw input before parking the page");
        y.update(cx, |view, cx| {
            let events = {
                let mut pages = view.pages.as_ref().unwrap().borrow_mut();
                assert_eq!(pages.identity_for(page(1)), Some(original));
                pages.take_events()
            };
            assert!(events.is_empty(), "raw X input must never be stamped as Y input: {events:?}");
            view.apply_events(events, window, cx);
        });
    });
    visual.run_until_parked();
    y.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, unrelated);
        assert!(view.browser.all_tabs().any(|tab| tab.id == original.tab));
        assert!(view.browser.archive.is_empty());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Close(_))));
}

#[test]
fn raw_engine_input_is_captured_before_activation_transfers_the_live_page() {
    let fake = FakeEngine::default();
    let mut browser = Browser::new();
    browser.new_tab(URL, NOW);
    let mut x = Pages::new(Box::new(fake.clone()));
    x.set_parent(1usize as *mut _);
    x.set_bounds(frame());
    x.sync(&browser, true);
    let original = x.identity_for(page(1)).unwrap();
    let mut y = x.for_host(2usize as *mut _);
    y.set_bounds(frame());
    fake.0.borrow_mut().events.push(Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } });
    y.activate_host(&browser);
    assert!(fake.0.borrow().events.is_empty());
    assert_eq!(y.identity_for(page(1)), Some(original));
    assert!(y.take_events().is_empty(), "activation must capture old-host input before moving the document");
}

#[gpui::test]
fn deferred_inspector_shortcut_is_rejected_after_frontend_close_and_reopen(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    accept_recording_inspector(&view, visual, &fake);
    visual.simulate_keystrokes("cmd-alt-i");
    visual.run_until_parked();
    let original = inspector_status(&view, visual).unwrap();
    let tab = view.read_with(visual, |view, _| view.browser.active_tab().unwrap().id);
    let mut replacement = None;
    fake.take_calls();
    visual.update(|window, cx| view.update(cx, |view, cx| {
        view.apply_events(vec![Event::InspectorInput(engine::InspectorEvent::Shortcut { inspector: original.id, shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } })], window, cx);
        // Source page/host/attachment remain valid; only the frontend identity
        // changes after intake and before the queued keystroke is dispatched.
        let mut pages = view.pages.as_ref().unwrap().borrow_mut();
        let identity = pages.identity_for(original.inspected).unwrap();
        pages.run_devtools(tab, crate::devtools::Command::Toggle).unwrap();
        pages.run_devtools(tab, crate::devtools::Command::Toggle).unwrap();
        let reopened = pages.devtools_status().unwrap();
        assert_ne!(reopened.id, original.id);
        assert_eq!(pages.identity_for(original.inspected), Some(identity));
        replacement = Some(reopened.id);
    }));
    visual.run_until_parked();
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, tab);
        assert!(view.browser.archive.is_empty());
    });
    assert_eq!(inspector_status(&view, visual).unwrap().id, replacement.unwrap());
    assert!(!fake.take_calls().contains(&Call::Close(raw(original.inspected))));
}

#[gpui::test]
fn metadata_waiting_behind_x_shortcut_cannot_overwrite_newer_y_url_after_transfer(cx: &mut TestAppContext) {
    let (x, visual, fake) = open_pages(cx);
    let original = x.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(page(1)).unwrap());
    let (y, _) = another_live_profile_window(&x, visual);
    x.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    fake.take_calls();
    let old_url = "https://metadata-order.example/old";
    let new_url = "https://metadata-order.example/new";
    visual.update(|window, cx| {
        x.update(cx, |view, cx| view.apply_events(vec![
            Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } },
            Event::UrlChanged { view: page(1), url: old_url.into() },
        ], window, cx));
        // X's URL is now held in its BrowserWindow queue behind deferred Cmd+W.
        y.update(cx, |view, cx| {
            view.edit_browser(|browser| browser.activate_tab(original.tab, NOW));
            view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser);
            assert_eq!(view.pages.as_ref().unwrap().borrow().identity_for(page(1)), Some(original));
            view.apply_events(vec![Event::UrlChanged { view: page(1), url: new_url.into() }], window, cx);
            assert_eq!(view.browser.active_tab().unwrap().url, new_url);
        });
    });
    visual.run_until_parked();
    for view in [&x, &y] {
        view.read_with(visual, |view, _| {
            assert_eq!(view.browser.all_tabs().find(|tab| tab.id == original.tab).unwrap().url, new_url);
            assert_eq!(view.browser.user_profile(original.profile).unwrap().history[0].url, new_url);
            assert!(view.browser.user_profile(original.profile).unwrap().history.iter().any(|entry| entry.url == old_url));
            assert!(view.browser.archive.is_empty());
        });
    }
    let calls = fake.take_calls();
    assert!(!calls.iter().any(|call| matches!(call, Call::Navigate(_, url) if url == old_url)), "stale X metadata navigated the shared page: {calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(_))), "{calls:?}");
}

#[gpui::test]
fn closing_x_recovers_metadata_from_its_window_queue_before_the_shortcut_continuation(cx: &mut TestAppContext) {
    let (x, visual, _) = open_pages(cx);
    let original = x.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(page(1)).unwrap());
    let (y, _) = another_live_profile_window(&x, visual);
    let unrelated = y.update(visual, |view, _| view.edit_browser(|browser| browser.new_tab("https://metadata-survivor.example/unrelated", NOW)));
    visual.run_until_parked();
    x.update(visual, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    let pending_url = "https://metadata-survivor.example/pending-in-x";
    visual.update(|window, cx| {
        x.update(cx, |view, cx| view.apply_events(vec![
            Event::Shortcut { view: page(1), shortcut: Shortcut { cmd: true, key: "w".into(), ..Default::default() } },
            Event::UrlChanged { view: page(1), url: pending_url.into() },
        ], window, cx));
        x.update(cx, |view, cx| view.close_window(cx));
        y.update(cx, |view, cx| {
            let events = view.pages.as_ref().unwrap().borrow_mut().take_routed_events();
            assert!(events.iter().any(|event| matches!(&event.event, Event::UrlChanged { url, .. } if url == pending_url)), "metadata must outlive X's window queue");
            view.apply_page_events(events, window, cx);
            assert_eq!(view.browser.all_tabs().find(|tab| tab.id == original.tab).unwrap().url, pending_url);
        });
    });
    visual.run_until_parked();
    y.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, unrelated);
        assert_eq!(view.browser.all_tabs().find(|tab| tab.id == original.tab).unwrap().url, pending_url);
        assert_eq!(view.browser.user_profile(original.profile).unwrap().history[0].url, pending_url);
        assert!(view.browser.archive.is_empty());
    });
}

#[gpui::test]
fn ready_context_is_retried_by_normal_poll_without_events_revision_or_user_input(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    fake.0.borrow_mut().pending_storage.insert(StorageSelection::Default);
    let (view, visual) = open_with(cx, Some(Box::new(fake.clone())));
    visual.executor().advance_clock(Duration::from_millis(50));
    visual.run_until_parked();
    assert_eq!(fake.0.borrow().next_view, 0);
    assert!(view.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().creation_retry_pending()));
    let revision = view.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().mirror_revision());
    assert!(fake.0.borrow().events.is_empty());
    // Only the context readiness changes. No model edit, notification, event,
    // layout mutation or user action may be required to create this page.
    fake.0.borrow_mut().pending_storage.clear();
    assert_eq!(view.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().mirror_revision()), revision);
    visual.executor().advance_clock(Duration::from_millis(50));
    visual.run_until_parked();
    assert_eq!(fake.0.borrow().next_view, 1);
    assert_eq!(fake.0.borrow().storage[&1], StorageSelection::Default);
    assert!(!view.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().creation_retry_pending()));
    assert!(fake.0.borrow().events.is_empty());
}

#[test]
fn context_retry_rebuilds_storage_ownership_before_creating_the_pending_page() {
    let fake = FakeEngine::default();
    fake.0.borrow_mut().pending_storage.insert(StorageSelection::Default);
    let mut browser = Browser::new();
    let tab = browser.new_tab(URL, NOW);
    let mut pages = Pages::new(Box::new(fake.clone()));
    pages.set_bounds(frame());
    pages.sync(&browser, true);
    assert!(pages.creation_retry_pending());
    assert_eq!(fake.0.borrow().next_view, 0);
    let named = browser.create_user_profile("Reassigned while pending").unwrap();
    browser.assign_space_user_profile(browser.space().id, named).unwrap();
    fake.0.borrow_mut().pending_storage.clear();
    assert!(pages.retry_pending_creation(&browser));
    assert_eq!(pages.identity_for(page(1)).unwrap().tab, tab);
    assert_eq!(pages.identity_for(page(1)).unwrap().profile, named);
    assert_eq!(fake.0.borrow().storage[&1], StorageSelection::for_profile(named));
    assert_eq!(fake.0.borrow().storage_attempts, vec![StorageSelection::Default, StorageSelection::for_profile(named)]);
}

#[test]
fn context_readiness_does_not_revive_a_cancelled_creation_intent() {
    let fake = FakeEngine::default();
    fake.0.borrow_mut().pending_storage.insert(StorageSelection::Default);
    let mut browser = Browser::new();
    let tab = browser.new_tab(URL, NOW);
    let mut pages = Pages::new(Box::new(fake.clone()));
    pages.set_bounds(frame());
    pages.sync(&browser, true);
    assert!(pages.creation_retry_pending());
    browser.close_tab(tab, NOW).unwrap();
    fake.0.borrow_mut().pending_storage.clear();
    assert!(pages.retry_pending_creation(&browser));
    assert!(!pages.creation_retry_pending());
    assert_eq!(fake.0.borrow().next_view, 0);
    assert_eq!(fake.0.borrow().storage_attempts, vec![StorageSelection::Default]);
}

#[gpui::test]
fn permanent_page_creation_failure_does_not_start_automatic_clock_retries(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    fake.0.borrow_mut().reject_create = true;
    let (view, visual) = open_with(cx, Some(Box::new(fake.clone())));
    // Settle initial layout/mirror notifications before measuring idle polls.
    visual.executor().advance_clock(Duration::from_secs(1));
    visual.run_until_parked();
    let attempts = fake.0.borrow().storage_attempts.len();
    assert!(attempts > 0);
    assert!(!view.read_with(visual, |view, _| view.pages.as_ref().unwrap().borrow().creation_retry_pending()));
    for _ in 0..10 {
        visual.executor().advance_clock(Duration::from_millis(50));
        visual.run_until_parked();
    }
    assert_eq!(fake.0.borrow().storage_attempts.len(), attempts, "permanent None/failure must not spin the automatic context retry path");
    assert_eq!(fake.0.borrow().next_view, 0);
    assert!(fake.0.borrow().events.is_empty());
}
