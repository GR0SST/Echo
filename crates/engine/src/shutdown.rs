//! Process-wide CEF shutdown, driven on the initialization thread without windows.
use crate::{Engine, RUNTIME, RequestContext, Runtime};
use cef::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::thread::ThreadId;

static INIT_THREAD: OnceLock<ThreadId> = OnceLock::new();
static CLOSING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownPhase {
    Running,
    WaitingForOwners,
    ClosingBrowsers,
    FlushingCookies,
    ShuttingDown,
    Complete,
    Failed,
}

/// A pending status has no deadline. Only Complete permits final application exit.
/// Cookie flush completion does not prove Preferences/localStorage persistence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShutdownStatus {
    pub phase: ShutdownPhase,
    /// Public engine pools and StorageContext handles still retaining Runtime.
    pub pending_owners: usize,
    /// Native browser IDs, including inspectors and removed/closing pages.
    pub pending_browsers: Vec<i32>,
    pub pending_cookie_flushes: usize,
    pub failures: Vec<String>,
}

pub(crate) type Queue = Arc<Mutex<Vec<Callback>>>;
#[derive(Debug)]
pub(crate) enum Callback {
    Created(i32),
    DestroyChild(i32, usize),
    Closed(i32),
    CookieReady(usize),
    Flushed(usize),
}
pub(crate) fn enqueue(queue: &Queue, callback: Callback) {
    queue.lock().unwrap().push(callback);
}

/// Remove only Closed and same-owner stale creation/destruction notifications.
/// Preserve unrelated callback order. No native release runs under this lock.
fn take_closed_callbacks(queue: &Queue) -> Vec<i32> {
    let mut queue = queue.lock().unwrap();
    let mut closed = Vec::new();
    let mut seen = HashSet::new();
    for callback in queue.iter() {
        if let Callback::Closed(id) = callback {
            if seen.insert(*id) { closed.push(*id); }
        }
    }
    queue.retain(|callback| match callback {
        Callback::Closed(_) => false,
        Callback::Created(id) | Callback::DestroyChild(id, _) => !seen.contains(id),
        _ => true,
    });
    closed
}

pub(crate) struct BrowserLease {
    pub browser: Browser,
    // The exact Rc registry lease, also for private contexts and inspectors.
    pub _context: Rc<RequestContext>,
    pub host: *mut std::ffi::c_void,
    pub retired_host: Option<Rc<crate::host::RetiredHost>>,
}
impl Drop for BrowserLease {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        crate::mac::release_clip_view(self.host);
    }
}

// Generic ownership ledger: callbacks hold only Queue, never Runtime or this ledger.
struct BrowserRecord<T> {
    lease: T,
    requested: bool,
    submitted: bool,
    destroy_submitted: bool,
}
struct Browsers<T>(HashMap<i32, BrowserRecord<T>>);
impl<T> Default for Browsers<T> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}
impl<T> Browsers<T> {
    fn insert(&mut self, id: i32, lease: T) {
        assert!(!self.0.contains_key(&id), "duplicate native browser ID");
        self.0.insert(
            id,
            BrowserRecord {
                lease,
                requested: false,
                submitted: false,
                destroy_submitted: false,
            },
        );
    }
    fn request(&mut self, id: i32) {
        if let Some(record) = self.0.get_mut(&id) {
            record.requested = true;
        }
    }
    fn close(&mut self, id: i32) -> Option<T> {
        self.0.remove(&id).map(|record| record.lease)
    }
    fn ids(&self) -> Vec<i32> {
        let mut ids: Vec<_> = self.0.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlushState {
    Initializing,
    Ready,
    Submitted,
    Complete,
    Rejected,
}
impl FlushState {
    fn ready(&mut self) {
        if *self == Self::Initializing {
            *self = Self::Ready;
        }
    }
    fn complete(&mut self) {
        if *self == Self::Submitted {
            *self = Self::Complete;
        }
    }
    fn accepted(&mut self, accepted: bool) {
        if !accepted {
            *self = Self::Rejected;
        }
    }
}
struct Flush {
    path: std::path::PathBuf,
    manager: Option<CookieManager>,
    state: FlushState,
}

pub(crate) struct Lifecycle {
    pub queue: Queue,
    phase: Cell<ShutdownPhase>,
    browsers: RefCell<Browsers<BrowserLease>>,
    flushes: RefCell<Vec<Flush>>,
    failures: RefCell<Vec<String>>,
    polling: Cell<bool>,
    unknown_browsers: RefCell<HashSet<i32>>,
    creating: Cell<usize>,
    pumping: Cell<bool>,
    retiring: Cell<bool>,
}
impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            queue: Queue::default(),
            phase: Cell::new(ShutdownPhase::Running),
            browsers: RefCell::new(Browsers::default()),
            flushes: RefCell::new(Vec::new()),
            failures: RefCell::new(Vec::new()),
            polling: Cell::new(false),
            unknown_browsers: RefCell::new(HashSet::new()),
            creating: Cell::new(0),
            pumping: Cell::new(false),
            retiring: Cell::new(false),
        }
    }
}
pub(crate) struct CreationGuard<'a>(&'a Cell<usize>);
impl Drop for CreationGuard<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
pub(crate) struct RetirementGuard<'a>(&'a Cell<bool>);
impl Drop for RetirementGuard<'_> {
    fn drop(&mut self) { self.0.set(false); }
}
impl Lifecycle {
    pub(crate) fn shutdown_phase(&self) -> ShutdownPhase { self.phase.get() }
    pub(crate) fn is_preparing_retirements(&self) -> bool { self.retiring.get() }
    fn shutdown_poll_allowed(&self) -> bool {
        self.creating.get() == 0 && !self.pumping.get() && !self.retiring.get()
    }
    pub(crate) fn preparing_retirements(&self) -> Option<RetirementGuard<'_>> {
        if self.retiring.replace(true) { None } else { Some(RetirementGuard(&self.retiring)) }
    }
    pub fn creating(&self) -> CreationGuard<'_> {
        self.creating.set(self.creating.get() + 1);
        CreationGuard(&self.creating)
    }
    pub(crate) fn download_dispatch_allowed(&self) -> bool {
        self.creating.get() == 0 && !self.pumping.get() && !self.polling.get() && !self.retiring.get()
    }
    fn begin(&self) {
        if self.phase.get() == ShutdownPhase::Running {
            self.phase.set(ShutdownPhase::WaitingForOwners);
        }
    }
    fn start_closing(&self, owners: usize) -> bool {
        if self.phase.get() != ShutdownPhase::WaitingForOwners || owners != 0 {
            return false;
        }
        self.phase.set(ShutdownPhase::ClosingBrowsers);
        true
    }
    fn start_flushing(&self) -> bool {
        if self.phase.get() != ShutdownPhase::ClosingBrowsers
            || !self.browsers.borrow().0.is_empty()
            || !self.unknown_browsers.borrow().is_empty()
        {
            return false;
        }
        self.phase.set(ShutdownPhase::FlushingCookies);
        true
    }
    fn start_finalizing(&self, owners: usize) -> bool {
        if self.phase.get() != ShutdownPhase::FlushingCookies
            || owners != 0
            || !self.browsers.borrow().0.is_empty()
            || !self.unknown_browsers.borrow().is_empty()
            || self
                .flushes
                .borrow()
                .iter()
                .any(|flush| flush.state != FlushState::Complete)
        {
            return false;
        }
        self.phase.set(ShutdownPhase::ShuttingDown);
        true
    }
    pub fn require_running(&self) -> Result<(), String> {
        if CLOSING.load(Ordering::SeqCst) || self.phase.get() != ShutdownPhase::Running {
            Err("Chromium runtime is closing".into())
        } else {
            Ok(())
        }
    }
    pub fn register(&self, id: i32, lease: BrowserLease) {
        self.browsers.borrow_mut().insert(id, lease);
    }
    /// Snapshot only. Never execute native work while lifecycle is borrowed.
    pub(crate) fn retirement_browser(&self, id: i32) -> Option<Browser> {
        self.browsers.borrow().0.get(&id).map(|r| r.lease.browser.clone())
    }
    pub(crate) fn park_download_browser(&self, id: i32) -> Result<(), String> {
        let (clip, existing) = {
            let browsers = self.browsers.borrow();
            let record = browsers.0.get(&id).ok_or("Retiring browser has already closed")?;
            (record.lease.host, record.lease.retired_host.clone())
        };
        let host = match existing {
            Some(host) => host,
            None => Rc::new(crate::host::RetiredHost::new()?),
        };
        crate::host::retain_and_attach(host, |host| {
            self.browsers.borrow_mut().0.get_mut(&id).ok_or("Retiring browser closed during host creation")?
                .lease.retired_host = Some(host);
            Ok(())
        }, |host| host.attach(clip))
    }
    pub fn request_close(&self, id: i32) {
        self.browsers.borrow_mut().request(id);
    }
    pub(crate) fn fail(&self, message: String) {
        self.failures.borrow_mut().push(message);
        self.phase.set(ShutdownPhase::Failed);
    }

    fn collect_closed_with(&self, mut release: impl FnMut(i32)) {
        if self.creating.get() != 0 { return; }
        loop {
            let closed = take_closed_callbacks(&self.queue);
            if closed.is_empty() { return; }
            for id in closed {
                self.unknown_browsers.borrow_mut().remove(&id);
                let lease = self.browsers.borrow_mut().close(id);
                release(id);
                drop(lease); // Never release native/context leases under a borrow.
                crate::downloads::native_lease_released(id);
            }
        }
    }

    pub(crate) fn collect_closed(&self) {
        self.collect_closed_with(crate::downloads::native_closed);
    }

    pub(crate) fn after_closed_with<R>(&self, release: impl FnMut(i32), work: impl FnOnce() -> R) -> R {
        self.collect_closed_with(release);
        work()
    }

    pub(crate) fn after_closed<R>(&self, work: impl FnOnce() -> R) -> R {
        self.after_closed_with(crate::downloads::native_closed, work)
    }

    fn collect(&self) {
        self.collect_closed();
        if self.creating.get() != 0 {
            return;
        }
        let callbacks = std::mem::take(&mut *self.queue.lock().unwrap());
        let closed: HashSet<_> = callbacks
            .iter()
            .filter_map(|event| {
                if let Callback::Closed(id) = event {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        // Closed wins over stale duplicate close/destroy messages in this batch.
        for callback in &callbacks {
            if let Callback::Closed(id) = callback {
                self.unknown_browsers.borrow_mut().remove(id);
                let lease = self.browsers.borrow_mut().close(*id);
                crate::downloads::native_closed(*id);
                drop(lease); // Native releases can re-enter: never under RefCell borrow.
                crate::downloads::native_lease_released(*id);
            }
        }
        for callback in callbacks {
            match callback {
                Callback::Created(id) => {
                    // All app creation is synchronous and registers before the
                    // outer pump drains callbacks. Unexpected creation blocks exit.
                    if !closed.contains(&id) && !self.browsers.borrow().0.contains_key(&id) {
                        self.unknown_browsers.borrow_mut().insert(id);
                        self.fail(format!("Unregistered native browser {id}; shutdown cannot release its leases safely"));
                    }
                }
                Callback::DestroyChild(id, child) => {
                    let browser = {
                        let mut browsers = self.browsers.borrow_mut();
                        browsers.0.get_mut(&id).and_then(|record| {
                            if record.destroy_submitted {
                                return None;
                            }
                            record.destroy_submitted = true;
                            Some(record.lease.browser.clone())
                        })
                    };
                    // A prior native operation can re-enter and close another
                    // child while this batch is being drained. IsValid is one
                    // of the few permitted calls after OnBeforeClose; never
                    // dereference that child's queued native pointer then.
                    if browser.is_some_and(|browser| browser.is_valid() != 0) {
                        if let Err(error) = crate::host::destroy_child(child as *mut _) {
                            self.fail(format!("Cannot destroy Chromium child {id}: {error}"));
                        }
                    }
                }
                Callback::CookieReady(id) => {
                    if let Some(flush) = self.flushes.borrow_mut().get_mut(id) {
                        flush.state.ready();
                    }
                }
                Callback::Flushed(id) => {
                    if let Some(flush) = self.flushes.borrow_mut().get_mut(id) {
                        flush.state.complete();
                    }
                }
                Callback::Closed(_) => {}
            }
        }
        let closes: Vec<_> = {
            let mut browsers = self.browsers.borrow_mut();
            browsers
                .0
                .values_mut()
                .filter_map(|record| {
                    if !record.requested || record.submitted {
                        return None;
                    }
                    record.submitted = true;
                    Some(record.lease.browser.clone())
                })
                .collect()
        };
        for browser in closes {
            crate::downloads::before_native_close(browser.identifier());
            if browser.is_valid() == 0 {
                continue;
            }
            if let Some(host) = browser.host() {
                host.close_browser(1);
            } else {
                self.fail(format!(
                    "Browser {} has no close host",
                    browser.identifier()
                ));
            }
        }
    }
}

pub(crate) fn check_start_thread() -> Result<(), String> {
    check_thread()?;
    if CLOSING.load(Ordering::SeqCst) {
        return Err("Chromium runtime is closing or has shut down; restart the process".into());
    }
    let thread = std::thread::current().id();
    let owner = INIT_THREAD.get_or_init(|| thread);
    if *owner != thread {
        return Err("CEF must be initialized and used on its initialization thread".into());
    }
    Ok(())
}
pub(crate) fn check_thread() -> Result<(), String> {
    if INIT_THREAD
        .get()
        .is_some_and(|thread| *thread != std::thread::current().id())
    {
        Err("CEF shutdown and pumping must run on its initialization thread".into())
    } else {
        Ok(())
    }
}
fn runtime() -> Option<Rc<Runtime>> {
    RUNTIME.with(|slot| slot.borrow().clone())
}
/// Pure lifecycle state, also valid after the retained Runtime reaches Complete.
pub(crate) fn shutdown_started() -> bool { CLOSING.load(Ordering::SeqCst) }
pub(crate) fn is_polling() -> bool {
    runtime().is_some_and(|r| r.lifecycle.polling.get())
}
pub(crate) fn pump_once() {
    if check_thread().is_err() {
        return;
    }
    let Some(runtime) = runtime() else {
        return;
    };
    let lifecycle = &runtime.lifecycle;
    if matches!(
        lifecycle.phase.get(),
        ShutdownPhase::Complete | ShutdownPhase::ShuttingDown
    ) || lifecycle.creating.get() != 0
        || lifecycle.retiring.get()
        || lifecycle.pumping.replace(true)
    {
        return;
    }
    struct Guard<'a>(&'a Cell<bool>);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }
    let _guard = Guard(&lifecycle.pumping);
    do_message_loop_work();
    lifecycle.after_closed(crate::downloads::poll_runtime);
    lifecycle.collect();
}

impl Engine {
    /// Seal creation immediately. Repeated calls are idempotent. Drop every
    /// Engine pool and StorageContext, then call poll_shutdown on an app-level
    /// task while native termination remains deferred (not GPUI final quit).
    pub fn begin_shutdown() -> Result<ShutdownStatus, String> {
        check_thread()?;
        CLOSING.store(true, Ordering::SeqCst);
        if let Some(runtime) = runtime() {
            runtime.lifecycle.begin();
            runtime.downloads.borrow_mut().seal();
        }
        Self::shutdown_status()
    }

    pub fn shutdown_status() -> Result<ShutdownStatus, String> {
        check_thread()?;
        let Some(runtime) = runtime() else {
            return Ok(ShutdownStatus {
                phase: if CLOSING.load(Ordering::SeqCst) {
                    ShutdownPhase::Complete
                } else {
                    ShutdownPhase::Running
                },
                pending_owners: 0,
                pending_browsers: vec![],
                pending_cookie_flushes: 0,
                failures: vec![],
            });
        };
        let lifecycle = &runtime.lifecycle;
        let status = ShutdownStatus {
            phase: lifecycle.phase.get(),
            pending_owners: Rc::strong_count(&runtime).saturating_sub(2),
            pending_browsers: {
                let mut ids = lifecycle.browsers.borrow().ids();
                ids.extend(lifecycle.unknown_browsers.borrow().iter().copied());
                ids.sort_unstable();
                ids.dedup();
                ids
            },
            pending_cookie_flushes: lifecycle
                .flushes
                .borrow()
                .iter()
                .filter(|f| f.state != FlushState::Complete)
                .count(),
            failures: lifecycle.failures.borrow().clone(),
        };
        Ok(status)
    }

    /// Perform one nonblocking progress step; no deadline or pump-count barrier.
    /// Poll from an app-owned task independent of GPUI windows, without any
    /// window/page/application RefCell borrow held. Keep the toolkit loop alive.
    pub fn poll_shutdown() -> Result<ShutdownStatus, String> {
        check_thread()?;
        let Some(runtime) = runtime() else {
            return Self::shutdown_status();
        };
        let lifecycle = &runtime.lifecycle;
        if !lifecycle.shutdown_poll_allowed() {
            return Err("CEF shutdown polling attempted inside a CEF call or native retirement".into());
        }
        if lifecycle.polling.replace(true) {
            return Err("CEF shutdown polling re-entered".into());
        }
        struct PollGuard<'a>(&'a Cell<bool>);
        impl Drop for PollGuard<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let guard = PollGuard(&lifecycle.polling);
        let phase = lifecycle.phase.get();
        if !matches!(
            phase,
            ShutdownPhase::Running | ShutdownPhase::Complete | ShutdownPhase::ShuttingDown
        ) {
            // Release download handles/exact context leases before browser-close
            // and owner/context/flush barriers; this task owns no window borrows.
            lifecycle.after_closed(crate::downloads::shutdown_runtime);
            lifecycle.collect();
            if lifecycle.start_closing(Rc::strong_count(&runtime).saturating_sub(2)) {
                let ids = lifecycle.browsers.borrow().ids();
                for id in ids {
                    lifecycle.request_close(id);
                }
                lifecycle.collect();
            }
            // Engine::pump retains AppKit event-tracking safeguards and drains
            // lifecycle callbacks even after all application windows are gone.
            Self::pump_impl();
            if lifecycle.start_flushing() {
                // Snapshot retains contexts only while initializing cookie managers.
                let contexts = runtime.persistent.snapshot();
                for (path, context) in contexts {
                    let id = lifecycle.flushes.borrow().len();
                    lifecycle.flushes.borrow_mut().push(Flush {
                        path,
                        manager: None,
                        state: FlushState::Initializing,
                    });
                    let mut ready = CookieCallback::new(lifecycle.queue.clone(), id, false);
                    let manager = context.cookie_manager(Some(&mut ready));
                    let missing = manager.is_none();
                    lifecycle.flushes.borrow_mut()[id].manager = manager;
                    if missing {
                        lifecycle.fail(format!("Cookie manager {id} unavailable"));
                    }
                }
            }
            if lifecycle.phase.get() == ShutdownPhase::FlushingCookies {
                lifecycle.collect();
                let requests: Vec<_> = {
                    let mut flushes = lifecycle.flushes.borrow_mut();
                    flushes
                        .iter_mut()
                        .enumerate()
                        .filter_map(|(id, flush)| {
                            if flush.state != FlushState::Ready {
                                return None;
                            }
                            flush.state = FlushState::Submitted;
                            Some((id, flush.manager.clone(), flush.path.clone()))
                        })
                        .collect()
                };
                for (id, manager, path) in requests {
                    let mut callback = CookieCallback::new(lifecycle.queue.clone(), id, true);
                    let accepted = manager.is_some_and(|m| m.flush_store(Some(&mut callback)) != 0);
                    lifecycle.flushes.borrow_mut()[id].state.accepted(accepted);
                    if !accepted {
                        lifecycle.fail(format!("Cookie flush rejected for {}", path.display()));
                    }
                }
                lifecycle.collect();
                // Owned terminal copies may outlive CEF. Native download
                // handles may not: all callback releases precede CefShutdown.
                let downloads_released = runtime.downloads.borrow().downloads_native_handles_released();
                if downloads_released && lifecycle.start_finalizing(Rc::strong_count(&runtime).saturating_sub(2)) {
                    let flushes = std::mem::take(&mut *lifecycle.flushes.borrow_mut());
                    drop(flushes);
                    runtime.persistent.clear();
                    *runtime.private.borrow_mut() = std::rc::Weak::new();
                    // All app CEF browser/context/manager/registration handles
                    // are gone. Runtime and its framework loader stay alive.
                    cef::shutdown();
                    lifecycle.phase.set(ShutdownPhase::Complete);
                }
            }
        }
        drop(guard);
        drop(runtime); // status owner count must exclude this local reference.
        Self::shutdown_status()
    }
}

wrap_completion_callback! {
    struct CookieCallback { queue: Queue, id: usize, flushed: bool }
    impl CompletionCallback {
        fn on_complete(&self) {
            enqueue(&self.queue, if self.flushed { Callback::Flushed(self.id) } else { Callback::CookieReady(self.id) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_record_retains_exact_context_and_host_until_before_close() {
        struct Lease {
            context: Rc<()>,
            host: Rc<Cell<usize>>,
        }
        impl Drop for Lease {
            fn drop(&mut self) {
                self.host.set(self.host.get() + 1);
            }
        }
        let context = Rc::new(());
        let host = Rc::new(Cell::new(0));
        let mut browsers = Browsers::default();
        // IDs are native, unrelated to local ViewId values in two pools.
        for id in [41, 89] {
            browsers.insert(
                id,
                Lease {
                    context: context.clone(),
                    host: host.clone(),
                },
            );
        }
        browsers.request(41);
        browsers.request(41); // Repeated tab close/quit is idempotent.
        assert_eq!(browsers.ids(), vec![41, 89]);
        assert_eq!(Rc::strong_count(&context), 3);
        assert_eq!(host.get(), 0);
        let closed = browsers.close(41).unwrap();
        assert!(Rc::ptr_eq(&closed.context, &context));
        assert_eq!(host.get(), 0); // Removal itself doesn't release under a borrow.
        drop(closed);
        assert_eq!(host.get(), 1);
        assert!(browsers.close(41).is_none());
        assert_eq!(browsers.ids(), vec![89]); // Inspector still blocks exit.
        drop(browsers.close(89));
        assert_eq!(host.get(), 2);
        assert_eq!(Rc::strong_count(&context), 1);
    }

    #[test]
    fn owner_close_and_flush_barriers_are_independent_and_repeated_quit_cannot_skip_them() {
        let state = Lifecycle::default();
        state.begin();
        state.begin();
        assert_eq!(state.phase.get(), ShutdownPhase::WaitingForOwners);
        assert!(!state.start_closing(2)); // Engine fork and public context.
        assert!(!state.start_closing(1));
        assert!(state.start_closing(0));
        assert!(!state.start_closing(0));
        state.begin();
        assert_eq!(state.phase.get(), ShutdownPhase::ClosingBrowsers);
        state.unknown_browsers.borrow_mut().insert(19);
        assert!(!state.start_flushing());
        enqueue(&state.queue, Callback::Closed(19));
        state.collect(); // CEF-free: only IDs, no native browser methods.
        assert!(state.start_flushing());
        state.flushes.borrow_mut().push(Flush {
            path: "Default".into(),
            manager: None,
            state: FlushState::Submitted,
        });
        assert!(!state.start_finalizing(0));
        enqueue(&state.queue, Callback::CookieReady(0)); // Late init cannot regress.
        enqueue(&state.queue, Callback::Flushed(0));
        enqueue(&state.queue, Callback::Flushed(999)); // Stale/unknown token ignored.
        state.collect();
        assert_eq!(state.flushes.borrow()[0].state, FlushState::Complete);
        assert!(!state.start_finalizing(1));
        assert!(state.start_finalizing(0));
        state.begin();
        assert!(!state.start_finalizing(0)); // Exactly one CefShutdown caller.
        state.phase.set(ShutdownPhase::Complete);
        state.begin();
        assert_eq!(state.phase.get(), ShutdownPhase::Complete);
    }

    #[test]
    fn immediate_flush_rejection_wins_over_queued_or_late_completion() {
        for completion_first in [false, true] {
            let mut state = FlushState::Submitted;
            if completion_first {
                state.complete();
            }
            state.accepted(false);
            state.ready();
            state.complete();
            assert_eq!(state, FlushState::Rejected);
        }
        let state = Lifecycle::default();
        state.begin();
        state.start_closing(0);
        state.start_flushing();
        state.flushes.borrow_mut().push(Flush {
            path: "profile-work".into(),
            manager: None,
            state: FlushState::Rejected,
        });
        state.fail("Cookie flush rejected".into());
        enqueue(&state.queue, Callback::Flushed(0));
        state.collect();
        state.begin();
        assert_eq!(state.phase.get(), ShutdownPhase::Failed);
        assert!(!state.start_finalizing(0));
        assert_eq!(
            state.failures.borrow().as_slice(),
            ["Cookie flush rejected"]
        );
    }

    #[test]
    fn initialization_callback_is_required_and_absent_callback_stays_pending() {
        let mut state = FlushState::Initializing;
        state.complete();
        assert_eq!(state, FlushState::Initializing);
        state.ready();
        assert_eq!(state, FlushState::Ready);
        state = FlushState::Submitted;
        state.accepted(true);
        assert_eq!(state, FlushState::Submitted); // Return true is not completion.
        state.complete();
        state.ready();
        state.complete();
        assert_eq!(state, FlushState::Complete);
    }

    #[test]
    fn late_closed_and_destroy_callbacks_cannot_touch_released_native_children() {
        let state = Lifecycle::default();
        enqueue(&state.queue, Callback::Created(7));
        enqueue(&state.queue, Callback::Closed(7));
        enqueue(&state.queue, Callback::DestroyChild(7, usize::MAX));
        enqueue(&state.queue, Callback::Closed(7));
        state.collect(); // Would crash if a stale pointer reached a native API.
        assert!(state.browsers.borrow().0.is_empty());
        assert!(state.failures.borrow().is_empty());
        enqueue(&state.queue, Callback::DestroyChild(7, usize::MAX));
        state.collect();
        assert!(state.failures.borrow().is_empty());
    }

    #[test]
    fn unexpected_browser_blocks_shutdown_and_creation_guard_defers_callback_drain() {
        let state = Lifecycle::default();
        let creation = state.creating();
        enqueue(&state.queue, Callback::Created(123));
        state.collect();
        assert_eq!(state.queue.lock().unwrap().len(), 1);
        drop(creation);
        state.collect();
        assert_eq!(state.phase.get(), ShutdownPhase::Failed);
        assert!(state.unknown_browsers.borrow().contains(&123));
        state.begin();
        assert!(!state.start_closing(0));
        enqueue(&state.queue, Callback::Closed(123));
        state.collect();
        assert!(state.unknown_browsers.borrow().is_empty());
        // Failure is sticky even if a late close cleans up the unknown ID.
        assert_eq!(state.phase.get(), ShutdownPhase::Failed);
    }

    #[test]
    fn callbacks_own_only_queue_and_cannot_retain_the_runtime() {
        let owner = Rc::new(Lifecycle::default());
        let weak = Rc::downgrade(&owner);
        let late_callback = owner.queue.clone();
        drop(owner);
        assert!(weak.upgrade().is_none());
        enqueue(&late_callback, Callback::Flushed(0));
        assert_eq!(late_callback.lock().unwrap().len(), 1);
    }
}

#[cfg(test)]
mod download_dispatch_tests {
    use super::Lifecycle;

    #[test]
    fn reentrant_shutdown_cannot_drain_registration_owners_during_native_handoff() {
        use crate::downloads::RuntimeDownloads;
        use crate::DownloadOwner;
        use std::rc::Rc;
        let lifecycle = Lifecycle::default();
        let mut downloads = RuntimeDownloads::<()>::default();
        let context = Rc::new(());
        let weak = Rc::downgrade(&context);
        let snapshot = downloads.prepare(context.clone(), DownloadOwner::continue_normal_after_source_close(91), false).unwrap();
        downloads.bind(&snapshot.registration, 701).unwrap();
        drop(context);
        let handoff = lifecycle.preparing_retirements().unwrap();
        // Native handoff reenters begin_shutdown + poll_shutdown. Use the exact
        // production poll gate before it can drain RuntimeDownloads owners.
        lifecycle.begin();
        if lifecycle.shutdown_poll_allowed() { downloads.shutdown().execute(); }
        assert!(!lifecycle.shutdown_poll_allowed());
        assert!(weak.upgrade().is_some());
        // Resume from the retained native-operation snapshot: the same browser
        // registration still exists instead of having been drained by polling.
        assert!(downloads.queue_retirement(701));
        drop(handoff);
        assert!(lifecycle.shutdown_poll_allowed());
        downloads.shutdown().execute();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn public_download_dispatch_rejects_nested_creation_pumping_and_shutdown_polling() {
        let lifecycle = Lifecycle::default();
        assert!(lifecycle.download_dispatch_allowed());
        let creation = lifecycle.creating();
        assert!(!lifecycle.download_dispatch_allowed());
        drop(creation);
        lifecycle.pumping.set(true);
        assert!(!lifecycle.download_dispatch_allowed());
        lifecycle.pumping.set(false);
        lifecycle.polling.set(true);
        assert!(!lifecycle.download_dispatch_allowed());
        lifecycle.polling.set(false);
        assert!(lifecycle.download_dispatch_allowed());
        let retirement = lifecycle.preparing_retirements().unwrap();
        assert!(!lifecycle.download_dispatch_allowed());
        assert!(!lifecycle.shutdown_poll_allowed());
        assert!(lifecycle.preparing_retirements().is_none());
        drop(retirement);
        assert!(lifecycle.download_dispatch_allowed());
    }
}
