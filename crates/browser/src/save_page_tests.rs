//! Save As through real GPUI shortcuts, Command Bar and platform path replies.
use super::*;
fn enable(fake: &FakeEngine, cx: &mut VisualTestContext) {
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    fake.0.borrow_mut().archive_data =
        Some(include_bytes!("save_page_test_fixture.mhtml").to_vec());
}
fn ready(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(30));
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
}
fn no_navigation(fake: &FakeEngine) {
    assert!(!fake.take_calls().iter().any(|call| matches!(
        call,
        Call::Create(..) | Call::Navigate(..) | Call::Reload(..) | Call::Close(..)
    )));
}

#[gpui::test]
fn save_as_cancel_closes_a_command_bar_draft_and_does_not_capture_or_replace_the_page(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    enable(&fake, cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    let mut command = bar(&view, cx);
    command.simulate_input("save page as");
    command.simulate_keystrokes("enter");
    ready(cx);
    assert!(view.read_with(cx, |view, _| view.command_bar.is_none()));
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| !view.is_saving_page()
        && !view.save_prompt_active));
    assert!(!fake.take_calls().iter().any(|call| matches!(
        call,
        Call::ArchivePage(..) | Call::Create(..) | Call::Navigate(..) | Call::Close(..)
    )));
}

#[gpui::test]
fn save_as_shortcut_writes_the_document_and_resources_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    enable(&fake, cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Fixture.html");
    cx.simulate_keystrokes("cmd-shift-s");
    ready(cx);
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    cx.run_until_parked();
    let html = std::fs::read_to_string(&path).unwrap();
    assert!(html.contains("Fixture_files/1_theme.css"));
    assert!(directory.path().join("Fixture_files/2_image.png").is_file());
    assert_eq!(active_url(&view, cx), original);
    assert!(view.read_with(cx, |view, _| !view.is_saving_page()));
    no_navigation(&fake);
}

#[gpui::test]
fn save_as_cancels_a_same_address_reload_while_the_path_panel_is_open(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    enable(&fake, cx);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Changed.html");
    dispatch_menu(cx, "File", "Save Page As");
    ready(cx);
    cx.simulate_keystrokes("cmd-r");
    fake.take_calls();
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    cx.run_until_parked();
    assert!(!path.exists());
    assert!(view.read_with(cx, |view, _| !view.is_saving_page()));
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::ArchivePage(..))));
}

#[gpui::test]
fn save_as_owner_close_cancels_the_pending_path_reply(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    enable(&fake, cx);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Closed.html");
    cx.simulate_keystrokes("cmd-shift-s");
    ready(cx);
    dispatch_menu(cx, "File", "Close Window");
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    cx.run_until_parked();
    assert!(!path.exists());
    assert!(view.read_with(cx, |view, _| !view.is_saving_page()));
}
