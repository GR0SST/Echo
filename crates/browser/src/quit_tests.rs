use super::*;
use std::collections::VecDeque;

fn status(phase: ShutdownPhase) -> ShutdownStatus {
    ShutdownStatus { phase, pending_owners: 0, pending_browsers: vec![], pending_cookie_flushes: 0, failures: vec![] }
}

struct FakeDriver {
    owners: Rc<Cell<usize>>,
    begins: usize,
    callbacks: VecDeque<Result<ShutdownStatus, String>>,
}
impl ShutdownDriver for FakeDriver {
    fn begin(&mut self) -> Result<ShutdownStatus, String> {
        self.begins += 1;
        Ok(status(ShutdownPhase::WaitingForOwners))
    }
    fn poll(&mut self) -> Result<ShutdownStatus, String> {
        if self.owners.get() != 0 {
            let mut pending = status(ShutdownPhase::WaitingForOwners);
            pending.pending_owners = self.owners.get();
            return Ok(pending);
        }
        self.callbacks.pop_front().unwrap_or_else(|| {
            let mut pending = status(ShutdownPhase::FlushingCookies);
            pending.pending_cookie_flushes = 1;
            Ok(pending)
        })
    }
}
struct Owner(Rc<Cell<usize>>);
impl Drop for Owner { fn drop(&mut self) { self.0.set(self.0.get() - 1); } }

#[test]
fn repeated_intents_join_owner_and_callback_gates_and_finalize_once() {
    let owners = Rc::new(Cell::new(2));
    let a = Owner(owners.clone());
    let b = Owner(owners.clone());
    let mut driver = FakeDriver { owners, begins: 0, callbacks: VecDeque::new() };
    let mut coordinator = Coordinator::default();
    assert!(coordinator.request());
    coordinator.started = true;
    driver.begin().unwrap();
    for _ in 0..4 { assert!(!coordinator.request()); assert!(!coordinator.observe(driver.poll())); }
    drop(a);
    assert!(!coordinator.observe(driver.poll()));
    drop(b);
    // No elapsed time or number of polls can stand in for the callback.
    for _ in 0..500 { assert!(!coordinator.observe(driver.poll())); }
    assert_eq!(coordinator.last_status.as_ref().unwrap().pending_cookie_flushes, 1);
    assert_eq!(coordinator.diagnostic, "Saving your browsing session…");
    driver.callbacks.push_back(Ok(status(ShutdownPhase::Complete)));
    assert!(coordinator.observe(driver.poll()));
    assert!(!coordinator.observe(Ok(status(ShutdownPhase::Complete))));
    assert!(!coordinator.request());
    assert_eq!(driver.begins, 1);
    assert!(!coordinator.finalized); // CEF alone cannot authorize native exit.
    assert!(coordinator.finalize()); // Caller has completed the Downloads barrier.
    assert!(!coordinator.finalize());
}

#[test]
fn failure_and_poll_error_remain_sticky_even_if_complete_arrives_later() {
    for result in [Err("Reentrant CEF call".into()), Ok(ShutdownStatus {
        failures: vec!["Cookie flush rejected".into()], ..status(ShutdownPhase::Failed)
    })] {
        let mut coordinator = Coordinator::default();
        coordinator.request();
        coordinator.started = true;
        assert!(!coordinator.observe(result));
        let diagnostic = coordinator.diagnostic.clone();
        for _ in 0..10 {
            assert!(!coordinator.observe(Ok(status(ShutdownPhase::Complete))));
            assert!(!coordinator.request());
            assert_eq!(coordinator.diagnostic, diagnostic);
        }
        assert!(!coordinator.finalized);
    }
}

#[test]
fn running_or_unrequested_status_cannot_authorize_quit() {
    let mut coordinator = Coordinator::default();
    assert!(!coordinator.observe(Ok(status(ShutdownPhase::Complete))));
    coordinator.request();
    coordinator.started = true;
    assert!(!coordinator.observe(Ok(status(ShutdownPhase::Running))));
    assert!(!coordinator.finalized);
}

// Restore thread-local producer state even if an assertion fails.
struct GateReset;
impl Drop for GateReset {
    fn drop(&mut self) {
        SEALED.with(|value| value.set(false));
        NATIVE_GATE.with(|gate| *gate.borrow_mut() = NativeQuitGate::default());
    }
}

#[gpui::test]
fn sealed_deferred_window_and_malformed_retry_cannot_create_a_normal_source(cx: &mut gpui::TestAppContext) {
    let _reset = GateReset;
    cx.update(|cx| {
        cx.defer(|cx| crate::new_window::open(false, cx));
        seal();
        let missing = std::env::temp_dir().join("echo-quit-must-not-load-session.json");
        assert!(crate::new_window::source_from_session(&missing, cx).is_err());
        assert!(crate::new_window::normal_source(cx).is_err());
        crate::blank_window::open(cx);
        crate::open_incognito_window(cx);
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(cx.windows().is_empty());
        assert!(!cx.has_global::<crate::new_window::NormalProfile>());
    });
}


#[test]
fn native_intents_return_and_join_original_suspensions_until_complete_once() {
    let mut gate = NativeQuitGate::default();
    assert_eq!(gate.capture(), Ok(true));
    gate.record_context(true, Some(41));
    assert_eq!(gate.take_request(), Ok(true));
    assert_eq!(gate.take_request(), Ok(false));
    assert_eq!(gate.capture(), Ok(false));
    gate.record_context(false, Some(73));
    gate.record_context(false, Some(41)); // no second resume for the same token
    gate.record_context(false, Some(0)); // nil is never a valid suspended event
    assert_eq!(gate.take_request(), Ok(true));
    for phase in [ShutdownPhase::Running, ShutdownPhase::WaitingForOwners,
        ShutdownPhase::ClosingBrowsers, ShutdownPhase::FlushingCookies, ShutdownPhase::ShuttingDown,
        ShutdownPhase::Failed] {
        assert_eq!(gate.claim_original(&status(phase)), Ok(None));
        assert!(!gate.is_finalizing());
    }
    let mut dishonest = status(ShutdownPhase::Complete);
    dishonest.pending_cookie_flushes = 1;
    assert_eq!(gate.claim_original(&dishonest), Ok(None));
    assert_eq!(gate.claim_original(&status(ShutdownPhase::Complete)), Ok(Some(NativeReplayPlan { first_suspension: Some(41), joined_suspensions: vec![73] })));
    assert!(gate.is_finalizing());
    assert!(gate.capture().is_err()); // native call reentry never replays original again
    assert_eq!(gate.claim_original(&status(ShutdownPhase::Complete)), Ok(None));
}

#[test]
fn native_replay_restores_original_context_before_call_and_never_replies_early() {
    #[derive(Default)]
    struct Host { calls: Vec<String> }
    impl NativeTerminationHost for Host {
        fn restore(&mut self, id: usize) { self.calls.push(format!("restore {id}")); }
        fn original_terminate(&mut self) -> NativePrimaryOutcome {
            assert_eq!(self.calls, ["restore 41"]);
            self.calls.push("original returned unexpectedly".into());
            NativePrimaryOutcome::Returned
        }
        fn resume_failed(&mut self, id: usize) { self.calls.push(format!("failure reply {id}")); }
    }
    let mut host = Host::default();
    let error = replay_native_termination(&NativeReplayPlan { first_suspension: Some(41), joined_suspensions: vec![73] }, &mut host);
    assert!(error.unwrap_err().contains("native quit is paused"));
    assert_eq!(host.calls, ["restore 41", "original returned unexpectedly", "failure reply 73", "failure reply 41"]);
    // Reverse release restores the primary's previous current-event context last.
}

#[test]
fn native_failure_prevents_original_call_even_after_later_engine_complete() {
    let mut gate = NativeQuitGate::default();
    gate.capture().unwrap();
    gate.record_context(true, Some(99));
    gate.fail("Cannot preserve the original OS event".into());
    assert!(gate.take_request().is_err());
    assert!(gate.claim_original(&status(ShutdownPhase::Complete)).is_err());
    assert!(!gate.is_finalizing());
    assert_eq!(gate.first_suspension, Some(Some(99)));
    assert!(gate.joined_suspensions.is_empty());
}

#[test]
fn menu_quit_without_apple_event_also_claims_original_once_and_no_fake_os_context() {
    let mut gate = NativeQuitGate::default();
    assert_eq!(gate.claim_original(&status(ShutdownPhase::Complete)), Ok(Some(NativeReplayPlan { first_suspension: None, joined_suspensions: vec![] })));
    assert_eq!(gate.claim_original(&status(ShutdownPhase::Complete)), Ok(None));
}


#[test]
fn native_sticky_failure_rejects_later_event_before_suspension_is_created() {
    let mut gate = NativeQuitGate::default();
    gate.capture().unwrap();
    gate.record_context(true, Some(41));
    gate.fail("Native finalization failed".into());
    let mut created_suspensions = Vec::new();
    let mut failure_replies = 0;
    for _ in 0..3 {
        match gate.capture() {
            Ok(first) => { created_suspensions.push(73); gate.record_context(first, Some(73)); }
            Err(_) => failure_replies += 1,
        }
    }
    assert!(created_suspensions.is_empty());
    assert_eq!(failure_replies, 3);
    assert_eq!(gate.first_suspension, Some(Some(41)));
    assert!(gate.joined_suspensions.is_empty());
}

#[test]
fn first_sender_without_event_never_replays_a_later_intents_event_context() {
    let mut gate = NativeQuitGate::default();
    assert_eq!(gate.capture(), Ok(true)); // sender A, no AppleEvent
    gate.record_context(true, None);
    assert_eq!(gate.capture(), Ok(false)); // sender B, AppleEvent 73
    gate.record_context(false, Some(73));
    let plan = gate.claim_original(&status(ShutdownPhase::Complete)).unwrap().unwrap();
    assert_eq!(plan.first_suspension, None);
    assert_eq!(plan.joined_suspensions, [73]);
    struct SenderA { calls: Vec<&'static str> }
    impl NativeTerminationHost for SenderA {
        fn restore(&mut self, _: usize) { panic!("sender A must retain its absent event context"); }
        fn original_terminate(&mut self) -> NativePrimaryOutcome {
            self.calls.push("original sender A without event");
            NativePrimaryOutcome::Returned
        }
        fn resume_failed(&mut self, id: usize) { assert_eq!(id, 73); self.calls.push("joined B failure reply"); }
    }
    let mut sender = SenderA { calls: vec![] };
    assert!(replay_native_termination(&plan, &mut sender).is_err());
    assert_eq!(sender.calls, ["original sender A without event", "joined B failure reply"]);
}


#[test]
fn successful_original_apple_event_reply_allows_exactly_one_later_native_terminate() {
    let mut gate = NativeQuitGate::default();
    gate.capture().unwrap();
    gate.record_context(true, Some(41));
    gate.claim_original(&status(ShutdownPhase::Complete)).unwrap().unwrap();
    gate.begin_event_dispatch();
    let mut live_reply = 0i32;
    let result = dispatch_original_quit_event(|| 0, |returned| {
        live_reply = native_event_result_code(Some(live_reply), returned);
        Ok(i16::try_from(live_reply).unwrap())
    }).unwrap();
    gate.finish_event_dispatch(result);
    assert_eq!(result, 0);
    assert_eq!(live_reply, 0);
    assert!(gate.failure.is_none());
    assert!(gate.is_finalizing());
    // AppKit's observed deferred terminate arrives AFTER the handler returns.
    assert!(gate.take_native_continuation());
    assert!(!gate.take_native_continuation());
    gate.begin_event_dispatch(); // cannot rearm a consumed continuation
    assert!(!gate.take_native_continuation());
}

#[test]
fn original_apple_event_nonzero_error_is_preserved_before_manager_sends_reply() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut live_error = 0i32;
    let dispatched = dispatch_original_quit_event(|| {
        calls.borrow_mut().push("original UPP returned error");
        -1708
    }, |returned| {
        live_error = native_event_result_code(Some(live_error), returned);
        calls.borrow_mut().push("live reply preserves error");
        Ok(i16::try_from(live_error).unwrap())
    }).unwrap();
    calls.borrow_mut().push("manager sends reply");
    assert_eq!(dispatched, -1708);
    assert_eq!(live_error, -1708);
    assert_eq!(*calls.borrow(), ["original UPP returned error", "live reply preserves error", "manager sends reply"]);
    assert_eq!(native_event_result_code(Some(-128), 0), -128);
    assert_eq!(native_event_result_code(None, -1708), -1708);
    assert_eq!(native_event_result_code(Some(0), 0), 0);
    let mut gate = NativeQuitGate::default();
    gate.claim_original(&status(ShutdownPhase::Complete)).unwrap();
    gate.begin_event_dispatch();
    gate.finish_event_dispatch(dispatched);
    assert!(gate.failure.is_some());
    assert!(!gate.take_native_continuation());
}

#[test]
fn accepted_apple_event_keeps_joined_requests_pending_without_failure_cleanup() {
    struct Accepted { failures: usize }
    impl NativeTerminationHost for Accepted {
        fn restore(&mut self, _: usize) {}
        fn original_terminate(&mut self) -> NativePrimaryOutcome { NativePrimaryOutcome::AppleEventAccepted }
        fn resume_failed(&mut self, _: usize) { self.failures += 1; }
    }
    let mut host = Accepted { failures: 0 };
    let plan = NativeReplayPlan { first_suspension: Some(41), joined_suspensions: vec![73] };
    assert_eq!(replay_native_termination(&plan, &mut host), Ok(()));
    assert_eq!(host.failures, 0);
}

#[cfg(target_os = "macos")]
#[path = "native/termination.rs"]
mod native_ffi_contract;


#[gpui::test]
fn production_menu_quit_request_records_absent_context_before_later_apple_event(cx: &mut gpui::TestAppContext) {
    let _reset = GateReset;
    cx.update(reset_for_test);
    // This is the same production seam used by the menus/Cmd+Q action.
    cx.update(request);
    NATIVE_GATE.with(|gate| {
        let mut gate = gate.borrow_mut();
        assert_eq!(gate.first_suspension, Some(None));
        assert_eq!(gate.capture(), Ok(false));
        gate.record_context(false, Some(73));
    });
    cx.update(request); // subsequent delivery of the captured OS intent
    NATIVE_GATE.with(|gate| {
        let plan = gate.borrow_mut().claim_original(&status(ShutdownPhase::Complete)).unwrap().unwrap();
        assert_eq!(plan.first_suspension, None);
        assert_eq!(plan.joined_suspensions, [73]);
    });
}

#[gpui::test]
fn production_quit_request_never_overwrites_a_native_first_intent(cx: &mut gpui::TestAppContext) {
    let _reset = GateReset;
    cx.update(reset_for_test);
    NATIVE_GATE.with(|gate| {
        let mut gate = gate.borrow_mut();
        assert_eq!(gate.capture(), Ok(true));
        gate.record_context(true, Some(41));
    });
    cx.update(request);
    NATIVE_GATE.with(|gate| assert_eq!(gate.borrow().first_suspension, Some(Some(41))));
}

#[test]
fn primary_resume_rejection_remains_tracked_and_cleanup_never_resumes_it_again() {
    struct Rejected { primary_resume_calls: usize, primary_tracked: bool, original_dispatches: usize }
    impl NativeTerminationHost for Rejected {
        fn restore(&mut self, id: usize) { assert_eq!(id, 41); }
        fn original_terminate(&mut self) -> NativePrimaryOutcome {
            self.primary_resume_calls += 1;
            // AEResume rejected before invoking our custom dispatcher. Preserve
            // its event/reply copies in the registry exactly as production does.
            self.primary_tracked = true;
            NativePrimaryOutcome::ResumeRejected
        }
        fn resume_failed(&mut self, id: usize) {
            assert_eq!(id, 41);
            self.primary_resume_calls += 1;
            self.primary_tracked = false;
        }
    }
    let mut host = Rejected { primary_resume_calls: 0, primary_tracked: false, original_dispatches: 0 };
    let error = replay_native_termination(&NativeReplayPlan { first_suspension: Some(41), joined_suspensions: vec![] }, &mut host);
    assert!(error.unwrap_err().contains("paused"));
    assert_eq!(host.primary_resume_calls, 1);
    assert_eq!(host.original_dispatches, 0);
    assert!(host.primary_tracked);
}


#[test]
fn download_lease_waits_without_poll_and_ingestion_precedes_each_poll() {
    struct Driver<'a>(&'a RefCell<Vec<&'static str>>);
    impl ShutdownDriver for Driver<'_> {
        fn begin(&mut self) -> Result<ShutdownStatus, String> { panic!("already begun") }
        fn poll(&mut self) -> Result<ShutdownStatus, String> {
            self.0.borrow_mut().push("poll");
            Ok(status(ShutdownPhase::ClosingBrowsers))
        }
    }
    let log = RefCell::new(Vec::new());
    let mut driver = Driver(&log);
    for _ in 0..3 {
        assert!(poll_after_downloads(&mut driver, || Err(ShutdownDownloadError::InFlight)).unwrap().is_none());
    }
    assert!(log.borrow().is_empty());
    for _ in 0..2 {
        assert!(poll_after_downloads(&mut driver, || {
            log.borrow_mut().push("ingest/persist");
            Ok(())
        }).unwrap().is_some());
    }
    assert_eq!(*log.borrow(), ["ingest/persist", "poll", "ingest/persist", "poll"]);
    assert!(poll_after_downloads(&mut driver, || Err(ShutdownDownloadError::Failure("writer failed".into()))).is_err());
    assert_eq!(log.borrow().len(), 4); // A real error cannot advance CEF either.
}

#[test]
fn final_download_barrier_remembers_each_success_across_legitimate_lease_waits() {
    for busy_step in 0..3 {
        let mut barrier = FinalDownloadBarrier::default();
        let calls = RefCell::new(Vec::new());
        let step = |name, index| {
            calls.borrow_mut().push(name);
            if index == busy_step { Err(ShutdownDownloadError::InFlight) } else { Ok(()) }
        };
        assert_eq!(barrier.advance(|| step("drain", 0), || step("flush", 1), || step("finish", 2)), Ok(false));
        assert_eq!(calls.borrow().len(), busy_step + 1);
        calls.borrow_mut().clear();
        assert_eq!(barrier.advance(
            || { calls.borrow_mut().push("drain"); Ok(()) },
            || { calls.borrow_mut().push("flush"); Ok(()) },
            || { calls.borrow_mut().push("finish"); Ok(()) },
        ), Ok(true));
        assert_eq!(*calls.borrow(), ["drain", "flush", "finish"][busy_step..]);
        assert_eq!(barrier.advance(|| panic!("drain replay"), || panic!("flush replay"), || panic!("finish replay")), Ok(true));
    }
}

#[test]
fn final_download_errors_are_sticky_and_never_authorize_original_native_exit() {
    for failed_step in 0..3 {
        let mut coordinator = Coordinator::default();
        coordinator.request();
        coordinator.started = true;
        assert!(coordinator.observe(Ok(status(ShutdownPhase::Complete))));
        assert!(!coordinator.finalized);
        let mut barrier = FinalDownloadBarrier::default();
        let calls = RefCell::new(Vec::new());
        let step = |index| {
            calls.borrow_mut().push(index);
            if index == failed_step { Err(ShutdownDownloadError::Failure("disk/drain/order failure".into())) } else { Ok(()) }
        };
        let error = barrier.advance(|| step(0), || step(1), || step(2)).unwrap_err();
        coordinator.fail(error.clone());
        assert_eq!(calls.borrow().len(), failed_step + 1);
        assert!(!coordinator.finalize());
        assert!(!coordinator.observe(Ok(status(ShutdownPhase::Complete))));
        coordinator.fail("later error must not replace first".into());
        assert_eq!(coordinator.failure, Some(error));
        assert!(!coordinator.finalized);
    }
    assert!(download_ready(Err(ShutdownDownloadError::Inactive)).is_err());
    assert!(!Coordinator::default().finalize());
}

#[gpui::test]
fn quit_only_hands_off_a_preinstalled_service_and_never_constructs_or_wakes_one(cx: &mut gpui::TestAppContext) {
    let _reset = GateReset;
    cx.update(|cx| {
        seal();
        for _ in 0..2 {
            assert!(take_installed_downloads(cx).unwrap().is_none());
            assert!(!cx.has_global::<AppDownloads>());
        }
        let app = AppDownloads::default();
        cx.set_global(app.clone()); // Test setup only, never production quit.
        let shutdown = take_installed_downloads(cx).unwrap().unwrap();
        assert!(take_installed_downloads(cx).is_err()); // One non-Clone consumer.
        assert!(app.begin_shutdown().is_err());
        assert!(shutdown.retained_normal_handles().unwrap().is_empty());
        let mut barrier = FinalDownloadBarrier::default();
        assert!(barrier.advance(
            || shutdown.drain_and_persist_with(|| Ok(Vec::new())),
            || shutdown.flush_retained(cx),
            || shutdown.finish(),
        ).unwrap());
        assert!(shutdown.ingest_and_persist(Vec::new()).is_err());
    });
}

#[gpui::test]
fn final_quit_barrier_flushes_retained_canonical_profile_after_sources_are_gone(cx: &mut gpui::TestAppContext) {
    use crate::profile::ProfileHandle;
    use browser_core::{Browser, downloads::{DownloadCompletion, DownloadTime}, session};
    let _reset = GateReset;
    let path = std::env::temp_dir().join(format!("echo-quit-final-download-{}-{}.json",
        std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let (shutdown, profile, named) = cx.update(|cx| {
        let mut browser = Browser::new();
        let profile = browser.default_user_profile_id();
        let named = browser.create_user_profile("Named").unwrap();
        let normal = ProfileHandle::new(browser, cx);
        normal.save_session_to(path.clone(), cx);
        let blank = ProfileHandle::new_with_shared_history(normal.blank_browser(profile), &normal, cx);
        let app = AppDownloads::default();
        for (handle, id) in [(&normal, profile), (&normal, named), (&blank, profile)] {
            app.service.borrow_mut().register_source(handle, id).unwrap();
        }
        cx.set_global(app);
        seal();
        let shutdown = take_installed_downloads(cx).unwrap().unwrap();
        drop(blank);
        drop(normal);
        (shutdown, profile, named)
    });
    // These canonical model writes stand in for the real copied-event writer.
    // Actual terminal ingestion/replay is covered by the generic service test;
    // here we exercise GPUI's actual retained ProfileHandle and session writer.
    let write = |name: &str, id| {
        let writers = shutdown.retained_normal_handles().unwrap();
        assert_eq!(writers.len(), 1);
        writers[0].record_completed_download(id, DownloadCompletion {
            final_path: std::env::temp_dir().join(name),
            source_url: "https://fixture.test/completed".into(),
            original_url: String::new(), mime_type: String::new(), finished_at: DownloadTime::Unknown,
        }).unwrap();
    };
    write("before-final.bin", profile);
    shutdown.drain_and_persist_with(|| Ok(Vec::new())).unwrap();
    cx.update(|cx| shutdown.flush_retained(cx).unwrap());
    write("at-cef-complete.bin", named); // No source window/global NormalProfile.
    let mut barrier = FinalDownloadBarrier::default();
    assert!(barrier.advance(
        || shutdown.drain_and_persist_with(|| Ok(Vec::new())),
        || cx.update(|cx| shutdown.flush_retained(cx)),
        || shutdown.finish(),
    ).unwrap());
    let restored = session::load(&path).unwrap().unwrap();
    assert_eq!(restored.completed_downloads(profile).unwrap().len(), 1);
    assert_eq!(restored.completed_downloads(named).unwrap().len(), 1);
    assert!(shutdown.retained_normal_handles().is_err()); // finish closed capability.
    std::fs::remove_file(path).unwrap();
}
