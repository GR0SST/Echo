//! Chromium (via CEF) web views embedded as child views of a native window.
//! The rest of the app talks to the engine only through this crate.
//!
//! Everything here runs on the main thread: CEF is configured with an
//! external message pump, and the UI toolkit calls [`Engine::pump`].

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cef::*;
mod devtools;
mod downloads;
mod retirement;
#[cfg(any(target_os = "linux", test))]
mod native_requests;
pub use retirement::{DownloadRetirementPreparation, DownloadRetirementStatus};
pub use downloads::{DownloadControl, DownloadDestination, DownloadError, DownloadEvent, DownloadInfo, DownloadOwner, DownloadPhase, DownloadProvenance, DownloadSnapshot, DownloadToken};
mod editing;
mod printing;
pub use printing::{PrintDispatch, PrintRequest};
mod paste_style;
pub use paste_style::{PasteDispatch, PasteRequest};
mod devtools_renderer;
pub use devtools::{InspectorEvent, InspectorId, InspectorPanel, InspectorPhase, InspectorStatus, INSPECTOR_FRONTEND_URL};
mod host;
mod snapshot;
mod storage;
mod shutdown;
pub use shutdown::{ShutdownPhase, ShutdownStatus};
pub use storage::StorageKey;
pub use editing::subprocess_app;
pub use snapshot::PageSnapshot;

#[cfg(target_os = "macos")]
mod mac;

/// Identifies one embedded content view. Inspector view IDs remain internal;
/// callers use the distinct [`InspectorId`] for frontend operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ViewId(u64);

impl ViewId {
    /// For alternative engines, e.g. test fakes.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// A rectangle in logical pixels, top-left origin, relative to the parent view.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A key combination the browser UI handles itself, even while a page has
/// keyboard focus. `key` is lowercase: a character, or `tab`/`up`/`down`/`left`/`right`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shortcut {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    TitleChanged { view: ViewId, title: String },
    Favicon { view: ViewId, page_url: String, png: Vec<u8> },
    Focused { view: ViewId },
    EditFocusChanged { view: ViewId, editable: bool },
    FindResult { view: ViewId, count: i32, active: i32, final_update: bool },
    UrlChanged { view: ViewId, url: String },
    /// Link target under the pointer; an empty value clears the status.
    StatusChanged { view: ViewId, text: String },
    LoadingChanged {
        view: ViewId,
        loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    /// The page asked for a new window (target=_blank, window.open, ⌘-click).
    OpenInNewTab { opener: ViewId, url: String },
    OpenPeek { opener: ViewId, url: String },
    /// A browser shortcut pressed while this exact page had focus. Preserve
    /// the callback view through queues; the current active page may change.
    Shortcut { view: ViewId, shortcut: Shortcut },
    /// Native frontend input in the same chronological stream as page input.
    /// The browser validates inspector ownership; this is never tab metadata.
    InspectorInput(InspectorEvent),
}

/// Events keyed by CEF browser id, filled in by CEF callbacks.
#[derive(Debug)]
enum RawEvent {
    Title(String),
    Favicon(String, Vec<u8>),
    Focused,
    EditFocus(bool),
    Find(i32, i32, bool),
    Url(String),
    Status(String),
    Loading(bool, bool, bool),
    LoadError(String, i32, String),
    Popup(String),
    Peek(String),
    Shortcut(Shortcut),
}

type EventQueue = Arc<Mutex<Vec<(i32, RawEvent)>>>;
type Shortcuts = Arc<Vec<Shortcut>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PeekPolicy {
    pub pinned: bool,
    pub shift_click: bool,
    pub capture_escape: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewRole { Page, Inspector(InspectorId) }

// Defaults are the native types. Generic payloads let lifecycle tests exercise
// the actual view ownership without loading CEF or constructing a native host.
struct View<B = Browser, C = RequestContext> {
    action_lifetime: std::rc::Rc<()>,
    role: ViewRole,
    browser: B,
    // Retain the exact registry Rc, not just a clone of the native CEF handle.
    // In particular, a private page in a normal pool must keep Runtime.private
    // upgradeable after the source private engine and public handle are dropped.
    context: std::rc::Rc<C>,
    parent: *mut c_void,
    /// What gets positioned and hidden: a rounded clip view holding the
    /// page on macOS, the page's own native view elsewhere.
    host: *mut c_void,
    frame: Bounds,
    /// The native view has not been positioned yet.
    needs_layout: bool,
    rounded_top: bool,
    link_policy: Arc<Mutex<PeekPolicy>>,
}

// CEF is initialized once per process, including across closed/reopened windows.
thread_local! {
    static RUNTIME: std::cell::RefCell<Option<std::rc::Rc<Runtime>>> = const { std::cell::RefCell::new(None) };
}

struct Runtime {
    root: PathBuf,
    persistent: storage::PersistentContexts<RequestContext>,
    private: std::cell::RefCell<std::rc::Weak<RequestContext>>,
    readiness: storage::ContextReadiness<RequestContext>,
    downloads: std::cell::RefCell<downloads::RuntimeDownloads<RequestContext>>,
    lifecycle: shutdown::Lifecycle,
    #[cfg(target_os = "macos")]
    _library: library_loader::LibraryLoader,
}

/// Opaque, cloneable storage lease retaining the initialized CEF runtime.
/// Resolve it through an engine, then use it for pages in that same engine or
/// an independent fork. It contains no application profile ID. Main thread only.
/// Dropping this handle does not delete storage or imply CEF has released it.
/// Physical deletion requires a future barrier covering pages, inspectors,
/// forks, retained handles and CEF background users. Until then, deletion in
/// the application must only remove metadata; never remove these directories.
pub struct StorageContext {
    context: std::rc::Rc<RequestContext>,
    runtime: std::rc::Rc<Runtime>,
}

impl Clone for StorageContext {
    fn clone(&self) -> Self {
        self.runtime.lifecycle.require_running().expect("Cannot clone storage during shutdown");
        Self { context: self.context.clone(), runtime: self.runtime.clone() }
    }
}

impl StorageContext {
    /// True after CEF has initialized this context, while the runtime is running.
    /// While running, false is pending, not a page-creation error: return to the
    /// normal event loop and retry on a later frame. Also returns false during
    /// shutdown. This never pumps or waits synchronously.
    /// Native parent availability is separate; readiness does not prove disk
    /// persistence or guarantee that browser creation will succeed.
    pub fn is_ready(&self) -> bool {
        self.runtime.lifecycle.require_running().is_ok()
            && self.runtime.readiness.is_ready(&self.context)
    }
}

wrap_request_context_handler! {
    struct StorageReadyHandler { ready: storage::ContextReady }
    impl RequestContextHandler {
        fn on_request_context_initialized(&self, request_context: Option<&mut RequestContext>) {
            // CEF posts this AFTER profile initialization. No Runtime capture,
            // application callback, native creation or nested pump is permitted.
            if request_context.is_some() { self.ready.mark_initialized(); }
        }
    }
}

impl Runtime {
    fn default_context(&self) -> Result<std::rc::Rc<RequestContext>, String> {
        self.lifecycle.require_running()?;
        let path = storage::default_path(&self.root)?;
        let context = self.persistent.resolve(path.clone(), || {
            let context = request_context_get_global_context()
                .ok_or_else(|| "Global Chromium request context is unavailable".to_string())?;
            storage::verify_cache_path(&path, &CefString::from(&context.cache_path()).to_string())?;
            if context.is_global() == 0 { return Err("Chromium Default context is not global".into()); }
            Ok(context)
        })?;
        // The global profile is bootstrapped before initialize() returns. It
        // has no per-request-context handler, unlike named/private profiles.
        self.readiness.track(&context, storage::ContextReady::initialized());
        Ok(context)
    }

    fn named_context(&self, key: &StorageKey) -> Result<std::rc::Rc<RequestContext>, String> {
        self.lifecycle.require_running()?;
        // Validate the directories on every lookup, including registry hits.
        let path = storage::named_path(&self.root, key)?;
        let ready = storage::ContextReady::default();
        let context = self.persistent.resolve(path.clone(), || {
            let settings = RequestContextSettings {
                cache_path: CefString::from(storage::utf8_path(&path)?),
                persist_session_cookies: 1,
                ..Default::default()
            };
            let mut handler = StorageReadyHandler::new(ready.clone());
            let context = request_context_create_context(Some(&settings), Some(&mut handler))
                .ok_or_else(|| "Cannot create a persistent Chromium context".to_string())?;
            // GetCachePath reports configuration, even if asynchronous profile
            // creation falls back to OffTheRecord. This check is necessary but
            // is not runtime persistence proof; native restart acceptance is required.
            storage::verify_cache_path(&path, &CefString::from(&context.cache_path()).to_string())?;
            if context.is_global() != 0 { return Err("Named Chromium context unexpectedly uses Default".into()); }
            Ok(context)
        })?;
        self.readiness.track(&context, ready);
        Ok(context)
    }
}

pub struct Engine {
    shortcuts: Shortcuts,
    events: EventQueue,
    views: HashMap<ViewId, View>,
    by_browser_id: HashMap<i32, ViewId>,
    next_id: u64,
    snapshots: snapshot::Snapshots,
    message_ids: devtools::MessageIds,
    inspectors: devtools::Inspectors,
    // Always the actual CEF context, including the global normal profile.
    // Keeping this explicit prevents a fork from silently using global storage.
    context: std::rc::Rc<RequestContext>,
    _runtime: std::rc::Rc<Runtime>,
}


impl Engine {
    /// Starts Chromium. Fails when the CEF framework is not next to the
    /// executable, e.g. when running the bare binary outside the app bundle.
    /// `shortcuts` are intercepted before pages see them and reported as
    /// [`Event::Shortcut`]. Chromium keeps its data (cookies, cache) in
    /// `data_dir`. Retains the actual global request context; fails if CEF
    /// cannot provide it instead of creating a replacement profile.
    pub fn start(data_dir: &Path, shortcuts: Vec<Shortcut>) -> Result<Self, String> {
        let runtime = Self::start_runtime(data_dir)?;
        let context = runtime.default_context()?;
        Ok(Self::with_context(runtime, context, Arc::new(shortcuts)))
    }

    fn start_runtime(data_dir: &Path) -> Result<std::rc::Rc<Runtime>, String> {
        shutdown::check_start_thread()?;
        // Never bypass path validation when returning an existing runtime.
        let root_cache = storage::canonical_root(data_dir)?;
        RUNTIME.with(|slot| -> Result<std::rc::Rc<Runtime>, String> {
            if let Some(runtime) = slot.borrow().as_ref() {
                runtime.lifecycle.require_running()?;
                storage::require_same_root(&runtime.root, &root_cache)?;
                storage::default_path(&root_cache)?;
                return Ok(runtime.clone());
            }
            let default_cache = storage::default_path(&root_cache)?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;

            #[cfg(target_os = "macos")]
            let library = {
                let framework = exe
                    .parent()
                    .map(|dir| dir.join("../Frameworks/Chromium Embedded Framework.framework"))
                    .filter(|path| path.exists());
                if framework.is_none() {
                    return Err("Chromium Embedded Framework is missing; run the app bundle".into());
                }
                let loader = library_loader::LibraryLoader::new(&exe, false);
                if !loader.load() {
                    return Err("cannot load Chromium Embedded Framework".into());
                }
                loader
            };

            let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

            #[cfg(target_os = "macos")]
            mac::install_app_protocols();

            let settings = Settings {
                no_sandbox: 0,
                external_message_pump: 1,
                multi_threaded_message_loop: 0,
                root_cache_path: CefString::from(storage::utf8_path(&root_cache)?),
                cache_path: CefString::from(storage::utf8_path(&default_cache)?),
                persist_session_cookies: 1,
                log_severity: LogSeverity::WARNING,
                ..Default::default()
            };
            let args = args::Args::new();
            let mut app = EngineApp::new();
            if initialize(
                Some(args.as_main_args()),
                Some(&settings),
                Some(&mut app),
                std::ptr::null_mut(),
            ) != 1
            {
                return Err("CEF failed to initialize".into());
            }

            let runtime = std::rc::Rc::new(Runtime {
                root: root_cache,
                lifecycle: shutdown::Lifecycle::default(),
                persistent: storage::PersistentContexts::default(),
                private: std::cell::RefCell::new(std::rc::Weak::new()),
                readiness: storage::ContextReadiness::default(),
                downloads: std::cell::RefCell::new(downloads::RuntimeDownloads::default()),
                #[cfg(target_os = "macos")]
                _library: library,
            });
            *slot.borrow_mut() = Some(runtime.clone());
            // Register Default even if a later pool constructor fails. Once
            // initialize succeeds the loader/runtime must remain available for
            // explicit shutdown, including an initialization-context failure.
            if let Err(error) = runtime.default_context() {
                runtime.lifecycle.fail(format!("Cannot retain Default for shutdown: {error}"));
                return Err(error);
            }
            Ok(runtime)
        })
    }

    // All constructors, including forks, allocate fresh per-engine state here.
    fn with_context(
        runtime: std::rc::Rc<Runtime>,
        context: std::rc::Rc<RequestContext>,
        shortcuts: Shortcuts,
    ) -> Self {
        runtime.lifecycle.require_running().expect("Cannot fork an engine during shutdown");
        let events = EventQueue::default();
        Self {
            shortcuts,
            events,
            views: HashMap::new(),
            by_browser_id: HashMap::new(),
            next_id: 1,
            snapshots: snapshot::Snapshots::default(),
            message_ids: devtools::MessageIds::default(),
            inspectors: devtools::Inspectors::default(),
            _runtime: runtime,
            context,
        }
    }

    /// Private windows share an in-memory context retained by private engines
    /// and their forks. Never fall back to the persistent global context.
    pub fn start_private(data_dir: &Path, shortcuts: Vec<Shortcut>) -> Result<Self, String> {
        let runtime = Self::start_runtime(data_dir)?;
        let existing = runtime.private.borrow().upgrade();
        let context = if let Some(context) = existing {
            context
        } else {
            // Private contexts never enter the persistent registry.
            let settings = RequestContextSettings {
                cache_path: CefString::from(""),
                persist_session_cookies: 0,
                ..Default::default()
            };
            let ready = storage::ContextReady::default();
            let mut handler = StorageReadyHandler::new(ready.clone());
            let context = request_context_create_context(Some(&settings), Some(&mut handler))
                .ok_or_else(|| "Cannot create a private Chromium context".to_string())?;
            if !CefString::from(&context.cache_path()).to_string().is_empty() || context.is_global() != 0 {
                return Err("Private Chromium context has unexpected cache-path or global configuration".into());
            }
            let context = std::rc::Rc::new(context);
            *runtime.private.borrow_mut() = std::rc::Rc::downgrade(&context);
            runtime.readiness.track(&context, ready);
            context
        };
        Ok(Self::with_context(runtime, context, Arc::new(shortcuts)))
    }

    /// Creates an empty, independent page pool using this engine's actual
    /// CEF request context and initialized runtime. No CEF initialization or
    /// new request context/cache is created. Cookies and origin storage are
    /// shared; views, callbacks, snapshots, inspectors and ID counters are new.
    /// Shortcuts retain the source's immutable configuration.
    ///
    /// This is profile-neutral: a private source produces a private fork.
    /// Callers implementing Blank windows must choose a normal-profile source,
    /// including when the command originated in a private window.
    ///
    /// Both retained handles outlive the source engine if needed. Dropping or
    /// closing either pool leaves the other intact. View and inspector IDs are
    /// local to each engine and must not be passed between pools. Like all
    /// engine operations, call this on the main thread. This is infallible
    /// because construction already requires a resolved, non-optional context;
    /// it never falls back to the global context.
    pub fn fork_with_shared_storage(&self) -> Self {
        Self::with_context(self._runtime.clone(), self.context.clone(), self.shortcuts.clone())
    }

    /// Resolves the actual global context at the legacy `data_dir/Default`.
    /// Even on a private engine, this explicitly selects persistent Default.
    pub fn default_storage_context(&self) -> Result<StorageContext, String> {
        Ok(StorageContext { runtime: self._runtime.clone(), context: self._runtime.default_context()? })
    }

    /// Configures persistent storage at `data_dir/profile-<key>`, a direct child
    /// required by CEF 154's disk-profile selection. Default remains unchanged.
    /// Calls for the same key reuse a retained context; errors never fall back
    /// to Default. Cache-path equality checks configuration, not actual runtime
    /// persistence: CEF can silently select OffTheRecord. Acceptance requires
    /// cookie and localStorage verification across a full process restart.
    /// Display names and application profile IDs must be mapped to stable keys
    /// by the caller. No physical profile deletion is supported (see storage.rs).
    pub fn named_storage_context(&self, key: &StorageKey) -> Result<StorageContext, String> {
        Ok(StorageContext { runtime: self._runtime.clone(), context: self._runtime.named_context(key)? })
    }

    /// Retains this pool's default context, including a private context.
    pub fn storage_context(&self) -> StorageContext {
        self._runtime.lifecycle.require_running().expect("Cannot acquire storage during shutdown");
        StorageContext { runtime: self._runtime.clone(), context: self.context.clone() }
    }

    fn check_storage_context(&self, context: &StorageContext) -> Result<(), String> {
        self._runtime.lifecycle.require_running()?;
        if !std::rc::Rc::ptr_eq(&self._runtime, &context.runtime) {
            return Err("Storage context belongs to a different CEF runtime".into());
        }
        Ok(())
    }

    /// Creates an independent empty page pool for Blank windows, retaining the
    /// selected context and this engine's shortcuts. All view/event/inspector/
    /// snapshot/ID state is fresh, as with `fork_with_shared_storage`.
    /// For workspace Pages use `create_view_in_context` on the existing engine.
    pub fn fork_with_context(&self, context: &StorageContext) -> Result<Self, String> {
        self.check_storage_context(context)?;
        Ok(Self::with_context(self._runtime.clone(), context.context.clone(), self.shortcuts.clone()))
    }

    /// Runs pending Chromium work. Call often (every few milliseconds).
    /// Chromium may process native UI events inside this call, so the
    /// caller must not hold state that those events' handlers need.
    pub fn pump() {
        if shutdown::is_polling() { return; }
        Self::pump_impl();
    }

    fn pump_impl() {
        // Chromium's work also takes native events off the queue and
        // dispatches them itself. AppKit's mouse tracking (window buttons,
        // resizing) then misses the button going up, so events are left to
        // the toolkit's loop: Chromium runs once the queue is empty, or
        // after a few skips so an event nobody takes cannot stall pages.
        #[cfg(target_os = "macos")]
        {
            const MAX_SKIPS: u32 = 6;
            thread_local!(static SKIPS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) });
            if mac::is_tracking_events() {
                return;
            }
            if mac::has_pending_events() && SKIPS.get() < MAX_SKIPS {
                SKIPS.set(SKIPS.get() + 1);
                return;
            }
            SKIPS.set(0);
        }
        shutdown::pump_once();
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        self.drain_events(false)
    }

    /// Browser input must retain CEF callback order across page and frontend
    /// focus. The diagnostic drain above keeps its separate inspector queue.
    pub fn take_browser_events(&mut self) -> Vec<Event> {
        self.drain_events(true)
    }

    fn drain_events(&mut self, ordered_input: bool) -> Vec<Event> {
        let raw = std::mem::take(&mut *self.events.lock().unwrap());
        raw.into_iter()
            .filter_map(|(browser_id, event)| {
                let view = *self.by_browser_id.get(&browser_id)?;
                if ordered_input {
                    if let Some(input) = browser_input(view, self.views.get(&view)?.role, &event) { return Some(input); }
                }
                if let ViewRole::Inspector(inspector) = self.views.get(&view)?.role {
                    let browser = &self.views.get(&view)?.browser;
                    match event {
                        RawEvent::Url(url) => self.inspectors.loading(inspector, url, browser.is_loading() != 0),
                        RawEvent::Loading(loading, _, _) => {
                            let url = browser.main_frame().map(|frame| CefString::from(&frame.url()).to_string()).unwrap_or_default();
                            self.inspectors.loading(inspector, url, loading);
                        }
                        RawEvent::LoadError(url, code, text) => self.inspectors.load_failed(inspector, url, code, text),
                        event => { if let Some(event) = inspector_input(inspector, &event) { self.inspectors.ready.push(event); } }
                    }
                    return None;
                }
                page_event(view, self.views.get(&view)?.role, event)
            })
            .collect()
    }

    /// Creates a page over `ui_view` (the toolkit's `NSView*` on macOS),
    /// with `frame` in that view's coordinates, clipped to `corner_radius`.
    pub fn create_view(
        &mut self,
        ui_view: *mut c_void,
        url: &str,
        frame: Bounds,
        corner_radius: f64,
    ) -> Option<ViewId> {
        let context = self.context.clone();
        self.create_content_view(ui_view, url, frame, corner_radius, ViewRole::Page, context, None)
    }

    /// Creates a page with explicitly selected storage in THIS engine's view,
    /// event, inspector and snapshot namespace. The pool's default is unchanged.
    /// A foreign runtime or failed native creation returns an error, never a
    /// page in Default. Existing pages keep their original context.
    /// The page retains the exact storage lease even after the supplied handle
    /// and its source engine are dropped, including private storage.
    /// Check `StorageContext::is_ready` first; pending contexts require returning
    /// to the event loop and retrying on a later frame, not logging a failure.
    pub fn create_view_in_context(
        &mut self, context: &StorageContext, ui_view: *mut c_void, url: &str,
        frame: Bounds, corner_radius: f64,
    ) -> Result<ViewId, String> {
        self.create_view_in_context_impl(context, ui_view, url, frame, corner_radius, None)
    }

    /// Explicit engine-only opt-in. Owner is the captured originating local
    /// profile, including Blank's retained source. The capability selects an
    /// explicit lifetime policy; continuation requires native acceptance and the
    /// outside-borrows prepare_download_retirements handshake.
    /// Privacy/storage/browser identities are derived from the actual engine
    /// context and immutable registration, never from current window selection.
    pub fn create_view_in_context_with_download_owner(
        &mut self, context: &StorageContext, owner: DownloadOwner,
        ui_view: *mut c_void, url: &str, frame: Bounds, corner_radius: f64,
    ) -> Result<ViewId, String> {
        self.create_view_in_context_impl(context, ui_view, url, frame, corner_radius, Some(owner))
    }

    fn create_view_in_context_impl(
        &mut self,
        context: &StorageContext,
        ui_view: *mut c_void,
        url: &str,
        frame: Bounds,
        corner_radius: f64,
        owner: Option<DownloadOwner>,
    ) -> Result<ViewId, String> {
        self.check_storage_context(context)?;
        if !context.is_ready() {
            return Err("Chromium storage context is still initializing; retry on a later frame".into());
        }
        let id = self.create_content_view(ui_view, url, frame, corner_radius, ViewRole::Page, context.context.clone(), owner)
            .ok_or_else(|| "Cannot create Chromium page in the selected storage context".to_string())?;
        let same_context = self.views.get(&id).and_then(|view| view.browser.host())
            .and_then(|host| host.request_context())
            .is_some_and(|actual| actual.is_same(Some(&mut context.context.as_ref().clone())) != 0);
        if !same_context {
            self.close(id);
            return Err("Created page did not use the selected Chromium storage context".into());
        }
        Ok(id)
    }

    fn create_content_view(
        &mut self, ui_view: *mut c_void, url: &str, frame: Bounds,
        corner_radius: f64, role: ViewRole, context: std::rc::Rc<RequestContext>,
        download_owner: Option<DownloadOwner>,
    ) -> Option<ViewId> {
        if self._runtime.lifecycle.require_running().is_err() || ui_view.is_null() { return None; }
        // CreateBrowserSync rejects an uninitialized BrowserContext. Do not
        // allocate native clips or call CEF speculatively; a later normal frame
        // can retry after OnRequestContextInitialized. No nested pumping.
        if !self._runtime.readiness.is_ready(&context) { return None; }
        // Nested native event dispatch must not drain Created before its lease
        // has been registered, nor recursively enter the CEF message pump.
        let runtime = self._runtime.clone();
        let creation = runtime.lifecycle.creating();
        let parent = host::parent_of(ui_view);
        if parent.is_null() { return None; }
        #[cfg(target_os = "macos")]
        let (page_parent, bounds) = (
            mac::create_clip_view(parent, corner_radius),
            Rect {
                x: 0,
                y: 0,
                width: frame.width as i32,
                height: frame.height as i32,
            },
        );
        #[cfg(not(target_os = "macos"))]
        let (page_parent, bounds) = (
            parent,
            Rect {
                x: frame.x as i32,
                y: frame.y as i32,
                width: frame.width as i32,
                height: frame.height as i32,
            },
        );
        let window_info = WindowInfo {
            runtime_style: RuntimeStyle::ALLOY,
            ..Default::default()
        }
        .set_as_child(page_parent as _, &bounds);
        let mut extra = None;
        let policy = Arc::new(Mutex::new(PeekPolicy::default()));
        let mut client = EngineClient::new(self.events.clone(), self.shortcuts.clone(), None, self._runtime.lifecycle.queue.clone(), None, None, policy.clone());
        if let ViewRole::Inspector(id) = role {
            let dictionary = dictionary_value_create()?;
            dictionary.set_string(Some(&CefString::from(devtools_renderer::ROLE_KEY)), Some(&CefString::from(id.0.to_string().as_str())));
            extra = Some(dictionary);
            client = EngineClient::new(self.events.clone(), self.shortcuts.clone(), Some((id, self.inspectors.queue.clone())), self._runtime.lifecycle.queue.clone(), None, None, Arc::default());
        }
        let prepared_downloads = if role == ViewRole::Page {
            if let Some(owner) = download_owner {
                let private = CefString::from(&context.cache_path()).to_string().is_empty();
                let prepared = runtime.downloads.borrow_mut().prepare(context.clone(), owner, private);
                match prepared {
                    Ok(prepared) => Some(prepared),
                    Err(_) => {
                        #[cfg(target_os = "macos")]
                        mac::release_clip_view(page_parent);
                        return None;
                    }
                }
            } else { None }
        } else { None };
        if let Some(prepared) = &prepared_downloads {
            client = EngineClient::new(self.events.clone(), self.shortcuts.clone(), None,
                runtime.lifecycle.queue.clone(), Some(prepared.registration.handler()),
                Some(prepared.registration.gate()), policy.clone());
        }
        let mut native_context = context.as_ref().clone();
        let browser = browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut client),
            Some(&CefString::from(url)),
            Some(&BrowserSettings::default()),
            extra.as_mut(),
            Some(&mut native_context),
        );
        let Some(browser) = browser else {
            if let Some(prepared) = &prepared_downloads {
                runtime.downloads.borrow_mut().creation_failed(&prepared.registration);
            }
            #[cfg(target_os = "macos")]
            mac::release_clip_view(page_parent);
            return None;
        };
        let download_binding_ok = if let Some(prepared) = &prepared_downloads {
            let actual_context_matches = browser.host().and_then(|host| host.request_context())
                .is_some_and(|actual| actual.is_same(Some(&mut context.as_ref().clone())) != 0);
            let bound = actual_context_matches && {
                let mut downloads = runtime.downloads.borrow_mut();
                downloads.bind(&prepared.registration, browser.identifier()).is_ok()
            };
            if bound { true }
            else {
                runtime.downloads.borrow_mut().creation_failed(&prepared.registration);
                false
            }
        } else { true };
        let page_view = browser
            .host()
            .map_or(std::ptr::null_mut(), |host| host.window_handle() as *mut c_void);
        #[cfg(target_os = "macos")]
        let host = {
            mac::fill_superview(page_view);
            page_parent
        };
        #[cfg(not(target_os = "macos"))]
        let host = page_view;

        self._runtime.lifecycle.register(browser.identifier(), shutdown::BrowserLease {
            browser: browser.clone(), _context: context.clone(), host, retired_host: None,
        });
        let id = ViewId(self.next_id);
        self.next_id += 1;
        self.by_browser_id.insert(browser.identifier(), id);
        self.views.insert(
            id,
            View {
                action_lifetime: std::rc::Rc::new(()),
                role,
                browser,
                context,
                parent,
                host,
                frame,
                needs_layout: true,
                rounded_top: true,
                link_policy: policy,
            },
        );
        drop(creation);
        if !download_binding_ok {
            self.close(id);
            return None;
        }
        self.set_frame(id, frame);
        Some(id)
    }

    pub fn set_frame(&mut self, id: ViewId, frame: Bounds) {
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        if view.frame == frame && !view.needs_layout {
            return;
        }
        let resized = view.needs_layout || view.frame.width != frame.width || view.frame.height != frame.height;
        view.frame = frame;
        view.needs_layout = false;
        host::set_frame(view.host, view.parent, frame);
        if resized {
            if let Some(host) = view.browser.host() { host.was_resized(); }
        }
    }

    /// Moves a live page to another UI host without creating a browser or
    /// navigating it. DOM state and back/forward history stay in the same CEF instance.
    pub fn move_view(&mut self, id: ViewId, ui_view: *mut c_void, frame: Bounds) -> bool {
        if ui_view.is_null() { return false; }
        let Some(view) = self.views.get_mut(&id) else { return false; };
        let parent = host::parent_of(ui_view);
        if parent.is_null() { return false; }
        if view.parent != parent {
            #[cfg(not(target_os = "macos"))]
            if let Some(browser) = view.browser.host() { browser.notify_move_or_resize_started(); }
            if !host::move_to(view.host, parent) { return false; }
            view.parent = parent;
            view.needs_layout = true;
        }
        self.set_frame(id, frame);
        true
    }

    /// Per-document link intent; only Peek pages capture unmodified Escape.
    pub fn set_peek_policy(&mut self, id: ViewId, policy: PeekPolicy) {
        if let Some(view) = self.views.get(&id) { *view.link_policy.lock().unwrap() = policy; }
    }

    pub fn set_corner_radius(&mut self, id: ViewId, radius: f64) {
        #[cfg(target_os = "macos")]
        if let Some(view) = self.views.get(&id) { mac::set_corner_radius(view.host, radius.max(0.)); }
        #[cfg(not(target_os = "macos"))]
        let _ = (id, radius);
    }

    /// Split headers own the pane's top corners; the page below joins them squarely.
    /// Restoring a standalone page rounds its top corners again without reloading.
    pub fn set_top_corners_rounded(&mut self, id: ViewId, rounded: bool) {
        let Some(view) = self.views.get_mut(&id) else { return; };
        if view.rounded_top == rounded { return; }
        view.rounded_top = rounded;
        #[cfg(target_os = "macos")]
        mac::set_top_corners_rounded(view.host, rounded);
    }

    pub fn frame(&self, id: ViewId) -> Option<Bounds> {
        self.views.get(&id).map(|view| view.frame)
    }

    pub fn set_visible(&mut self, id: ViewId, visible: bool) {
        #[cfg(target_os = "macos")]
        if let Some(view) = self.views.get(&id) {
            mac::set_hidden(view.host, !visible);
        }
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            host.was_hidden((!visible).into());
        }
    }

    /// Gives keyboard focus to the page.
    pub fn focus(&mut self, id: ViewId) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            host.set_focus(1);
        }
    }

    /// Takes keyboard focus away from all pages, back to `ui_view`.
    pub fn focus_parent(&mut self, ui_view: *mut c_void) {
        #[cfg(target_os = "macos")]
        mac::make_first_responder(ui_view);
    }

    pub fn navigate(&mut self, id: ViewId, url: &str) {
        if let Some(frame) = self.views.get(&id).and_then(|view| view.browser.main_frame()) {
            frame.load_url(Some(&CefString::from(url)));
        }
    }

    pub fn go_back(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) {
            view.browser.go_back();
        }
    }

    /// Edit history belongs to the focused frame, including embedded editors.
    pub fn undo(&mut self, id: ViewId) {
        if let Some(frame) = self.views.get(&id).and_then(|view| view.browser.focused_frame()) { frame.undo(); }
    }

    pub fn redo(&mut self, id: ViewId) {
        if let Some(frame) = self.views.get(&id).and_then(|view| view.browser.focused_frame()) { frame.redo(); }
    }

    pub fn go_forward(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) {
            view.browser.go_forward();
        }
    }

    pub fn reload(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) {
            view.browser.reload();
        }
    }

    /// Reloads from the network, bypassing the HTTP cache.
    pub fn force_reload(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) { view.browser.reload_ignore_cache(); }
    }

    pub fn stop_loading(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) { view.browser.stop_load(); }
    }

    pub fn set_audio_muted(&mut self, id: ViewId, muted: bool) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) { host.set_audio_muted(muted.into()); }
    }

    pub fn audio_muted(&self, id: ViewId) -> Option<bool> {
        self.views.get(&id).and_then(|view| view.browser.host()).map(|host| host.is_audio_muted() != 0)
    }

    /// Steps through Chromium's standard zoom factors. Zero resets to 100%.
    pub fn zoom(&mut self, id: ViewId, direction: i32) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            let factor = next_zoom_factor(1.2_f64.powf(host.zoom_level()), direction);
            host.set_zoom_level(factor.ln() / 1.2_f64.ln());
        }
    }

    pub fn find(&mut self, id: ViewId, text: &str, forward: bool, next: bool) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            host.find(Some(&CefString::from(text)), forward.into(), 0, next.into());
        }
    }

    pub fn stop_finding(&mut self, id: ViewId) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            host.stop_finding(1);
        }
    }

    /// Captures the visible viewport of the same existing CEF browser without
    /// navigation or emulation overrides. Returns false for an unknown/busy
    /// view or submission failure; accepted requests complete via
    /// [`Self::take_snapshots`]. Keep pumping CEF and polling for completions.
    pub fn request_snapshot(&mut self, view: ViewId, request: u64) -> bool {
        if self._runtime.lifecycle.require_running().is_err() { return false; }
        let Some(page) = self.views.get(&view) else { return false };
        if page.role != ViewRole::Page { return false; }
        self.snapshots.request(view, request, &page.browser, &self.message_ids)
    }

    /// Drains screenshot completions and expires requests after five seconds.
    /// Closing a view discards its pending and completed screenshots.
    pub fn take_snapshots(&mut self) -> Vec<PageSnapshot> {
        self.snapshots.take()
    }

    /// Opens the original bundled frontend with an authenticated renderer host
    /// in an ordinary Alloy child. The historical probe name is retained for
    /// native integrations. Repeated opens coalesce and request the new panel.
    /// FrontendReady requires a verified protocol roundtrip and frontend boot.
    /// No ShowDevTools, remote port or global-context fallback is used.
    pub fn open_inspector_probe(&mut self, inspected: ViewId, ui_view: *mut c_void, frame: Bounds, panel: InspectorPanel) -> Result<InspectorId, String> {
        self._runtime.lifecycle.require_running()?;
        let page = self.views.get(&inspected).filter(|view| view.role == ViewRole::Page)
            .ok_or("Unknown inspected page")?;
        if ui_view.is_null() { return Err("A native GPUI parent is required".into()); }
        let host = page.browser.host().ok_or("Inspected page has no host")?;
        // Retain the inspected view's exact registry lease, never this engine's
        // default or a fresh Rc around a native handle. Still check that the
        // inspected browser actually uses it; missing/mismatched contexts fail closed.
        let context = page.context.clone();
        let actual = host.request_context().ok_or("Inspected request context is unavailable")?;
        if actual.is_same(Some(&mut context.as_ref().clone())) == 0 {
            return Err("Inspected page context differs from its retained storage lease".into());
        }
        if let Some(id) = self.inspectors.existing(inspected, panel) { self.show_inspector_panel(id, panel); return Ok(id); }
        let id = self.inspectors.allocate().ok_or("Inspector generation exhausted")?;
        let mut observer = devtools::InspectorObserver::new(self.inspectors.queue.clone(), id);
        let registration = host.add_dev_tools_message_observer(Some(&mut observer))
            .ok_or("Cannot register local DevTools observer")?;
        let frontend = self.create_content_view(ui_view, INSPECTOR_FRONTEND_URL, frame, 0.0, ViewRole::Inspector(id), context.clone(), None)
            .ok_or("Cannot create Alloy inspector content view")?;
        let is_alloy = self.views.get(&frontend).and_then(|view| view.browser.host())
            .is_some_and(|host| host.runtime_style() == RuntimeStyle::ALLOY);
        if !is_alloy {
            self.close(frontend);
            return Err("Inspector did not create an Alloy content browser; probe closed".into());
        }
        let same_context = self.views.get(&frontend).and_then(|view| view.browser.host())
            .and_then(|host| host.request_context())
            .is_some_and(|actual| actual.is_same(Some(&mut context.as_ref().clone())) != 0);
        if !same_context {
            self.close(frontend);
            return Err("Inspector context differs from the inspected page; probe closed".into());
        }
        self.inspectors.insert(id, inspected, frontend, panel, devtools::NativeBinding { _context: context.as_ref().clone(), _registration: registration });
        Ok(id)
    }

    pub fn inspector_status(&self, inspector: InspectorId) -> Option<InspectorStatus> {
        self.inspectors.sessions.get(&inspector).map(|session| session.status.clone())
    }

    pub fn set_inspector_frame(&mut self, inspector: InspectorId, frame: Bounds) {
        if let Some(session) = self.inspectors.sessions.get(&inspector) { self.set_frame(session.frontend, frame); }
    }

    pub fn set_inspector_visible(&mut self, inspector: InspectorId, visible: bool) {
        if let Some(session) = self.inspectors.sessions.get(&inspector) { self.set_visible(session.frontend, visible); }
    }

    /// Trusted native diagnostic API, correlated separately from the renderer
    /// connection. Its replies only appear as InspectorEvent::Protocol.
    pub fn send_inspector_probe_message(&mut self, inspector: InspectorId, message: &str) -> bool {
        let Some(session) = self.inspectors.sessions.get(&inspector) else { return false; };
        let Some(host) = self.views.get(&session.status.inspected).and_then(|view| view.browser.host()) else { return false; };
        let Some((id, bytes)) = self.inspectors.submit(inspector, &self.message_ids, message) else { return false; };
        if host.send_dev_tools_message(Some(&bytes)) == 0 {
            self.inspectors.failed(inspector, id);
            return false;
        }
        true
    }

    /// Call take_events or take_browser_events first to process load callbacks, then drain this queue.
    /// Opening/FrontendLoaded do not assert that frontend JS or the host booted.
    pub fn take_inspector_events(&mut self) -> Vec<InspectorEvent> {
        let events = self.inspectors.take();
        let actions = std::mem::take(&mut self.inspectors.actions);
        for (id, epoch, action) in actions {
            let Some(session) = self.inspectors.sessions.get_mut(&id) else { continue; };
            if session.bridge.epoch != epoch { continue; }
            match action {
                devtools::BridgeAction::Submit(message) => {
                    if !session.bridge.active { continue; }
                    let Some(host) = self.views.get(&session.status.inspected).and_then(|v| v.browser.host()) else { continue; };
                    if let Some((wire, bytes)) = session.renderer_protocol.submit(&self.message_ids, &message) {
                        if host.send_dev_tools_message(Some(&bytes)) == 0 {
                            session.renderer_protocol.pending.remove(&wire);
                            self.inspectors.queue.lock().unwrap().push((id, devtools::Backend::Detached));
                        }
                    } else { self.inspectors.queue.lock().unwrap().push((id, devtools::Backend::Detached)); }
                }
                devtools::BridgeAction::Deliver(kind, payload) => {
                    if !session.bridge.active && kind != "revoke" { continue; }
                    if kind == "panel" && session.status.selected_panel != Some(session.status.panel) {
                        session.status.selected_panel = None;
                    }
                    if let Some(frame) = self.views.get(&session.frontend).and_then(|v| v.browser.main_frame()).filter(|f| devtools::expected_frontend(&CefString::from(&f.url()).to_string())) {
                        let identifier = CefString::from(&frame.identifier()).to_string();
                        if let Some(mut message) = devtools::bridge_message(id.0, epoch, kind, &payload, &identifier) { frame.send_process_message(ProcessId::RENDERER, Some(&mut message)); }
                    }
                }
            }
        }
        events
    }

    /// Focus the frontend content browser without exposing its private ViewId.
    pub fn focus_inspector(&mut self, id: InspectorId) -> bool {
        let Some(frontend) = self.inspectors.sessions.get(&id).filter(|s| s.status.phase != InspectorPhase::Detached).map(|s| s.frontend) else { return false; };
        self.focus(frontend);
        true
    }

    /// Apply an original frontend panel API after its authenticated bootstrap.
    pub fn show_inspector_panel(&mut self, id: InspectorId, panel: InspectorPanel) -> bool {
        let Some(session) = self.inspectors.sessions.get_mut(&id) else { return false; };
        session.status.panel = panel;
        if session.bridge.is_ready() {
            self.inspectors.actions.push((id, session.bridge.epoch, devtools::BridgeAction::Deliver("panel", devtools::panel_name(panel).into())));
        }
        true
    }

    /// One-shot toggle via the original frontend. Before readiness, calls
    /// coalesce by parity and execute once after the verified handshake.
    /// Release/detach/close discard pending intent. True means accepted.
    pub fn toggle_inspector_element_mode(&mut self, id: InspectorId) -> bool {
        let Some(session) = self.inspectors.sessions.get_mut(&id).filter(|s| !matches!(s.status.phase, InspectorPhase::Detached | InspectorPhase::LoadFailed)) else { return false; };
        if session.bridge.toggle_inspect() {
            self.inspectors.actions.push((id, session.bridge.epoch, devtools::BridgeAction::Deliver("inspect", String::new())));
        }
        true
    }

    /// Drops the observer and closes only the frontend. The inspected document
    /// remains alive and is neither navigated nor reloaded.
    pub fn close_inspector(&mut self, inspector: InspectorId) {
        if let Some(frontend) = self.inspectors.close(inspector) { self.close(frontend); }
    }

    pub fn close(&mut self, id: ViewId) {
        let inspectors: Vec<_> = self.inspectors.sessions.iter()
            .filter_map(|(&inspector, session)| (session.status.inspected == id || session.frontend == id).then_some(inspector)).collect();
        for inspector in inspectors { self.close_inspector(inspector); }
        self.snapshots.close(id);
        if let Some(view) = self.views.remove(&id) {
            self.by_browser_id.remove(&view.browser.identifier());
            // Runtime owns the exact context and child host until OnBeforeClose.
            // Close is queued: no CEF call or callback while page state is borrowed.
            if !self._runtime.downloads.borrow_mut().queue_retirement(view.browser.identifier()) {
                self._runtime.lifecycle.request_close(view.browser.identifier());
            }
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        for id in self.views.keys().copied().collect::<Vec<_>>() {
            self.close(id);
        }
    }
}

// This is the only conversion from CEF callbacks into tab-facing events.
// Keep role filtering here as well as in take_events so a future callback cannot
// accidentally publish frontend metadata, popups, focus or shortcuts as tabs.
fn browser_input(view: ViewId, role: ViewRole, event: &RawEvent) -> Option<Event> {
    match role {
        ViewRole::Inspector(inspector) => inspector_input(inspector, event).map(Event::InspectorInput),
        ViewRole::Page => match event {
            RawEvent::Focused => Some(Event::Focused { view }),
            RawEvent::Shortcut(shortcut) => Some(Event::Shortcut { view, shortcut: shortcut.clone() }),
            _ => None,
        },
    }
}

fn inspector_input(inspector: InspectorId, event: &RawEvent) -> Option<InspectorEvent> {
    match event {
        RawEvent::Focused => Some(InspectorEvent::Focused { inspector }),
        RawEvent::Shortcut(shortcut) => Some(InspectorEvent::Shortcut { inspector, shortcut: shortcut.clone() }),
        _ => None,
    }
}

fn page_event(view: ViewId, role: ViewRole, event: RawEvent) -> Option<Event> {
    if role != ViewRole::Page { return None; }
    if let RawEvent::Shortcut(shortcut) = event {
        return Some(Event::Shortcut { view, shortcut });
    }
    Some(match event {
        RawEvent::Shortcut(_) => unreachable!(),
        RawEvent::LoadError(_, _, _) => return None,
        RawEvent::Title(title) => Event::TitleChanged { view, title },
        RawEvent::Favicon(page_url, png) => Event::Favicon { view, page_url, png },
        RawEvent::Focused => Event::Focused { view },
        RawEvent::EditFocus(editable) => Event::EditFocusChanged { view, editable },
        RawEvent::Find(count, active, final_update) => Event::FindResult { view, count, active, final_update },
        RawEvent::Url(url) => Event::UrlChanged { view, url },
        RawEvent::Status(text) => Event::StatusChanged { view, text },
        RawEvent::Loading(loading, can_go_back, can_go_forward) => {
            Event::LoadingChanged {
                view,
                loading,
                can_go_back,
                can_go_forward,
            }
        }
        RawEvent::Popup(url) => Event::OpenInNewTab { opener: view, url },
        RawEvent::Peek(url) => Event::OpenPeek { opener: view, url },
    })
}

fn push(events: &EventQueue, browser: Option<&mut Browser>, event: RawEvent) {
    if let Some(browser) = browser {
        events.lock().unwrap().push((browser.identifier(), event));
    }
}

wrap_client! {
    struct EngineClient {
        events: EventQueue,
        shortcuts: Shortcuts,
        inspector: Option<(InspectorId, devtools::Queue)>,
        lifecycle: shutdown::Queue,
        downloads: Option<DownloadHandler>,
        download_gate: Option<downloads::DownloadGate>,
        policy: Arc<Mutex<PeekPolicy>>,
    }

    impl Client {
        fn on_process_message_received(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, source_process: ProcessId, message: Option<&mut ProcessMessage>) -> ::std::os::raw::c_int {
            if let Some((id, queue)) = &self.inspector {
                return devtools::receive_frontend(*id, queue, browser, frame, source_process, message);
            }
            let Some(message) = message.filter(|message| source_process == ProcessId::RENDERER && CefString::from(&message.name()).to_string() == editing::EDIT_FOCUS_MESSAGE) else { return 0; };
            let Some(arguments) = message.argument_list() else { return 0; };
            if arguments.size() != 1 || arguments.get_type(0) != ValueType::BOOL { return 0; }
            push(&self.events, browser, RawEvent::EditFocus(arguments.bool(0) != 0));
            1
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            if let Some((id, queue)) = &self.inspector { Some(devtools::InspectorRequestHandler::new(*id, queue.clone())) }
            else { Some(PeekRequestHandler::new(self.events.clone(), self.policy.clone())) }
        }
        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(self.downloads.clone().unwrap_or_else(downloads::denied_handler))
        }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(EngineKeyboardHandler::new(self.events.clone(), self.shortcuts.clone(), self.policy.clone()))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(EngineDisplayHandler::new(self.events.clone()))
        }

        fn find_handler(&self) -> Option<FindHandler> {
            Some(EngineFindHandler::new(self.events.clone()))
        }
        fn focus_handler(&self) -> Option<FocusHandler> {
            Some(EngineFocusHandler::new(self.events.clone()))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(EngineLoadHandler::new(self.events.clone(), self.download_gate.clone()))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(EngineLifeSpanHandler::new(self.events.clone(), self.policy.clone(), self.lifecycle.clone(), self.download_gate.clone()))
        }
    }
}

fn peek_cross_domain(current: &str, target: &str, pinned: bool, gesture: bool, redirect: bool) -> bool {
    if !pinned || !gesture || redirect { return false; }
    let (Ok(current), Ok(target)) = (url::Url::parse(current), url::Url::parse(target)) else { return false; };
    matches!(current.scheme(), "http" | "https") && matches!(target.scheme(), "http" | "https")
        && current.host_str().is_some() && target.host_str().is_some() && current.host_str() != target.host_str()
}

wrap_request_handler! {
    struct PeekRequestHandler { events: EventQueue, policy: Arc<Mutex<PeekPolicy>> }
    impl RequestHandler {
        fn on_before_browse(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, request: Option<&mut Request>, gesture: i32, redirect: i32) -> i32 {
            let Some(frame) = frame.filter(|frame| frame.is_main() != 0) else { return 0; };
            let Some(request) = request else { return 0; };
            let target = CefString::from(&request.url()).to_string();
            if CefString::from(&request.method()).to_string() != "GET" || !peek_cross_domain(&CefString::from(&frame.url()).to_string(), &target, self.policy.lock().unwrap().pinned, gesture != 0, redirect != 0) { return 0; }
            push(&self.events, browser, RawEvent::Peek(target));
            1
        }
        fn on_open_urlfrom_tab(&self, browser: Option<&mut Browser>, _frame: Option<&mut Frame>, target: Option<&CefString>, disposition: WindowOpenDisposition, gesture: i32) -> i32 {
            if gesture == 0 || !self.policy.lock().unwrap().shift_click || disposition != WindowOpenDisposition::NEW_WINDOW { return 0; }
            let Some(target) = target else { return 0; };
            push(&self.events, browser, RawEvent::Peek(target.to_string()));
            1
        }
    }
}

wrap_find_handler! {
    struct EngineFindHandler { events: EventQueue }

    impl FindHandler {
        fn on_find_result(
            &self,
            browser: Option<&mut Browser>,
            _identifier: ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
            _selection_rect: Option<&Rect>,
            active_match_ordinal: ::std::os::raw::c_int,
            final_update: ::std::os::raw::c_int,
        ) {
            push(&self.events, browser, RawEvent::Find(count, active_match_ordinal, final_update != 0));
        }
    }
}

wrap_display_handler! {
    struct EngineDisplayHandler {
        events: EventQueue,
    }

    impl DisplayHandler {
        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            let (Some(browser), Some(urls)) = (browser, icon_urls) else { return; };
            // clone() produces an immutable borrowed wrapper, whose consuming
            // iterator is empty. Move the callback's mutable wrapper instead.
            let Some(url) = std::mem::take(urls).into_iter().next() else { return; };
            let Some(host) = browser.host() else { return; };
            let Some(frame) = browser.main_frame() else { return; };
            let mut callback = FaviconDownload::new(self.events.clone(), browser.identifier(), CefString::from(&frame.url()).to_string());
            host.download_image(Some(&CefString::from(url.as_str())), 1, 64, 0, Some(&mut callback));
        }

        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(CefString::to_string).unwrap_or_default();
            push(&self.events, browser, RawEvent::Title(title));
        }

        fn on_status_message(&self, browser: Option<&mut Browser>, value: Option<&CefString>) {
            push(&self.events, browser, RawEvent::Status(value.map(CefString::to_string).unwrap_or_default()));
        }

        fn on_address_change(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut cef::Frame>,
            url: Option<&CefString>,
        ) {
            if frame.is_some_and(|frame| frame.is_main() == 1) {
                let url = url.map(CefString::to_string).unwrap_or_default();
                // CEF reports the underlying document URL for source views.
                // Its visible entry retains the source-mode prefix, including
                // after redirects and back/forward navigation.
                let url = browser.as_ref().and_then(|browser| browser.host())
                    .and_then(|host| host.visible_navigation_entry())
                    .map(|entry| CefString::from(&entry.display_url()).to_string())
                    .filter(|display| display.starts_with("view-source:"))
                    .unwrap_or(url);
                push(&self.events, browser, RawEvent::Url(url));
            }
        }
    }
}

wrap_focus_handler! {
    struct EngineFocusHandler { events: EventQueue, }
    impl FocusHandler {
        fn on_got_focus(&self, browser: Option<&mut Browser>) { push(&self.events, browser, RawEvent::Focused); }
    }
}

wrap_download_image_callback! {
    struct FaviconDownload {
        events: EventQueue,
        browser_id: i32,
        page_url: String,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(&self, _image_url: Option<&CefString>, _http_status_code: i32, image: Option<&mut Image>) {
            let Some(image) = image.filter(|image| image.is_empty() == 0) else { return; };
            let (mut width, mut height) = (0, 0);
            let Some(data) = image.as_png(1.0, 1, Some(&mut width), Some(&mut height)) else { return; };
            if width <= 0 || height <= 0 || width > 128 || height > 128 || data.size() > 64 * 1024 { return; }
            let mut png = vec![0; data.size()];
            if data.data(Some(&mut png), 0) != png.len() { return; }
            self.events.lock().unwrap().push((self.browser_id, RawEvent::Favicon(self.page_url.clone(), png)));
        }
    }
}

wrap_load_handler! {
    struct EngineLoadHandler {
        events: EventQueue,
        download_gate: Option<downloads::DownloadGate>,
    }

    impl LoadHandler {
        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _http_status_code: i32) {
            if let Some(frame) = frame.filter(|frame| frame.is_main() != 0) {
                if let Some(gate) = &self.download_gate {
                    gate.document_result(&CefString::from(&frame.url()).to_string(), true);
                }
            }
        }
        fn on_load_error(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, error_code: Errorcode, error_text: Option<&CefString>, failed_url: Option<&CefString>) {
            if frame.is_some_and(|frame| frame.is_main() != 0) {
                if let Some(gate) = &self.download_gate {
                    gate.document_result(&failed_url.map(CefString::to_string).unwrap_or_default(), false);
                }
                push(&self.events, browser, RawEvent::LoadError(
                    failed_url.map(CefString::to_string).unwrap_or_default(),
                    error_code.get_raw(),
                    error_text.map(CefString::to_string).unwrap_or_default(),
                ));
            }
        }
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            let event = RawEvent::Loading(is_loading != 0, can_go_back != 0, can_go_forward != 0);
            push(&self.events, browser, event);
        }
    }
}

wrap_life_span_handler! {
    struct EngineLifeSpanHandler {
        events: EventQueue,
        policy: Arc<Mutex<PeekPolicy>>,
        lifecycle: shutdown::Queue,
        download_gate: Option<downloads::DownloadGate>,
    }

    impl LifeSpanHandler {
        /// Popups become tabs instead of separate native windows.
        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut cef::Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let url = target_url.map(CefString::to_string).unwrap_or_default();
            if !url.is_empty() {
                push(&self.events, browser, if user_gesture != 0 && self.policy.lock().unwrap().shift_click && target_disposition == WindowOpenDisposition::NEW_WINDOW { RawEvent::Peek(url) } else { RawEvent::Popup(url) });
            }
            1
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                shutdown::enqueue(&self.lifecycle, shutdown::Callback::Created(browser.identifier()));
            }
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(gate) = &self.download_gate { gate.seal(); }
            if let Some(browser) = browser {
                shutdown::enqueue(&self.lifecycle, shutdown::Callback::Closed(browser.identifier()));
            }
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            // CEF154 Linux CloseHostWindow targets CefWindowX11, not its parent.
            // Its WM_DELETE_WINDOW path must run to call WindowDestroyed; raw
            // XDestroyWindow alone does not implement that CEF callback path.
            #[cfg(target_os = "linux")]
            { let _ = browser; 0 }
            #[cfg(not(target_os = "linux"))]
            {
                if let Some(browser) = browser {
                    let child = browser.host().map(|host| host.window_handle() as usize).unwrap_or(0);
                    shutdown::enqueue(&self.lifecycle, shutdown::Callback::DestroyChild(browser.identifier(), child));
                }
                // macOS/Windows default close targets the GPUI top-level window.
                1
            }
        }
    }
}

/// Windows virtual key codes CEF reports for arrow keys.
const VKEY_TAB: i32 = 0x09;
const VKEY_LEFT: i32 = 0x25;
const VKEY_UP: i32 = 0x26;
const VKEY_RIGHT: i32 = 0x27;
const VKEY_DOWN: i32 = 0x28;
const VKEY_F12: i32 = 0x7b;

fn shortcut_for(event: &KeyEvent) -> Shortcut {
    let flags = event.modifiers;
    let has = |flag: sys::cef_event_flags_t| flags & flag.0 != 0;
    let key = match event.windows_key_code {
        0x1b => "escape".to_string(),
        VKEY_TAB => "tab".to_string(),
        VKEY_LEFT => "left".to_string(),
        VKEY_UP => "up".to_string(),
        VKEY_RIGHT => "right".to_string(),
        VKEY_DOWN => "down".to_string(),
        VKEY_F12 => "f12".to_string(),
        _ => char::from_u32(u32::from(event.unmodified_character))
            .map(|c| c.to_lowercase().to_string())
            .unwrap_or_default(),
    };
    Shortcut {
        cmd: has(sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN),
        ctrl: has(sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN),
        alt: has(sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN),
        shift: has(sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN),
        key,
    }
}

fn intercepted_shortcut(event: &KeyEvent, shortcuts: &[Shortcut]) -> Option<Shortcut> {
    if event.type_ != KeyEventType::RAWKEYDOWN { return None; }
    let shortcut = shortcut_for(event);
    // Chromium owns text editing shortcuts. Only a non-editable page
    // delegates Undo/Redo to the browser's sidebar history.
    if shortcut.key == "z" && event.focus_on_editable_field != 0 { return None; }
    shortcuts.contains(&shortcut).then_some(shortcut)
}

wrap_keyboard_handler! {
    struct EngineKeyboardHandler {
        events: EventQueue,
        shortcuts: Shortcuts,
        policy: Arc<Mutex<PeekPolicy>>,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let Some(event) = event else {
                return 0;
            };
            let escape = shortcut_for(event);
            let peek_escape = self.policy.lock().unwrap().capture_escape && event.type_ == KeyEventType::RAWKEYDOWN && escape.key == "escape" && !escape.cmd && !escape.ctrl && !escape.alt && !escape.shift;
            let Some(shortcut) = peek_escape.then_some(escape).or_else(|| intercepted_shortcut(event, &self.shortcuts)) else { return 0; };
            if shortcut.key == "z" {
                if let Some(browser) = browser.as_ref() { self.events.lock().unwrap().push((browser.identifier(), RawEvent::EditFocus(false))); }
            }
            push(&self.events, browser, RawEvent::Shortcut(shortcut));
            1
        }
    }
}

wrap_app! {
    struct EngineApp;

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            // Development builds are unsigned, so macOS asks for the keychain
            // password after every rebuild. Release builds use the real keychain.
            if cfg!(debug_assertions) {
                if let Some(command_line) = command_line {
                    command_line.append_switch(Some(&CefString::from("use-mock-keychain")));
                }
            }
        }
    }
}


/// Chromium expresses zoom logarithmically, while its UI uses these factors.
fn next_zoom_factor(current: f64, direction: i32) -> f64 {
    const FACTORS: &[f64] = &[0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0];
    if direction > 0 {
        FACTORS.iter().copied().find(|factor| *factor > current + 0.001).unwrap_or(5.0)
    } else if direction < 0 {
        FACTORS.iter().rev().copied().find(|factor| *factor < current - 0.001).unwrap_or(0.25)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_callbacks_never_become_tab_events() {
        let view = ViewId::from_raw(8);
        let role = ViewRole::Inspector(InspectorId(3));
        for event in [
            RawEvent::Title("DevTools".into()), RawEvent::Url(INSPECTOR_FRONTEND_URL.into()),
            RawEvent::Loading(false, false, false), RawEvent::Focused,
            RawEvent::EditFocus(true), RawEvent::Status("internal".into()),
            RawEvent::Popup("https://example.com".into()), RawEvent::Shortcut(Shortcut::default()),
            RawEvent::Find(1, 1, true), RawEvent::Favicon("internal".into(), vec![]),
            RawEvent::LoadError(INSPECTOR_FRONTEND_URL.into(), -302, "Unknown scheme".into()),
        ] { assert!(page_event(view, role, event).is_none()); }
        assert_eq!(page_event(view, ViewRole::Page, RawEvent::Title("Page".into())), Some(Event::TitleChanged { view, title: "Page".into() }));
    }

    #[test]
    fn inspector_shortcuts_and_focus_route_separately_from_tab_metadata() {
        let inspector = InspectorId(7);
        for key in ["i", "j", "c", "t", "w", "f12"] {
            let shortcut = Shortcut { key: key.into(), ..Default::default() };
            assert_eq!(inspector_input(inspector, &RawEvent::Shortcut(shortcut.clone())), Some(InspectorEvent::Shortcut { inspector, shortcut: shortcut.clone() }));
            assert_eq!(page_event(ViewId::from_raw(1), ViewRole::Inspector(inspector), RawEvent::Shortcut(shortcut)), None);
        }
        assert_eq!(inspector_input(inspector, &RawEvent::Focused), Some(InspectorEvent::Focused { inspector }));
        for metadata in [RawEvent::Title("private frontend title".into()), RawEvent::Url(INSPECTOR_FRONTEND_URL.into()), RawEvent::EditFocus(true), RawEvent::Loading(false, false, false)] { assert_eq!(inspector_input(inspector, &metadata), None); }
    }

    #[test]
    fn cef_function_key_without_a_character_maps_to_f12_on_every_platform() {
        let shortcut = Shortcut { key: "f12".into(), ..Default::default() };
        let event = KeyEvent { type_: KeyEventType::RAWKEYDOWN, windows_key_code: 0x7b, unmodified_character: 0, ..Default::default() };
        assert_eq!(shortcut_for(&event), shortcut);
        assert_eq!(intercepted_shortcut(&event, &[shortcut.clone()]), Some(shortcut.clone()));
        assert_eq!(inspector_input(InspectorId(1), &RawEvent::Shortcut(shortcut.clone())), Some(InspectorEvent::Shortcut { inspector: InspectorId(1), shortcut: shortcut.clone() }));
        assert_eq!(page_event(ViewId::from_raw(1), ViewRole::Page, RawEvent::Shortcut(shortcut.clone())), Some(Event::Shortcut { view: ViewId::from_raw(1), shortcut }));
    }

    #[test]
    fn editable_web_fields_own_undo_redo_while_browser_shortcuts_still_work() {
        let undo = Shortcut { cmd: true, key: "z".into(), ..Default::default() };
        let redo = Shortcut { shift: true, ..undo.clone() };
        let pin = Shortcut { key: "d".into(), ..undo.clone() };
        let shortcuts = [undo.clone(), redo.clone(), pin.clone()];
        let mut event = KeyEvent { type_: KeyEventType::RAWKEYDOWN, windows_key_code: b'Z'.into(), unmodified_character: b'z'.into(), modifiers: sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0, ..Default::default() };
        assert_eq!(intercepted_shortcut(&event, &shortcuts), Some(undo));
        event.focus_on_editable_field = 1;
        assert_eq!(intercepted_shortcut(&event, &shortcuts), None);
        event.modifiers |= sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0;
        assert_eq!(intercepted_shortcut(&event, &shortcuts), None);
        event.focus_on_editable_field = 0;
        assert_eq!(intercepted_shortcut(&event, &shortcuts), Some(redo));
        event.focus_on_editable_field = 1;
        event.modifiers &= !sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0;
        event.windows_key_code = b'D'.into();
        event.unmodified_character = b'd'.into();
        assert_eq!(intercepted_shortcut(&event, &shortcuts), Some(pin));
        event.type_ = KeyEventType::KEYUP;
        assert_eq!(intercepted_shortcut(&event, &shortcuts), None);
    }

    #[test]
    fn zoom_steps_handle_rounding_custom_levels_limits_and_reset() {
        assert_eq!(next_zoom_factor(1.0, 1), 1.1);
        assert_eq!(next_zoom_factor(1.1 - 1e-12, 1), 1.25);
        assert_eq!(next_zoom_factor(1.0, -1), 0.9);
        assert_eq!(next_zoom_factor(1.13, -1), 1.1);
        assert_eq!(next_zoom_factor(1.13, 1), 1.25);
        assert_eq!(next_zoom_factor(5.0, 1), 5.0);
        assert_eq!(next_zoom_factor(0.25, -1), 0.25);
        assert_eq!(next_zoom_factor(2.5, 0), 1.0);
    }
}

#[cfg(test)]
mod view_storage_lifetime_tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    // Exercise the actual View ownership with inert native payloads. This does
    // not initialize CEF or assert native cookie/disk persistence behavior.
    fn view<C>(role: ViewRole, context: Rc<C>) -> View<(), C> {
        View {
            action_lifetime: Rc::new(()),
            role,
            browser: (),
            context,
            parent: std::ptr::null_mut(),
            host: std::ptr::null_mut(),
            frame: Bounds::default(),
            needs_layout: false,
            rounded_top: true,
            link_policy: Arc::default(),
        }
    }

    #[test]
    fn private_page_in_normal_pool_keeps_weak_registry_lease_alive() {
        let normal_pool_context = Rc::new(Cell::new(0));
        let private_source_context = Rc::new(Cell::new(7));
        // Same weak ownership used by Runtime.private; native-handle clones
        // alone do not keep this particular Rc allocation upgradeable.
        let private_registry = Rc::downgrade(&private_source_context);
        let public_handle_context = private_source_context.clone();
        let page = view(ViewRole::Page, public_handle_context.clone());
        drop(private_source_context);
        drop(public_handle_context);

        // A subsequent start_private must find the existing lease while a
        // private page remains open in the otherwise normal engine pool.
        let next_private_context = private_registry.upgrade().expect("open page lost private registry lease");
        assert!(Rc::ptr_eq(&next_private_context, &page.context));
        next_private_context.set(8);
        assert_eq!(page.context.get(), 8);
        assert_eq!(normal_pool_context.get(), 0);
        drop(next_private_context);
        drop(page);
        assert!(private_registry.upgrade().is_none());
    }

    #[test]
    fn inspector_frontend_retains_exact_inspected_page_lease() {
        let source_context = Rc::new(Cell::new(7));
        let private_registry = Rc::downgrade(&source_context);
        let page = view(ViewRole::Page, source_context.clone());
        // Inspector creation derives its lease from the page, not the pool's
        // default context and not a new Rc around a native RequestContext clone.
        let frontend = view(ViewRole::Inspector(InspectorId::from_raw(1)), page.context.clone());
        assert!(Rc::ptr_eq(&page.context, &frontend.context));
        drop(source_context);
        drop(page);
        let retained = private_registry.upgrade().expect("frontend lost inspected storage lease");
        assert!(Rc::ptr_eq(&retained, &frontend.context));
        assert_eq!(retained.get(), 7);
        drop(retained);
        drop(frontend);
        assert!(private_registry.upgrade().is_none());
    }
}

#[cfg(test)]
mod ordered_input_tests {
    use super::*;

    #[test]
    fn browser_drain_keeps_both_focus_orders_and_shortcuts_in_one_stream() {
        let page = ViewId::from_raw(1);
        let frontend = ViewId::from_raw(2);
        let inspector = InspectorId::from_raw(3);
        let shortcut = Shortcut { cmd: true, key: "w".into(), ..Default::default() };
        for frontend_last in [false, true] {
            let page_focus = (page, ViewRole::Page, RawEvent::Focused);
            let frontend_focus = (frontend, ViewRole::Inspector(inspector), RawEvent::Focused);
            let callbacks = if frontend_last {
                vec![page_focus, frontend_focus, (frontend, ViewRole::Inspector(inspector), RawEvent::Shortcut(shortcut.clone()))]
            } else {
                vec![frontend_focus, page_focus, (page, ViewRole::Page, RawEvent::Shortcut(shortcut.clone()))]
            };
            let events: Vec<_> = callbacks.into_iter().filter_map(|(view, role, event)| browser_input(view, role, &event)).collect();
            let page_focus = Event::Focused { view: page };
            let frontend_focus = Event::InspectorInput(InspectorEvent::Focused { inspector });
            let expected = if frontend_last {
                vec![page_focus, frontend_focus, Event::InspectorInput(InspectorEvent::Shortcut { inspector, shortcut: shortcut.clone() })]
            } else {
                vec![frontend_focus, page_focus, Event::Shortcut { view: page, shortcut: shortcut.clone() }]
            };
            assert_eq!(events, expected);
        }
        assert!(browser_input(frontend, ViewRole::Inspector(inspector), &RawEvent::Title("DevTools".into())).is_none());
    }
}

#[cfg(test)]
mod peek_tests {
    use super::*;
    #[test]
    fn pinned_links_only_peek_on_user_cross_host_navigation() {
        let a = "https://a.example/path";
        assert!(peek_cross_domain(a,"https://b.example/path",true,true,false));
        for (url,pinned,gesture,redirect) in [
            ("https://a.example/other",true,true,false), ("https://a.example/#hash",true,true,false),
            ("https://b.example",false,true,false), ("https://b.example",true,false,false),
            ("https://b.example",true,true,true), ("javascript:alert(1)",true,true,false),
            ("file:///tmp/a",true,true,false), ("mailto:a@example.com",true,true,false),
        ] { assert!(!peek_cross_domain(a,url,pinned,gesture,redirect),"{url}"); }
    }
    #[test]
    fn escape_has_a_named_shortcut_instead_of_a_control_character() {
        let event = KeyEvent { windows_key_code: 0x1b, unmodified_character: 0x1b, ..Default::default() };
        assert_eq!(shortcut_for(&event).key,"escape");
        assert!(intercepted_shortcut(&event,&[]).is_none());
    }
}
