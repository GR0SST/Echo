//! Engine-owned download callbacks, ledger and explicit lifetime policies.
//! See DOWNLOADS_INTEGRATION.md before activation.
//!
//! CEF callbacks copy items immediately and own only an inbox and immutable
//! provenance. The reducer never invokes CEF. Execute returned effects on the
//! initialization/UI thread AFTER releasing application/engine/model borrows.
//! No persistence, destination selection, filesystem writes, or UI lives here.

use cef::rc::Rc as _;
use cef::*;
#[cfg(test)]
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Immutable application-supplied ownership. Values are opaque to the engine.
/// `owner` identifies the originating local profile (the retained normal source
/// for Blank); `storage` identifies its actual storage lease; `browser` is a
/// never-reused browser-generation identity, NOT a TabId or a window ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DownloadProvenance {
    owner: u64,
    storage: u64,
    browser: u64,
    private: bool,
}

impl DownloadProvenance {
    pub fn new(owner: u64, storage: u64, browser: u64, private: bool) -> Self {
        Self {
            owner,
            storage,
            browser,
            private,
        }
    }
    pub fn owner(self) -> u64 {
        self.owner
    }
    pub fn storage(self) -> u64 {
        self.storage
    }
    pub fn browser(self) -> u64 {
        self.browser
    }
    pub fn is_private(self) -> bool {
        self.private
    }
}

/// Process-local, ledger-qualified identity; never serialize or reconstruct it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DownloadToken {
    ledger: u64,
    sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadPhase {
    Pending,
    InProgress,
    Paused,
    Complete,
    Canceled,
    Interrupted,
}

impl DownloadPhase {
    /// Interrupted transfers may resume in CEF. Only completion/cancellation
    /// ends control ownership and installs a late-update tombstone.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Canceled)
    }
}

/// All values are copied while DownloadItem is valid. Byte counts are retained
/// verbatim; `percent_complete == -1` denotes an unknown receive total. Times
/// retain CEF Basetime's representation, not Unix timestamps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadInfo {
    pub phase: DownloadPhase,
    pub interrupt_reason: u32,
    pub current_speed: i64,
    pub percent_complete: i32,
    pub total_bytes: i64,
    pub received_bytes: i64,
    pub start_time: i64,
    pub end_time: i64,
    pub full_path: PathBuf,
    pub url: String,
    pub original_url: String,
    pub suggested_file_name: String,
    pub content_disposition: String,
    pub mime_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadDestination {
    /// An update can precede OnBeforeDownload; no destination callback yet.
    Unrequested,
    Requested {
        suggested_name: String,
    },
    /// Requested target, not proof of the final on-disk filename. Read full_path
    /// from subsequent CEF snapshots. No collision/overwrite policy is implied.
    Resolved(PathBuf),
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadSnapshot {
    pub token: DownloadToken,
    pub provenance: DownloadProvenance,
    pub info: DownloadInfo,
    pub destination: DownloadDestination,
    pub cancel_requested: bool,
    pub closing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadEvent {
    DestinationRequested(DownloadSnapshot),
    Updated(DownloadSnapshot),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadError {
    UnknownToken,
    Closed,
    Terminal,
    DestinationUnavailable,
    InvalidDestination,
    ControlUnavailable,
    UnsupportedPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadControl {
    Cancel,
    Pause,
    Resume,
}

// A registration is unique even if application provenance or native IDs repeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    registration: u64,
    native_browser: i32,
    native_download: u32,
}

enum Incoming<B, C> {
    Before {
        key: Key,
        origin: DownloadProvenance,
        info: DownloadInfo,
        suggested: String,
        callback: B,
    },
    Update {
        key: Key,
        origin: DownloadProvenance,
        info: DownloadInfo,
        callback: Option<C>,
    },
}

impl<B, C> Incoming<B, C> {
    fn key(&self) -> Key {
        match self {
            Self::Before { key, .. } | Self::Update { key, .. } => *key,
        }
    }
    fn is_progress(&self) -> bool {
        matches!(self, Self::Update { info, .. } if info.phase == DownloadPhase::InProgress)
    }
}

/// UI-thread callbacks may be released across threads. Admission and native
/// liveness are separate: teardown closes admission before canceling a LIVE
/// owner; OnBeforeClose closes native liveness before queuing Closed.
#[derive(Clone)]
struct NativeCallback<T> {
    value: T,
    alive: Arc<Gate>,
}
impl<T> NativeCallback<T> {
    fn when_alive(&self, run: impl FnOnce(&T)) {
        if self.alive.get() {
            run(&self.value);
        }
    }
}
type NativeBefore = NativeCallback<BeforeDownloadCallback>;
type NativeControl = NativeCallback<DownloadItemCallback>;

struct Gate(AtomicBool);

impl Gate {
    fn new(open: bool) -> Self {
        Self(AtomicBool::new(open))
    }
    fn get(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    fn set(&self, open: bool) {
        self.0.store(open, Ordering::Release);
    }
}

/// Native-safe inbox. Methods are called on the CEF UI thread; atomics/Mutex
/// also allow native handler reference releases on another thread. No Runtime,
/// StorageContext, Browser, GPUI entity or DownloadItem is retained here.
struct Inbox<B, C> {
    closed: Gate,
    queue: Mutex<Vec<Incoming<B, C>>>,
    shutdown_copies: Mutex<ShutdownCopies>,
}

#[derive(Clone)]
struct CopiedTerminal {
    key: Key,
    origin: DownloadProvenance,
    info: DownloadInfo,
}
#[derive(Default)]
struct ShutdownCopies {
    // Frozen before sealing native ingress. No token can be invented by a late
    // callback, and neither Browser nor any callback handle enters this queue.
    known: HashMap<Key, DownloadProvenance>,
    ready: Vec<CopiedTerminal>,
    terminal_seen: HashSet<Key>,
}
impl ShutdownCopies {
    fn push(&mut self, key: Key, origin: DownloadProvenance, info: DownloadInfo) -> bool {
        if !info.phase.is_terminal() || self.known.get(&key) != Some(&origin)
            || !self.terminal_seen.insert(key) { return false; }
        self.ready.push(CopiedTerminal { key, origin, info });
        true
    }
}

impl<B, C> Default for Inbox<B, C> {
    fn default() -> Self {
        Self {
            closed: Gate::new(false),
            queue: Mutex::new(Vec::new()),
            shutdown_copies: Mutex::default(),
        }
    }
}

impl<B, C> Inbox<B, C> {
    fn copy_shutdown_terminal(&self, key: Key, origin: DownloadProvenance, info: DownloadInfo) -> bool {
        self.closed.get() && self.shutdown_copies.lock().unwrap().push(key, origin, info)
    }
    fn push(&self, mut event: Incoming<B, C>) {
        // Only progress for the same key may replace progress. A before or
        // terminal event forms a barrier. Drop displaced handles AFTER borrow.
        let displaced = {
            let mut queue = self.queue.lock().unwrap();
            let previous = queue.iter().rposition(|old| old.key() == event.key());
            if event.is_progress() && previous.is_some_and(|i| queue[i].is_progress()) {
                let previous = &mut queue[previous.unwrap()];
                // A defensive null control handle in a newer progress update
                // must not discard the only usable earlier control handle.
                if let (
                    Incoming::Update { callback: old, .. },
                    Incoming::Update { callback: new, .. },
                ) = (&mut *previous, &mut event)
                {
                    if new.is_none() {
                        *new = old.take();
                    }
                }
                Some(std::mem::replace(previous, event))
            } else {
                queue.push(event);
                None
            }
        };
        drop(displaced);
    }
    fn take(&self) -> Vec<Incoming<B, C>> {
        std::mem::take(&mut *self.queue.lock().unwrap())
    }
}

struct Entry<B, C> {
    key: Key,
    snapshot: DownloadSnapshot,
    terminal: bool,
    before: Option<B>,
    control: Option<C>,
}

struct RegistrationState {
    origin: DownloadProvenance,
    accepting: Arc<Gate>,
    native_browser: Option<i32>,
    continuing: bool,
    retiring: bool,
}

enum Effect<B, C> {
    Continue(B, String),
    Control(C, DownloadControl),
    ReleaseBefore(B),
    ReleaseControl(C),
}

struct Ledger<B, C> {
    id: u64,
    _main_thread: PhantomData<Rc<()>>,
    next_token: u64,
    next_registration: u64,
    stopped: bool,
    inbox: Arc<Inbox<B, C>>,
    entries: HashMap<DownloadToken, Entry<B, C>>,
    keys: HashMap<Key, DownloadToken>,
    registrations: HashMap<u64, RegistrationState>,
    ready: Vec<DownloadEvent>,
}

impl<B, C: Clone> Default for Ledger<B, C> {
    fn default() -> Self {
        static NEXT_LEDGER: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_LEDGER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("download ledger identity exhausted");
        Self {
            id,
            _main_thread: PhantomData,
            next_token: 1,
            next_registration: 1,
            stopped: false,
            inbox: Arc::default(),
            entries: HashMap::new(),
            keys: HashMap::new(),
            registrations: HashMap::new(),
            ready: Vec::new(),
        }
    }
}

impl<B, C: Clone> Ledger<B, C> {
    fn register(&mut self, origin: DownloadProvenance) -> Result<(u64, Arc<Gate>), DownloadError> {
        if self.stopped {
            return Err(DownloadError::Closed);
        }
        let id = self.next_registration;
        self.next_registration = id
            .checked_add(1)
            .expect("download registration identity exhausted");
        let accepting = Arc::new(Gate::new(true));
        self.registrations.insert(
            id,
            RegistrationState {
                origin,
                accepting: accepting.clone(),
                native_browser: None,
                continuing: false,
                retiring: false,
            },
        );
        Ok((id, accepting))
    }

    fn ensure_entry(&mut self, key: Key, origin: DownloadProvenance, info: &DownloadInfo) -> DownloadToken {
        if let Some(token) = self.keys.get(&key) { return *token; }
        let token = DownloadToken { ledger: self.id, sequence: self.next_token };
        self.next_token = self.next_token.checked_add(1).expect("download identity exhausted");
        self.keys.insert(key, token);
        self.entries.insert(token, Entry {
            key, terminal: false, before: None, control: None,
            snapshot: DownloadSnapshot {
                token, provenance: origin, info: info.clone(),
                destination: DownloadDestination::Unrequested,
                cancel_requested: false, closing: false,
            },
        });
        token
    }

    /// No CEF effects, callback drops or native handles. Materialize identities
    /// of callbacks already received before the seal, then freeze late capture.
    fn seal_ingress(&mut self) {
        if self.inbox.closed.get() { return; }
        let received: Vec<_> = self.inbox.queue.lock().unwrap().iter().map(|incoming| {
            match incoming {
                Incoming::Before { key, origin, info, .. } | Incoming::Update { key, origin, info, .. } =>
                    (*key, *origin, info.clone()),
            }
        }).collect();
        for (key, origin, info) in &received {
            let known = self.keys.contains_key(key);
            let admitted = self.registrations.get_mut(&key.registration).is_some_and(|state| {
                state.origin == *origin
                    && (state.accepting.get() || state.retiring || (state.continuing && known))
                    && *state.native_browser.get_or_insert(key.native_browser) == key.native_browser
            });
            if admitted { self.ensure_entry(*key, *origin, info); }
        }
        {
            let mut copies = self.inbox.shutdown_copies.lock().unwrap();
            copies.known = self.entries.values().map(|entry| (entry.key, entry.snapshot.provenance)).collect();
            // The ordinary inbox still owns callback handles until the existing
            // outside-borrows dispatcher. Copy terminal data only, never handles.
            for (key, origin, info) in received { copies.push(key, origin, info); }
        }
        self.inbox.closed.set(true);
    }

    /// Pure metadata reduction. Native handles, if any remain before the first
    /// shutdown poll, are untouched; shutdown/Closed releases them separately.
    fn collect_shutdown_copies(&mut self) {
        let copied = std::mem::take(&mut self.inbox.shutdown_copies.lock().unwrap().ready);
        for copy in copied {
            let Some(token) = self.keys.get(&copy.key) else { continue; };
            let entry = self.entries.get_mut(token).unwrap();
            if entry.terminal || entry.snapshot.provenance != copy.origin || !copy.info.phase.is_terminal() { continue; }
            entry.snapshot.info = copy.info;
            if matches!(entry.snapshot.destination, DownloadDestination::Unrequested) {
                entry.snapshot.destination = DownloadDestination::Rejected;
            }
            entry.snapshot.closing = false;
            entry.terminal = true;
            self.ready.push(DownloadEvent::Updated(entry.snapshot.clone()));
        }
    }

    /// One shared owned-event consumer, including after CEF Complete. This
    /// operation never collects/drops native handles and returns no effects.
    fn take_shutdown_events(&mut self) -> Result<Vec<DownloadEvent>, String> {
        if !self.inbox.closed.get() { return Err("Shutdown download drain requires begin_shutdown".into()); }
        self.collect_shutdown_copies();
        Ok(std::mem::take(&mut self.ready))
    }

    fn native_handles_released(&self) -> bool {
        self.entries.values().all(|entry| entry.before.is_none() && entry.control.is_none())
            && self.inbox.queue.lock().unwrap().is_empty()
    }

    fn collect(&mut self) -> Vec<Effect<B, C>> {
        self.collect_shutdown_copies();
        self.reduce_received(self.inbox.take(), false)
    }

    /// Closed-first dispatch still preserves metadata already copied by CEF.
    /// No native control or destination continuation can escape this mode.
    fn collect_closed_metadata(&mut self, id: u64) -> Vec<Effect<B, C>> {
        self.collect_shutdown_copies();
        let (closed, keep): (Vec<_>, Vec<_>) = self.inbox.take().into_iter()
            .partition(|incoming| incoming.key().registration == id);
        self.inbox.queue.lock().unwrap().extend(keep);
        self.reduce_received(closed, true)
    }

    fn reduce_received(&mut self, received: Vec<Incoming<B, C>>, release_only: bool) -> Vec<Effect<B, C>> {
        let mut effects = Vec::new();
        for incoming in received {
            let key = incoming.key();
            let origin = match &incoming {
                Incoming::Before { origin, .. } | Incoming::Update { origin, .. } => *origin,
            };
            let known = self.keys.contains_key(&key);
            let shutdown_known = !self.inbox.closed.get()
                || self.inbox.shutdown_copies.lock().unwrap().known.get(&key) == Some(&origin);
            let accepted = shutdown_known && (release_only || !self.stopped)
                && self
                    .registrations
                    .get_mut(&key.registration)
                    .is_some_and(|registration| {
                        if registration.origin != origin || !(release_only || registration.accepting.get() || registration.retiring || (registration.continuing && known)) {
                            return false;
                        }
                        *registration
                            .native_browser
                            .get_or_insert(key.native_browser)
                            == key.native_browser
                    });
            if !accepted {
                if release_only { Self::release(incoming, &mut effects); }
                else { Self::reject(incoming, &mut effects); }
                continue;
            }
            let info = match &incoming {
                Incoming::Before { info, .. } | Incoming::Update { info, .. } => info,
            };
            let token = self.ensure_entry(key, origin, info);
            let entry = self.entries.get_mut(&token).unwrap();
            // The first event may itself be terminal. Existing terminal entries
            // have already emitted Updated; their tombstone prevents revival.
            if entry.terminal {
                Self::release(incoming, &mut effects);
                continue;
            }
            if release_only {
                let info = match &incoming {
                    Incoming::Before { info, .. } | Incoming::Update { info, .. } => info.clone(),
                };
                entry.terminal = info.phase.is_terminal();
                entry.snapshot.info = info;
                entry.snapshot.closing = !entry.terminal;
                if matches!(entry.snapshot.destination, DownloadDestination::Unrequested) {
                    entry.snapshot.destination = DownloadDestination::Rejected;
                }
                Self::release(incoming, &mut effects);
                Self::release_handles(entry, &mut effects);
                self.ready.push(DownloadEvent::Updated(entry.snapshot.clone()));
                continue;
            }
            match incoming {
                Incoming::Before {
                    info,
                    suggested,
                    callback,
                    ..
                } => {
                    if entry.snapshot.cancel_requested || entry.snapshot.closing {
                        effects.push(Effect::ReleaseBefore(callback));
                        continue;
                    }
                    // OnBeforeDownload is expected once. Keep the first
                    // decision; never replace an already resolved destination.
                    if !matches!(entry.snapshot.destination, DownloadDestination::Unrequested) {
                        effects.push(Effect::ReleaseBefore(callback));
                        continue;
                    }
                    entry.snapshot.info = info;
                    if entry.snapshot.info.phase.is_terminal() {
                        entry.terminal = true;
                        entry.snapshot.destination = DownloadDestination::Rejected;
                        effects.push(Effect::ReleaseBefore(callback));
                        Self::release_handles(entry, &mut effects);
                        self.ready
                            .push(DownloadEvent::Updated(entry.snapshot.clone()));
                    } else {
                        entry.before = Some(callback);
                        entry.snapshot.destination = DownloadDestination::Requested {
                            suggested_name: suggested,
                        };
                        self.ready
                            .push(DownloadEvent::DestinationRequested(entry.snapshot.clone()));
                    }
                }
                Incoming::Update { info, callback, .. } => {
                    entry.snapshot.info = info;
                    if entry.snapshot.info.phase.is_terminal() {
                        entry.terminal = true;
                        // Mark even a terminal-first item so late callbacks
                        // cannot recreate a pending destination or controls.
                        if matches!(entry.snapshot.destination, DownloadDestination::Unrequested) {
                            entry.snapshot.destination = DownloadDestination::Rejected;
                        }
                        Self::release_handles(entry, &mut effects);
                        if let Some(callback) = callback {
                            effects.push(Effect::ReleaseControl(callback));
                        }
                    } else if let Some(callback) = callback {
                        if entry.snapshot.cancel_requested {
                            effects.push(Effect::Control(callback, DownloadControl::Cancel));
                        } else if let Some(old) = entry.control.replace(callback) {
                            effects.push(Effect::ReleaseControl(old));
                        }
                    }
                    self.ready
                        .push(DownloadEvent::Updated(entry.snapshot.clone()));
                }
            }
        }
        effects
    }

    fn release(incoming: Incoming<B, C>, effects: &mut Vec<Effect<B, C>>) {
        match incoming {
            Incoming::Before { callback, .. } => effects.push(Effect::ReleaseBefore(callback)),
            Incoming::Update {
                callback: Some(callback),
                ..
            } => effects.push(Effect::ReleaseControl(callback)),
            Incoming::Update { callback: None, .. } => {}
        }
    }
    fn reject(incoming: Incoming<B, C>, effects: &mut Vec<Effect<B, C>>) {
        match incoming {
            Incoming::Before { callback, .. } => effects.push(Effect::ReleaseBefore(callback)),
            Incoming::Update {
                info,
                callback: Some(callback),
                ..
            } if !info.phase.is_terminal() => {
                effects.push(Effect::Control(callback, DownloadControl::Cancel));
            }
            incoming => Self::release(incoming, effects),
        }
    }
    fn release_handles(entry: &mut Entry<B, C>, effects: &mut Vec<Effect<B, C>>) {
        if let Some(callback) = entry.before.take() {
            effects.push(Effect::ReleaseBefore(callback));
        }
        if let Some(callback) = entry.control.take() {
            effects.push(Effect::ReleaseControl(callback));
        }
    }

    fn entry(&mut self, token: DownloadToken) -> Result<&mut Entry<B, C>, DownloadError> {
        if self.stopped {
            return Err(DownloadError::Closed);
        }
        let entry = self
            .entries
            .get_mut(&token)
            .ok_or(DownloadError::UnknownToken)?;
        if entry.snapshot.closing {
            return Err(DownloadError::Closed);
        }
        if entry.snapshot.info.phase.is_terminal() {
            return Err(DownloadError::Terminal);
        }
        Ok(entry)
    }

    fn resolve(
        &mut self,
        token: DownloadToken,
        path: PathBuf,
    ) -> Result<Vec<Effect<B, C>>, DownloadError> {
        // No empty/temp fallback, path normalization, file creation or overwrite
        // decision. Caller supplies an externally selected full destination.
        let text = path
            .to_str()
            .filter(|text| !text.is_empty() && !text.contains('\0'))
            .filter(|_| path.is_absolute() && path.file_name().is_some())
            .ok_or(DownloadError::InvalidDestination)?
            .to_owned();
        let entry = self.entry(token)?;
        if entry.snapshot.cancel_requested {
            return Err(DownloadError::DestinationUnavailable);
        }
        let callback = entry
            .before
            .take()
            .ok_or(DownloadError::DestinationUnavailable)?;
        entry.snapshot.destination = DownloadDestination::Resolved(path);
        let snapshot = entry.snapshot.clone();
        self.ready.push(DownloadEvent::Updated(snapshot));
        Ok(vec![Effect::Continue(callback, text)])
    }

    fn control(
        &mut self,
        token: DownloadToken,
        command: DownloadControl,
    ) -> Result<Vec<Effect<B, C>>, DownloadError> {
        let entry = self.entry(token)?;
        let mut effects = Vec::new();
        if command == DownloadControl::Cancel {
            entry.snapshot.cancel_requested = true;
            if let Some(before) = entry.before.take() {
                entry.snapshot.destination = DownloadDestination::Rejected;
                effects.push(Effect::ReleaseBefore(before));
            }
            if let Some(callback) = entry.control.take() {
                effects.push(Effect::Control(callback, command));
            }
            // A pre-update cancel is sticky: a later update supplies its handle.
        } else {
            if entry.snapshot.cancel_requested {
                return Err(DownloadError::ControlUnavailable);
            }
            let callback = entry
                .control
                .as_ref()
                .ok_or(DownloadError::ControlUnavailable)?
                .clone();
            effects.push(Effect::Control(callback, command));
        }
        let snapshot = entry.snapshot.clone();
        self.ready.push(DownloadEvent::Updated(snapshot));
        Ok(effects)
    }

    fn close_registration(&mut self, registration: u64) -> Vec<Effect<B, C>> {
        if let Some(registration) = self.registrations.get_mut(&registration) {
            registration.accepting.set(false);
            registration.continuing = false;
            registration.retiring = false;
        }
        let mut effects = Vec::new();
        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.key.registration == registration)
        {
            if entry.snapshot.info.phase.is_terminal() || entry.snapshot.closing {
                continue;
            }
            entry.snapshot.closing = true;
            entry.snapshot.cancel_requested = true;
            if let Some(before) = entry.before.take() {
                entry.snapshot.destination = DownloadDestination::Rejected;
                effects.push(Effect::ReleaseBefore(before));
            }
            if let Some(callback) = entry.control.take() {
                effects.push(Effect::Control(callback, DownloadControl::Cancel));
            }
            self.ready
                .push(DownloadEvent::Updated(entry.snapshot.clone()));
        }
        effects
    }

    fn shutdown(&mut self) -> Vec<Effect<B, C>> {
        self.seal_ingress();
        self.collect_shutdown_copies();
        let mut effects = Vec::new();
        for id in self.registrations.keys().copied().collect::<Vec<_>>() {
            effects.extend(self.close_registration(id));
        }
        for entry in self.entries.values_mut() {
            // Genuine terminal copies can precede the first shutdown poll.
            // Their retained controls are release-only, never cancellation.
            Self::release_handles(entry, &mut effects);
        }
        self.stopped = true;
        for incoming in self.inbox.take() {
            let terminal = self.keys.get(&incoming.key()).is_some_and(|token| self.entries[token].terminal);
            if terminal { Self::release(incoming, &mut effects); }
            else { Self::reject(incoming, &mut effects); }
        }
        effects
    }
}

impl<B, C> Drop for Ledger<B, C> {
    fn drop(&mut self) {
        // Explicit shutdown is required to execute cancellation effects. This
        // fallback closes ingress and releases handles; it never calls CEF.
        self.inbox.closed.set(true);
        for registration in self.registrations.values() {
            registration.accepting.set(false);
        }
        drop(self.inbox.take());
    }
}

/// Opaque engine-internal effects; execute/drop on the CEF initialization thread.
/// Execution calls CEF with no ledger/inbox borrow. Cancellation is asynchronous;
/// a requested cancel is not a fabricated terminal status.
#[must_use = "execute effects outside application/engine borrows, or explicitly drop them"]
pub struct DownloadEffects(
    Vec<Effect<NativeBefore, NativeControl>>,
    PhantomData<Rc<()>>,
);

impl DownloadEffects {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn append(&mut self, other: Self) {
        self.0.extend(other.0);
    }
    pub fn execute(self) {
        for effect in self.0 {
            match effect {
                Effect::Continue(callback, path) => callback
                    .when_alive(|callback| callback.cont(Some(&CefString::from(path.as_str())), 0)),
                Effect::Control(callback, DownloadControl::Cancel) => {
                    callback.when_alive(|callback| callback.cancel())
                }
                Effect::Control(callback, DownloadControl::Pause) => {
                    callback.when_alive(|callback| callback.pause())
                }
                Effect::Control(callback, DownloadControl::Resume) => {
                    callback.when_alive(|callback| callback.resume())
                }
                Effect::ReleaseBefore(callback) => drop(callback),
                Effect::ReleaseControl(callback) => drop(callback),
            }
        }
    }
}

/// Engine-owned main-thread ledger, independent of views and tab retirement.
/// It intentionally has no Serialize implementation, including for private data.
#[derive(Default)]
pub struct Downloads(Ledger<NativeBefore, NativeControl>);

impl Downloads {
    pub fn register_browser(
        &mut self,
        origin: DownloadProvenance,
        allow_downloads: bool,
    ) -> Result<DownloadRegistration, DownloadError> {
        let (id, accepting) = self.0.register(origin)?;
        Ok(DownloadRegistration {
            ledger: self.0.id,
            _main_thread: PhantomData,
            id,
            origin,
            accepting,
            allow_downloads,
            native_alive: Arc::new(Gate::new(true)),
            quiescence: Arc::default(),
            inbox: self.0.inbox.clone(),
        })
    }
    /// Always collect before issuing commands; execute returned effects after
    /// releasing enclosing engine/model borrows, then drain events for the owner.
    pub fn collect(&mut self) -> DownloadEffects {
        DownloadEffects(self.0.collect(), PhantomData)
    }
    pub fn take_events(&mut self) -> Vec<DownloadEvent> {
        std::mem::take(&mut self.0.ready)
    }
    pub fn snapshot(&self, token: DownloadToken) -> Option<&DownloadSnapshot> {
        self.0.entries.get(&token).map(|entry| &entry.snapshot)
    }
    pub fn resolve_destination(
        &mut self,
        token: DownloadToken,
        path: PathBuf,
    ) -> Result<DownloadEffects, DownloadError> {
        self.0
            .resolve(token, path)
            .map(|effects| DownloadEffects(effects, PhantomData))
    }
    pub fn control(
        &mut self,
        token: DownloadToken,
        command: DownloadControl,
    ) -> Result<DownloadEffects, DownloadError> {
        self.0
            .control(token, command)
            .map(|effects| DownloadEffects(effects, PhantomData))
    }
    /// Explicit teardown/cancel policy hook, NOT ordinary tab retirement. Whether
    /// Arc keeps a transfer alive after closing a tab/window needs observation.
    pub fn close_registration(
        &mut self,
        registration: &DownloadRegistration,
    ) -> Result<DownloadEffects, DownloadError> {
        if registration.ledger != self.0.id {
            return Err(DownloadError::UnknownToken);
        }
        Ok(DownloadEffects(
            self.0.close_registration(registration.id),
            PhantomData,
        ))
    }
    pub fn shutdown(&mut self) -> DownloadEffects {
        DownloadEffects(self.0.shutdown(), PhantomData)
    }
}

/// Engine-internal native handler registration, retained until native browser
/// closure. It retains only ingress, never a Runtime/context/page lease.
#[derive(Clone)]
pub struct DownloadRegistration {
    ledger: u64,
    _main_thread: PhantomData<Rc<()>>,
    id: u64,
    origin: DownloadProvenance,
    accepting: Arc<Gate>,
    allow_downloads: bool,
    native_alive: Arc<Gate>,
    quiescence: Arc<Quiescence>,
    inbox: Arc<Inbox<NativeBefore, NativeControl>>,
}

impl DownloadRegistration {
    /// Only the engine's Client implementation should call this CEF-facing hook.
    pub(crate) fn handler(&self) -> DownloadHandler {
        NativeDownloadHandler::new(
            self.id,
            self.origin,
            self.accepting.clone(),
            self.allow_downloads,
            self.native_alive.clone(),
            self.inbox.clone(),
        )
    }
}

fn copy_valid<T>(valid: bool, read: impl FnOnce() -> T) -> Option<T> {
    valid.then(read)
}

fn admission_allowed(policy: bool, accepting: bool, closed: bool) -> bool {
    policy && accepting && !closed
}

fn copy_item(item: Option<&mut DownloadItem>) -> Option<(u32, DownloadInfo)> {
    let item = item?;
    copy_valid(item.is_valid() != 0, || {
        let phase = if item.is_complete() != 0 {
            DownloadPhase::Complete
        } else if item.is_canceled() != 0 {
            DownloadPhase::Canceled
        } else if item.is_interrupted() != 0 {
            DownloadPhase::Interrupted
        } else if item.is_paused() != 0 {
            DownloadPhase::Paused
        } else if item.is_in_progress() != 0 {
            DownloadPhase::InProgress
        } else {
            DownloadPhase::Pending
        };
        (
            item.id(),
            DownloadInfo {
                phase,
                interrupt_reason: item.interrupt_reason().get_raw(),
                current_speed: item.current_speed(),
                percent_complete: item.percent_complete(),
                total_bytes: item.total_bytes(),
                received_bytes: item.received_bytes(),
                start_time: item.start_time().val,
                end_time: item.end_time().val,
                full_path: PathBuf::from(CefString::from(&item.full_path()).to_string()),
                url: CefString::from(&item.url()).to_string(),
                original_url: CefString::from(&item.original_url()).to_string(),
                suggested_file_name: CefString::from(&item.suggested_file_name()).to_string(),
                content_disposition: CefString::from(&item.content_disposition()).to_string(),
                mime_type: CefString::from(&item.mime_type()).to_string(),
            },
        )
    })
}

wrap_download_handler! {
    struct NativeDownloadHandler {
        registration: u64,
        origin: DownloadProvenance,
        accepting: Arc<Gate>,
        allow_downloads: bool,
        native_alive: Arc<Gate>,
        inbox: Arc<Inbox<NativeBefore, NativeControl>>,
    }
    impl DownloadHandler {
        fn can_download(&self, _browser: Option<&mut Browser>, _url: Option<&CefString>, _request_method: Option<&CefString>) -> i32 {
            // Rust's generated default is 0, unlike the C++ header's true.
            i32::from(self.native_alive.get() && admission_allowed(self.allow_downloads, self.accepting.get(), self.inbox.closed.get()))
        }
        fn on_before_download(&self, browser: Option<&mut Browser>, download_item: Option<&mut DownloadItem>, suggested_name: Option<&CefString>, callback: Option<&mut BeforeDownloadCallback>) -> i32 {
            // Always claim handling: false delegates to Alloy's cancellation.
            // Rejected/invalid requests drop the callback without continuing.
            if !self.native_alive.get() || !self.allow_downloads || self.inbox.closed.get() { return 1; }
            if let (Some(browser), Some((native_download, info)), Some(callback)) = (browser, copy_item(download_item), callback) {
                self.inbox.push(Incoming::Before {
                    key: Key { registration: self.registration, native_browser: browser.identifier(), native_download },
                    origin: self.origin, info,
                    suggested: suggested_name.map(ToString::to_string).unwrap_or_default(),
                    callback: NativeCallback { value: callback.clone(), alive: self.native_alive.clone() },
                });
            }
            1
        }
        fn on_download_updated(&self, browser: Option<&mut Browser>, download_item: Option<&mut DownloadItem>, callback: Option<&mut DownloadItemCallback>) {
            // OnBeforeClose is a release-only boundary, even before its queued
            // lifecycle notification is collected. IsValid is allowed afterward.
            if !self.native_alive.get() || !browser.as_ref().is_some_and(|browser| browser.is_valid() != 0) { return; }
            let copied = copy_item(download_item);
            if self.allow_downloads && self.inbox.closed.get() {
                if let (Some(browser), Some((native_download, info))) = (browser.as_ref(), copied.as_ref()) {
                    if info.phase.is_terminal() {
                        self.inbox.copy_shutdown_terminal(Key {
                            registration: self.registration, native_browser: browser.identifier(), native_download: *native_download,
                        }, self.origin, info.clone());
                        return; // No callback clone/control/native release here.
                    }
                }
            }
            if !self.allow_downloads || self.inbox.closed.get() || copied.is_none() || browser.is_none() {
                // No inbox/model borrow exists while calling CEF, including
                // late callbacks after explicit shutdown. Terminal updates only
                // release their handle; invalid items are never read further.
                if !copied.as_ref().is_some_and(|(_, info)| info.phase.is_terminal()) {
                    if let Some(callback) = callback {
                        if self.native_alive.get() { callback.cancel(); }
                    }
                }
                return;
            }
            let browser = browser.unwrap();
            let (native_download, info) = copied.unwrap();
            self.inbox.push(Incoming::Update {
                key: Key { registration: self.registration, native_browser: browser.identifier(), native_download },
                origin: self.origin, info, callback: callback.map(|callback| NativeCallback { value: callback.clone(), alive: self.native_alive.clone() }),
            });
        }
    }
}

fn release_closed_registration<B, C: Clone>(
    ledger: &mut Ledger<B, C>,
    id: u64,
) -> Vec<Effect<B, C>> {
    let mut effects = ledger.collect_closed_metadata(id);
    if let Some(state) = ledger.registrations.get_mut(&id) {
        state.accepting.set(false);
        state.retiring = false;
        state.continuing = false;
    }
    for entry in ledger
        .entries
        .values_mut()
        .filter(|entry| entry.key.registration == id)
    {
        if !entry.terminal && !entry.snapshot.closing {
            entry.snapshot.closing = true;
            ledger
                .ready
                .push(DownloadEvent::Updated(entry.snapshot.clone()));
        }
        Ledger::release_handles(entry, &mut effects);
    }
    // Native closure has already happened (or creation failed). Release
    // handles without executing controls against the retired native owner.
    // Preserve every unrelated registration's queued callback order.
    let mut keep = Vec::new();
    for incoming in ledger.inbox.take() {
        if incoming.key().registration == id {
            Ledger::release(incoming, &mut effects);
        } else {
            keep.push(incoming);
        }
    }
    ledger.inbox.queue.lock().unwrap().extend(keep);
    effects
}

/// Explicit opt-in for a captured local profile's downloads. Source-close
/// cancellation and candidate normal continuation are distinct policies.
/// Existing page creation methods never construct either capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadOwner {
    profile: u64,
    policy: SourceClosePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceClosePolicy { Cancel, ContinueNormal }

impl DownloadOwner {
    pub fn cancel_before_native_close(profile: u64) -> Self {
        Self { profile, policy: SourceClosePolicy::Cancel }
    }
    /// Candidate normal-only policy. Requires the outside-borrows retirement
    /// handshake before destroying the source native parent. Not native accepted.
    pub fn continue_normal_after_source_close(owner: u64) -> Self {
        Self { profile: owner, policy: SourceClosePolicy::ContinueNormal }
    }
}

#[derive(Default)]
struct Quiescence { retiring: AtomicBool, requested: AtomicBool, complete: AtomicBool, failed: AtomicBool, initiator_failed: AtomicBool }

#[derive(Clone)]
pub(crate) struct DownloadGate {
    admission: Arc<Gate>,
    native_alive: Arc<Gate>,
    quiescence: Arc<Quiescence>,
}
impl DownloadGate {
    pub(crate) fn document_result(&self, url: &str, success: bool) {
        if !success && self.quiescence.retiring.load(Ordering::Acquire) && !self.quiescence.requested.load(Ordering::Acquire) {
            self.quiescence.initiator_failed.store(true, Ordering::Release);
        }
        if url == "about:blank" && self.quiescence.requested.load(Ordering::Acquire) {
            if success { self.quiescence.complete.store(true, Ordering::Release); }
            else { self.quiescence.failed.store(true, Ordering::Release); }
        }
    }
    pub(crate) fn seal(&self) {
        self.native_alive.set(false);
        self.admission.set(false);
    }
}

impl DownloadRegistration {
    pub(crate) fn gate(&self) -> DownloadGate {
        DownloadGate {
            admission: self.accepting.clone(),
            native_alive: self.native_alive.clone(),
            quiescence: self.quiescence.clone(),
        }
    }
}

/// Callback ownership never points back to this owner or to Runtime. Contexts
/// are the exact registry Rc leases, not StorageContext (which retains Runtime).
struct OwnedBrowser<C> {
    registration: DownloadRegistration,
    context: Rc<C>,
    native: Option<i32>,
    policy: SourceClosePolicy,
    retirement: Option<crate::retirement::Retirement>,
}

pub(crate) struct PreparedDownloads {
    pub(crate) registration: DownloadRegistration,
}

pub(crate) struct RuntimeDownloads<C> {
    downloads: Downloads,
    browsers: HashMap<u64, OwnedBrowser<C>>,
    storage: Vec<(u64, std::rc::Weak<C>)>,
    next_identity: u64,
    failed: Vec<u64>,
    retirement_history: Vec<crate::DownloadRetirementStatus>,
}

impl<C> Default for RuntimeDownloads<C> {
    fn default() -> Self {
        Self {
            downloads: Downloads::default(),
            browsers: HashMap::new(),
            storage: Vec::new(),
            next_identity: 1,
            failed: Vec::new(),
            retirement_history: Vec::new(),
        }
    }
}

/// Native effects and final context releases are moved out before execution.
/// Native browser references remain solely in the existing lifecycle ledger.
pub(crate) struct RuntimeDownloadWork<C> {
    effects: DownloadEffects,
    contexts: Vec<Rc<C>>,
}
impl<C> RuntimeDownloadWork<C> {
    pub(crate) fn execute(self) {
        self.effects.execute();
        drop(self.contexts);
    }
}

impl<C> RuntimeDownloads<C> {
    fn identity(&mut self) -> u64 {
        let id = self.next_identity;
        self.next_identity = id
            .checked_add(1)
            .expect("download ownership identity exhausted");
        id
    }

    pub(crate) fn prepare(
        &mut self,
        context: Rc<C>,
        owner: DownloadOwner,
        private: bool,
    ) -> Result<PreparedDownloads, DownloadError> {
        if private && owner.policy == SourceClosePolicy::ContinueNormal {
            return Err(DownloadError::UnsupportedPolicy);
        }
        self.storage
            .retain(|(_, context)| context.strong_count() != 0);
        let storage = self.storage.iter().find_map(|(id, existing)| {
            existing
                .upgrade()
                .filter(|existing| Rc::ptr_eq(existing, &context))
                .map(|_| *id)
        });
        let storage = storage.unwrap_or_else(|| {
            let id = self.identity();
            self.storage.push((id, Rc::downgrade(&context)));
            id
        });
        let browser = self.identity();
        let provenance = DownloadProvenance::new(owner.profile, storage, browser, private);
        let registration = self.downloads.register_browser(provenance, true)?;
        self.browsers.insert(
            registration.id,
            OwnedBrowser {
                registration: registration.clone(),
                context,
                native: None,
                policy: owner.policy,
                retirement: None,
            },
        );
        Ok(PreparedDownloads { registration })
    }

    /// Bind before the creation guard is dropped or any download collection.
    pub(crate) fn bind(
        &mut self,
        registration: &DownloadRegistration,
        native: i32,
    ) -> Result<(), DownloadError> {
        if registration.ledger != self.downloads.0.id {
            return Err(DownloadError::UnknownToken);
        }
        if self
            .browsers
            .values()
            .any(|browser| browser.native == Some(native))
        {
            return Err(DownloadError::Closed);
        }
        let browser = self
            .browsers
            .get_mut(&registration.id)
            .ok_or(DownloadError::UnknownToken)?;
        if browser.native.is_some() {
            return Err(DownloadError::Closed);
        }
        let state = self
            .downloads
            .0
            .registrations
            .get_mut(&registration.id)
            .ok_or(DownloadError::UnknownToken)?;
        if state.native_browser.is_some_and(|id| id != native) {
            return Err(DownloadError::Closed);
        }
        state.native_browser = Some(native);
        browser.native = Some(native);
        Ok(())
    }

    pub(crate) fn creation_failed(&mut self, registration: &DownloadRegistration) {
        if registration.ledger == self.downloads.0.id {
            registration.accepting.set(false);
            registration.native_alive.set(false);
            self.failed.push(registration.id);
        }
    }

    fn release_registration(&mut self, id: u64) -> RuntimeDownloadWork<C> {
        if let Some(browser) = self.browsers.get(&id) {
            browser.registration.gate().seal();
        }
        let effects = release_closed_registration(&mut self.downloads.0, id);
        let contexts = self
            .browsers
            .remove(&id)
            .map(|browser| browser.context)
            .into_iter()
            .collect();
        RuntimeDownloadWork {
            effects: DownloadEffects(effects, PhantomData),
            contexts,
        }
    }

    pub(crate) fn collect(&mut self) -> RuntimeDownloadWork<C> {
        let mut work = RuntimeDownloadWork {
            effects: DownloadEffects(Vec::new(), PhantomData),
            contexts: Vec::new(),
        };
        for id in std::mem::take(&mut self.failed) {
            let failed = self.release_registration(id);
            work.effects.append(failed.effects);
            work.contexts.extend(failed.contexts);
        }
        work.effects.append(self.downloads.collect());
        work
    }

    /// Logical close only queues work: no callbacks, handles or native calls.
    pub(crate) fn queue_retirement(&mut self, native: i32) -> bool {
        if self.downloads.0.inbox.closed.get() { return false; }
        let Some(browser) = self.browsers.values_mut().find(|b| b.native == Some(native)) else { return false; };
        if browser.policy != SourceClosePolicy::ContinueNormal { return false; }
        browser.registration.accepting.set(false);
        browser.registration.quiescence.retiring.store(true, Ordering::Release);
        self.downloads.0.registrations.get_mut(&browser.registration.id).unwrap().retiring = true;
        browser.retirement.get_or_insert_with(Default::default);
        true
    }

    fn retirement_ids(&self) -> Vec<i32> {
        self.browsers.values().filter_map(|b| b.retirement.as_ref().and(b.native)).collect()
    }

    fn transfer_state(&self, native: i32) -> crate::retirement::Transfers {
        let browser = self.browsers.values().find(|b| b.native == Some(native)).unwrap();
        let mut state = crate::retirement::Transfers::default();
        for entry in self.downloads.0.entries.values().filter(|e| e.key.registration == browser.registration.id) {
            state.associated = true;
            if !entry.terminal {
                state.unfinished += 1;
                state.destinations_ready &= matches!(entry.snapshot.destination, DownloadDestination::Resolved(_));
            }
        }
        state
    }

    fn seal_retirement_admission(&mut self, native: i32) {
        let browser = self.browsers.values().find(|b| b.native == Some(native)).unwrap();
        let state = self.downloads.0.registrations.get_mut(&browser.registration.id).unwrap();
        state.retiring = false;
        state.continuing = true;
        state.accepting.set(false);
    }

    fn retirement(&self, native: i32) -> crate::retirement::Retirement {
        self.browsers.values().find(|b| b.native == Some(native)).unwrap().retirement.as_ref().unwrap().clone()
    }

    fn set_retirement(&mut self, native: i32, state: crate::retirement::Retirement) {
        if let Some(browser) = self.browsers.values_mut().find(|b| b.native == Some(native)) {
            browser.retirement = Some(state);
        }
    }

    pub(crate) fn before_native_close(&mut self, native: i32) -> RuntimeDownloadWork<C> {
        let mut work = self.collect();
        if let Some(id) = self
            .browsers
            .iter()
            .find_map(|(id, browser)| (browser.native == Some(native)).then_some(*id))
        {
            work.effects.append(DownloadEffects(
                self.downloads.0.close_registration(id),
                PhantomData,
            ));
        }
        work
    }

    pub(crate) fn native_closed(&mut self, native: i32) -> RuntimeDownloadWork<C> {
        let id = self
            .browsers
            .iter()
            .find_map(|(id, browser)| (browser.native == Some(native)).then_some(*id));
        let metadata = if let Some(id) = id {
            self.browsers[&id].registration.gate().seal();
            self.downloads.0.collect_closed_metadata(id)
        } else { Vec::new() };
        if let Some(id) = id {
            let browser = &self.browsers[&id];
            if let Some(retirement) = &browser.retirement {
                let mut status = retirement.status(native, browser.registration.origin, self.transfer_state(native));
                status.closed = true;
                if !retirement.close_requested && !self.downloads.0.inbox.closed.get() {
                    status.failure = Some("Native browser closed before continuation released its transfer lease".into());
                }
                self.retirement_history.push(status);
            }
        }
        let mut work = id.map(|id| self.release_registration(id))
            .unwrap_or_else(|| RuntimeDownloadWork {
                effects: DownloadEffects(Vec::new(), PhantomData),
                contexts: Vec::new(),
            });
        let mut effects = DownloadEffects(metadata, PhantomData);
        effects.append(work.effects);
        work.effects = effects;
        work
    }

    /// Admission seal performs no CEF calls and releases no native handles.
    pub(crate) fn seal(&mut self) {
        self.downloads.0.seal_ingress();
        for browser in self.browsers.values() {
            browser.registration.accepting.set(false);
        }
    }

    pub(crate) fn downloads_native_handles_released(&self) -> bool {
        self.browsers.is_empty() && self.downloads.0.native_handles_released()
    }

    pub(crate) fn shutdown(&mut self) -> RuntimeDownloadWork<C> {
        let effects = self.downloads.shutdown();
        self.failed.clear();
        self.storage.clear();
        let contexts = self
            .browsers
            .drain()
            .map(|(_, browser)| browser.context)
            .collect();
        RuntimeDownloadWork { effects, contexts }
    }
}

/// Explicit deny handler also blocks StartDownload, which bypasses can_download.
pub(crate) fn denied_handler() -> DownloadHandler {
    NativeDownloadHandler::new(
        0,
        DownloadProvenance::new(0, 0, 0, true),
        Arc::new(Gate::new(false)),
        false,
        Arc::new(Gate::new(true)),
        Arc::default(),
    )
}

fn current_runtime() -> Option<Rc<crate::Runtime>> {
    crate::RUNTIME.with(|slot| slot.borrow().clone())
}

pub(crate) fn poll_runtime() {
    if let Some(runtime) = current_runtime() {
        runtime.lifecycle.collect_closed();
        let work = runtime.downloads.borrow_mut().collect();
        work.execute();
        let _ = progress_retirements(&runtime);
    }
}
pub(crate) fn before_native_close(native: i32) {
    if let Some(runtime) = current_runtime() {
        runtime.lifecycle.collect_closed();
        let work = runtime.downloads.borrow_mut().before_native_close(native);
        work.execute();
    }
}
pub(crate) fn native_closed(native: i32) {
    if let Some(runtime) = current_runtime() {
        let work = runtime.downloads.borrow_mut().native_closed(native);
        work.execute();
    }
}
pub(crate) fn shutdown_runtime() {
    if let Some(runtime) = current_runtime() {
        runtime.lifecycle.collect_closed();
        let work = runtime.downloads.borrow_mut().shutdown();
        work.execute();
    }
}

fn running_runtime() -> Result<Rc<crate::Runtime>, String> {
    crate::shutdown::check_thread()?;
    let runtime = current_runtime().ok_or("Chromium runtime is unavailable")?;
    runtime.lifecycle.require_running()?;
    if !runtime.lifecycle.download_dispatch_allowed() {
        return Err(
            "Download dispatch cannot run inside CEF creation, pumping or shutdown polling".into(),
        );
    }
    runtime.lifecycle.collect_closed();
    Ok(runtime)
}

fn shutdown_owned_dispatch(
    phase: crate::ShutdownPhase,
    between_native_calls: bool,
    drain: impl FnOnce() -> Result<Vec<DownloadEvent>, String>,
) -> Result<Vec<DownloadEvent>, String> {
    if phase == crate::ShutdownPhase::Running {
        return Err("Shutdown download drain requires begin_shutdown".into());
    }
    if !between_native_calls {
        return Err("Shutdown download drain must run between native calls and shutdown polls".into());
    }
    drain()
}

impl crate::Engine {
    /// One process-wide drain for a future central profile/private router, not
    /// per-window page events. Call outside ALL window/model/engine borrows.
    pub fn take_download_events() -> Result<Vec<DownloadEvent>, String> {
        let runtime = running_runtime()?;
        runtime.lifecycle.collect_closed();
        let work = runtime.downloads.borrow_mut().collect();
        work.execute();
        let events = runtime.downloads.borrow_mut().downloads.take_events();
        Ok(events)
    }

    /// Owned copies only, after begin_shutdown and between poll_shutdown calls,
    /// including after Complete. The unique app shutdown consumer replaces the
    /// normal take_download_events consumer; both consume the same ready queue.
    /// No CEF calls, callback collection/release, controls or destination effects.
    pub fn take_shutdown_download_events() -> Result<Vec<DownloadEvent>, String> {
        crate::shutdown::check_thread()?;
        let runtime = current_runtime();
        let phase = runtime.as_ref().map(|runtime| runtime.lifecycle.shutdown_phase())
            .unwrap_or(if crate::shutdown::shutdown_started() { crate::ShutdownPhase::Complete } else { crate::ShutdownPhase::Running });
        let between = runtime.as_ref().is_none_or(|runtime| runtime.lifecycle.download_dispatch_allowed());
        shutdown_owned_dispatch(phase, between, || {
            let Some(runtime) = runtime else { return Ok(Vec::new()); };
            let events = runtime.downloads.borrow_mut().downloads.0.take_shutdown_events()?;
            Ok(events)
        })
    }

    pub fn download_snapshot(token: DownloadToken) -> Result<Option<DownloadSnapshot>, String> {
        let runtime = running_runtime()?;
        let snapshot = runtime
            .downloads
            .borrow()
            .downloads
            .snapshot(token)
            .cloned();
        Ok(snapshot)
    }

    /// Caller has selected a full path externally. No picker/default path.
    /// Call outside ALL window/model/engine borrows; native effects do not escape.
    pub fn resolve_download_destination(token: DownloadToken, path: PathBuf) -> Result<(), String> {
        let runtime = running_runtime()?;
        runtime.lifecycle.collect_closed();
        let work = runtime.downloads.borrow_mut().collect();
        work.execute();
        let effects = runtime
            .downloads
            .borrow_mut()
            .downloads
            .resolve_destination(token, path)
            .map_err(|error| format!("Cannot resolve download destination: {error:?}"))?;
        effects.execute();
        Ok(())
    }

    /// Call outside ALL window/model/engine borrows. CEF confirms actual state.
    pub fn control_download(token: DownloadToken, command: DownloadControl) -> Result<(), String> {
        let runtime = running_runtime()?;
        runtime.lifecycle.collect_closed();
        let work = runtime.downloads.borrow_mut().collect();
        work.execute();
        let effects = runtime
            .downloads
            .borrow_mut()
            .downloads
            .control(token, command)
            .map_err(|error| format!("Cannot control download: {error:?}"))?;
        effects.execute();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Handle(Rc<Cell<usize>>);
    impl Drop for Handle {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    fn handle() -> (Handle, Rc<Cell<usize>>) {
        let drops = Rc::new(Cell::new(0));
        (Handle(drops.clone()), drops)
    }
    fn info(phase: DownloadPhase, bytes: i64) -> DownloadInfo {
        DownloadInfo {
            phase,
            interrupt_reason: 0,
            current_speed: 7,
            percent_complete: -1,
            total_bytes: -1,
            received_bytes: bytes,
            start_time: 123,
            end_time: 0,
            full_path: PathBuf::new(),
            url: "https://fixture.test/file".into(),
            original_url: "https://fixture.test/redirect".into(),
            suggested_file_name: "file.bin".into(),
            content_disposition: "attachment".into(),
            mime_type: "application/octet-stream".into(),
        }
    }
    fn fixture() -> (Ledger<Handle, Handle>, Key, DownloadProvenance) {
        let mut ledger = Ledger::default();
        let origin = DownloadProvenance::new(10, 20, 30, false);
        let (registration, _) = ledger.register(origin).unwrap();
        (
            ledger,
            Key {
                registration,
                native_browser: 5,
                native_download: 1,
            },
            origin,
        )
    }
    fn before(
        ledger: &Ledger<Handle, Handle>,
        key: Key,
        origin: DownloadProvenance,
    ) -> Rc<Cell<usize>> {
        let (callback, drops) = handle();
        ledger.inbox.push(Incoming::Before {
            key,
            origin,
            info: info(DownloadPhase::InProgress, 0),
            suggested: "file.bin".into(),
            callback,
        });
        drops
    }
    fn update(
        ledger: &Ledger<Handle, Handle>,
        key: Key,
        origin: DownloadProvenance,
        phase: DownloadPhase,
        bytes: i64,
    ) -> Rc<Cell<usize>> {
        let (callback, drops) = handle();
        ledger.inbox.push(Incoming::Update {
            key,
            origin,
            info: info(phase, bytes),
            callback: Some(callback),
        });
        drops
    }
    fn token(ledger: &Ledger<Handle, Handle>, key: Key) -> DownloadToken {
        ledger.keys[&key]
    }
    fn destination() -> PathBuf {
        std::env::temp_dir().join("echo-download-foundation.bin")
    }

    #[test]
    fn shutdown_complete_copied_before_seal_drains_once_without_native_releases_or_effects() {
        let (mut ledger, key, origin) = fixture();
        let active = update(&ledger, key, origin, DownloadPhase::InProgress, 12);
        drop(ledger.collect());
        ledger.ready.clear();
        let complete = update(&ledger, key, origin, DownloadPhase::Complete, 32);
        ledger.seal_ingress();
        let events = ledger.take_shutdown_events().unwrap();
        assert!(matches!(events.as_slice(), [DownloadEvent::Updated(snapshot)]
            if snapshot.info.phase == DownloadPhase::Complete && snapshot.info.received_bytes == 32));
        assert_eq!(active.get(), 0);
        assert_eq!(complete.get(), 0); // Owned drain never drops native handles.
        assert!(ledger.entries[&token(&ledger, key)].control.is_some());
        assert!(ledger.take_shutdown_events().unwrap().is_empty());
        let cleanup = ledger.shutdown();
        assert!(cleanup.iter().all(|effect| matches!(effect, Effect::ReleaseBefore(_) | Effect::ReleaseControl(_))));
        assert_eq!(active.get(), 0); // Outside-borrows caller still owns releases.
        assert!(ledger.native_handles_released());
        drop(cleanup);
        assert_eq!(active.get(), 1);
        assert_eq!(complete.get(), 1);
        assert!(ledger.take_shutdown_events().unwrap().is_empty());
    }

    #[test]
    fn queued_progress_and_complete_at_seal_release_handles_without_canceling_completed_item() {
        let (mut ledger, key, origin) = fixture();
        let progress = update(&ledger, key, origin, DownloadPhase::InProgress, 12);
        let complete = update(&ledger, key, origin, DownloadPhase::Complete, 32);
        ledger.seal_ingress();
        let cleanup = ledger.shutdown();
        assert!(cleanup.iter().all(|effect| matches!(effect, Effect::ReleaseBefore(_) | Effect::ReleaseControl(_))));
        assert_eq!(progress.get(), 0);
        assert_eq!(complete.get(), 0);
        drop(cleanup);
        assert_eq!(progress.get(), 1);
        assert_eq!(complete.get(), 1);
        let events = ledger.take_shutdown_events().unwrap();
        assert!(matches!(events.as_slice(), [DownloadEvent::Updated(snapshot)] if snapshot.info.phase == DownloadPhase::Complete));
    }

    #[test]
    fn shutdown_terminal_during_cancel_preserves_real_complete_without_commanded_success() {
        let (mut ledger, key, origin) = fixture();
        let active = update(&ledger, key, origin, DownloadPhase::Paused, 12);
        drop(ledger.collect());
        ledger.ready.clear();
        ledger.seal_ingress();
        let cancellation = ledger.shutdown();
        assert!(matches!(cancellation.as_slice(), [Effect::Control(_, DownloadControl::Cancel)]));
        assert_ne!(ledger.entries[&token(&ledger, key)].snapshot.info.phase, DownloadPhase::Complete);
        ledger.take_shutdown_events().unwrap(); // Cancel request is not Complete.
        // Same metadata-only ingress used by the real callback while Cancel is
        // executing and native owner is still alive. It clones no callback.
        assert!(ledger.inbox.copy_shutdown_terminal(key, origin, info(DownloadPhase::Complete, 32)));
        let events = ledger.take_shutdown_events().unwrap();
        assert!(matches!(events.as_slice(), [DownloadEvent::Updated(snapshot)]
            if snapshot.info.phase == DownloadPhase::Complete && snapshot.cancel_requested));
        assert_eq!(active.get(), 0);
        drop(cancellation);
        assert_eq!(active.get(), 1);
        assert!(!ledger.inbox.copy_shutdown_terminal(key, origin, info(DownloadPhase::Complete, 99)));
        assert!(ledger.take_shutdown_events().unwrap().is_empty());
        assert_eq!(ledger.entries[&token(&ledger, key)].snapshot.info.received_bytes, 32);
    }

    #[test]
    fn shutdown_capture_rejects_unknown_spoofed_native_identity_origin_and_private_drift() {
        let (mut ledger, key, origin) = fixture();
        update(&ledger, key, origin, DownloadPhase::InProgress, 12);
        drop(ledger.collect());
        ledger.ready.clear();
        ledger.seal_ingress();
        for (wrong_key, wrong_origin) in [
            (Key { native_download: 999, ..key }, origin),
            (Key { native_browser: 999, ..key }, origin),
            (key, DownloadProvenance::new(999, 20, 30, false)),
            (key, DownloadProvenance::new(10, 20, 30, true)),
        ] {
            assert!(!ledger.inbox.copy_shutdown_terminal(wrong_key, wrong_origin, info(DownloadPhase::Complete, 32)));
        }
        assert!(!ledger.inbox.copy_shutdown_terminal(key, origin, info(DownloadPhase::Interrupted, 12)));
        assert!(ledger.take_shutdown_events().unwrap().is_empty());
        assert_eq!(ledger.keys.len(), 1);
        assert_ne!(ledger.entries[&token(&ledger, key)].snapshot.info.phase, DownloadPhase::Complete);
        drop(ledger.shutdown());
    }

    #[test]
    fn complete_queued_without_prior_drain_is_known_at_seal_and_unknown_after_seal_is_rejected() {
        let (mut ledger, key, origin) = fixture();
        update(&ledger, key, origin, DownloadPhase::Complete, 32);
        ledger.seal_ingress();
        let events = ledger.take_shutdown_events().unwrap();
        assert!(matches!(events.as_slice(), [DownloadEvent::Updated(snapshot)] if snapshot.info.phase == DownloadPhase::Complete));
        let unknown = Key { native_download: 8, ..key };
        update(&ledger, unknown, origin, DownloadPhase::Complete, 45);
        let release = release_closed_registration(&mut ledger, key.registration);
        assert!(release.iter().all(|effect| matches!(effect, Effect::ReleaseBefore(_) | Effect::ReleaseControl(_))));
        assert!(!ledger.keys.contains_key(&unknown));
        drop(release);
        drop(ledger.shutdown());
    }

    #[test]
    fn terminal_after_runtime_shutdown_owner_drain_remains_copied_without_native_leases() {
        use std::cell::RefCell;
        let runtime = RefCell::new(RuntimeDownloads::<()>::default());
        let context = Rc::new(());
        let weak = Rc::downgrade(&context);
        let prepared = runtime.borrow_mut().prepare(context.clone(), DownloadOwner::cancel_before_native_close(94), false).unwrap();
        runtime.borrow_mut().bind(&prepared.registration, 703).unwrap();
        let key = Key { registration: prepared.registration.id, native_browser: 703, native_download: 1 };
        prepared.registration.inbox.push(Incoming::Update {
            key, origin: prepared.registration.origin, info: info(DownloadPhase::InProgress, 12), callback: None,
        });
        let collect = runtime.borrow_mut().collect();
        collect.execute();
        runtime.borrow_mut().downloads.take_events();
        runtime.borrow_mut().seal();
        drop(context);
        let cleanup = runtime.borrow_mut().shutdown();
        assert!(runtime.try_borrow_mut().is_ok());
        assert!(weak.upgrade().is_some()); // Release work still owns exact Rc.
        cleanup.execute(); // Existing native dispatcher executes outside borrow.
        assert!(weak.upgrade().is_none());
        runtime.borrow_mut().downloads.0.take_shutdown_events().unwrap();
        assert!(runtime.borrow().downloads_native_handles_released());
        // Client/gate remains alive until native Closed, independently of the
        // drained RuntimeDownloads owner map. No callback handle is cloned.
        assert!(prepared.registration.inbox.copy_shutdown_terminal(key, prepared.registration.origin, info(DownloadPhase::Complete, 32)));
        let closed = runtime.borrow_mut().native_closed(703);
        closed.execute();
        let events = shutdown_owned_dispatch(crate::ShutdownPhase::Complete, true,
            || runtime.borrow_mut().downloads.0.take_shutdown_events()).unwrap();
        assert!(matches!(events.as_slice(), [DownloadEvent::Updated(snapshot)] if snapshot.info.phase == DownloadPhase::Complete));
        assert!(runtime.borrow().downloads_native_handles_released());
        assert!(runtime.borrow_mut().downloads.0.take_shutdown_events().unwrap().is_empty());
    }

    #[test]
    fn post_complete_owned_dispatch_is_pure_exactly_once_and_running_actions_stay_closed() {
        let (mut ledger, key, origin) = fixture();
        update(&ledger, key, origin, DownloadPhase::Complete, 32);
        ledger.seal_ingress();
        drop(ledger.shutdown());
        assert!(ledger.native_handles_released()); // CEF finalization precondition.
        let calls = Cell::new(0);
        let events = shutdown_owned_dispatch(crate::ShutdownPhase::Complete, true, || {
            calls.set(calls.get() + 1);
            ledger.take_shutdown_events()
        }).unwrap();
        assert!(matches!(events.as_slice(), [DownloadEvent::Updated(snapshot)] if snapshot.info.phase == DownloadPhase::Complete));
        assert_eq!(calls.get(), 1);
        assert!(shutdown_owned_dispatch(crate::ShutdownPhase::Complete, true, || ledger.take_shutdown_events()).unwrap().is_empty());
        for (phase, allowed) in [(crate::ShutdownPhase::Running, true), (crate::ShutdownPhase::ClosingBrowsers, false)] {
            assert!(shutdown_owned_dispatch(phase, allowed, || panic!("reentrant/running drain must not execute")).is_err());
        }
        let id = token(&ledger, key);
        assert_eq!(ledger.control(id, DownloadControl::Resume).err(), Some(DownloadError::Closed));
        assert_eq!(ledger.resolve(id, destination()).err(), Some(DownloadError::Closed));
        assert!(ledger.native_handles_released());
    }

    #[test]
    fn retirement_collects_queued_two_items_then_accepts_known_updates_and_delayed_destination_only() {
        let (mut ledger, first, origin) = fixture();
        let second = Key { native_download: 2, ..first };
        // Callback order at logical close: Update can precede Before.
        update(&ledger, first, origin, DownloadPhase::Pending, 0);
        before(&ledger, second, origin);
        {
            let registration = ledger.registrations.get_mut(&first.registration).unwrap();
            registration.accepting.set(false);
            registration.retiring = true;
        }
        drop(ledger.collect());
        let first_token = token(&ledger, first);
        let second_token = token(&ledger, second);
        let registration = ledger.registrations.get_mut(&first.registration).unwrap();
        registration.retiring = false;
        registration.continuing = true;
        before(&ledger, first, origin); // Known pending item's delayed callback.
        update(&ledger, second, origin, DownloadPhase::Paused, 12);
        drop(ledger.collect());
        let effects = ledger.resolve(first_token, destination()).unwrap();
        assert!(matches!(effects.as_slice(), [Effect::Continue(_, _)]));
        drop(effects);
        assert!(!ledger.entries[&first_token].snapshot.closing);
        assert!(!ledger.entries[&second_token].snapshot.cancel_requested);
        let unknown = Key { native_download: 3, ..first };
        before(&ledger, unknown, origin);
        update(&ledger, unknown, origin, DownloadPhase::InProgress, 1);
        let effects = ledger.collect();
        assert!(effects.iter().any(|e| matches!(e, Effect::Control(_, DownloadControl::Cancel))));
        assert!(!ledger.keys.contains_key(&unknown));
        drop(effects);
        update(&ledger, first, origin, DownloadPhase::Complete, 20);
        update(&ledger, second, origin, DownloadPhase::Interrupted, 12);
        drop(ledger.collect());
        assert!(ledger.entries[&first_token].terminal);
        assert!(!ledger.entries[&second_token].terminal);
        update(&ledger, second, origin, DownloadPhase::Canceled, 12);
        drop(ledger.collect());
        assert!(ledger.entries[&second_token].terminal);
    }

    #[test]
    fn continuation_native_close_is_failure_history_and_exact_context_release_is_deferred() {
        let mut runtime = RuntimeDownloads::<()>::default();
        let context = Rc::new(());
        let weak = Rc::downgrade(&context);
        let prepared = runtime.prepare(context.clone(), DownloadOwner::continue_normal_after_source_close(91), false).unwrap();
        runtime.bind(&prepared.registration, 601).unwrap();
        assert!(runtime.queue_retirement(601));
        let key = Key { registration: prepared.registration.id, native_browser: 601, native_download: 1 };
        runtime.downloads.0.inbox.push(Incoming::Update {
            key, origin: prepared.registration.origin, info: info(DownloadPhase::Pending, 0), callback: None,
        });
        runtime.collect().execute();
        assert_eq!(runtime.transfer_state(601).unfinished, 1);
        runtime.seal_retirement_admission(601);
        assert!(!prepared.registration.accepting.get());
        assert!(prepared.registration.native_alive.get());
        drop(context);
        let release = runtime.native_closed(601);
        assert!(weak.upgrade().is_some());
        assert!(runtime.retirement_history[0].failure.is_some());
        assert!(runtime.retirement_history[0].closed);
        assert!(!runtime.retirement_history[0].host_released);
        let snapshot = &runtime.downloads.0.entries[&runtime.downloads.0.keys[&key]].snapshot;
        assert_eq!(snapshot.info.phase, DownloadPhase::Pending);
        assert!(!snapshot.cancel_requested); // Never invent cancellation/completion.
        release.execute();
        assert!(weak.upgrade().is_none());
        assert!(!prepared.registration.native_alive.get());
    }

    #[test]
    fn copied_complete_then_unexpected_closed_before_drain_preserves_snapshot_and_history() {
        let mut runtime = RuntimeDownloads::<()>::default();
        let prepared = runtime.prepare(Rc::new(()), DownloadOwner::continue_normal_after_source_close(93), false).unwrap();
        runtime.bind(&prepared.registration, 602).unwrap();
        let key = Key { registration: prepared.registration.id, native_browser: 602, native_download: 1 };
        runtime.downloads.0.inbox.push(Incoming::Update {
            key, origin: prepared.registration.origin, info: info(DownloadPhase::InProgress, 12), callback: None,
        });
        runtime.collect().execute();
        runtime.downloads.take_events();
        assert!(runtime.queue_retirement(602));
        runtime.seal_retirement_admission(602);
        runtime.downloads.0.inbox.push(Incoming::Update {
            key, origin: prepared.registration.origin, info: info(DownloadPhase::Complete, 32), callback: None,
        });
        // Exact production Closed-first seam, with Complete copied but undrained.
        let release = runtime.native_closed(602);
        let snapshot = &runtime.downloads.0.entries[&runtime.downloads.0.keys[&key]].snapshot;
        assert_eq!(snapshot.info.phase, DownloadPhase::Complete);
        assert_eq!(snapshot.info.received_bytes, 32);
        assert!(!snapshot.cancel_requested);
        let history = &runtime.retirement_history[0];
        assert_eq!(history.unfinished, 0);
        assert!(history.closed);
        assert!(history.failure.is_some()); // Unexpected close is still reported.
        let events = runtime.downloads.take_events();
        assert!(matches!(events.as_slice(), [DownloadEvent::Updated(snapshot)] if snapshot.info.phase == DownloadPhase::Complete));
        assert!(release.effects.0.iter().all(|effect| matches!(effect, Effect::ReleaseBefore(_) | Effect::ReleaseControl(_))));
        release.execute();
    }

    #[test]
    fn release_only_closed_reducer_preserves_terminal_first_and_never_emits_native_controls() {
        let (mut ledger, key, origin) = fixture();
        let other = Key { native_download: 2, ..key };
        // An already-canceled request must not produce another Cancel from a
        // copied progress update once its native owner has closed.
        update(&ledger, key, origin, DownloadPhase::InProgress, 1);
        drop(ledger.collect());
        drop(ledger.control(token(&ledger, key), DownloadControl::Cancel).unwrap());
        update(&ledger, key, origin, DownloadPhase::Interrupted, 7);
        update(&ledger, other, origin, DownloadPhase::Complete, 32);
        let effects = release_closed_registration(&mut ledger, key.registration);
        assert!(effects.iter().all(|effect| matches!(effect, Effect::ReleaseBefore(_) | Effect::ReleaseControl(_))));
        assert_eq!(ledger.entries[&token(&ledger, key)].snapshot.info.phase, DownloadPhase::Interrupted);
        assert_eq!(ledger.entries[&token(&ledger, other)].snapshot.info.phase, DownloadPhase::Complete);
        assert!(ledger.entries[&token(&ledger, other)].terminal);
        drop(effects);
    }

    #[test]
    fn continuation_is_normal_only_and_app_shutdown_overrides_known_item_gate() {
        let mut runtime = RuntimeDownloads::<()>::default();
        assert_eq!(runtime.prepare(Rc::new(()), DownloadOwner::continue_normal_after_source_close(92), true).err(), Some(DownloadError::UnsupportedPolicy));
        let (mut ledger, key, origin) = fixture();
        update(&ledger, key, origin, DownloadPhase::Paused, 1);
        drop(ledger.collect());
        let state = ledger.registrations.get_mut(&key.registration).unwrap();
        state.accepting.set(false);
        state.continuing = true;
        let effects = ledger.shutdown();
        assert!(effects.iter().any(|e| matches!(e, Effect::Control(_, DownloadControl::Cancel))));
        assert!(ledger.entries[&token(&ledger, key)].snapshot.cancel_requested);
        assert!(!ledger.registrations[&key.registration].continuing);
        drop(effects);
        update(&ledger, key, origin, DownloadPhase::InProgress, 2);
        let effects = ledger.collect();
        assert!(effects.iter().any(|e| matches!(e, Effect::Control(_, DownloadControl::Cancel))));
        assert_eq!(ledger.entries[&token(&ledger, key)].snapshot.info.phase, DownloadPhase::Paused);
    }

    #[test]
    fn update_before_before_preserves_identity_and_requests_destination_once() {
        let (mut ledger, key, origin) = fixture();
        update(&ledger, key, origin, DownloadPhase::InProgress, 12);
        drop(ledger.collect());
        let id = token(&ledger, key);
        before(&ledger, key, origin);
        drop(ledger.collect());
        assert_eq!(token(&ledger, key), id);
        assert_eq!(ledger.entries[&id].snapshot.provenance, origin);
        assert!(matches!(
            ledger.entries[&id].snapshot.destination,
            DownloadDestination::Requested { .. }
        ));
        assert_eq!(
            ledger
                .ready
                .iter()
                .filter(|event| matches!(event, DownloadEvent::DestinationRequested(_)))
                .count(),
            1
        );
    }

    #[test]
    fn destination_is_external_one_shot_and_empty_or_relative_paths_are_rejected() {
        let (mut ledger, key, origin) = fixture();
        let drops = before(&ledger, key, origin);
        drop(ledger.collect());
        let id = token(&ledger, key);
        assert_eq!(
            ledger.resolve(id, PathBuf::new()).err(),
            Some(DownloadError::InvalidDestination)
        );
        assert_eq!(
            ledger.resolve(id, "relative.bin".into()).err(),
            Some(DownloadError::InvalidDestination)
        );
        assert_eq!(drops.get(), 0);
        let effects = ledger.resolve(id, destination()).unwrap();
        assert!(
            matches!(&effects[..], [Effect::Continue(_, path)] if path == destination().to_str().unwrap())
        );
        assert_eq!(drops.get(), 0);
        assert_eq!(
            ledger.resolve(id, destination()).err(),
            Some(DownloadError::DestinationUnavailable)
        );
        drop(effects);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn destination_cancel_releases_before_outside_reducer_and_cancels_later_control() {
        let (mut ledger, key, origin) = fixture();
        let drops = before(&ledger, key, origin);
        drop(ledger.collect());
        let id = token(&ledger, key);
        let effects = ledger.control(id, DownloadControl::Cancel).unwrap();
        assert_eq!(drops.get(), 0);
        assert!(matches!(&effects[..], [Effect::ReleaseBefore(_)]));
        drop(effects);
        assert_eq!(drops.get(), 1);
        update(&ledger, key, origin, DownloadPhase::InProgress, 2);
        let effects = ledger.collect();
        assert!(matches!(
            &effects[..],
            [Effect::Control(_, DownloadControl::Cancel)]
        ));
        assert!(ledger.entries[&id].snapshot.cancel_requested);
        assert!(!ledger.entries[&id].snapshot.info.phase.is_terminal());
    }

    #[test]
    fn coalescing_preserves_destination_terminal_and_per_key_barriers() {
        let (mut ledger, key, origin) = fixture();
        let first = update(&ledger, key, origin, DownloadPhase::InProgress, 1);
        update(&ledger, key, origin, DownloadPhase::InProgress, 2);
        assert_eq!(first.get(), 1);
        before(&ledger, key, origin);
        update(&ledger, key, origin, DownloadPhase::InProgress, 3);
        update(&ledger, key, origin, DownloadPhase::Complete, 4);
        update(&ledger, key, origin, DownloadPhase::InProgress, 5);
        assert_eq!(ledger.inbox.queue.lock().unwrap().len(), 5);
        drop(ledger.collect());
        let snapshot = &ledger.entries[&token(&ledger, key)].snapshot;
        assert_eq!(snapshot.info.phase, DownloadPhase::Complete);
        assert_eq!(snapshot.info.received_bytes, 4);
        assert_eq!(
            ledger
                .ready
                .iter()
                .filter(|event| matches!(event, DownloadEvent::DestinationRequested(_)))
                .count(),
            1
        );
        assert_eq!(ledger.ready.iter().filter(|event| matches!(event, DownloadEvent::Updated(s) if s.info.phase == DownloadPhase::Complete)).count(), 1);
    }

    #[test]
    fn terminal_before_before_cannot_resurrect_destination_or_controls() {
        let (mut ledger, key, origin) = fixture();
        update(&ledger, key, origin, DownloadPhase::Complete, 10);
        drop(ledger.collect());
        let drops = before(&ledger, key, origin);
        let effects = ledger.collect();
        assert!(matches!(&effects[..], [Effect::ReleaseBefore(_)]));
        assert_eq!(drops.get(), 0);
        drop(effects);
        assert_eq!(drops.get(), 1);
        let id = token(&ledger, key);
        assert_eq!(
            ledger.control(id, DownloadControl::Pause).err(),
            Some(DownloadError::Terminal)
        );
        assert_eq!(ledger.ready.len(), 1);
    }

    #[test]
    fn opaque_provenance_and_tokens_do_not_cross_profiles_registrations_or_ledgers() {
        let (mut ledger, key, origin) = fixture();
        before(&ledger, key, origin);
        drop(ledger.collect());
        let id = token(&ledger, key);
        let private = DownloadProvenance::new(11, 21, 31, true);
        let (registration, _) = ledger.register(private).unwrap();
        let other = Key {
            registration,
            ..key
        };
        before(&ledger, other, private);
        drop(ledger.collect());
        assert_ne!(id, token(&ledger, other));
        assert_eq!(ledger.entries[&id].snapshot.provenance, origin);
        assert!(ledger.entries[&token(&ledger, other)]
            .snapshot
            .provenance
            .is_private());
        let rejected = before(&ledger, key, private);
        drop(ledger.collect());
        assert_eq!(rejected.get(), 1);
        let (mut second, second_key, second_origin) = fixture();
        before(&second, second_key, second_origin);
        drop(second.collect());
        assert_eq!(
            second.control(id, DownloadControl::Cancel).err(),
            Some(DownloadError::UnknownToken)
        );
    }

    #[test]
    fn unknown_length_and_interruption_reason_survive_without_invented_progress() {
        let (mut ledger, key, origin) = fixture();
        let mut copied = info(DownloadPhase::Interrupted, 23);
        copied.interrupt_reason = 41;
        ledger.inbox.push(Incoming::Update {
            key,
            origin,
            info: copied.clone(),
            callback: None,
        });
        drop(ledger.collect());
        assert_eq!(ledger.entries[&token(&ledger, key)].snapshot.info, copied);
    }

    #[test]
    fn interrupted_transfer_retains_control_and_resumes_through_progress_to_completion() {
        let (mut ledger, key, origin) = fixture();
        before(&ledger, key, origin);
        drop(ledger.collect());
        let id = token(&ledger, key);
        drop(ledger.resolve(id, destination()).unwrap());
        update(&ledger, key, origin, DownloadPhase::InProgress, 12);
        drop(ledger.collect());
        let interrupted_control = update(&ledger, key, origin, DownloadPhase::Interrupted, 12);
        drop(ledger.collect());
        assert_eq!(interrupted_control.get(), 0);
        assert!(!ledger.entries[&id].terminal);
        assert!(!ledger.entries[&id].snapshot.info.phase.is_terminal());
        assert!(ledger.entries[&id].control.is_some());
        assert!(matches!(
            ledger.entries[&id].snapshot.destination,
            DownloadDestination::Resolved(_)
        ));

        let resume = ledger.control(id, DownloadControl::Resume).unwrap();
        assert!(matches!(
            &resume[..],
            [Effect::Control(_, DownloadControl::Resume)]
        ));
        // A request does not fabricate progress before CEF confirms it.
        assert_eq!(
            ledger.entries[&id].snapshot.info.phase,
            DownloadPhase::Interrupted
        );
        drop(resume);
        assert!(ledger.entries[&id].control.is_some());

        let progress_control = update(&ledger, key, origin, DownloadPhase::InProgress, 24);
        drop(ledger.collect());
        assert_eq!(token(&ledger, key), id);
        assert_eq!(
            ledger.entries[&id].snapshot.info.phase,
            DownloadPhase::InProgress
        );
        assert_eq!(ledger.entries[&id].snapshot.info.received_bytes, 24);
        assert_eq!(interrupted_control.get(), 2);

        let complete_control = update(&ledger, key, origin, DownloadPhase::Complete, 32);
        let terminal_effects = ledger.collect();
        assert!(ledger.entries[&id].terminal);
        assert!(ledger.entries[&id].control.is_none());
        assert_eq!(
            ledger.entries[&id].snapshot.info.phase,
            DownloadPhase::Complete
        );
        assert_eq!(progress_control.get(), 0);
        assert_eq!(complete_control.get(), 0);
        drop(terminal_effects);
        assert_eq!(progress_control.get(), 1);
        assert_eq!(complete_control.get(), 1);
        assert_eq!(
            ledger.control(id, DownloadControl::Resume).err(),
            Some(DownloadError::Terminal)
        );

        update(&ledger, key, origin, DownloadPhase::InProgress, 40);
        drop(ledger.collect());
        assert_eq!(ledger.entries[&id].snapshot.info.received_bytes, 32);
    }

    #[test]
    fn interrupted_transfer_accepts_cancel_and_retains_previous_handle_if_update_has_none() {
        let (mut ledger, key, origin) = fixture();
        let retained = update(&ledger, key, origin, DownloadPhase::InProgress, 12);
        drop(ledger.collect());
        let id = token(&ledger, key);
        ledger.inbox.push(Incoming::Update {
            key,
            origin,
            info: info(DownloadPhase::Interrupted, 12),
            callback: None,
        });
        drop(ledger.collect());
        assert_eq!(retained.get(), 0);
        let cancel = ledger.control(id, DownloadControl::Cancel).unwrap();
        assert!(matches!(
            &cancel[..],
            [Effect::Control(_, DownloadControl::Cancel)]
        ));
        assert!(ledger.entries[&id].snapshot.cancel_requested);
        assert!(!ledger.entries[&id].terminal);
        drop(cancel);
        assert_eq!(retained.get(), 1);

        let later = update(&ledger, key, origin, DownloadPhase::Interrupted, 12);
        let effects = ledger.collect();
        assert!(matches!(
            &effects[..],
            [Effect::Control(_, DownloadControl::Cancel)]
        ));
        drop(effects);
        assert_eq!(later.get(), 1);
        update(&ledger, key, origin, DownloadPhase::Canceled, 12);
        drop(ledger.collect());
        assert!(ledger.entries[&id].terminal);
        assert_eq!(
            ledger.entries[&id].snapshot.info.phase,
            DownloadPhase::Canceled
        );
    }

    #[test]
    fn shutdown_cancels_both_retained_and_queued_interrupted_transfers() {
        let (mut ledger, key, origin) = fixture();
        let retained = update(&ledger, key, origin, DownloadPhase::Interrupted, 12);
        drop(ledger.collect());
        let id = token(&ledger, key);
        let queued_key = Key {
            native_download: 2,
            ..key
        };
        let queued = update(&ledger, queued_key, origin, DownloadPhase::Interrupted, 20);
        let effects = ledger.shutdown();
        assert_eq!(effects.len(), 2);
        assert!(effects
            .iter()
            .all(|effect| matches!(effect, Effect::Control(_, DownloadControl::Cancel))));
        assert_eq!(retained.get(), 0);
        assert_eq!(queued.get(), 0);
        assert!(ledger.entries[&id].snapshot.closing);
        assert!(ledger.entries[&id].snapshot.cancel_requested);
        assert!(!ledger.entries[&id].terminal);
        assert!(ledger.entries[&id].control.is_none());
        assert!(ledger.inbox.queue.lock().unwrap().is_empty());
        drop(effects);
        assert_eq!(retained.get(), 1);
        assert_eq!(queued.get(), 1);
        assert!(ledger.shutdown().is_empty());
    }

    #[test]
    fn controls_are_deferred_and_closing_rejects_new_commands_without_revival() {
        let (mut ledger, key, origin) = fixture();
        update(&ledger, key, origin, DownloadPhase::InProgress, 1);
        drop(ledger.collect());
        let id = token(&ledger, key);
        let pause = ledger.control(id, DownloadControl::Pause).unwrap();
        assert!(matches!(
            &pause[..],
            [Effect::Control(_, DownloadControl::Pause)]
        ));
        let resume = ledger.control(id, DownloadControl::Resume).unwrap();
        assert!(matches!(
            &resume[..],
            [Effect::Control(_, DownloadControl::Resume)]
        ));
        drop((pause, resume));
        let effects = ledger.close_registration(key.registration);
        assert!(matches!(
            &effects[..],
            [Effect::Control(_, DownloadControl::Cancel)]
        ));
        assert_eq!(
            ledger.control(id, DownloadControl::Resume).err(),
            Some(DownloadError::Closed)
        );
        assert!(ledger.close_registration(key.registration).is_empty());
        update(&ledger, key, origin, DownloadPhase::InProgress, 3);
        let late = ledger.collect();
        assert!(matches!(
            &late[..],
            [Effect::Control(_, DownloadControl::Cancel)]
        ));
        assert_eq!(ledger.entries[&id].snapshot.info.received_bytes, 1);
    }

    #[test]
    fn shutdown_releases_pending_and_queued_handles_without_runtime_cycles() {
        let (mut ledger, key, origin) = fixture();
        let pending = before(&ledger, key, origin);
        drop(ledger.collect());
        let queued = update(&ledger, key, origin, DownloadPhase::InProgress, 1);
        let inbox = ledger.inbox.clone();
        let weak = Arc::downgrade(&inbox);
        let effects = ledger.shutdown();
        assert!(inbox.closed.get());
        assert_eq!(pending.get(), 0);
        assert_eq!(queued.get(), 0);
        drop(effects);
        assert_eq!(pending.get(), 1);
        assert_eq!(queued.get(), 1);
        assert!(ledger.shutdown().is_empty());
        assert_eq!(ledger.register(origin).err(), Some(DownloadError::Closed));
        drop(ledger);
        drop(inbox);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn invalid_or_absent_item_produces_no_snapshot_or_destination() {
        // The exact adapter guard must not read any field of an invalid item.
        assert!(copy_valid(false, || -> () { panic!("invalid item was read") }).is_none());
        assert_eq!(copy_valid(true, || 7), Some(7));
        assert!(copy_item(None).is_none());
        let (mut ledger, _, _) = fixture();
        assert!(ledger.collect().is_empty());
        assert!(ledger.entries.is_empty());
    }

    #[test]
    fn admission_policy_explicitly_allows_only_open_permitted_registrations() {
        assert_eq!(i32::from(admission_allowed(true, true, false)), 1);
        assert!(!admission_allowed(false, true, false));
        assert!(!admission_allowed(true, false, false));
        assert!(!admission_allowed(true, true, true));
    }

    #[test]
    fn pause_and_resume_snapshots_are_not_coalesced_as_byte_progress() {
        let (mut ledger, key, origin) = fixture();
        update(&ledger, key, origin, DownloadPhase::InProgress, 1);
        update(&ledger, key, origin, DownloadPhase::Paused, 1);
        update(&ledger, key, origin, DownloadPhase::InProgress, 2);
        update(&ledger, key, origin, DownloadPhase::InProgress, 3);
        assert_eq!(ledger.inbox.queue.lock().unwrap().len(), 3);
        drop(ledger.collect());
        let phases: Vec<_> = ledger
            .ready
            .iter()
            .filter_map(|event| match event {
                DownloadEvent::Updated(snapshot) => Some(snapshot.info.phase),
                _ => None,
            })
            .collect();
        assert_eq!(
            phases,
            [
                DownloadPhase::InProgress,
                DownloadPhase::Paused,
                DownloadPhase::InProgress
            ]
        );
    }

    #[test]
    fn registration_cannot_rebind_to_a_different_native_browser() {
        let (mut ledger, key, origin) = fixture();
        before(&ledger, key, origin);
        drop(ledger.collect());
        let wrong = Key {
            native_browser: 99,
            ..key
        };
        let drops = before(&ledger, wrong, origin);
        drop(ledger.collect());
        assert_eq!(drops.get(), 1);
        assert!(!ledger.keys.contains_key(&wrong));
        assert_eq!(ledger.entries.len(), 1);
    }

    #[test]
    fn coalesced_progress_without_handle_retains_previous_control() {
        let (mut ledger, key, origin) = fixture();
        let drops = update(&ledger, key, origin, DownloadPhase::InProgress, 1);
        ledger.inbox.push(Incoming::Update {
            key,
            origin,
            info: info(DownloadPhase::InProgress, 2),
            callback: None,
        });
        assert_eq!(ledger.inbox.queue.lock().unwrap().len(), 1);
        assert_eq!(drops.get(), 0);
        drop(ledger.collect());
        let id = token(&ledger, key);
        let effects = ledger.control(id, DownloadControl::Cancel).unwrap();
        assert!(matches!(
            &effects[..],
            [Effect::Control(_, DownloadControl::Cancel)]
        ));
        drop(effects);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn coalescing_and_terminal_release_do_not_hold_inbox_borrows() {
        #[derive(Clone)]
        struct Reentrant(Rc<dyn Fn()>);
        impl Drop for Reentrant {
            fn drop(&mut self) {
                (self.0)();
            }
        }

        let mut ledger = Ledger::<(), Reentrant>::default();
        let origin = DownloadProvenance::new(1, 2, 3, false);
        let (registration, _) = ledger.register(origin).unwrap();
        let key = Key {
            registration,
            native_browser: 1,
            native_download: 1,
        };
        let weak = Arc::downgrade(&ledger.inbox);
        let drops = Rc::new(Cell::new(0));
        let callback = || {
            let weak = weak.clone();
            let drops = drops.clone();
            Reentrant(Rc::new(move || {
                let inbox = weak.upgrade().expect("inbox lifetime");
                assert!(inbox.queue.try_lock().is_ok());
                drops.set(drops.get() + 1);
            }))
        };
        ledger.inbox.push(Incoming::Update {
            key,
            origin,
            info: info(DownloadPhase::InProgress, 1),
            callback: Some(callback()),
        });
        ledger.inbox.push(Incoming::Update {
            key,
            origin,
            info: info(DownloadPhase::InProgress, 2),
            callback: Some(callback()),
        });
        assert_eq!(drops.get(), 1);
        drop(ledger.collect());
        ledger.inbox.push(Incoming::Update {
            key,
            origin,
            info: info(DownloadPhase::Complete, 2),
            callback: Some(callback()),
        });
        let effects = ledger.collect();
        assert_eq!(drops.get(), 1);
        drop(effects);
        assert_eq!(drops.get(), 3);
        drop(ledger);
    }

    #[test]
    fn native_ingress_can_be_released_across_threads_without_unsafe_impls() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Inbox<NativeBefore, NativeControl>>();
    }
    #[test]
    fn runtime_registrations_capture_shared_storage_and_distinct_browser_generations() {
        let mut runtime = RuntimeDownloads::<()>::default();
        let context = Rc::new(());
        let owner = DownloadOwner::cancel_before_native_close(17);
        let first = runtime.prepare(context.clone(), owner, false).unwrap();
        let second = runtime.prepare(context.clone(), owner, false).unwrap();
        assert_eq!(first.registration.origin.owner(), 17);
        assert_eq!(
            first.registration.origin.storage(),
            second.registration.origin.storage()
        );
        assert_ne!(
            first.registration.origin.browser(),
            second.registration.origin.browser()
        );
        assert!(!first.registration.origin.is_private());
        assert!(Rc::ptr_eq(
            &runtime.browsers[&first.registration.id].context,
            &context
        ));
        runtime.bind(&first.registration, 100).unwrap();
        runtime.bind(&second.registration, 101).unwrap();
        assert_eq!(
            runtime.bind(&first.registration, 102),
            Err(DownloadError::Closed)
        );
        runtime.shutdown().execute();
    }

    #[test]
    fn runtime_keeps_exact_private_context_after_view_retirement_until_native_close() {
        struct Context(Rc<Cell<usize>>);
        impl Drop for Context {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let context = Rc::new(Context(drops.clone()));
        let weak = Rc::downgrade(&context);
        let mut runtime = RuntimeDownloads::default();
        let prepared = runtime
            .prepare(
                context.clone(),
                DownloadOwner::cancel_before_native_close(19),
                true,
            )
            .unwrap();
        runtime.bind(&prepared.registration, 103).unwrap();
        let key = Key {
            registration: prepared.registration.id,
            native_browser: 103,
            native_download: 1,
        };
        runtime.downloads.0.inbox.push(Incoming::Update {
            key,
            origin: prepared.registration.origin,
            info: info(DownloadPhase::Interrupted, 12),
            callback: None,
        });
        runtime.collect().execute();
        let id = runtime.downloads.0.keys[&key];
        drop(context); // Source view/pool drops its exact registry Rc.
        assert!(weak.upgrade().is_some());
        runtime.before_native_close(103).execute();
        assert!(weak.upgrade().is_some());
        assert!(runtime.downloads.0.entries[&id].snapshot.cancel_requested);
        assert!(runtime.downloads.0.entries[&id]
            .snapshot
            .provenance
            .is_private());
        let release = runtime.native_closed(103);
        assert_eq!(drops.get(), 0); // Lease is moved out of the state borrow.
        release.execute();
        assert_eq!(drops.get(), 1);
        assert!(weak.upgrade().is_none());
        assert!(runtime.browsers.is_empty());
    }

    #[test]
    fn runtime_bind_rejects_callbacks_for_another_native_browser_without_new_tokens() {
        let mut runtime = RuntimeDownloads::<()>::default();
        let prepared = runtime
            .prepare(
                Rc::new(()),
                DownloadOwner::cancel_before_native_close(21),
                false,
            )
            .unwrap();
        runtime.bind(&prepared.registration, 104).unwrap();
        let wrong = Key {
            registration: prepared.registration.id,
            native_browser: 105,
            native_download: 1,
        };
        runtime.downloads.0.inbox.push(Incoming::Update {
            key: wrong,
            origin: prepared.registration.origin,
            info: info(DownloadPhase::InProgress, 1),
            callback: None,
        });
        runtime.collect().execute();
        assert!(runtime.downloads.0.keys.is_empty());
        assert!(runtime.downloads.0.ready.is_empty());
        runtime.shutdown().execute();
    }

    #[test]
    fn failed_creation_defers_exact_context_release_and_does_not_affect_other_browser() {
        let mut runtime = RuntimeDownloads::<()>::default();
        let failed_context = Rc::new(());
        let weak = Rc::downgrade(&failed_context);
        let failed = runtime
            .prepare(
                failed_context.clone(),
                DownloadOwner::cancel_before_native_close(23),
                false,
            )
            .unwrap();
        let live = runtime
            .prepare(
                Rc::new(()),
                DownloadOwner::cancel_before_native_close(24),
                false,
            )
            .unwrap();
        runtime.bind(&live.registration, 106).unwrap();
        let live_key = Key {
            registration: live.registration.id,
            native_browser: 106,
            native_download: 2,
        };
        runtime.downloads.0.inbox.push(Incoming::Update {
            key: live_key,
            origin: live.registration.origin,
            info: info(DownloadPhase::InProgress, 3),
            callback: None,
        });
        runtime.creation_failed(&failed.registration);
        drop(failed_context);
        assert!(weak.upgrade().is_some());
        assert!(!failed.registration.accepting.get());
        let work = runtime.collect();
        assert!(!runtime.browsers.contains_key(&failed.registration.id));
        assert!(runtime.browsers.contains_key(&live.registration.id));
        assert!(runtime.downloads.0.keys.contains_key(&live_key));
        assert!(weak.upgrade().is_some());
        work.execute();
        assert!(weak.upgrade().is_none());
        runtime.shutdown().execute();
    }

    #[test]
    fn runtime_shutdown_seals_all_ingress_and_moves_contexts_out_before_owner_barrier() {
        let mut runtime = RuntimeDownloads::<()>::default();
        let context = Rc::new(());
        let weak = Rc::downgrade(&context);
        let first = runtime
            .prepare(
                context.clone(),
                DownloadOwner::cancel_before_native_close(25),
                false,
            )
            .unwrap();
        let second = runtime
            .prepare(
                context.clone(),
                DownloadOwner::cancel_before_native_close(25),
                false,
            )
            .unwrap();
        runtime.bind(&first.registration, 107).unwrap();
        runtime.bind(&second.registration, 108).unwrap();
        drop(context);
        runtime.seal();
        assert!(!first.registration.accepting.get());
        assert!(!second.registration.accepting.get());
        assert!(runtime.downloads.0.inbox.closed.get());
        assert!(weak.upgrade().is_some());
        let work = runtime.shutdown();
        assert!(runtime.browsers.is_empty());
        assert!(weak.upgrade().is_some());
        work.execute();
        assert!(weak.upgrade().is_none());
        assert_eq!(
            runtime
                .prepare(
                    Rc::new(()),
                    DownloadOwner::cancel_before_native_close(26),
                    false
                )
                .err(),
            Some(DownloadError::Closed)
        );
        runtime.shutdown().execute();
    }

    #[test]
    fn late_native_close_releases_only_its_context_and_preserves_other_owner_events() {
        let mut runtime = RuntimeDownloads::<()>::default();
        let first = runtime
            .prepare(
                Rc::new(()),
                DownloadOwner::cancel_before_native_close(27),
                false,
            )
            .unwrap();
        let second = runtime
            .prepare(
                Rc::new(()),
                DownloadOwner::cancel_before_native_close(28),
                true,
            )
            .unwrap();
        runtime.bind(&first.registration, 109).unwrap();
        runtime.bind(&second.registration, 110).unwrap();
        for prepared in [&first, &second] {
            let native = runtime.browsers[&prepared.registration.id].native.unwrap();
            runtime.downloads.0.inbox.push(Incoming::Update {
                key: Key {
                    registration: prepared.registration.id,
                    native_browser: native,
                    native_download: 1,
                },
                origin: prepared.registration.origin,
                info: info(DownloadPhase::Interrupted, 1),
                callback: None,
            });
        }
        runtime.native_closed(109).execute();
        runtime.collect().execute();
        let events = runtime.downloads.take_events();
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DownloadEvent::Updated(snapshot)
            if snapshot.provenance.owner() == 27 && snapshot.info.phase == DownloadPhase::Interrupted && snapshot.closing));
        assert!(
            matches!(&events[1], DownloadEvent::Updated(snapshot) if snapshot.provenance.owner() == 28 && snapshot.provenance.is_private())
        );
        assert!(runtime.browsers.contains_key(&second.registration.id));
        assert!(!runtime.browsers.contains_key(&first.registration.id));
        runtime.shutdown().execute();
    }
}

#[cfg(test)]
mod closure_order_tests {
    use super::*;
    use crate::shutdown::{Callback, Lifecycle};
    use std::cell::RefCell;

    #[derive(Clone)]
    struct Probe {
        native: i32,
        trace: Rc<RefCell<Vec<String>>>,
        released_outside_borrows: Rc<dyn Fn()>,
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            (self.released_outside_borrows)();
        }
    }
    struct ContextProbe {
        drops: Rc<Cell<usize>>,
        verify: Rc<dyn Fn()>,
    }
    impl Drop for ContextProbe {
        fn drop(&mut self) {
            (self.verify)();
            self.drops.set(self.drops.get() + 1);
        }
    }
    struct Owned {
        registration: u64,
        alive: Arc<Gate>,
        context: Rc<ContextProbe>,
    }
    #[derive(Default)]
    struct TestRuntime {
        ledger: Ledger<(), NativeCallback<Probe>>,
        owners: HashMap<i32, Owned>,
    }
    struct Owner {
        key: Key,
        origin: DownloadProvenance,
        admission: Arc<Gate>,
        alive: Arc<Gate>,
        context: std::rc::Weak<ContextProbe>,
        drops: Rc<Cell<usize>>,
        verify: Rc<dyn Fn()>,
    }
    fn add_owner(runtime: &Rc<RefCell<TestRuntime>>, lifecycle: &Lifecycle, native: i32) -> Owner {
        let weak = Rc::downgrade(runtime);
        let queue = lifecycle.queue.clone();
        let verify: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(runtime) = weak.upgrade() {
                assert!(
                    runtime.try_borrow_mut().is_ok(),
                    "native/context release under runtime borrow"
                );
            }
            assert!(
                queue.try_lock().is_ok(),
                "native/context release under lifecycle lock"
            );
        });
        let drops = Rc::new(Cell::new(0));
        let context = Rc::new(ContextProbe {
            drops: drops.clone(),
            verify: verify.clone(),
        });
        let weak_context = Rc::downgrade(&context);
        let origin = DownloadProvenance::new(native as u64, native as u64, native as u64, false);
        let mut state = runtime.borrow_mut();
        let (registration, admission) = state.ledger.register(origin).unwrap();
        let alive = Arc::new(Gate::new(true));
        state.owners.insert(
            native,
            Owned {
                registration,
                alive: alive.clone(),
                context,
            },
        );
        Owner {
            key: Key {
                registration,
                native_browser: native,
                native_download: 1,
            },
            origin,
            admission,
            alive,
            context: weak_context,
            drops,
            verify,
        }
    }
    fn update(
        runtime: &Rc<RefCell<TestRuntime>>,
        owner: &Owner,
        phase: DownloadPhase,
        trace: &Rc<RefCell<Vec<String>>>,
    ) {
        let info = DownloadInfo {
            phase,
            interrupt_reason: 41,
            current_speed: 1,
            percent_complete: -1,
            total_bytes: -1,
            received_bytes: 12,
            start_time: 1,
            end_time: 0,
            full_path: PathBuf::new(),
            url: "https://fixture.test/file".into(),
            original_url: String::new(),
            suggested_file_name: "file.bin".into(),
            content_disposition: String::new(),
            mime_type: String::new(),
        };
        let callback = NativeCallback {
            value: Probe {
                native: owner.key.native_browser,
                trace: trace.clone(),
                released_outside_borrows: owner.verify.clone(),
            },
            alive: owner.alive.clone(),
        };
        let inbox = runtime.borrow().ledger.inbox.clone();
        inbox.push(Incoming::Update {
            key: owner.key,
            origin: owner.origin,
            info,
            callback: Some(callback),
        });
    }
    fn run(effects: Vec<Effect<(), NativeCallback<Probe>>>) {
        for effect in effects {
            match effect {
                Effect::Control(callback, command) => callback.when_alive(|probe| {
                    (probe.released_outside_borrows)();
                    probe
                        .trace
                        .borrow_mut()
                        .push(format!("{command:?}:{}", probe.native));
                }),
                Effect::ReleaseControl(callback) => drop(callback),
                Effect::ReleaseBefore(()) => (),
                Effect::Continue((), _) => panic!("no destination is selected by ordering tests"),
            }
        }
    }
    fn queue_close(owner: &Owner, lifecycle: &Lifecycle) {
        // Exact OnBeforeClose order: native liveness/admission seal, then Closed.
        owner.alive.set(false);
        owner.admission.set(false);
        lifecycle
            .queue
            .lock()
            .unwrap()
            .push(Callback::Closed(owner.key.native_browser));
    }
    fn release(runtime: &Rc<RefCell<TestRuntime>>, native: i32) {
        let (effects, context) = {
            let mut state = runtime.borrow_mut();
            let Some(owner) = state.owners.remove(&native) else {
                return;
            };
            owner.alive.set(false);
            let effects = release_closed_registration(&mut state.ledger, owner.registration);
            (effects, owner.context)
        };
        run(effects);
        drop(context);
    }
    fn shutdown(runtime: &Rc<RefCell<TestRuntime>>) {
        let (effects, contexts) = {
            let mut state = runtime.borrow_mut();
            let effects = state.ledger.shutdown();
            let contexts: Vec<_> = state
                .owners
                .drain()
                .map(|(_, owner)| owner.context)
                .collect();
            (effects, contexts)
        };
        run(effects);
        drop(contexts);
    }

    #[test]
    fn shutdown_complete_then_closed_same_pump_preserves_owned_copy_and_context_release_order() {
        let lifecycle = Lifecycle::default();
        let runtime = Rc::new(RefCell::new(TestRuntime::default()));
        let trace = Rc::new(RefCell::new(Vec::new()));
        let owner = add_owner(&runtime, &lifecycle, 211);
        update(&runtime, &owner, DownloadPhase::InProgress, &trace);
        let work = runtime.borrow_mut().ledger.collect();
        run(work);
        runtime.borrow_mut().ledger.ready.clear();
        runtime.borrow_mut().ledger.seal_ingress();
        assert!(runtime.borrow().ledger.inbox.copy_shutdown_terminal(owner.key, owner.origin,
            DownloadInfo { phase: DownloadPhase::Complete, received_bytes: 32,
                ..runtime.borrow().ledger.entries.values().next().unwrap().snapshot.info.clone() }));
        queue_close(&owner, &lifecycle);
        lifecycle.after_closed_with(|native| release(&runtime, native), || {
            assert!(runtime.try_borrow_mut().is_ok());
            assert_eq!(owner.drops.get(), 1);
            assert!(owner.context.upgrade().is_none());
            assert!(runtime.borrow().ledger.native_handles_released());
        });
        let events = shutdown_owned_dispatch(crate::ShutdownPhase::Complete, true, || runtime.borrow_mut().ledger.take_shutdown_events()).unwrap();
        assert!(matches!(events.as_slice(), [DownloadEvent::Updated(snapshot)]
            if snapshot.info.phase == DownloadPhase::Complete && snapshot.info.received_bytes == 32));
        assert!(trace.borrow().is_empty());
        assert!(runtime.borrow_mut().ledger.take_shutdown_events().unwrap().is_empty());
    }

    #[test]
    fn queued_active_or_interrupted_update_then_closed_is_release_only_before_pump_collection() {
        for phase in [DownloadPhase::InProgress, DownloadPhase::Interrupted] {
            let lifecycle = Lifecycle::default();
            let runtime = Rc::new(RefCell::new(TestRuntime::default()));
            let trace = Rc::new(RefCell::new(Vec::new()));
            let closed = add_owner(&runtime, &lifecycle, 201);
            let live = add_owner(&runtime, &lifecycle, 202);
            update(&runtime, &closed, phase, &trace);
            update(&runtime, &live, phase, &trace);
            queue_close(&closed, &lifecycle);
            lifecycle
                .queue
                .lock()
                .unwrap()
                .push(Callback::CookieReady(9));

            // Same production ordering used by pump_once/after_closed. The
            // injected sink replaces only native CEF ownership with pure probes.
            lifecycle.after_closed_with(
                |native| release(&runtime, native),
                || {
                    assert_eq!(closed.drops.get(), 1);
                    assert!(closed.context.upgrade().is_none());
                    let effects = runtime.borrow_mut().ledger.collect();
                    assert!(
                        effects.is_empty(),
                        "closed update was reduced into a cancel effect"
                    );
                    run(effects);
                },
            );
            assert!(trace.borrow().is_empty());
            let closed_token = runtime.borrow().ledger.keys[&closed.key];
            {
                let state = runtime.borrow();
                let entry = &state.ledger.entries[&closed_token];
                assert_eq!(entry.snapshot.info.phase, phase);
                assert_eq!(entry.snapshot.info.received_bytes, 12);
                assert!(entry.snapshot.closing);
                assert!(!entry.snapshot.cancel_requested);
                assert!(!entry.terminal);
                assert!(entry.before.is_none() && entry.control.is_none());
                assert!(!state.owners.contains_key(&201));
            }
            assert_eq!(runtime.borrow_mut().ledger.control(closed_token, DownloadControl::Resume).err(), Some(DownloadError::Closed));
            let token = runtime.borrow().ledger.keys[&live.key];
            let effects = runtime
                .borrow_mut()
                .ledger
                .control(token, DownloadControl::Pause)
                .unwrap();
            run(effects);
            assert_eq!(*trace.borrow(), vec!["Pause:202".to_owned()]);
            assert_eq!(live.drops.get(), 0);
            assert!(matches!(
                &lifecycle.queue.lock().unwrap()[..],
                [Callback::CookieReady(9)]
            ));
            shutdown(&runtime);
            assert_eq!(live.drops.get(), 1);
            assert!(trace.borrow().iter().all(|entry| !entry.ends_with(":201")));
        }
    }

    #[test]
    fn retained_control_then_queued_closed_is_released_before_shutdown_cancels_unrelated_owner() {
        let lifecycle = Lifecycle::default();
        let runtime = Rc::new(RefCell::new(TestRuntime::default()));
        let trace = Rc::new(RefCell::new(Vec::new()));
        let closed = add_owner(&runtime, &lifecycle, 203);
        let live = add_owner(&runtime, &lifecycle, 204);
        update(&runtime, &closed, DownloadPhase::Interrupted, &trace);
        update(&runtime, &live, DownloadPhase::Interrupted, &trace);
        let effects = runtime.borrow_mut().ledger.collect();
        run(effects);
        assert!(runtime
            .borrow()
            .ledger
            .entries
            .values()
            .all(|entry| entry.control.is_some()));
        queue_close(&closed, &lifecycle);
        lifecycle.after_closed_with(
            |native| release(&runtime, native),
            || {
                assert_eq!(closed.drops.get(), 1);
                shutdown(&runtime);
                // A new native close submission follows cancellation, never leads it.
                trace.borrow_mut().push("SubmitClose:204".into());
            },
        );
        assert_eq!(
            *trace.borrow(),
            vec!["Cancel:204".to_owned(), "SubmitClose:204".to_owned()]
        );
        assert_eq!(closed.drops.get(), 1);
        assert_eq!(live.drops.get(), 1);
        assert!(runtime.borrow().owners.is_empty());
        assert!(runtime
            .borrow()
            .ledger
            .entries
            .values()
            .all(|entry| entry.control.is_none()));
    }

    #[test]
    fn already_prepared_native_control_effect_does_not_call_owner_closed_before_execution() {
        let lifecycle = Lifecycle::default();
        let runtime = Rc::new(RefCell::new(TestRuntime::default()));
        let trace = Rc::new(RefCell::new(Vec::new()));
        let owner = add_owner(&runtime, &lifecycle, 205);
        update(&runtime, &owner, DownloadPhase::InProgress, &trace);
        let effects = runtime.borrow_mut().ledger.collect();
        run(effects);
        let token = runtime.borrow().ledger.keys[&owner.key];
        let effects = runtime
            .borrow_mut()
            .ledger
            .control(token, DownloadControl::Resume)
            .unwrap();
        queue_close(&owner, &lifecycle);
        run(effects); // Same generic liveness guard used by actual CEF effects.
        assert!(trace.borrow().is_empty());
        lifecycle.after_closed_with(|native| release(&runtime, native), || shutdown(&runtime));
        assert_eq!(owner.drops.get(), 1);
        assert!(trace.borrow().is_empty());
    }
}

/// Observe the release separately from Closed: lifecycle drops exact clip and
/// independent parent only after its callback dispatcher has returned.
pub(crate) fn native_lease_released(native: i32) {
    if let Some(runtime) = current_runtime() {
        if let Some(status) = runtime.downloads.borrow_mut().retirement_history.iter_mut().rev()
            .find(|status| status.native_browser == native && status.closed) {
            status.host_released = true;
        }
    }
}

fn progress_retirements(runtime: &crate::Runtime) -> Result<crate::DownloadRetirementPreparation, String> {
    use crate::retirement::Action;
    let Some(_guard) = runtime.lifecycle.preparing_retirements() else {
        return Ok(crate::DownloadRetirementPreparation::Pending);
    };
    // App quit overrides continuation; existing shutdown barriers own closure.
    if runtime.downloads.borrow().downloads.0.inbox.closed.get() {
        return Ok(crate::DownloadRetirementPreparation::Ready);
    }
    let ids = runtime.downloads.borrow().retirement_ids();
    let mut failures = Vec::new();
    for native in ids {
        // All inbox work preceding the active decision is collected first.
        let work = runtime.downloads.borrow_mut().collect();
        work.execute();
        let (mut retirement, transfers, quiescence) = {
            let downloads = runtime.downloads.borrow();
            let Some(browser) = downloads.browsers.values().find(|b| b.native == Some(native)) else { continue; };
            (downloads.retirement(native), downloads.transfer_state(native), browser.registration.quiescence.clone())
        };
        if let Some(failure) = &retirement.failure { failures.push(failure.clone()); continue; }
        if retirement.close_requested { continue; }
        let Some(browser) = runtime.lifecycle.retirement_browser(native) else {
            retirement.failure = Some(format!("Retiring native browser {native} lost its lifecycle lease"));
            runtime.downloads.borrow_mut().set_retirement(native, retirement);
            failures.push(format!("Retiring browser {native} is unavailable"));
            continue;
        };
        let result = (|| -> Result<(), String> {
            if browser.is_valid() == 0 { return Err(format!("Retiring browser {native} closed unexpectedly")); }
            // Empty/terminal sources use the ordinary close path immediately;
            // no native continuation host is needed (even on unsupported hosts).
            if retirement.next(transfers, browser.is_loading() != 0, false) == Action::Close {
                retirement.close_requested = true;
                runtime.lifecycle.request_close(native);
                return Ok(());
            }
            let host = browser.host().ok_or("Retiring browser has no native host")?;
            host.set_audio_muted(1);
            if browser.is_valid() == 0 || host.is_audio_muted() == 0 {
                return Err("Cannot verify retired browser audio mute".into());
            }
            // Active and pending initiators move the original native host
            // synchronously before the caller can destroy its GPUI parent.
            if !retirement.parked {
                runtime.lifecycle.park_download_browser(native)?;
                retirement.parked = true;
            }
            if browser.is_valid() == 0 { return Err("Retiring browser closed during native handoff".into()); }
            if !transfers.associated && quiescence.initiator_failed.load(Ordering::Acquire) {
                return Err("Retired source initiating navigation failed before any download association".into());
            }
            if transfers.associated || browser.is_loading() == 0 {
                runtime.downloads.borrow_mut().seal_retirement_admission(native);
            }
            let frame = browser.main_frame();
            let blank = frame.as_ref().is_some_and(|frame| CefString::from(&frame.url()).to_string() == "about:blank");
            if quiescence.failed.load(Ordering::Acquire) {
                return Err("Retirement about:blank navigation failed; continuation is not accepted".into());
            }
            if retirement.blank_requested && quiescence.complete.load(Ordering::Acquire) && blank && browser.is_loading() == 0 {
                retirement.blank_verified = true;
            }
            match retirement.next(transfers, browser.is_loading() != 0, blank) {
                Action::Blank => {
                    let frame = frame.ok_or("Retiring browser has no main frame for document quiescence")?;
                    quiescence.requested.store(true, Ordering::Release);
                    retirement.blank_requested = true;
                    // Never StopLoad: an unresolved initiating request must not
                    // be aborted. Every known unfinished destination is accepted.
                    frame.load_url(Some(&CefString::from("about:blank")));
                }
                Action::Close => {
                    retirement.close_requested = true;
                    runtime.lifecycle.request_close(native);
                }
                Action::Park | Action::Wait | Action::Failed => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            retirement.failure = Some(error.clone());
            failures.push(error);
        }
        runtime.downloads.borrow_mut().set_retirement(native, retirement);
    }
    if !failures.is_empty() { return Err(failures.join("; ")); }
    Ok(crate::DownloadRetirementPreparation::Ready)
}

impl crate::Engine {
    /// No runtime/no queued work is Ready. Call OUTSIDE all App/model/engine
    /// borrows, BEFORE destroying a source GPUI parent. Native parking is
    /// synchronous. Ready permits parent destruction, not a native parity claim.
    pub fn prepare_download_retirements() -> Result<crate::DownloadRetirementPreparation, String> {
        crate::shutdown::check_thread()?;
        let Some(runtime) = current_runtime() else { return Ok(crate::DownloadRetirementPreparation::Ready); };
        if runtime.lifecycle.is_preparing_retirements() {
            return Ok(crate::DownloadRetirementPreparation::Pending);
        }
        if runtime.downloads.borrow().retirement_ids().is_empty() {
            return Ok(crate::DownloadRetirementPreparation::Ready);
        }
        if !runtime.lifecycle.download_dispatch_allowed() {
            return Err("Download retirement cannot run inside CEF creation, pumping or shutdown polling".into());
        }
        runtime.lifecycle.collect_closed();
        let work = runtime.downloads.borrow_mut().collect();
        work.execute();
        progress_retirements(&runtime)
    }

    /// Pure scheduling hint: no pumping, callback collection or CEF calls.
    /// A central app service must keep pumping while true, including without
    /// source Engine/GPUI owners, and independently drain take_download_events.
    pub fn download_work_pending() -> bool {
        current_runtime().is_some_and(|runtime| {
            let downloads = runtime.downloads.borrow();
            let queued = !downloads.downloads.0.inbox.queue.lock().unwrap().is_empty();
            queued || !downloads.downloads.0.ready.is_empty()
                || !downloads.failed.is_empty()
                || downloads.browsers.values().any(|b| b.retirement.is_some())
                || downloads.downloads.0.entries.values().any(|e| !e.terminal && !e.snapshot.closing)
        })
    }

    /// Pure diagnostics; retains neither native handles nor Runtime owners.
    /// Closed history is process-local, with original immutable provenance.
    pub fn download_retirement_status() -> Vec<crate::DownloadRetirementStatus> {
        let Some(runtime) = current_runtime() else { return Vec::new(); };
        let downloads = runtime.downloads.borrow();
        let mut statuses = downloads.retirement_history.clone();
        for browser in downloads.browsers.values() {
            if let (Some(native), Some(retirement)) = (browser.native, &browser.retirement) {
                statuses.push(retirement.status(native, browser.registration.origin, downloads.transfer_state(native)));
            }
        }
        statuses.sort_by_key(|s| s.native_browser);
        statuses
    }
}
