use super::*;

#[gpui::test]
fn little_popup_navigates_the_existing_lookup_without_hidden_tabs(cx: &mut TestAppContext) {
    let (parent, cx, fake) = open_pages(cx);
    let parent_before = parent.read_with(cx, |v, _| (v.browser.all_tabs().count(), v.browser.space().active_tab));
    cx.simulate_keystrokes("cmd-alt-n"); cx.run_until_parked();
    let (little, mut lookup) = little_root(cx);
    bar(&little, &mut lookup).simulate_input("https://lookup.example/child");
    bar(&little, &mut lookup).simulate_keystrokes("enter"); lookup.run_until_parked();
    let engine = fake.0.borrow().forks.last().unwrap().clone();
    let page = engine.0.borrow().focused_page.unwrap();
    let original = little.read_with(&lookup, |v, _| (v.main_window.window_id(), v.browser.space().active_tab, v.browser.all_tabs().count()));
    engine.take_calls();
    engine.send(&mut lookup, Event::OpenInNewTab { opener: page, url: "https://lookup.example/blank".into() });
    lookup.run_until_parked();
    little.read_with(&lookup, |v, _| {
        assert_eq!((v.main_window.window_id(), v.browser.space().active_tab, v.browser.all_tabs().count()), original);
        assert_eq!(v.browser.active_tab().unwrap().url, "https://lookup.example/blank");
    });
    let calls = engine.take_calls();
    assert!(calls.iter().any(|c| matches!(c, Call::Navigate(_, url) if url=="https://lookup.example/blank")), "{calls:?}");
    assert!(!calls.iter().any(|c| matches!(c, Call::Create(..)|Call::Close(..))), "{calls:?}");
    assert_eq!(parent.read_with(cx, |v, _| (v.browser.all_tabs().count(), v.browser.space().active_tab)), parent_before);
}

#[gpui::test]
fn peek_popup_opens_parent_tab_and_return_restores_the_same_live_lookup(cx: &mut TestAppContext) {
    let (parent, cx, fake) = open_pages(cx);
    let original = parent.read_with(cx, |v, _| (v.browser.space().active_tab.unwrap(), v.browser.active_tab().unwrap().url.clone(), v.browser.all_tabs().count()));
    fake.send(cx, Event::OpenPeek { opener: page(1), url: "https://lookup.example/child".into() });
    let (child, mut lookup) = peek_window(&parent, cx);
    let engine = fake.0.borrow().forks.last().unwrap().clone();
    let page = engine.0.borrow().focused_page.unwrap();
    engine.send(&mut lookup, Event::UrlChanged { view: page, url: "https://lookup.example/grand".into() });
    let window = child.read_with(&lookup, |v, _| v.main_window.window_id());
    engine.take_calls(); fake.take_calls();
    engine.send(&mut lookup, Event::OpenInNewTab { opener: page, url: "https://lookup.example/blank".into() });
    lookup.run_until_parked(); cx.run_until_parked();
    parent.read_with(cx, |v, _| {
        assert!(v.peek_child.is_none()); assert_eq!(v.parked_peeks.len(), 1);
        assert_eq!(v.browser.active_tab().unwrap().url, "https://lookup.example/blank");
        assert_eq!(v.browser.all_tabs().count(), original.2+1);
        assert_eq!(v.browser.all_tabs().find(|t|t.id==original.0).unwrap().url, original.1);
    });
    child.read_with(&lookup, |v, _| {
        assert!(v.peek_owner.as_ref().unwrap().parked);
        assert_eq!(v.browser.all_tabs().count(), 1);
        assert_eq!(v.browser.active_tab().unwrap().url, "https://lookup.example/grand");
    });
    assert!(!engine.take_calls().iter().any(|c| matches!(c, Call::Create(..)|Call::Navigate(..)|Call::Close(..))));
    click(cx, test_selector(format!("tab-{}", original.0.get()))); cx.run_until_parked(); lookup.run_until_parked();
    parent.read_with(cx, |v, _| { assert_eq!(v.peek_child.unwrap().window_id(), window); assert!(v.parked_peeks.is_empty()); });
    assert!(lookup.debug_bounds("peek-card").is_some());
    child.read_with(&lookup, |v, _| { assert!(!v.peek_owner.as_ref().unwrap().parked); assert_eq!(v.browser.active_tab().unwrap().url, "https://lookup.example/grand"); });
    assert!(!engine.take_calls().iter().any(|c| matches!(c, Call::Create(..)|Call::Navigate(..)|Call::Close(..))));
}

#[gpui::test]
fn peek_background_popup_keeps_the_lookup_and_parent_selection(cx: &mut TestAppContext) {
    let (parent, cx, fake) = open_pages(cx);
    let original = parent.read_with(cx, |v, _| (v.browser.space().active_tab, v.browser.active_user_profile_id(), v.browser.all_tabs().count()));
    fake.send(cx, Event::OpenPeek { opener: page(1), url: "https://lookup.example/child".into() });
    let (child, mut lookup) = peek_window(&parent, cx);
    let engine = fake.0.borrow().forks.last().unwrap().clone();
    let page = engine.0.borrow().focused_page.unwrap();
    engine.take_calls();
    engine.send(&mut lookup, Event::OpenInBackgroundTab { opener: page, url: "https://lookup.example/background".into() });
    lookup.run_until_parked(); cx.run_until_parked();
    parent.read_with(cx, |v, _| {
        assert!(v.peek_child.is_some()); assert!(v.parked_peeks.is_empty());
        assert_eq!((v.browser.space().active_tab, v.browser.active_user_profile_id()), (original.0, original.1));
        assert_eq!(v.browser.all_tabs().count(), original.2+1);
        let new = v.browser.all_tabs().find(|t|t.url=="https://lookup.example/background").unwrap();
        assert_eq!(v.browser.user_profile_id_for_tab(new.id), Some(original.1));
    });
    assert_eq!(child.read_with(&lookup, |v, _| v.browser.all_tabs().count()), 1);
    assert!(!engine.take_calls().iter().any(|c| matches!(c, Call::Create(..)|Call::Navigate(..)|Call::Close(..))));
}

#[gpui::test]
fn retiring_the_source_releases_its_parked_lookup(cx: &mut TestAppContext) {
    let (parent, cx, fake) = open_pages(cx);
    let source = parent.read_with(cx, |v, _| v.browser.space().active_tab.unwrap());
    fake.send(cx, Event::OpenPeek { opener: page(1), url: "https://lookup.example/child".into() });
    let (_, mut lookup) = peek_window(&parent, cx);
    let engine = fake.0.borrow().forks.last().unwrap().clone();
    let page = engine.0.borrow().focused_page.unwrap();
    engine.send(&mut lookup, Event::OpenInNewTab { opener: page, url: "https://lookup.example/blank".into() });
    cx.run_until_parked(); lookup.run_until_parked(); engine.take_calls();
    parent.update(cx, |v, cx| { v.edit_browser(|b| b.close_tab(source, std::time::SystemTime::now())); cx.notify(); });
    cx.run_until_parked(); lookup.run_until_parked(); cx.run_until_parked();
    assert!(parent.read_with(cx, |v, _| v.parked_peeks.is_empty()));
    assert!(engine.take_calls().iter().any(|c| matches!(c, Call::Close(_))));
}

#[gpui::test]
fn closing_the_parent_releases_its_parked_lookup(cx: &mut TestAppContext) {
    let (parent, cx, fake) = open_pages(cx);
    fake.send(cx, Event::OpenPeek { opener: page(1), url: "https://lookup.example/child".into() });
    let (_, mut lookup) = peek_window(&parent, cx);
    let engine = fake.0.borrow().forks.last().unwrap().clone();
    let page = engine.0.borrow().focused_page.unwrap();
    engine.send(&mut lookup, Event::OpenInNewTab { opener: page, url: "https://lookup.example/blank".into() });
    cx.run_until_parked(); lookup.run_until_parked(); engine.take_calls();
    parent.update(cx, |v, cx| v.close_window(cx));
    cx.run_until_parked(); lookup.run_until_parked();
    assert!(engine.take_calls().iter().any(|c| matches!(c, Call::Close(_))));
}

#[gpui::test]
fn retiring_the_source_releases_a_visible_lookup(cx: &mut TestAppContext) {
    let (parent, cx, fake) = open_pages(cx);
    let source = parent.read_with(cx, |v, _| v.browser.space().active_tab.unwrap());
    fake.send(cx, Event::OpenPeek { opener: page(1), url: "https://lookup.example/child".into() });
    let (_, mut lookup) = peek_window(&parent, cx);
    let engine = fake.0.borrow().forks.last().unwrap().clone();
    engine.take_calls();
    parent.update(cx, |v, cx| { v.edit_browser(|b| b.close_tab(source, std::time::SystemTime::now())); cx.notify(); });
    cx.run_until_parked(); lookup.run_until_parked(); cx.run_until_parked();
    assert!(parent.read_with(cx, |v, _| v.peek_child.is_none() && v.parked_peeks.is_empty()));
    assert!(engine.take_calls().iter().any(|c| matches!(c, Call::Close(_))));
}

#[gpui::test]
fn source_hover_cannot_put_a_sibling_overlay_above_the_active_peek(cx: &mut TestAppContext) {
    let (parent, cx, fake) = open_pages(cx);
    fake.send(cx, Event::OpenPeek { opener: page(1), url: "https://lookup.example/child".into() });
    let (_, mut lookup) = peek_window(&parent, cx);
    fake.send(cx, Event::StatusChanged { view: page(1), text: "https://source.example/link".into() });
    assert!(parent.read_with(cx, |v, _| v.link_status.is_none()));
    let engine = fake.0.borrow().forks.last().unwrap().clone();
    let child_page = engine.0.borrow().focused_page.unwrap();
    engine.send(&mut lookup, Event::StatusChanged { view: child_page, text: "https://lookup.example/link".into() });
    let child = parent.read_with(cx, |v, _| v.peek_child.unwrap());
    assert!(child.read_with(cx, |v, _| v.link_status.is_some()).unwrap());
}
