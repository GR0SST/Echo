//! Handoff policy exercised through real GPUI windows and the shared fake engine.
use super::*;
use std::sync::Arc;

fn supported_engine() -> FakeEngine {
    let fake = FakeEngine::default();
    fake.0.borrow_mut().capture_supported = true;
    fake
}

fn capture_request(fake: &FakeEngine) -> u64 {
    let calls = fake.take_calls();
    assert!(!calls.iter().any(|call| matches!(call, Call::MoveHost(..) | Call::Create(..) | Call::Navigate(..))), "{calls:?}");
    let requests: Vec<_> = calls.iter().filter_map(|call| match call { Call::Snapshot(1, request) => Some(*request), _ => None }).collect();
    assert_eq!(requests.len(), 1, "{calls:?}");
    requests[0]
}

fn complete(fake: &FakeEngine, request: u64, png: Option<Vec<u8>>) {
    fake.0.borrow_mut().snapshots.push(engine::PageSnapshot { view: page(1), request, png });
}

#[gpui::test]
fn handoff_captures_before_move_and_source_poll_wakes_the_target(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let source_frame = fake.0.borrow().bounds[&1];
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    assert_eq!(fake.0.borrow().bounds[&1], source_frame);
    assert!(a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
    assert!(!b.read_with(&vb, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
    // Repeated layouts coalesce rather than starting more captures.
    b.update(&mut vb, |view, _| view.pages.as_ref().unwrap().borrow_mut().sync(&view.browser, true));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Snapshot(..) | Call::MoveHost(..))));
    // A result immediately before the deadline must still preserve the source.
    vb.executor().advance_clock(Duration::from_millis(249));
    vb.run_until_parked();
    complete(&fake, request, Some(vec![1, 2, 3]));
    a.update(va, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::MoveHost(..))));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::MoveHost(1, 2, _))).count(), 1, "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
    a.read_with(va, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        let mirrors = pages.mirrors();
        assert_eq!(mirrors.len(), 1);
        assert_eq!((mirrors[0].0, mirrors[0].1), (tab, source_frame));
        assert_eq!(mirrors[0].2.as_slice(), &[1, 2, 3]);
    });
}

#[gpui::test]
fn each_host_keeps_its_own_viewport_image_and_stable_arc(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let first = capture_request(&fake);
    let target_frame = Bounds { x: 80., y: 30., width: 420., height: 310. };
    b.update(&mut vb, |view, _| view.pages.as_ref().unwrap().borrow_mut().set_bounds(target_frame));
    complete(&fake, first, Some(vec![10]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert_eq!(fake.0.borrow().bounds[&1], target_frame);
    let photograph = a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().mirrors()[0].2.clone());
    let diagnostic = b.update(&mut vb, |view, _| view.pages.as_ref().unwrap().borrow_mut().request_snapshot(tab).unwrap());
    complete(&fake, diagnostic, Some(vec![20]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    a.read_with(va, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        assert!(Arc::ptr_eq(&photograph, &pages.mirrors()[0].2));
        assert_eq!(pages.mirrors()[0].2.as_slice(), &[10]);
    });
    fake.take_calls();
    a.update(va, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    let returning = capture_request(&fake);
    complete(&fake, returning, Some(vec![30]));
    a.update(va, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().mirrors().is_empty()));
    b.read_with(&vb, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        assert_eq!(pages.mirrors()[0].1, target_frame);
        assert_eq!(pages.mirrors()[0].2.as_slice(), &[30]);
    });
}

#[gpui::test]
fn failed_capture_keeps_valid_source_cache_and_transfers(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let cached = a.update(va, |view, _| view.pages.as_ref().unwrap().borrow_mut().request_snapshot(tab).unwrap());
    complete(&fake, cached, Some(vec![7]));
    a.update(va, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    complete(&fake, request, None);
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::MoveHost(1, 2, _))));
    assert_eq!(a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().mirrors()[0].2.as_slice().to_vec()), vec![7]);
}

#[gpui::test]
fn cancelled_activation_ignores_late_capture_and_new_claim_has_new_generation(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let obsolete = capture_request(&fake);
    a.update(va, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    complete(&fake, obsolete, Some(vec![99]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::MoveHost(..))));
    b.update(&mut vb, |view, _| view.pages.as_ref().unwrap().borrow_mut().activate_host(&view.browser));
    let current = capture_request(&fake);
    assert_ne!(current, obsolete);
    complete(&fake, obsolete, Some(vec![98]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::MoveHost(..))));
    complete(&fake, current, Some(vec![3]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert_eq!(a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().mirrors()[0].2.as_slice().to_vec()), vec![3]);
}

#[gpui::test]
fn stalled_capture_has_a_bounded_deadline_and_late_result_cannot_create_a_mirror(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    vb.executor().advance_clock(Duration::from_millis(249));
    vb.run_until_parked();
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::MoveHost(..))));
    assert!(a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
    assert!(b.read_with(&vb, |view, _| view.pages.as_ref().unwrap().borrow().snapshot_pending(tab, request)));
    vb.executor().advance_clock(Duration::from_millis(1));
    vb.run_until_parked();
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::MoveHost(1, 2, _))));
    complete(&fake, request, Some(vec![99]));
    a.update(va, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().mirrors().is_empty()));
    assert!(b.read_with(&vb, |view, _| view.pages.as_ref().unwrap().borrow().snapshot(tab).is_none()));
}

#[gpui::test]
fn capture_arriving_at_the_deadline_before_polling_cannot_create_a_mirror(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    vb.executor().advance_clock(Duration::from_millis(250));
    // Queue the external callback before the next poll processes the timeout.
    complete(&fake, request, Some(vec![99]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::MoveHost(1, 2, _))));
    assert!(a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().mirrors().is_empty()));
    assert!(b.read_with(&vb, |view, _| view.pages.as_ref().unwrap().borrow().snapshot(tab).is_none()));
}

#[gpui::test]
fn closing_the_source_cancels_capture_before_parking_the_live_view(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    a.update(va, |view, _| view.pages.as_ref().unwrap().borrow_mut().release_host());
    complete(&fake, request, Some(vec![99]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(b.read_with(&vb, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
    assert!(a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().mirrors().is_empty()));
    assert!(b.read_with(&vb, |view, _| view.pages.as_ref().unwrap().borrow().snapshot(tab).is_none()));
}

#[gpui::test]
fn deferred_transfer_preserves_ui_focus_until_explicit_page_intent(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    b.update(&mut vb, |view, _| view.pages.as_ref().unwrap().borrow_mut().focus_ui());
    fake.take_calls();
    complete(&fake, request, Some(vec![5]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::FocusUi), "{calls:?}");
    assert!(!calls.contains(&Call::FocusPage(1)), "{calls:?}");
    b.update(&mut vb, |view, _| {
        let mut pages = view.pages.as_ref().unwrap().borrow_mut();
        pages.request_page_focus();
        pages.sync(&view.browser, true);
    });
    assert!(fake.take_calls().contains(&Call::FocusPage(1)));
}

#[gpui::test]
fn opening_the_command_bar_preserves_input_focus_when_the_pending_page_arrives(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    vb.simulate_keystrokes("cmd-t");
    complete(&fake, request, Some(vec![99]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(..))));
    assert!(!a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().is_visible(tab)));
    b.read_with(&vb, |view, _| {
        assert!(view.command_bar.is_some());
        assert!(!view.pages.as_ref().unwrap().borrow().snapshot_pending(tab, request));
    });
}

#[gpui::test]
fn discarding_a_pending_source_view_rejects_its_callback_after_recreation(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    a.update(va, |view, _| view.pages.as_ref().unwrap().borrow_mut().discard(tab));
    b.update(&mut vb, |view, _| view.pages.as_ref().unwrap().borrow_mut().sync(&view.browser, true));
    complete(&fake, request, Some(vec![99]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    b.read_with(&vb, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        assert!(pages.is_visible(tab));
        assert!(pages.snapshot(tab).is_none());
    });
    assert!(a.read_with(va, |view, _| view.pages.as_ref().unwrap().borrow().mirrors().is_empty()));
}

#[gpui::test]
fn mirror_geometry_changes_revision_without_recapturing_or_changing_pixels(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    complete(&fake, request, Some(vec![8]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    let (revision, image) = a.read_with(va, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        (pages.mirror_revision(), pages.mirrors()[0].2.clone())
    });
    fake.take_calls();
    let bounds = Bounds { x: 130., y: 42., width: 560., height: 390. };
    a.update(va, |view, _| view.pages.as_ref().unwrap().borrow_mut().set_bounds(bounds));
    a.read_with(va, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        assert_ne!(pages.mirror_revision(), revision);
        assert_eq!(pages.mirrors()[0].1, bounds);
        assert!(Arc::ptr_eq(&pages.mirrors()[0].2, &image));
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Snapshot(..) | Call::MoveHost(..))));
}

#[gpui::test]
fn deferred_capture_completion_does_not_navigate_from_cached_window_intent(cx: &mut TestAppContext) {
    let fake = supported_engine();
    let (a, va) = open_with(cx, Some(Box::new(fake.clone())));
    let tab = a.read_with(va, |view, _| view.browser.active_tab().unwrap().id);
    let (b, mut vb) = another_live_profile_window(&a, va);
    fake.take_calls();
    click(&mut vb, test_selector(format!("tab-{}", tab.get())));
    let request = capture_request(&fake);
    // A redirect arrives while B's capture is pending, before B renders the
    // updated profile projection. Completion must only transfer the document.
    a.update(va, |view, _| view.pages.as_ref().unwrap().borrow_mut().record(&Event::UrlChanged {
        view: page(1), url: "https://redirect.example/final".into(),
    }));
    complete(&fake, request, Some(vec![42]));
    b.update(&mut vb, |view, _| { view.pages.as_ref().unwrap().borrow_mut().take_events(); });
    let calls = fake.take_calls();
    assert!(calls.iter().any(|call| matches!(call, Call::MoveHost(..))), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Navigate(..))), "{calls:?}");
}
