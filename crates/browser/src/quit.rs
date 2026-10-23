//! Pre-quit runs before GPUI's final, time-limited shutdown callback.
use std::{cell::{Cell, RefCell}, rc::Rc, time::Duration};
use gpui::{prelude::*, App, Global};
use engine::{Engine, ShutdownPhase, ShutdownStatus};

thread_local! { static SEALED: Cell<bool> = const { Cell::new(false) }; }
#[cfg(any(target_os = "macos", test))]
thread_local! {
    pub(crate) static NATIVE_GATE: RefCell<NativeQuitGate> = RefCell::new(NativeQuitGate::default());
}

/// This gate is set synchronously, including inside native termination callbacks.
/// Deferred producers must check it again when they actually execute.
pub(crate) fn is_quitting() -> bool { SEALED.with(Cell::get) }
pub(crate) fn seal() { SEALED.with(|value| value.set(true)); }

#[derive(Default)]
struct Coordinator {
    requested: bool,
    started: bool,
    finalized: bool,
    failure: Option<String>,
    diagnostic: String,
    last_status: Option<ShutdownStatus>,
}
impl Coordinator {
    fn request(&mut self) -> bool {
        if self.requested { return false; }
        self.requested = true;
        eprintln!("[quit] Requested; saving sessions before engine teardown");
        self.diagnostic = "Saving your browsing session…".into();
        true
    }
    fn fail(&mut self, error: String) {
        if self.failure.is_none() { eprintln!("Termination deferred: {error}"); self.failure = Some(error); }
        self.diagnostic = "Quit is paused. Echo could not finish saving or closing browsing data. Keep Echo open; details are in the diagnostic log.".into();
    }
    fn observe(&mut self, result: Result<ShutdownStatus, String>) -> bool {
        if !self.requested || !self.started || self.failure.is_some() || self.finalized { return false; }
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
                    self.finalized = true;
                    return true;
                }
            }
        }
        false
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

trait ShutdownDriver {
    fn begin(&mut self) -> Result<ShutdownStatus, String>;
    fn poll(&mut self) -> Result<ShutdownStatus, String>;
}
struct EngineDriver;
impl ShutdownDriver for EngineDriver {
    fn begin(&mut self) -> Result<ShutdownStatus, String> { Engine::begin_shutdown() }
    fn poll(&mut self) -> Result<ShutdownStatus, String> { Engine::poll_shutdown() }
}
struct QuitState(Rc<RefCell<Coordinator>>);
impl Global for QuitState {}

pub(crate) fn request(cx: &mut App) {
    seal();
    #[cfg(any(target_os = "macos", test))]
    NATIVE_GATE.with(|gate| gate.borrow_mut().note_app_request());
    let first = cx.try_global::<QuitState>().is_some_and(|state| state.0.borrow_mut().request());
    if first { cx.refresh_windows(); }
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
    let _ = cx.open_window(Default::default(), |window, cx| {
        window.on_window_should_close(cx, |_, _| false);
        cx.new(|_| Progress)
    });
}

/// App-owned task: no window entity, page pool, or storage context is captured.
/// Every CEF entry is outside cx.update and outside the coordinator borrow.
pub(crate) fn init(cx: &mut App) {
    SEALED.with(|value| value.set(false));
    #[cfg(any(target_os = "macos", test))]
    NATIVE_GATE.with(|gate| *gate.borrow_mut() = NativeQuitGate::default());
    let state = Rc::new(RefCell::new(Coordinator::default()));
    cx.set_global(QuitState(state.clone()));
    #[cfg(all(target_os = "macos", not(test)))]
    if let Err(error) = crate::native::termination::install() { eprintln!("Native quit deferral unavailable: {error}"); }
    cx.spawn(async move |cx| {
        let mut driver = EngineDriver;
        loop {
            cx.background_executor().timer(Duration::from_millis(10)).await;
            #[cfg(all(target_os = "macos", not(test)))]
            match crate::native::termination::take_request() {
                Ok(true) => { let _ = cx.update(request); }
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
                if state.requested && !state.started && state.failure.is_none() { state.started = true; true } else { false }
            };
            if prepare {
                // Save every model before removing any native page metadata.
                let prepared = cx.update(|cx| -> Result<Vec<Box<dyn crate::pages::WebEngine>>, String> {
                    for handle in cx.windows() {
                        if let Some(handle) = handle.downcast::<crate::window::BrowserWindow>() {
                            handle.update(cx, |view, _, cx| view.prepare_quit(cx)).map_err(|e| e.to_string())??;
                        }
                    }
                    if cx.has_global::<crate::new_window::NormalProfile>() {
                        let profile = cx.global::<crate::new_window::NormalProfile>().profile.clone();
                        profile.flush_for_quit(cx)?;
                    }
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
                });
                match prepared {
                    Ok(Ok(owners)) => {
                        let begun = driver.begin();
                        // Dropping pools queues native close; polling does the work.
                        drop(owners);
                        if let Err(error) = begun { state.borrow_mut().fail(error); }
                    }
                    Ok(Err(error)) => state.borrow_mut().fail(error),
                    Err(error) => state.borrow_mut().fail(error.to_string()),
                }
                let _ = cx.update(show_progress);
            }
            let poll = { let state = state.borrow(); state.started && state.failure.is_none() && !state.finalized };
            if poll {
                let result = driver.poll();
                let finish = state.borrow_mut().observe(result);
                let _ = cx.update(|cx| cx.refresh_windows());
                if finish {
                    // Original native terminate may invoke applicationWillTerminate
                    // synchronously. No GPUI/coordinator borrow may cross it.
                    #[cfg(all(target_os = "macos", not(test)))]
                    {
                        let completed = state.borrow().last_status.clone().expect("completed engine status");
                        if let Err(error) = crate::native::termination::complete(&completed) {
                            state.borrow_mut().fail(error);
                        }
                        // A successful AppleEvent can await its own queued native
                        // continuation. Never invent a second quit API/IMP call.
                        continue;
                    }
                    #[cfg(any(not(target_os = "macos"), test))]
                    {
                        let _ = cx.update(|cx| cx.quit());
                        break;
                    }
                }
            } else if state.borrow().failure.is_some() {
                let _ = cx.update(|cx| cx.refresh_windows());
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
