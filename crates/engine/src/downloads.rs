//! Compile-registered download foundation; runtime wiring remains pending.
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
use std::collections::HashMap;
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
}

impl<B, C> Default for Inbox<B, C> {
    fn default() -> Self {
        Self {
            closed: Gate::new(false),
            queue: Mutex::new(Vec::new()),
        }
    }
}

impl<B, C> Inbox<B, C> {
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
            },
        );
        Ok((id, accepting))
    }

    fn collect(&mut self) -> Vec<Effect<B, C>> {
        let mut effects = Vec::new();
        for incoming in self.inbox.take() {
            let key = incoming.key();
            let origin = match &incoming {
                Incoming::Before { origin, .. } | Incoming::Update { origin, .. } => *origin,
            };
            let accepted = !self.stopped
                && self
                    .registrations
                    .get_mut(&key.registration)
                    .is_some_and(|registration| {
                        if registration.origin != origin || !registration.accepting.get() {
                            return false;
                        }
                        *registration
                            .native_browser
                            .get_or_insert(key.native_browser)
                            == key.native_browser
                    });
            if !accepted {
                Self::reject(incoming, &mut effects);
                continue;
            }
            let token = if let Some(token) = self.keys.get(&key) {
                *token
            } else {
                let token = DownloadToken {
                    ledger: self.id,
                    sequence: self.next_token,
                };
                self.next_token = self
                    .next_token
                    .checked_add(1)
                    .expect("download identity exhausted");
                let info = match &incoming {
                    Incoming::Before { info, .. } | Incoming::Update { info, .. } => info.clone(),
                };
                self.keys.insert(key, token);
                self.entries.insert(
                    token,
                    Entry {
                        key,
                        terminal: false,
                        before: None,
                        control: None,
                        snapshot: DownloadSnapshot {
                            token,
                            provenance: origin,
                            info,
                            destination: DownloadDestination::Unrequested,
                            cancel_requested: false,
                            closing: false,
                        },
                    },
                );
                token
            };
            let entry = self.entries.get_mut(&token).unwrap();
            // The first event may itself be terminal. Existing terminal entries
            // have already emitted Updated; their tombstone prevents revival.
            if entry.terminal {
                Self::release(incoming, &mut effects);
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
        if let Some(registration) = self.registrations.get(&registration) {
            registration.accepting.set(false);
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
        self.inbox.closed.set(true);
        let mut effects = Vec::new();
        for id in self.registrations.keys().copied().collect::<Vec<_>>() {
            effects.extend(self.close_registration(id));
        }
        self.stopped = true;
        for incoming in self.inbox.take() {
            Self::reject(incoming, &mut effects);
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
    Vec<Effect<BeforeDownloadCallback, DownloadItemCallback>>,
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
                Effect::Continue(callback, path) => {
                    callback.cont(Some(&CefString::from(path.as_str())), 0)
                }
                Effect::Control(callback, DownloadControl::Cancel) => callback.cancel(),
                Effect::Control(callback, DownloadControl::Pause) => callback.pause(),
                Effect::Control(callback, DownloadControl::Resume) => callback.resume(),
                Effect::ReleaseBefore(callback) => drop(callback),
                Effect::ReleaseControl(callback) => drop(callback),
            }
        }
    }
}

/// Engine-owned main-thread ledger, independent of views and tab retirement.
/// It intentionally has no Serialize implementation, including for private data.
#[derive(Default)]
pub struct Downloads(Ledger<BeforeDownloadCallback, DownloadItemCallback>);

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
pub struct DownloadRegistration {
    ledger: u64,
    _main_thread: PhantomData<Rc<()>>,
    id: u64,
    origin: DownloadProvenance,
    accepting: Arc<Gate>,
    allow_downloads: bool,
    inbox: Arc<Inbox<BeforeDownloadCallback, DownloadItemCallback>>,
}

impl DownloadRegistration {
    /// Only the engine's Client implementation should call this CEF-facing hook.
    pub(crate) fn handler(&self) -> DownloadHandler {
        NativeDownloadHandler::new(
            self.id,
            self.origin,
            self.accepting.clone(),
            self.allow_downloads,
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
        inbox: Arc<Inbox<BeforeDownloadCallback, DownloadItemCallback>>,
    }
    impl DownloadHandler {
        fn can_download(&self, _browser: Option<&mut Browser>, _url: Option<&CefString>, _request_method: Option<&CefString>) -> i32 {
            // Rust's generated default is 0, unlike the C++ header's true.
            i32::from(admission_allowed(self.allow_downloads, self.accepting.get(), self.inbox.closed.get()))
        }
        fn on_before_download(&self, browser: Option<&mut Browser>, download_item: Option<&mut DownloadItem>, suggested_name: Option<&CefString>, callback: Option<&mut BeforeDownloadCallback>) -> i32 {
            // Always claim handling: false delegates to Alloy's cancellation.
            // Rejected/invalid requests drop the callback without continuing.
            if !self.allow_downloads || !self.accepting.get() || self.inbox.closed.get() { return 1; }
            if let (Some(browser), Some((native_download, info)), Some(callback)) = (browser, copy_item(download_item), callback) {
                self.inbox.push(Incoming::Before {
                    key: Key { registration: self.registration, native_browser: browser.identifier(), native_download },
                    origin: self.origin, info,
                    suggested: suggested_name.map(ToString::to_string).unwrap_or_default(),
                    callback: callback.clone(),
                });
            }
            1
        }
        fn on_download_updated(&self, browser: Option<&mut Browser>, download_item: Option<&mut DownloadItem>, callback: Option<&mut DownloadItemCallback>) {
            let copied = copy_item(download_item);
            if !self.allow_downloads || !self.accepting.get() || self.inbox.closed.get() || copied.is_none() || browser.is_none() {
                // No inbox/model borrow exists while calling CEF, including
                // late callbacks after explicit shutdown. Terminal updates only
                // release their handle; invalid items are never read further.
                if !copied.as_ref().is_some_and(|(_, info)| info.phase.is_terminal()) {
                    if let Some(callback) = callback { callback.cancel(); }
                }
                return;
            }
            let browser = browser.unwrap();
            let (native_download, info) = copied.unwrap();
            self.inbox.push(Incoming::Update {
                key: Key { registration: self.registration, native_browser: browser.identifier(), native_download },
                origin: self.origin, info, callback: callback.map(|callback| callback.clone()),
            });
        }
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
        assert_send_sync::<Inbox<BeforeDownloadCallback, DownloadItemCallback>>();
    }
}
