use super::*;
use gpui::{TestAppContext, VisualTestContext};
thread_local! {
    static PREPARATIONS: RefCell<Vec<Result<DownloadRetirementPreparation, String>>> = RefCell::new(Vec::new());
    static CALLS: RefCell<Vec<&'static str>> = RefCell::new(Vec::new());
}
fn fake_prepare() -> Result<DownloadRetirementPreparation, String> {
    CALLS.with(|calls| calls.borrow_mut().push("prepare"));
    PREPARATIONS.with(|results| results.borrow_mut().remove(0))
}
fn fake_pump() {
    CALLS.with(|calls| calls.borrow_mut().push("pump"));
}
fn script(results: Vec<Result<DownloadRetirementPreparation, String>>) {
    PREPARATIONS.with(|p| *p.borrow_mut() = results);
    CALLS.with(|p| p.borrow_mut().clear());
}
#[test]
fn only_pending_retries_and_failure_never_unlocks_parent() {
    let mut phase = Phase::Preparing;
    assert!(!observe(
        &mut phase,
        Ok(DownloadRetirementPreparation::Pending)
    ));
    assert_eq!(phase, Phase::Preparing);
    assert!(!observe(&mut phase, Err("handoff rejected".into())));
    assert_eq!(phase, Phase::Failed("handoff rejected".into()));
    assert!(!observe(
        &mut phase,
        Ok(DownloadRetirementPreparation::Ready)
    ));
    let mut ready = Phase::Preparing;
    assert!(observe(
        &mut ready,
        Ok(DownloadRetirementPreparation::Ready)
    ));
    assert!(!observe(
        &mut ready,
        Ok(DownloadRetirementPreparation::Ready)
    ));
}

fn root(cx: &mut TestAppContext) -> AnyWindowHandle {
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            cx.new(|cx| crate::window::BrowserWindow::new(crate::demo_browser(), None, window, cx))
        })
        .unwrap()
        .into()
    })
}
#[gpui::test]
fn root_survives_pending_and_duplicate_close_then_drops_only_after_ready(cx: &mut TestAppContext) {
    script(vec![
        Ok(DownloadRetirementPreparation::Pending),
        Ok(DownloadRetirementPreparation::Ready),
    ]);
    let root = root(cx);
    cx.update(|cx| {
        assert!(claim(root, cx));
        // Represents release_host, before any static preparation can enter CEF.
        CALLS.with(|calls| calls.borrow_mut().push("release"));
        assert!(!claim(root, cx));
        prepare_with(root, cx, fake_prepare, fake_pump);
        CALLS.with(|calls| assert_eq!(*calls.borrow(), ["release"]));
    });
    let visual = VisualTestContext::from_window(root, cx);
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(1));
    visual.run_until_parked();
    assert!(cx.read(|cx| cx
        .windows()
        .iter()
        .any(|w| w.window_id() == root.window_id())));
    CALLS.with(|calls| assert_eq!(*calls.borrow(), ["release", "prepare", "pump"]));
    visual.executor().advance_clock(Duration::from_millis(7));
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(1));
    visual.run_until_parked();
    assert!(cx.read(|cx| cx
        .windows()
        .iter()
        .all(|w| w.window_id() != root.window_id())));
    CALLS.with(|calls| assert_eq!(*calls.borrow(), ["release", "prepare", "pump", "prepare"]));
}
#[gpui::test]
fn actual_handoff_error_keeps_root_and_does_not_pump_or_retry(cx: &mut TestAppContext) {
    script(vec![Err("native host unavailable".into())]);
    let root = root(cx);
    cx.update(|cx| {
        assert!(claim(root, cx));
        prepare_with(root, cx, fake_prepare, fake_pump);
    });
    let visual = VisualTestContext::from_window(root, cx);
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(1));
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_secs(5));
    visual.run_until_parked();
    assert!(cx.read(|cx| cx
        .windows()
        .iter()
        .any(|w| w.window_id() == root.window_id())));
    CALLS.with(|calls| assert_eq!(*calls.borrow(), ["prepare"]));
    cx.update(|cx| assert!(!claim(root, cx)));
}
#[gpui::test]
fn quit_owns_pending_parent_and_preempts_window_removal(cx: &mut TestAppContext) {
    script(vec![]);
    let root = root(cx);
    cx.update(|cx| {
        assert!(claim(root, cx));
        prepare_with(root, cx, fake_prepare, fake_pump);
        crate::quit::seal();
    });
    let visual = VisualTestContext::from_window(root, cx);
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(1));
    visual.run_until_parked();
    assert!(cx.read(|cx| cx
        .windows()
        .iter()
        .any(|w| w.window_id() == root.window_id())));
    CALLS.with(|calls| assert!(calls.borrow().is_empty()));
    cx.update(crate::quit::reset_for_test);
}
