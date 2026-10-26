//! Pre-quit runs before GPUI's final, time-limited shutdown callback.
use std::{cell::{Cell, RefCell}, rc::Rc, time::Duration};
use gpui::{prelude::*, App, Global};
use engine::{Engine, ShutdownPhase, ShutdownStatus};
use crate::downloads::{AppDownloads, ShutdownDownloadError, ShutdownDownloads};

thread_local! { static SEALED: Cell<bool> = const { Cell::new(false) }; }
#[cfg(any(target_os = "macos", test))]
thread_local! {
    pub(crate) static NATIVE_GATE: RefCell<NativeQuitGate> = RefCell::new(NativeQuitGate::default());
}

/// Collect every lookup before any canonical writer is flushed. A Little
/// window can outlive its normal source window and the NormalProfile global.
pub(crate) fn prepare_models_for_quit(cx: &mut App) -> Result<(), String> {
    let windows: Vec<_> = cx.windows().into_iter()
        .filter_map(|handle| handle.downcast::<crate::window::BrowserWindow>()).collect();
    let mut retained = Vec::<crate::profile::ProfileHandle>::new();
    for handle in &windows {
        let writer = handle.update(cx, |view, _, _| {
            if view.peek_owner.is_some() { return None; }
            view.retain_little_archive();
            view.transient.as_ref().map(|state| state.source.profile.clone())
        }).map_err(|error| error.to_string())?;
        if let Some(writer) = writer {
            if !retained.iter().any(|entry| entry.same_workspace(&writer)) {
                retained.push(writer);
            }
        }
    }
    for handle in windows {
        handle.update(cx, |view, _, cx| view.prepare_quit(cx))
            .map_err(|error| error.to_string())??;
    }
    if let Some(normal) = cx.try_global::<crate::new_window::NormalProfile>() {
        let writer = normal.profile.clone();
        if !retained.iter().any(|entry| entry.same_workspace(&writer)) {
            retained.push(writer);
        }
    }
    for writer in retained { writer.flush_for_quit(cx)?; }
    Ok(())
}

/// This gate is set synchronously after quit is accepted (or a native failure).
/// Deferred producers must check it again when they actually execute.
pub(crate) fn is_quitting() -> bool { SEALED.with(Cell::get) }
pub(crate) fn seal() { SEALED.with(|value| value.set(true)); }

#[derive(Default)]
pub(crate) struct Coordinator {
    requested: bool,
    started: bool,
    engine_complete: bool,
    finalized: bool,
    failure: Option<String>,
    failure_presented: bool,
    diagnostic: String,
    last_status: Option<ShutdownStatus>,
}
impl Coordinator {
    pub(crate) fn request(&mut self) -> bool {
        if self.requested { return false; }
        self.requested = true;
        eprintln!("[quit] Requested; saving sessions before engine teardown request_unix_us={}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_micros());
        self.diagnostic = "Saving your browsing session…".into();
        true
    }
    pub(crate) fn start(&mut self) -> bool {
        if !self.requested || self.started || self.failure.is_some() { return false; }
        self.started = true;
        true
    }
    fn fail(&mut self, error: String) {
        if self.failure.is_none() { eprintln!("Termination deferred: {error}"); self.failure = Some(error); }
        self.diagnostic = "Quit is paused. Echo could not finish saving or closing browsing data. Keep Echo open; details are in the diagnostic log.".into();
    }
    fn observe(&mut self, result: Result<ShutdownStatus, String>) -> bool {
        if !self.requested || !self.started || self.failure.is_some() || self.engine_complete || self.finalized { return false; }
        match result {
            Err(error) => self.fail(error),
            Ok(status) => {
                if self.last_status.as_ref() != Some(&status) {
                    eprintln!("[quit] CEF {:?}; owners={}; browsers={:?}; cookie_flushes={}; failures={:?}",
                        status.phase, status.pending_owners, status.pending_browsers, status.pending_cookie_flushes, status.failures);
                    self.last_status = Some(status.clone());
                }
                self.diagnostic = match status.phase {
                    ShutdownPhase::FlushingCookies => "Saving your browsing session…",
                    _ => "Closing Echo…",
                }.into();
                if status.phase == ShutdownPhase::Failed || !status.failures.is_empty() {
                    self.fail(format!("{status:?}"));
                } else if status.phase == ShutdownPhase::Complete {
                    self.engine_complete = true;
                    return true;
                }
            }
        }
        false
    }
    fn finalize(&mut self) -> bool {
        if !self.engine_complete || self.failure.is_some() || self.finalized { return false; }
        self.finalized = true;
        true
    }
}

/// Only inspect an existing global. Quit never installs, admits or wakes Downloads.
fn take_installed_downloads(cx: &App) -> Result<Option<ShutdownDownloads>, String> {
    cx.try_global::<AppDownloads>().map(AppDownloads::begin_shutdown).transpose()
}

/// An older normal batch owns its lease through ingestion/effects, even if quit
/// was requested from its native call. Yield an app turn; never spin or bypass it.
pub(crate) fn download_ready(result: Result<(), ShutdownDownloadError>) -> Result<bool, String> {
    match result {
        Ok(()) => Ok(true),
        Err(ShutdownDownloadError::InFlight) => Ok(false),
        Err(ShutdownDownloadError::Inactive) => Err("Quit download consumer is inactive".into()),
        Err(ShutdownDownloadError::Failure(error)) => Err(error),
    }
}

#[derive(Default)]
pub(crate) struct FinalDownloadBarrier {
    drained: bool,
    flushed: bool,
    finished: bool,
}
impl FinalDownloadBarrier {
    /// Called only after CEF Complete. Remember successful steps when a later
    /// lease is busy: repeating the final drain would dirty the flush barrier.
    pub(crate) fn advance(
        &mut self,
        drain: impl FnOnce() -> Result<(), ShutdownDownloadError>,
        flush: impl FnOnce() -> Result<(), ShutdownDownloadError>,
        finish: impl FnOnce() -> Result<(), ShutdownDownloadError>,
    ) -> Result<bool, String> {
        if !self.drained {
            if !download_ready(drain())? { return Ok(false); }
            self.drained = true;
        }
        if !self.flushed {
            if !download_ready(flush())? { return Ok(false); }
            self.flushed = true;
        }
        if !self.finished {
            if !download_ready(finish())? { return Ok(false); }
            self.finished = true;
        }
        Ok(true)
    }
}
/// Pure native-intent bookkeeping; no AppKit call or user state borrow lives here.
/// Suspensions stay owned until native finalization, including joined OS requests.
#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
pub(crate) struct NativeQuitGate {
    requested: bool,
    pending: bool,
    original_called: bool,
    complete: bool,
    native_continuation_available: bool,
    native_continuation_claimed: bool,
    first_suspension: Option<Option<usize>>,
    joined_suspensions: Vec<usize>,
    failure: Option<String>,
}
#[cfg(any(target_os = "macos", test))]
impl NativeQuitGate {
    pub(crate) fn note_app_request(&mut self) {
        // Cmd+Q/menu/automation is itself the first intent, even though it has
        // no native sender/event. Never promote a later joined OS event.
        if !self.requested && !self.original_called && self.failure.is_none() {
            self.requested = true;
            self.first_suspension = Some(None);
        }
    }
    pub(crate) fn capture(&mut self) -> Result<bool, String> {
        if let Some(error) = &self.failure { return Err(error.clone()); }
        if self.original_called { return Err("Original native termination was already invoked".into()); }
        let first = !self.requested;
        if !first {
            if let Err(error) = self.joined_suspensions.try_reserve(1) {
                let error = format!("Cannot track joined native quit: {error}");
                self.fail(error.clone());
                return Err(error);
            }
        }
        self.requested = true;
        self.pending = true;
        Ok(first)
    }
    pub(crate) fn record_context(&mut self, first: bool, id: Option<usize>) {
        let id = id.filter(|id| *id != 0);
        if first {
            if self.first_suspension.is_none() { self.first_suspension = Some(id); }
        } else if let Some(id) = id {
            if self.first_suspension.flatten() != Some(id) && !self.joined_suspensions.contains(&id) {
                self.joined_suspensions.push(id);
            }
        }
    }
    pub(crate) fn take_request(&mut self) -> Result<bool, String> {
        if let Some(error) = &self.failure { return Err(error.clone()); }
        Ok(std::mem::take(&mut self.pending))
    }
    /// A user's refusal precedes the irreversible shutdown barrier. Return
    /// only that intent's suspended events, then allow a fresh quit intent.
    pub(crate) fn cancel_intent(&mut self) -> Result<Vec<usize>, String> {
        if let Some(error) = &self.failure { return Err(error.clone()); }
        if self.original_called || self.complete {
            return Err("Cannot cancel accepted native termination".into());
        }
        let mut ids = Vec::new();
        if let Some(id) = self.first_suspension.flatten() { ids.push(id); }
        ids.extend(self.joined_suspensions.iter().copied());
        *self = Self::default();
        Ok(ids)
    }
    pub(crate) fn fail(&mut self, error: String) {
        self.native_continuation_available = false;
        if self.failure.is_none() { self.failure = Some(error); }
    }
    pub(crate) fn begin_event_dispatch(&mut self) {
        if self.is_finalizing() && !self.native_continuation_claimed {
            self.native_continuation_available = true;
        }
    }
    pub(crate) fn finish_event_dispatch(&mut self, result: i16) {
        // AppKit's original handler may return noErr and enqueue terminate:.
        // Keep its one continuation armed; zero itself is not a failure.
        if result != 0 { self.fail(format!("Original quit AppleEvent failed: {result}")); }
    }
    pub(crate) fn take_native_continuation(&mut self) -> bool {
        if !self.is_finalizing() || !self.native_continuation_available || self.native_continuation_claimed { return false; }
        self.native_continuation_available = false;
        self.native_continuation_claimed = true;
        true
    }
    pub(crate) fn is_finalizing(&self) -> bool { self.complete && self.original_called && self.failure.is_none() }
    pub(crate) fn claim_original(&mut self, status: &ShutdownStatus) -> Result<Option<NativeReplayPlan>, String> {
        if let Some(error) = &self.failure { return Err(error.clone()); }
        if status.phase != ShutdownPhase::Complete || status.pending_owners != 0
            || !status.pending_browsers.is_empty() || status.pending_cookie_flushes != 0 || !status.failures.is_empty() {
            return Ok(None);
        }
        if self.original_called { return Ok(None); }
        self.complete = true;
        self.original_called = true;
        Ok(Some(NativeReplayPlan { first_suspension: self.first_suspension.flatten(), joined_suspensions: self.joined_suspensions.clone() }))
    }
    pub(crate) fn released(&mut self, id: usize) {
        if self.first_suspension == Some(Some(id)) { self.first_suspension = Some(None); }
        self.joined_suspensions.retain(|pending| *pending != id);
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NativeReplayPlan {
    pub(crate) first_suspension: Option<usize>,
    pub(crate) joined_suspensions: Vec<usize>,
}

/// Restoring must precede the original call; resuming beforehand would send an
/// early OS reply and undo current-event context. The original normally exits.
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativePrimaryOutcome {
    Returned,
    ResumeRejected,
    AppleEventAccepted,
}
#[cfg(any(target_os = "macos", test))]
pub(crate) trait NativeTerminationHost {
    fn restore(&mut self, suspension: usize);
    fn original_terminate(&mut self) -> NativePrimaryOutcome;
    fn resume_failed(&mut self, suspension: usize);
}
#[cfg(any(target_os = "macos", test))]
pub(crate) fn replay_native_termination(plan: &NativeReplayPlan, host: &mut impl NativeTerminationHost) -> Result<(), String> {
    if let Some(primary) = plan.first_suspension { host.restore(primary); }
    let outcome = host.original_terminate();
    if outcome == NativePrimaryOutcome::AppleEventAccepted {
        // The original event completed after the engine barrier, but AppKit's
        // own terminate continuation may still be queued. Keep joined events
        // pending for native exit; do not synthesize failures or a new quit.
        return Ok(());
    }
    // Reaching here is not successful termination. Reply with failure to every
    // joined OS request, restoring the primary context last, and never retry.
    for id in plan.joined_suspensions.iter().rev() { host.resume_failed(*id); }
    if outcome == NativePrimaryOutcome::Returned {
        if let Some(primary) = plan.first_suspension { host.resume_failed(primary); }
    }
    // A rejected primary resume remains tracked. Do not try a no-dispatch
    // resume in cleanup: that would be a second native operation on it.
    Err("AppKit termination returned after engine shutdown; native quit is paused".into())
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn native_event_failure_code(existing: Option<i32>, returned: i16) -> i32 {
    existing.filter(|value| *value != 0).unwrap_or(if returned != 0 { i32::from(returned) } else { -10000 })
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn native_event_result_code(existing: Option<i32>, returned: i16) -> i32 {
    existing.filter(|value| *value != 0).unwrap_or(i32::from(returned))
}

/// AEResume can transmit the reply immediately on dispatcher return. Preserve
/// actual nonzero errors inside the callback; a native noErr stays successful.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn dispatch_original_quit_event(
    original: impl FnOnce() -> i16,
    finish_live_reply: impl FnOnce(i16) -> Result<i16, String>,
) -> Result<i16, String> {
    let returned = original();
    finish_live_reply(returned)
}

pub(crate) trait ShutdownDriver {
    fn begin(&mut self) -> Result<ShutdownStatus, String>;
    fn poll(&mut self) -> Result<ShutdownStatus, String>;
}
/// The same ordering seam is exercised without a live CEF runtime in tests.
fn poll_after_downloads(
    driver: &mut impl ShutdownDriver,
    drain: impl FnOnce() -> Result<(), ShutdownDownloadError>,
) -> Result<Option<ShutdownStatus>, String> {
    if !download_ready(drain())? { return Ok(None); }
    driver.poll().map(Some)
}
/// Shared production preparation gate. Every callback/native call runs with no
/// coordinator borrow. Failure is sticky before any owner detach or engine begin.
pub(crate) fn begin_after_downloads<O>(
    state: &Rc<RefCell<Coordinator>>,
    driver: &mut impl ShutdownDriver,
    drain: impl FnOnce() -> Result<(), ShutdownDownloadError>,
    prepare: impl FnOnce() -> Result<O, String>,
) -> bool {
    if state.borrow().failure.is_some() { return false; }
    let result = (|| {
        if !download_ready(drain())? { return Ok(false); }
        let owners = prepare()?;
        let begun = driver.begin();
        drop(owners); // Native polling owns the queued close work.
        begun?;
        Ok(true)
    })();
    match result {
        Ok(ready) => ready,
        Err(error) => { state.borrow_mut().fail(error); false }
    }
}

/// Production poll/final-drain/flush/finish/exit authorization, shared with the
/// injected driver tests. Complete is observed only from the actual driver seam.
pub(crate) fn advance_shutdown(
    state: &Rc<RefCell<Coordinator>>,
    driver: &mut impl ShutdownDriver,
    barrier: &mut FinalDownloadBarrier,
    drain: impl Fn() -> Result<(), ShutdownDownloadError>,
    flush: impl FnOnce() -> Result<(), ShutdownDownloadError>,
    finish: impl FnOnce() -> Result<(), ShutdownDownloadError>,
    original_exit: impl FnOnce() -> Result<(), String>,
) {
    {
        let state = state.borrow();
        if !state.started || state.failure.is_some() || state.finalized { return; }
    }
    if !state.borrow().engine_complete {
        match poll_after_downloads(driver, &drain) {
            Ok(None) => return,
            Err(error) => { state.borrow_mut().fail(error); return; }
            Ok(Some(status)) => { state.borrow_mut().observe(Ok(status)); }
        }
    }
    if !state.borrow().engine_complete || state.borrow().failure.is_some() { return; }
    match barrier.advance(drain, flush, finish) {
        Ok(true) => {
            let finalized = state.borrow_mut().finalize();
            if finalized {
                if let Err(error) = original_exit() { state.borrow_mut().fail(error); }
            }
        }
        Ok(false) => {}
        Err(error) => state.borrow_mut().fail(error),
    }
}

struct EngineDriver;
impl ShutdownDriver for EngineDriver {
    fn begin(&mut self) -> Result<ShutdownStatus, String> { Engine::begin_shutdown() }
    fn poll(&mut self) -> Result<ShutdownStatus, String> { Engine::poll_shutdown() }
}
struct QuitState(Rc<RefCell<Coordinator>>);
impl Global for QuitState {}

pub(crate) fn request(cx: &mut App) {
    crate::quit_confirmation::close_all(cx);
    seal();
    #[cfg(any(target_os = "macos", test))]
    NATIVE_GATE.with(|gate| gate.borrow_mut().note_app_request());
    let first = cx.try_global::<QuitState>().is_some_and(|state| state.0.borrow_mut().request());
    if first {
        let start = std::time::Instant::now();
        // Visibility does not destroy windows, pages or their storage owners.
        // GPUI's test platform has no hide implementation; native qualification
        // measures actual window disappearance independently via CoreGraphics.
        #[cfg(not(test))]
        cx.hide();
        eprintln!("[quit] presentation-hidden elapsed_us={}", start.elapsed().as_micros());
    }
}

pub(crate) fn last_window_close(cx: &mut App) -> bool {
    #[cfg(not(target_os = "macos"))]
    if cx.has_global::<QuitState>() && cx.windows().into_iter()
        .filter(|window| window.downcast::<crate::window::BrowserWindow>().is_some()).count() <= 1 {
        request(cx);
        return true;
    }
    let _ = cx;
    false
}

pub(crate) fn diagnostic(cx: &App) -> String {
    cx.try_global::<QuitState>().map(|state| state.0.borrow().diagnostic.clone())
        .unwrap_or_else(|| "Closing Echo…".into())
}

struct Progress;
impl gpui::Render for Progress {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        gpui::div().size_full().p_8().bg(gpui::rgb(0x202126)).text_color(gpui::rgb(0xf2f2f2))
            .child(diagnostic(cx))
    }
}

fn show_progress(cx: &mut App) {
    let Some(state) = cx.try_global::<QuitState>() else { return; };
    {
        let mut state = state.0.borrow_mut();
        if state.failure.is_none() || state.failure_presented { return; }
        state.failure_presented = true;
    }
    cx.activate(true);
    let _ = cx.open_window(Default::default(), |window, cx| {
        window.on_window_should_close(cx, |_, _| false);
        cx.new(|_| Progress)
    });
}

/// App-owned task: no window entity, page pool, or storage context is captured.
/// Every CEF entry is outside cx.update and outside the coordinator borrow.
pub(crate) fn init(cx: &mut App) {
    crate::quit_confirmation::init(cx);
    SEALED.with(|value| value.set(false));
    #[cfg(any(target_os = "macos", test))]
    NATIVE_GATE.with(|gate| *gate.borrow_mut() = NativeQuitGate::default());
    let state = Rc::new(RefCell::new(Coordinator::default()));
    cx.set_global(QuitState(state.clone()));
    #[cfg(all(target_os = "macos", not(test)))]
    if let Err(error) = crate::native::termination::install() { eprintln!("Native quit deferral unavailable: {error}"); }
    cx.spawn(async move |cx| {
        let mut driver = EngineDriver;
        // Retain the non-Clone capability independently of every source window.
        let mut downloads = None;
        let mut preparing = false;
        let mut extension_retirements: Option<Vec<Rc<Cell<usize>>>> = None;
        let mut final_downloads = FinalDownloadBarrier::default();
        loop {
            cx.background_executor().timer(Duration::from_millis(10)).await;
            #[cfg(all(target_os = "macos", not(test)))]
            match crate::native::termination::take_request() {
                Ok(true) => { let _ = cx.update(crate::quit_confirmation::request); }
                Ok(false) => {}
                Err(error) => {
                    let first_failure = state.borrow().failure.is_none();
                    state.borrow_mut().fail(error);
                    let _ = cx.update(|cx| {
                        request(cx);
                        if first_failure { show_progress(cx); }
                    });
                }
            }
            let prepare = {
                let mut state = state.borrow_mut();
                state.start()
            };
            if prepare {
                match cx.update(|cx| take_installed_downloads(cx)) {
                    Ok(Ok(capability)) => { downloads = capability; preparing = true; }
                    Ok(Err(error)) => state.borrow_mut().fail(error),
                    Err(error) => state.borrow_mut().fail(error.to_string()),
                }
                // Ordinary teardown stays hidden. A failed data barrier gets
                // one visible diagnostic rather than silently abandoning data.
                let _ = cx.update(show_progress);
            }
            if preparing && state.borrow().failure.is_none()
                && !cx.update(|cx| crate::password_flow::writes_pending(cx)).unwrap_or(true)
                && !cx.update(|cx| crate::max_downloads::file_moves_pending(cx)).unwrap_or(true) {
                // Popup native callbacks detach borrowed views synchronously.
                // Run their deferred closes before begin_shutdown seals CEF calls.
                if extension_retirements.is_none() {
                    match cx.update(|cx| cx.windows().into_iter()
                        .filter_map(|handle|handle.downcast::<crate::window::BrowserWindow>())
                        .map(|handle|handle.update(cx,|view,_,cx|view.retire_extensions_for_quit(cx))
                            .map_err(|error|error.to_string()))
                        .collect::<Result<Vec<_>, String>>()) {
                        Ok(Ok(counts)) => extension_retirements = Some(counts),
                        Ok(Err(error)) => state.borrow_mut().fail(error),
                        Err(error) => state.borrow_mut().fail(error.to_string()),
                    }
                    continue;
                }
                if extension_retirements.as_ref().is_some_and(|counts|counts.iter().any(|count|count.get()!=0)) {continue;}
                // Exactly one running drain under the SHUTDOWN consumer lease,
                // before detachment/begin_shutdown. No service means no drain:
                // the disposable probe remains the sole running consumer.
                let begun = begin_after_downloads(&state, &mut driver,
                    || downloads.as_ref().map_or(Ok(()), |downloads|
                        downloads.drain_and_persist_with(Engine::take_download_events)),
                    || {
                // Explicit Save As initiators are not Pages owners. Native leases
                // retain their hidden hosts through OnBeforeClose.
                Engine::release_file_download_initiators();
                // Save every model before removing any native page metadata.
                cx.update(|cx| -> Result<Vec<Box<dyn crate::pages::WebEngine>>, String> {
                    prepare_models_for_quit(cx)?;
                    // Extract owners before dropping the last layout references.
                    let owners = crate::pages::take_shutdown_engines()?;
                    for handle in cx.windows() {
                        if let Some(handle) = handle.downcast::<crate::window::BrowserWindow>() {
                            let _ = handle.update(cx, |view, _, cx| view.detach_for_quit(cx));
                        }
                    }
                    if cx.has_global::<crate::new_window::NormalProfile>() {
                        drop(cx.remove_global::<crate::new_window::NormalProfile>());
                    }
                    Ok(owners)
                })
                    .map_err(|error| error.to_string())?
                    });
                if !begun { continue; }
                preparing = false;
            }
            let poll = { let state = state.borrow(); state.started && !preparing && state.failure.is_none() && !state.finalized };
            if poll {
                let app = RefCell::new(&mut *cx);
                advance_shutdown(&state, &mut driver, &mut final_downloads,
                    || downloads.as_ref().map_or(Ok(()), |downloads|
                        downloads.drain_and_persist_with(Engine::take_shutdown_download_events)),
                    || match downloads.as_ref() {
                        None => Ok(()),
                        Some(downloads) => app.borrow_mut().update(|cx| downloads.flush_retained(cx))
                            .map_err(|error| ShutdownDownloadError::Failure(error.to_string()))?,
                    },
                    || downloads.as_ref().map_or(Ok(()), ShutdownDownloads::finish),
                    || {
                        app.borrow_mut().update(crate::updates::arm_after_shutdown).map_err(|error| error.to_string())??;
                        #[cfg(all(target_os = "macos", not(test)))]
                        {
                            let completed = state.borrow().last_status.clone().expect("completed engine status");
                            crate::native::termination::complete(&completed)
                        }
                        #[cfg(any(not(target_os = "macos"), test))]
                        { app.borrow_mut().update(|cx| cx.quit()).map_err(|error| error.to_string()) }
                    });
                drop(app);
                let _ = cx.update(|cx| cx.refresh_windows());
                #[cfg(any(not(target_os = "macos"), test))]
                if state.borrow().finalized && state.borrow().failure.is_none() { break; }
            } else if state.borrow().failure.is_some() {
                let _ = cx.update(show_progress);
            }
        }
    }).detach();
}

#[cfg(test)]
#[path = "quit_tests.rs"]
mod tests;

#[cfg(test)]
pub(crate) fn reset_for_test(cx: &mut App) {
    SEALED.with(|value| value.set(false));
    #[cfg(any(target_os = "macos", test))]
    NATIVE_GATE.with(|gate| *gate.borrow_mut() = NativeQuitGate::default());
    if let Some(state) = cx.try_global::<QuitState>() { *state.0.borrow_mut() = Coordinator::default(); }
}
