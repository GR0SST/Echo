//! Recording adapter and real GPUI tests for the neutral identity seam.
use super::*;
use crate::pages::{Pages, StorageSelection};
use crate::window::download_probe::{self, admission};
use browser_core::Browser;
use gpui::AppContext;

fn frame() -> Bounds {
    Bounds {
        x: 0.,
        y: 0.,
        width: 800.,
        height: 600.,
    }
}
fn probe_config() -> download_probe::Config {
    download_probe::Config::parse(&format!(
        "http://127.0.0.1:18473 {} continue-normal",
        std::env::temp_dir().display()
    ))
    .unwrap()
}
#[test]
fn pending_retry_passes_fresh_typed_identity_instead_of_cached_profile() {
    let fake = FakeEngine::default();
    fake.0
        .borrow_mut()
        .pending_storage
        .insert(StorageSelection::Default);
    let mut browser = Browser::new();
    let tab = browser.new_tab(
        "https://example.test/pending",
        std::time::SystemTime::UNIX_EPOCH,
    );
    let mut pages = Pages::new(Box::new(fake.clone()));
    pages.set_bounds(frame());
    pages.sync(&browser, true);
    assert!(pages.creation_retry_pending());
    let first = fake.0.borrow().identity_attempts[0].clone();
    assert_eq!(first.0.tab, tab);
    assert_eq!(first.0.profile, browser.active_user_profile_id());
    assert_eq!(first.1, browser.space().id);
    let named = browser
        .create_user_profile("Pending identity fixture")
        .unwrap();
    browser
        .assign_space_user_profile(browser.space().id, named)
        .unwrap();
    fake.0.borrow_mut().pending_storage.clear();
    assert!(pages.retry_pending_creation(&browser));
    let state = fake.0.borrow();
    let accepted = state.identity_attempts.last().unwrap();
    assert_eq!(accepted.0.tab, tab);
    assert_eq!(accepted.0.profile, named);
    assert_ne!(accepted.0.generation, first.0.generation);
    assert_eq!(accepted.2, StorageSelection::for_profile(named));
    assert_eq!(state.next_view, 1);
}
#[test]
fn source_capture_accepts_only_new_normal_today_fixture() {
    let config = probe_config();
    let mut browser = Browser::new();
    let fixture = browser.new_tab(config.fixture_url(), std::time::SystemTime::UNIX_EPOCH);
    let mut pages = Pages::new(Box::new(FakeEngine::default()));
    pages.set_parent(1usize as *mut std::ffi::c_void); // Recording fake, no native work.
    pages.reconcile_inventory(&browser);
    let source = pages
        .download_probe_source(&browser, fixture, &config)
        .unwrap();
    assert_eq!(source.identity.tab, fixture);
    assert_eq!(source.identity.profile, browser.active_user_profile_id());
    assert_eq!(source.space, browser.space().id);
    assert_eq!(
        source.storage,
        StorageSelection::for_profile(source.identity.profile)
    );
    let other = browser.new_tab(
        "https://example.test/ordinary",
        std::time::SystemTime::UNIX_EPOCH,
    );
    pages.reconcile_inventory(&browser);
    assert!(pages
        .download_probe_source(&browser, other, &config)
        .is_err());
    browser.activate_tab(fixture, std::time::SystemTime::UNIX_EPOCH);
    pages.set_bounds(frame());
    pages.sync(&browser, true);
    assert!(pages
        .download_probe_source(&browser, fixture, &config)
        .is_err()); // Existing view is never adopted.
}
#[gpui::test]
fn arm_creates_one_today_fixture_and_repeated_arm_does_not_create_another(cx: &mut TestAppContext) {
    let fake = FakeEngine::default();
    let (view, visual) = open_with(cx, Some(Box::new(fake.clone())));
    let before = view.read_with(visual, |view, _| view.browser.space().today.len());
    let config = probe_config();
    let fixture = config.fixture_url();
    view.update(visual, |view, cx| {
        // GPUI's test platform has no real parent. The recording fake never
        // dereferences this marker, but the source descriptor still scopes it.
        view.pages
            .as_ref()
            .unwrap()
            .borrow_mut()
            .set_parent(1usize as *mut std::ffi::c_void);
        view.arm_download_probe(config, cx).unwrap();
    });
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.space().today.len(), before + 1);
        assert_eq!(view.browser.active_tab().unwrap().url, fixture);
    });
    view.update(visual, |view, cx| {
        assert!(view.arm_download_probe(probe_config(), cx).is_err())
    });
    assert_eq!(
        view.read_with(visual, |view, _| view.browser.space().today.len()),
        before + 1
    );
    assert!(fake
        .0
        .borrow()
        .identity_attempts
        .iter()
        .any(|attempt| attempt.3 == fixture));
    admission::reset();
}
#[gpui::test]
fn close_window_shortcut_uses_deferred_parent_barrier(cx: &mut TestAppContext) {
    let (view, visual) = open(cx);
    let root = view.read_with(visual, |view, _| view.main_window);
    let (_survivor, _) = another_profile_window(&view, visual);
    visual.simulate_keystrokes("cmd-shift-w");
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(1));
    visual.run_until_parked();
    assert!(visual.cx.read(|cx| cx
        .windows()
        .iter()
        .all(|w| w.window_id() != root.window_id())));
}
