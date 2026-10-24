//! Keeps the engine's web views in sync with the browser model: one view per
//! opened page, with the active tab or Split View laid out over the content card.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::c_void;
use std::time::{Duration, Instant};
use std::{cell::RefCell, rc::Rc, sync::Arc};

use browser_core::{Browser, TabId, UserProfileId};
use engine::{Bounds, Engine, Event, ViewId};

#[path = "devtools.rs"]
pub(crate) mod devtools;

/// Storage selection at the browser/engine boundary. Current preserves the
/// independent ephemeral context of Incognito; normal profiles never use it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StorageSelection { Current, Default, Named(String) }

impl StorageSelection {
    pub fn for_profile(id: UserProfileId) -> Self {
        id.storage_key().map_or(Self::Default, Self::Named)
    }
}

fn retarget_page_metadata(event: &mut Event, previous: ViewId, current: ViewId) -> bool {
    let view = match event {
        Event::TitleChanged { view, .. } | Event::Favicon { view, .. } | Event::EditFocusChanged { view, .. }
        | Event::FindResult { view, .. } | Event::UrlChanged { view, .. } | Event::StatusChanged { view, .. }
        | Event::LoadingProgress { view, .. } | Event::LoadingChanged { view, .. } => view,
        _ => return false,
    };
    if *view != previous { return false; }
    *view = current;
    true
}

pub(crate) fn event_view(event: &Event) -> Option<ViewId> {
    match event {
        Event::OpenInNewTab { opener, .. } | Event::OpenInBackgroundTab { opener, .. } | Event::OpenPeek { opener, .. } => Some(*opener),
        Event::ContextImage { view, .. } | Event::PageContext { view, .. } | Event::QuoteSelection { view, .. } | Event::BoostZap { view, .. } | Event::TitleChanged { view, .. } | Event::Favicon { view, .. } | Event::PictureInPictureReturnRequested { view } | Event::Focused { view }
        | Event::EditFocusChanged { view, .. } | Event::FindResult { view, .. } | Event::UrlChanged { view, .. }
        | Event::MediaStateChanged { view, .. } | Event::AudioStateChanged { view, .. } | Event::StatusChanged { view, .. } | Event::LoadingProgress { view, .. } | Event::LoadingChanged { view, .. } => Some(*view),
        Event::PermissionRequested { view, .. } | Event::PermissionDismissed { view, .. } | Event::Shortcut { view, .. } => Some(*view),
        Event::InspectorInput(_) => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageIdentity {
    pub tab: TabId,
    pub profile: UserProfileId,
    pub generation: u64,
}

/// Only Pending authorizes automatic clock retries. Resolution/native errors
/// are Failed, never a request to retry or fall back to another storage context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageCreation { Created(ViewId), Pending, Failed }

/// A prepared request owns native leases, never Pages or the application.
/// Invoke and drop it outside all application/model/engine borrows.
pub type NativePrint = Box<dyn FnMut() -> engine::PrintDispatch>;
pub type NativePaste = Box<dyn FnMut() -> engine::PasteDispatch>;

/// What the window needs from a web engine. Implemented by Chromium in the
/// app and by a recording fake in tests.
pub trait WebEngine {
    fn permission_pending(&self, _view: ViewId, _request: engine::PermissionRequestId) -> bool { false }
    fn respond_permission(&mut self, _view: ViewId, _request: engine::PermissionRequestId, _decision: engine::PermissionDecision) -> bool { false }
    fn request_quote(&mut self, _view: ViewId, _request: u64) -> bool { false }
    fn site_info(&self, _view: ViewId) -> Option<engine::SiteInfo> { None }
    fn set_site_permission(&mut self, _view: ViewId, _origin: &str, _kind: engine::PermissionKind, _value: engine::PermissionValue) -> bool { false }
    fn clear_site_data(&mut self, _view: ViewId, _origin: &str, _kind: engine::ClearSiteData) -> bool { false }
    fn zoom_percent(&self, _view: ViewId) -> u16 { 100 }

    fn set_audio_volume(&mut self, _view: ViewId, _volume: f32) -> bool { false }
    fn audio_volume(&self, _view: ViewId) -> Option<f32> { None }
    fn set_picture_in_picture(&mut self, _view: ViewId, _enabled: bool) -> bool { false }
    fn media_action(&mut self, _view: ViewId, _action: engine::MediaAction) -> bool { false }
    fn set_boost_picker(&mut self, _view: ViewId, _enabled: bool) {}
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> { None }
    fn transfer_view(&mut self, _view: ViewId, _target: &mut dyn WebEngine, _parent: *mut c_void, _bounds: Bounds) -> Option<ViewId> { None }
    fn set_site_boosts(&mut self, _storage: &StorageSelection, _boosts: Vec<engine::SiteBoost>) {}
    fn prepare_paste_and_match_style(&mut self, _view: ViewId) -> Option<NativePaste> { None }
    fn prepare_inspector_paste_and_match_style(&mut self, _id: engine::InspectorId) -> Option<NativePaste> { None }
    fn supports_printing(&self) -> bool { false }
    fn prepare_print(&mut self, _view: ViewId) -> Option<NativePrint> { None }
    /// Independent page/event ownership over this engine's regular storage.
    fn fork_with_shared_storage(&self) -> Option<Box<dyn WebEngine>> { None }
    fn fork_in_storage(&self, storage: &StorageSelection) -> Option<Box<dyn WebEngine>> {
        match storage { StorageSelection::Current | StorageSelection::Default => self.fork_with_shared_storage(), StorageSelection::Named(_) => None }
    }
    fn create_in_storage(&mut self, storage: &StorageSelection, parent: *mut c_void, url: &str, bounds: Bounds) -> Option<ViewId> {
        match storage { StorageSelection::Current | StorageSelection::Default => self.create_in_host(parent, url, bounds), StorageSelection::Named(_) => None }
    }
    fn create_page_in_storage(&mut self, storage: &StorageSelection, parent: *mut c_void, url: &str, bounds: Bounds) -> PageCreation {
        self.create_in_storage(storage, parent, url, bounds).map_or(PageCreation::Failed, PageCreation::Created)
    }
    /// Captured creation identity; default preserves existing storage behavior.
    fn create_page_with_identity(&mut self, _identity: PageIdentity, _source_space: browser_core::SpaceId,
        storage: &StorageSelection, parent: *mut c_void, url: &str, bounds: Bounds) -> PageCreation {
        self.create_page_in_storage(storage, parent, url, bounds)
    }
    /// Product commands stay disabled until the renderer host bridge is ready.
    fn supports_inspector(&self) -> bool { false }
    /// Reuse this page's frontend, selecting the requested panel. `inspect`
    /// requests element selection; it must never close the frontend.
    fn open_inspector(&mut self, _view: ViewId, _parent: *mut c_void, _bounds: Bounds, _panel: engine::InspectorPanel, _inspect: bool) -> Result<engine::InspectorId, String> { Err("Inspector bridge is unavailable".into()) }
    fn focus_inspector(&mut self, _id: engine::InspectorId) {}

    fn open_inspector_probe(&mut self, _view: ViewId, _parent: *mut c_void, _bounds: Bounds) -> Result<engine::InspectorId, String> { Err("Inspector probe is unsupported".into()) }
    fn inspector_status(&self, _id: engine::InspectorId) -> Option<engine::InspectorStatus> { None }
    fn set_inspector_frame(&mut self, _id: engine::InspectorId, _bounds: Bounds) {}
    fn set_inspector_visible(&mut self, _id: engine::InspectorId, _visible: bool) {}
    fn send_inspector_probe_message(&mut self, _id: engine::InspectorId, _message: &str) -> bool { false }
    fn take_inspector_events(&mut self) -> Vec<engine::InspectorEvent> { Vec::new() }
    fn close_inspector(&mut self, _id: engine::InspectorId) {}
    fn create(&mut self, url: &str, bounds: Bounds) -> Option<ViewId>;
    /// Whether transfers should wait briefly for an owner-viewport photograph.
    fn supports_snapshots(&self) -> bool { false }
    fn request_snapshot(&mut self, _view: ViewId, _request: u64) -> bool { false }
    fn take_snapshots(&mut self) -> Vec<engine::PageSnapshot> { Vec::new() }
    fn create_in_host(&mut self, _parent: *mut c_void, url: &str, bounds: Bounds) -> Option<ViewId> { self.create(url, bounds) }
    fn focus_ui_in_host(&mut self, _parent: *mut c_void) { self.focus_ui(); }
    /// Transfer an existing native page. Engines without native hosting reject it.
    fn move_view(&mut self, _view: ViewId, _parent: *mut c_void, _bounds: Bounds) -> bool { false }
    fn set_bounds(&mut self, view: ViewId, bounds: Bounds);
    fn set_top_corners_rounded(&mut self, view: ViewId, rounded: bool);
    fn copy_context_image(&mut self, _view:ViewId, _request:u64, _page_url:&str, _url:&str)->bool {false}
    fn set_page_context_policy(&mut self, _view: ViewId, _policy: engine::page_context::Policy) {}
    fn set_peek_policy(&mut self, _view: ViewId, _policy: engine::PeekPolicy) {}
    fn set_corner_radius(&mut self, _view: ViewId, _radius: f64) {}
    fn set_visible(&mut self, view: ViewId, visible: bool);
    /// Keyboard focus to the page.
    fn focus_page(&mut self, view: ViewId);
    /// Keyboard focus to the browser UI (e.g. the command bar).
    fn focus_ui(&mut self);
    fn navigate(&mut self, view: ViewId, url: &str);
    fn go_back(&mut self, view: ViewId);
    fn go_forward(&mut self, view: ViewId);
    fn undo(&mut self, view: ViewId);
    fn redo(&mut self, view: ViewId);
    fn reload(&mut self, view: ViewId);
    fn force_reload(&mut self, view: ViewId);
    fn stop_loading(&mut self, view: ViewId);
    fn set_audio_muted(&mut self, view: ViewId, muted: bool);
    fn audio_muted(&self, view: ViewId) -> Option<bool>;
    fn set_color_scheme(&mut self, _view: ViewId, _dark: Option<bool>) -> bool { true }
    fn zoom(&mut self, view: ViewId, direction: i32);
    fn find(&mut self, view: ViewId, text: &str, forward: bool, next: bool);
    fn stop_finding(&mut self, view: ViewId);
    fn close(&mut self, view: ViewId);
    /// Runs engine work. Takes no `self`: the engine may dispatch native UI
    /// events from inside it, which re-enter the window and its pages.
    fn pump_fn(&self) -> fn();
    /// Preserve native page/frontend input order in this stream. Frontend focus
    /// and shortcuts use Event::InspectorInput, not the diagnostic event drain.
    fn take_events(&mut self) -> Vec<Event>;
}

/// Chromium views hosted in a native parent view (`NSView*` on macOS).
pub struct Chromium {
    pub engine: Engine,
    pub parent: *mut c_void,
}

/// macOS window corner radius (measured on macOS 26+).
pub const WINDOW_CORNER_RADIUS: f32 = 16.0;
/// Gap between the window edge and the content card.
pub const CONTENT_INSET: f32 = 10.0;
/// Concentric with the window corner: inner radius = outer radius - inset.
pub const CONTENT_CORNER_RADIUS: f32 = WINDOW_CORNER_RADIUS - CONTENT_INSET;

impl Chromium {
    fn resolve_storage(&self, storage: &StorageSelection) -> Result<engine::StorageContext, String> {
        if crate::quit::is_quitting() { return Err("Browser is quitting".into()); }
        match storage {
            StorageSelection::Current => Ok(self.engine.storage_context()),
            StorageSelection::Default => self.engine.default_storage_context(),
            StorageSelection::Named(key) => self.engine.named_storage_context(&engine::StorageKey::new(key)?),
        }
    }
}

impl WebEngine for Chromium {
    fn permission_pending(&self, view: ViewId, request: engine::PermissionRequestId) -> bool { self.engine.permission_pending(view, request) }
    fn respond_permission(&mut self, view: ViewId, request: engine::PermissionRequestId, decision: engine::PermissionDecision) -> bool { self.engine.respond_permission(view, request, decision) }
    fn request_quote(&mut self, view: ViewId, request: u64) -> bool { self.engine.request_quote(view, request) }
    fn site_info(&self, view: ViewId) -> Option<engine::SiteInfo> { self.engine.site_info(view) }
    fn set_site_permission(&mut self, view: ViewId, origin: &str, kind: engine::PermissionKind, value: engine::PermissionValue) -> bool { self.engine.set_site_permission(view, origin, kind, value) }
    fn clear_site_data(&mut self, view: ViewId, origin: &str, kind: engine::ClearSiteData) -> bool { self.engine.clear_site_data(view, origin, kind) }
    fn zoom_percent(&self, view: ViewId) -> u16 { self.engine.zoom_percent(view) }

    fn set_audio_volume(&mut self, view: ViewId, volume: f32) -> bool { self.engine.set_audio_volume(view,volume) }
    fn audio_volume(&self, view: ViewId) -> Option<f32> { self.engine.audio_volume(view) }
    fn set_picture_in_picture(&mut self, view: ViewId, enabled: bool) -> bool { self.engine.set_picture_in_picture(view, enabled) }
    fn media_action(&mut self, view: ViewId, action: engine::MediaAction) -> bool { self.engine.media_action(view, action) }
    fn set_boost_picker(&mut self, view: ViewId, enabled: bool) { self.engine.set_boost_picker(view,enabled); }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> { Some(self) }
    fn transfer_view(&mut self, view: ViewId, target: &mut dyn WebEngine, parent: *mut c_void, bounds: Bounds) -> Option<ViewId> {
        let target = target.as_any_mut()?.downcast_mut::<Self>()?;
        self.engine.transfer_view(view, &mut target.engine, parent, bounds)
    }
    fn set_corner_radius(&mut self, view: ViewId, radius: f64) { self.engine.set_corner_radius(view, radius); }
    fn copy_context_image(&mut self, view:ViewId, request:u64, page_url:&str, url:&str)->bool {self.engine.copy_context_image(view,request,page_url,url)}
    fn set_page_context_policy(&mut self, view: ViewId, policy: engine::page_context::Policy) { self.engine.set_page_context_policy(view, policy); }
    fn set_peek_policy(&mut self, view: ViewId, policy: engine::PeekPolicy) { self.engine.set_peek_policy(view, policy); }
    fn set_site_boosts(&mut self, storage: &StorageSelection, boosts: Vec<engine::SiteBoost>) {
        if let Ok(context) = self.resolve_storage(storage) {
            if let Err(error) = self.engine.set_site_boosts(&context, boosts) { eprintln!("Cannot configure Boosts: {error}"); }
        }
    }
    fn prepare_paste_and_match_style(&mut self, view: ViewId) -> Option<NativePaste> {
        let mut request = self.engine.prepare_paste_and_match_style(view)?;
        Some(Box::new(move || request.try_dispatch()))
    }
    fn prepare_inspector_paste_and_match_style(&mut self, id: engine::InspectorId) -> Option<NativePaste> {
        let mut request = self.engine.prepare_inspector_paste_and_match_style(id)?;
        Some(Box::new(move || request.try_dispatch()))
    }
    fn supports_printing(&self) -> bool { self.engine.supports_printing() }
    fn prepare_print(&mut self, view: ViewId) -> Option<NativePrint> {
        let mut request = self.engine.prepare_print(view)?;
        Some(Box::new(move || request.try_dispatch()))
    }
    fn fork_in_storage(&self, storage: &StorageSelection) -> Option<Box<dyn WebEngine>> {
        let context = self.resolve_storage(storage).ok()?;
        let engine = self.engine.fork_with_context(&context).ok()?;
        Some(Box::new(Self { engine, parent: std::ptr::null_mut() }))
    }
    fn create_in_storage(&mut self, storage: &StorageSelection, parent: *mut c_void, url: &str, bounds: Bounds) -> Option<ViewId> {
        match self.create_page_in_storage(storage, parent, url, bounds) {
            PageCreation::Created(view) => Some(view), PageCreation::Pending | PageCreation::Failed => None,
        }
    }
    fn create_page_in_storage(&mut self, storage: &StorageSelection, parent: *mut c_void, url: &str, bounds: Bounds) -> PageCreation {
        let context = match self.resolve_storage(storage) {
            Ok(context) => context,
            Err(error) => { eprintln!("Cannot resolve page storage: {error}"); return PageCreation::Failed; }
        };
        if !context.is_ready() { return PageCreation::Pending; }
        let parent = if parent.is_null() { self.parent } else { parent };
        match self.engine.create_view_in_context(&context, parent, url, bounds, f64::from(CONTENT_CORNER_RADIUS)) {
            Ok(view) => PageCreation::Created(view),
            Err(error) => { eprintln!("Cannot create page: {error}"); PageCreation::Failed }
        }
    }

    fn create_page_with_identity(&mut self, identity: PageIdentity, source_space: browser_core::SpaceId,
        storage: &StorageSelection, parent: *mut c_void, url: &str, bounds: Bounds) -> PageCreation {
        use crate::window::download_probe::admission::{self, Decision};
        let parent = if parent.is_null() { self.parent } else { parent };
        let candidate = match admission::inspect(identity, source_space, storage, parent as usize, url) {
            Decision::Ordinary => return self.create_page_in_storage(storage, parent, url, bounds),
            Decision::Rejected => return PageCreation::Failed,
            Decision::Candidate(candidate) => candidate,
        };
        // Uses the same guarded synchronous creator as ordinary pages. No pump,
        // destination/control effects, parking, or close preparation runs here.
        let context = match self.resolve_storage(storage) {
            Ok(context) => context,
            Err(_) => { admission::fail(&candidate); return PageCreation::Failed; }
        };
        if !context.is_ready() { return PageCreation::Pending; }
        let Some(owner) = admission::begin(&candidate) else { return PageCreation::Failed; };
        let result = self.engine.create_view_in_context_with_download_owner(
            &context, owner, parent, url, bounds, f64::from(CONTENT_CORNER_RADIUS));
        admission::finish(&candidate, result.is_ok());
        match result {
            Ok(view) => PageCreation::Created(view),
            Err(_) => { eprintln!("[download-probe] explicit fixture creation failed"); PageCreation::Failed }
        }
    }

    fn fork_with_shared_storage(&self) -> Option<Box<dyn WebEngine>> {
        if crate::quit::is_quitting() { return None; }
        Some(Box::new(Self { engine: self.engine.fork_with_shared_storage(), parent: std::ptr::null_mut() }))
    }
    fn supports_inspector(&self) -> bool { true }
    fn open_inspector(&mut self, view: ViewId, parent: *mut c_void, bounds: Bounds, panel: engine::InspectorPanel, inspect: bool) -> Result<engine::InspectorId, String> {
        let id = self.engine.open_inspector_probe(view, parent, bounds, panel)?;
        // The engine accepts and coalesces inspect toggles before FrontendReady.
        if inspect && !self.engine.toggle_inspector_element_mode(id) { self.engine.close_inspector(id); return Err("Inspector did not accept element selection".into()); }
        Ok(id)
    }
    fn focus_inspector(&mut self, id: engine::InspectorId) { self.engine.focus_inspector(id); }

    fn open_inspector_probe(&mut self, view: ViewId, parent: *mut c_void, bounds: Bounds) -> Result<engine::InspectorId, String> {
        self.engine.open_inspector_probe(view, parent, bounds, engine::InspectorPanel::Elements)
    }
    fn inspector_status(&self, id: engine::InspectorId) -> Option<engine::InspectorStatus> { self.engine.inspector_status(id) }
    fn set_inspector_frame(&mut self, id: engine::InspectorId, bounds: Bounds) { self.engine.set_inspector_frame(id, bounds); }
    fn set_inspector_visible(&mut self, id: engine::InspectorId, visible: bool) { self.engine.set_inspector_visible(id, visible); }
    fn send_inspector_probe_message(&mut self, id: engine::InspectorId, message: &str) -> bool { self.engine.send_inspector_probe_message(id, message) }
    fn take_inspector_events(&mut self) -> Vec<engine::InspectorEvent> { self.engine.take_inspector_events() }
    fn close_inspector(&mut self, id: engine::InspectorId) { self.engine.close_inspector(id); }
    fn supports_snapshots(&self) -> bool { true }
    fn request_snapshot(&mut self, view: ViewId, request: u64) -> bool { self.engine.request_snapshot(view, request) }
    fn take_snapshots(&mut self) -> Vec<engine::PageSnapshot> { self.engine.take_snapshots() }
    fn create_in_host(&mut self, parent: *mut c_void, url: &str, bounds: Bounds) -> Option<ViewId> {
        let parent = if parent.is_null() { self.parent } else { parent };
        self.engine.create_view(parent, url, bounds, f64::from(CONTENT_CORNER_RADIUS))
    }
    fn focus_ui_in_host(&mut self, parent: *mut c_void) { self.engine.focus_parent(if parent.is_null() { self.parent } else { parent }); }
    fn create(&mut self, url: &str, bounds: Bounds) -> Option<ViewId> {
        self.engine
            .create_view(self.parent, url, bounds, f64::from(CONTENT_CORNER_RADIUS))
    }
    fn set_bounds(&mut self, view: ViewId, bounds: Bounds) {
        self.engine.set_frame(view, bounds)
    }
    fn move_view(&mut self, view: ViewId, parent: *mut c_void, bounds: Bounds) -> bool {
        self.engine.move_view(view, parent, bounds)
    }
    fn set_top_corners_rounded(&mut self, view: ViewId, rounded: bool) {
        self.engine.set_top_corners_rounded(view, rounded)
    }
    fn set_visible(&mut self, view: ViewId, visible: bool) {
        self.engine.set_visible(view, visible)
    }
    fn focus_page(&mut self, view: ViewId) {
        self.engine.focus(view)
    }
    fn focus_ui(&mut self) {
        self.engine.focus_parent(self.parent)
    }
    fn navigate(&mut self, view: ViewId, url: &str) {
        self.engine.navigate(view, url)
    }
    fn go_back(&mut self, view: ViewId) {
        self.engine.go_back(view)
    }
    fn go_forward(&mut self, view: ViewId) {
        self.engine.go_forward(view)
    }
    fn undo(&mut self, view: ViewId) { self.engine.undo(view) }
    fn redo(&mut self, view: ViewId) { self.engine.redo(view) }
    fn reload(&mut self, view: ViewId) {
        self.engine.reload(view)
    }
    fn force_reload(&mut self, view: ViewId) { self.engine.force_reload(view) }
    fn stop_loading(&mut self, view: ViewId) { self.engine.stop_loading(view) }
    fn set_audio_muted(&mut self, view: ViewId, muted: bool) { self.engine.set_audio_muted(view, muted) }
    fn set_color_scheme(&mut self, view: ViewId, dark: Option<bool>) -> bool { self.engine.set_color_scheme(view, dark) }
    fn audio_muted(&self, view: ViewId) -> Option<bool> { self.engine.audio_muted(view) }
    fn zoom(&mut self, view: ViewId, direction: i32) { self.engine.zoom(view, direction) }
    fn find(&mut self, view: ViewId, text: &str, forward: bool, next: bool) { self.engine.find(view, text, forward, next) }
    fn stop_finding(&mut self, view: ViewId) { self.engine.stop_finding(view) }
    fn close(&mut self, view: ViewId) {
        self.engine.close(view)
    }
    fn pump_fn(&self) -> fn() {
        Engine::pump
    }
    fn take_events(&mut self) -> Vec<Event> {
        self.engine.take_browser_events()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageState {
    pub loading_progress: u16,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub status: String,
    pub editable: bool,
    pub audible: bool,
    pub media: engine::MediaState,
}

struct Page {
    input_attachment: u64,
    identity: PageIdentity,
    source_space: browser_core::SpaceId,
    owner: u64,
    frame: Bounds,
    visible: bool,
    view: ViewId,
    /// The URL the engine is showing or was last told to load.
    url: String,
    state: PageState,
    muted: bool,
    color_scheme: Option<bool>,
    snapshot: Option<(u64, Arc<Vec<u8>>)>,
    snapshot_pending: Option<u64>,
    snapshot_origin: Option<(u64, Bounds)>,
    snapshot_good_origin: Option<(u64, Bounds)>,
}

#[derive(Clone)]
struct PageIntent {
    identity: PageIdentity,
    source_space: browser_core::SpaceId,
    storage: StorageSelection,
    tab: TabId,
    url: String,
    muted: bool,
    weight: f32,
    internal: bool,
}

/// A claim belongs to one activation/selection and one unchanged source viewport.
/// Its request ID also acts as the generation: cancelled IDs are never reused.
struct Handoff {
    source: u64,
    target: u64,
    view: ViewId,
    frame: Bounds,
    request: u64,
    deadline: Instant,
    ready: bool,
}

const HANDOFF_TIMEOUT: Duration = Duration::from_millis(250);

/// Temporary native diagnostic dock; never part of the browser model or session.
#[derive(Clone, Copy)]
struct InspectorProbe {
    id: engine::InspectorId,
    tab: TabId,
    view: ViewId,
    host: u64,
}

/// Provenance is captured before a host retains a callback for another host.
/// A later owner must never stamp a replacement page onto an older callback.
#[derive(Clone)]
pub(crate) struct PageEvent {
    pub sequence: u64,
    pub event: Event,
    pub identity: PageIdentity,
    pub source_space: browser_core::SpaceId,
    pub input_origin: Option<InputOrigin>,
}

impl PageEvent {
    pub(crate) fn is_metadata(&self) -> bool {
        matches!(self.event, Event::TitleChanged { .. } | Event::Favicon { .. } | Event::UrlChanged { .. }
            | Event::MediaStateChanged { .. } | Event::AudioStateChanged { .. } | Event::StatusChanged { .. } | Event::LoadingProgress { .. } | Event::LoadingChanged { .. } | Event::EditFocusChanged { .. } | Event::FindResult { .. })
    }
}

/// Input belongs to one continuous host attachment, independently of the
/// document generation. A live document may move without carrying old input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InputOrigin {
    pub identity: PageIdentity,
    host: u64,
    attachment: u64,
    inspector: Option<engine::InspectorId>,
}

/// A GPUI document responder belongs to this exact Pages host and model
/// generation, even when the document has no native view (for example History).
#[derive(Clone)]
pub(crate) struct UiPageFocusOwner {
    store: std::rc::Weak<RefCell<PageStore>>,
    host: u64,
    parent: *mut c_void,
    identity: PageIdentity,
}

/// Focus intent at the start of a sidebar resize. This does not retain an engine
/// or authorize focus in a replacement Pages/store. UI restoration belongs to
/// the host, which must also validate its popup/inline generation and FocusHandle.
#[derive(Clone)]
pub(crate) struct ResizePageFocus {
    store: std::rc::Weak<RefCell<PageStore>>,
    host: u64,
    parent: *mut c_void,
    target: ResizeFocusTarget,
}

#[derive(Clone, Copy, PartialEq)]
enum ResizeFocusTarget {
    Ui,
    Unavailable,
    Native { origin: InputOrigin, view: ViewId, frontend: Option<(engine::InspectorId, u64)> },
}

impl ResizePageFocus {
    /// A live page/inspector intent was captured; restoration still revalidates
    /// its original owner, attachment, document and frontend generations.
    pub(crate) fn has_native_focus(&self) -> bool { matches!(self.target, ResizeFocusTarget::Native { .. }) }

    /// Only explicit UI intent permits the host to restore its GPUI handle.
    pub(crate) fn is_ui(&self) -> bool { matches!(self.target, ResizeFocusTarget::Ui) }
}

struct PageStore {
    #[cfg(test)]
    clock: Option<gpui::BackgroundExecutor>,
    inspector_probe: Option<InspectorProbe>,
    devtools: Option<devtools::Dock>,
    devtools_restore: HashSet<u64>,
    next_inspector_eval: u64,
    engine: Box<dyn WebEngine>,
    pages: HashMap<TabId, Page>,
    identities: HashMap<TabId, PageIdentity>,
    next_generation: u64,
    hosts: HashMap<u64, *mut c_void>,
    next_host: u64,
    active_host: u64,
    events: Vec<PageEvent>,
    next_event: u64,
    metadata: BTreeMap<u64, (PageEvent, Option<u64>)>,
    next_snapshot: u64,
    handoffs: HashMap<TabId, Handoff>,
    layouts: HashMap<u64, HashMap<TabId, Bounds>>,
    frozen: HashMap<(u64, TabId), Arc<Vec<u8>>>,
    mirror_revision: u64,
}

/// Native layout ownership is independent of the virtual sidebar selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PageViewport {
    #[default]
    Content,
    Hidden,
    LibraryMedia,
    LibraryDownloads,
}
impl PageViewport {
    fn retains_library_page(self) -> bool {
        matches!(self, Self::LibraryMedia | Self::LibraryDownloads)
    }
}

pub struct Pages {
    viewport: PageViewport,
    viewport_generation: u64,
    store: Rc<RefCell<PageStore>>,
    host: u64,
    parent: *mut c_void,
    page_focus: bool,
    page_has_focus: bool,
    devtools_focused: bool,
    preserve_ui_focus: bool,
    resize_focus_suppressed: bool,
    wanted: Vec<PageIntent>,
    wanted_focus: Option<TabId>,
    pip_selected: Option<TabId>,
    axis: browser_core::splits::Axis,
    split: bool,
    square_top: bool,
    top_toolbar: bool,
    pub(crate) peek_mode: bool,
    visible: HashSet<TabId>,
    shown: Option<TabId>,
    bounds: Option<Bounds>,
    seen_revision: u64,
    creation_pending: bool,
}

thread_local! {
    // Weak registration never extends an engine's normal lifetime. During quit it
    // reaches stores retained by deferred closures, splitters, or diagnostics.
    static STORES: RefCell<Vec<std::rc::Weak<RefCell<PageStore>>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn take_shutdown_engines() -> Result<Vec<Box<dyn WebEngine>>, String> {
    let stores = STORES.with(|stores| stores.borrow().iter().filter_map(std::rc::Weak::upgrade).collect::<Vec<_>>());
    // Check all stores before mutation. No CEF pumping happens under these borrows.
    for store in &stores { store.try_borrow_mut().map_err(|_| "Page store is still borrowed during quit".to_owned())?; }
    let mut engines = Vec::new();
    for store in stores {
        let mut store = store.borrow_mut();
        engines.push(std::mem::replace(&mut store.engine, Box::new(RetiredEngine)));
        store.pages.clear();
        store.identities.clear();
        store.events.clear();
        store.metadata.clear();
        store.devtools = None;
        store.inspector_probe = None;
        store.handoffs.clear();
        store.frozen.clear();
    }
    Ok(engines)
}

/// Stale UI closures may keep an inert layout alive, never a CEF owner.
struct RetiredEngine;
impl WebEngine for RetiredEngine {
    fn create(&mut self, _: &str, _: Bounds) -> Option<ViewId> { None }
    fn set_bounds(&mut self, _: ViewId, _: Bounds) {}
    fn set_top_corners_rounded(&mut self, _: ViewId, _: bool) {}
    fn set_visible(&mut self, _: ViewId, _: bool) {}
    fn focus_page(&mut self, _: ViewId) {}
    fn focus_ui(&mut self) {}
    fn navigate(&mut self, _: ViewId, _: &str) {}
    fn go_back(&mut self, _: ViewId) {}
    fn go_forward(&mut self, _: ViewId) {}
    fn undo(&mut self, _: ViewId) {}
    fn redo(&mut self, _: ViewId) {}
    fn reload(&mut self, _: ViewId) {}
    fn force_reload(&mut self, _: ViewId) {}
    fn stop_loading(&mut self, _: ViewId) {}
    fn set_audio_muted(&mut self, _: ViewId, _: bool) {}
    fn audio_muted(&self, _: ViewId) -> Option<bool> { None }
    fn zoom(&mut self, _: ViewId, _: i32) {}
    fn find(&mut self, _: ViewId, _: &str, _: bool, _: bool) {}
    fn stop_finding(&mut self, _: ViewId) {}
    fn close(&mut self, _: ViewId) {}
    fn pump_fn(&self) -> fn() { || {} }
    fn take_events(&mut self) -> Vec<Event> { Vec::new() }
}

impl Pages {
    pub fn new(engine: Box<dyn WebEngine>) -> Self {
        let pages = Self {
            store: Rc::new(RefCell::new(PageStore {
                #[cfg(test)]
                clock: None,
                devtools_restore: HashSet::new(), devtools: None, inspector_probe: None, next_inspector_eval: 0, engine, pages: HashMap::new(), identities: HashMap::new(), next_generation: 0, hosts: HashMap::from([(1, std::ptr::null_mut())]), next_host: 1, active_host: 1,
                events: Vec::new(), next_event: 0, metadata: BTreeMap::new(), next_snapshot: 0, handoffs: HashMap::new(), layouts: HashMap::new(), frozen: HashMap::new(), mirror_revision: 0 })),
            viewport: PageViewport::Content,
            viewport_generation: 0,
            host: 1,
            parent: std::ptr::null_mut(),
            page_focus: true,
            page_has_focus: false,
            devtools_focused: false,
            preserve_ui_focus: false,
            resize_focus_suppressed: false,
            wanted: Vec::new(),
            wanted_focus: None,
            pip_selected: None,
            axis: browser_core::splits::Axis::Horizontal,
            split: false,
            square_top: false, top_toolbar: false, peek_mode: false,
            visible: HashSet::new(),
            shown: None,
            bounds: None,
            seen_revision: 0,
            creation_pending: false,
        };
        STORES.with(|stores| {
            let mut stores = stores.borrow_mut();
            stores.retain(|store| store.strong_count() > 0);
            stores.push(Rc::downgrade(&pages.store));
        });
        pages
    }

    /// All hosts share the same controlled deadline clock in GPUI tests.
    #[cfg(test)]
    pub(crate) fn set_clock(&mut self, executor: gpui::BackgroundExecutor) {
        self.store.borrow_mut().clock = Some(executor);
    }

    /// Independent models may reuse TabIds, so they must never share PageStore.
    pub fn independent_with_shared_storage(&self, parent: *mut c_void) -> Option<Self> {
        if crate::quit::is_quitting() { return None; }
        let engine = self.store.borrow().engine.fork_with_shared_storage()?;
        let mut pages = Self::new(engine);
        pages.set_parent(parent);
        Some(pages)
    }

    pub fn independent_in_storage(&self, parent: *mut c_void, storage: &StorageSelection) -> Option<Self> {
        if crate::quit::is_quitting() { return None; }
        let engine = self.store.borrow().engine.fork_in_storage(storage)?;
        let mut pages = Self::new(engine);
        pages.set_parent(parent);
        Some(pages)
    }

    pub(crate) fn has_page(&self, tab: TabId) -> bool {
        self.store.borrow().pages.get(&tab).is_some_and(|page| page.owner == self.host)
    }

    /// Adopt a document from an independent lookup namespace. Never navigate
    /// or create a replacement browser. Rejected transfers leave the source live.
    pub(crate) fn transfer_page(&mut self, tab: TabId, target: &mut Self, destination: TabId, browser: &Browser) -> bool {
        if Rc::ptr_eq(&self.store, &target.store) || crate::quit::is_quitting() { return false; }
        target.reconcile_inventory(browser);
        let mut source = self.store.borrow_mut();
        let mut target_store = target.store.borrow_mut();
        let Some(identity) = target_store.identities.get(&destination).copied() else { return false; };
        let Some(page) = source.pages.get(&tab).filter(|page| page.owner == self.host && page.identity.profile == identity.profile) else { return false; };
        if target_store.pages.contains_key(&destination) { return false; }
        let frame = target.bounds.unwrap_or(page.frame);
        let original_view = page.view;
        let Some(view) = source.engine.transfer_view(original_view, &mut *target_store.engine, target.parent, frame) else { return false; };
        source.cancel_handoff(tab);
        let mut page = source.pages.remove(&tab).unwrap();
        let old_view = page.view;
        page.view = view;
        page.identity = identity;
        page.source_space = browser.space().id;
        page.owner = target.host;
        page.frame = frame;
        page.input_attachment = page.input_attachment.checked_add(1).expect("input attachment generations exhausted");
        page.snapshot = None; page.snapshot_pending = None; page.snapshot_origin = None; page.snapshot_good_origin = None;
        page.visible = false;
        // Metadata already in flight will update the adopted model on its next
        // poll. Until then reconciliation must not navigate to a stale model URL.
        if let Some(tab) = browser.all_tabs().find(|tab| tab.id == destination) { page.url = tab.url.clone(); }
        target_store.engine.set_visible(view, false);
        target_store.engine.set_corner_radius(view, if target.peek_mode { 8. } else { f64::from(CONTENT_CORNER_RADIUS) });
        target_store.pages.insert(destination, page);
        let metadata: Vec<_> = source.metadata.values().filter(|(event, _)| event.identity.tab == tab).map(|(event, _)| event.event.clone()).collect();
        source.metadata.retain(|_, (event, _)| event.identity.tab != tab);
        source.events.retain(|event| event.identity.tab != tab);
        source.identities.remove(&tab);
        source.frozen.retain(|(_, id), _| *id != tab);
        for layout in source.layouts.values_mut() { layout.remove(&tab); }
        for mut event in metadata {
            if retarget_page_metadata(&mut event, old_view, view) {
                if let Some(event) = target_store.capture_event(event) { target_store.events.push(event); }
            }
        }
        source.mirror_revision = source.mirror_revision.wrapping_add(1);
        target_store.mirror_revision = target_store.mirror_revision.wrapping_add(1);
        self.visible.remove(&tab);
        if self.shown == Some(tab) { self.shown = None; }
        target.visible.remove(&destination);
        target.shown = None;
        true
    }

    pub(crate) fn identity_for_tab(&self, tab: TabId) -> Option<PageIdentity> { self.view(tab).and_then(|view| self.identity_for(view)) }

    pub(crate) fn identity_for(&self, view: ViewId) -> Option<PageIdentity> {
        self.store.borrow().pages.values().find(|page| page.view == view).map(|page| page.identity)
    }

    pub(crate) fn inspector_input_identity(&self, input: &engine::InspectorEvent) -> Option<PageIdentity> {
        let store = self.store.borrow();
        let dock = store.inspector_input_owner(input).filter(|dock| dock.host == self.host)?;
        store.pages.get(&dock.tab).filter(|page| page.view == dock.view).map(|page| page.identity)
    }

    pub(crate) fn source_space_for(&self, view: ViewId) -> Option<browser_core::SpaceId> {
        self.store.borrow().pages.values().find(|page| page.view == view).map(|page| page.source_space)
    }

    pub(crate) fn copy_context_image(&mut self, tab:TabId, request:u64, page_url:&str, url:&str)->bool {
        let mut store=self.store.borrow_mut();
        let Some(view)=store.pages.get(&tab).filter(|page| page.owner==self.host && page.visible).map(|page|page.view) else{return false;};
        store.engine.copy_context_image(view,request,page_url,url)
    }
    pub(crate) fn capture_event(&self, event: Event) -> Option<PageEvent> {
        let mut store = self.store.borrow_mut();
        let event = store.capture_event(event)?;
        if let Some((_, host)) = store.metadata.get_mut(&event.sequence) { *host = Some(self.host); }
        Some(event)
    }

    /// Claim every unapplied metadata callback up to this event for its live
    /// document, including callbacks held behind another window's shortcut.
    /// Claiming atomically makes later copies in window queues harmless.
    pub(crate) fn claim_metadata_through(&self, event: &PageEvent) -> Vec<PageEvent> {
        let mut store = self.store.borrow_mut();
        if !store.pages.get(&event.identity.tab).is_some_and(|page| page.identity == event.identity) { return Vec::new(); }
        let sequences: Vec<_> = store.metadata.range(..=event.sequence)
            .filter(|(_, (pending, _))| pending.identity == event.identity).map(|(sequence, _)| *sequence).collect();
        sequences.into_iter().filter_map(|sequence| store.metadata.remove(&sequence).map(|(event, _)| event)).collect()
    }

    pub(crate) fn accepts_input_origin(&self, origin: InputOrigin) -> bool {
        let store = self.store.borrow();
        origin.host == self.host && store.hosts.contains_key(&origin.host)
            && store.pages.get(&origin.identity.tab).is_some_and(|page|
                page.identity == origin.identity && page.owner == origin.host && page.input_attachment == origin.attachment)
            && origin.inspector.is_none_or(|inspector| store.inspector_event_owner(inspector)
                .is_some_and(|dock| dock.host == origin.host && dock.tab == origin.identity.tab))
    }

    pub(crate) fn accepts_identity(&self, identity: PageIdentity) -> bool {
        self.store.borrow().pages.get(&identity.tab).is_some_and(|page| page.identity == identity)
    }

    /// A closed, undoable Peek keeps its renderer but must not play audio.
    pub(crate) fn mute_retained_document(&mut self) {
        let mut store = self.store.borrow_mut();
        let PageStore { engine, pages, .. } = &mut *store;
        for page in pages.values_mut() {
            engine.set_audio_muted(page.view, true);
            page.muted = true;
        }
    }

    /// Creates an independent layout over the same live pages and engine.
    /// The new host cannot take another window's page until explicitly activated.
    pub fn for_host(&self, parent: *mut c_void) -> Self {
        let mut store = self.store.borrow_mut();
        store.next_host += 1;
        let host = store.next_host;
        store.hosts.insert(host, parent);
        Self { viewport: PageViewport::Content, viewport_generation: 0, store: self.store.clone(), host, parent, page_focus: true, page_has_focus: false, devtools_focused: false, preserve_ui_focus: false, resize_focus_suppressed: false, wanted: Vec::new(), wanted_focus: None, pip_selected: None,
            axis: browser_core::splits::Axis::Horizontal, split: false, square_top: self.square_top, top_toolbar: false, peek_mode: false, visible: HashSet::new(), shown: None, bounds: None, seen_revision: 0, creation_pending: false }
    }

    pub fn set_parent(&mut self, parent: *mut c_void) {
        self.parent = parent;
        self.store.borrow_mut().hosts.insert(self.host, parent);
    }

    /// Called when a browser root becomes active, before reconciling its layout.
    pub fn activate_host(&mut self, browser: &Browser) {
        self.activate_host_with_visibility(browser, true);
    }

    /// Activating a virtual sidebar page must not restore native page views.
    pub fn activate_host_with_visibility(&mut self, browser: &Browser, visible: bool) {
        {
            let mut store = self.store.borrow_mut();
            if store.active_host != self.host { self.shown = None; }
            store.active_host = self.host;
            let cancelled: Vec<_> = store.handoffs.iter().filter(|(_, claim)| claim.target != self.host).map(|(tab, _)| *tab).collect();
            for tab in cancelled { store.cancel_handoff(tab); }
        }
        // Rebuild intent from the freshly projected model before transferring:
        // the previous window may have navigated while this host was inactive.
        self.preserve_ui_focus = !self.page_focus;
        self.sync(browser, visible);
        self.preserve_ui_focus = false;
    }

    /// Call before destroying the native parent. Unselected live pages are
    /// parked, hidden, in another surviving host until selected there.
    pub fn release_host(&mut self) {
        let mut store = self.store.borrow_mut();
        store.capture_pending_engine_events();
        if store.devtools.is_some_and(|dock| dock.host == self.host) { store.close_devtools(); }
        if store.inspector_probe.is_some_and(|probe| probe.host == self.host) { store.close_inspector_probe(); }
        if store.hosts.remove(&self.host).is_none() { return; }
        store.layouts.remove(&self.host);
        store.frozen.retain(|(host, _), _| *host != self.host);
        let cancelled: Vec<_> = store.handoffs.iter().filter(|(_, claim)| claim.source == self.host || claim.target == self.host).map(|(tab, _)| *tab).collect();
        for tab in cancelled { store.cancel_handoff(tab); }
        store.mirror_revision = store.mirror_revision.wrapping_add(1);
        let destination = store.hosts.iter().filter(|(_, parent)| !parent.is_null())
            .min_by_key(|(host, _)| **host).map(|(host, parent)| (*host, *parent));
        let owned: Vec<_> = store.pages.iter().filter(|(_, page)| page.owner == self.host).map(|(tab, _)| *tab).collect();
        for tab in owned {
            let PageStore { engine, pages, .. } = &mut *store;
            let page = pages.get_mut(&tab).unwrap();
            if let Some((host, _)) = destination.filter(|(_, parent)| engine.move_view(page.view, *parent, page.frame)) {
                engine.set_visible(page.view, false);
                page.visible = false;
                page.state.status.clear();
                page.input_attachment = page.input_attachment.checked_add(1).expect("input attachment generations exhausted");
                page.owner = host;
            } else {
                // A rejected native transfer must never leave a child attached
                // to a window that is about to be destroyed.
                engine.close(page.view);
                pages.remove(&tab);
                store.next_generation = store.next_generation.checked_add(1).expect("page generations exhausted");
                let generation = store.next_generation;
                if let Some(identity) = store.identities.get_mut(&tab) { identity.generation = generation; }
            }
        }
        if store.active_host == self.host { store.active_host = destination.map_or(0, |(host, _)| host); }
        self.visible.clear();
        self.shown = None;
    }

    pub fn site_info(&self, tab: TabId) -> Option<engine::SiteInfo> {
        let store = self.store.borrow();
        let page = store.pages.get(&tab).filter(|page| page.owner == self.host)?;
        store.engine.site_info(page.view)
    }
    pub fn set_site_permission(&mut self, tab: TabId, origin: &str, kind: engine::PermissionKind, value: engine::PermissionValue) -> bool {
        let mut store = self.store.borrow_mut();
        let Some(view) = store.pages.get(&tab).filter(|page| page.owner == self.host).map(|page| page.view) else { return false; };
        store.engine.set_site_permission(view, origin, kind, value)
    }
    pub fn clear_site_data(&mut self, tab: TabId, origin: &str, kind: engine::ClearSiteData) -> bool {
        let mut store = self.store.borrow_mut();
        let Some(view) = store.pages.get(&tab).filter(|page| page.owner == self.host).map(|page| page.view) else { return false; };
        store.engine.clear_site_data(view, origin, kind)
    }
    pub fn permission_pending(&self, tab: TabId, request: engine::PermissionRequestId) -> bool {
        let store=self.store.borrow();
        store.pages.get(&tab).filter(|page|page.owner==self.host).is_some_and(|page|store.engine.permission_pending(page.view,request))
    }

    pub fn respond_permission(&mut self, tab: TabId, request: engine::PermissionRequestId, decision: engine::PermissionDecision) -> bool {
        let mut store = self.store.borrow_mut();
        let Some(view) = store.pages.get(&tab).filter(|page| page.owner == self.host).map(|page|page.view) else { return false; };
        store.engine.respond_permission(view, request, decision)
    }

    pub fn zoom_percent(&self, tab: TabId) -> u16 {
        let store = self.store.borrow();
        store.pages.get(&tab).filter(|page| page.owner == self.host).map(|page| store.engine.zoom_percent(page.view)).unwrap_or(100)
    }

    pub(crate) fn owned_media(&self, tab: TabId) -> Option<engine::MediaState> {
        self.store.borrow().pages.get(&tab).filter(|page|page.owner==self.host).map(|page|page.state.media.clone())
    }

    pub fn state(&self, tab: TabId) -> PageState {
        self.store.borrow().pages
            .get(&tab)
            .map(|page| page.state.clone())
            .unwrap_or_default()
    }

    pub fn tab_for(&self, view: ViewId) -> Option<TabId> {
        self.store.borrow().pages
            .iter()
            .find_map(|(tab, page)| (page.view == view).then_some(*tab))
    }

    /// Reconciles views with the model. `visible` is false while UI covers
    /// the page (the command bar), since native views draw above the UI.
    pub(crate) fn reconcile_inventory(&mut self, browser: &Browser) {
        if crate::quit::is_quitting() { return; }
        {
            let mut store = self.store.borrow_mut();
            // all_tabs is the canonical workspace inventory, including Favorites
            // of profiles that this window is not currently displaying.
            let open: HashMap<_, _> = browser.all_tabs()
                .filter_map(|tab| browser.user_profile_id_for_tab(tab.id).map(|profile| (tab.id, profile))).collect();
            let retired: Vec<_> = store.identities.iter().filter(|(tab, identity)| open.get(tab) != Some(&identity.profile)
                || (store.pages.contains_key(tab) && browser.all_tabs().any(|entry| entry.id == **tab && browser_core::omnibox::is_internal_url(&entry.url))))
                .map(|(tab, _)| *tab).collect();
            for tab in retired {
                store.retire(tab);
                store.identities.remove(&tab);
                self.visible.remove(&tab);
                if self.shown == Some(tab) {
                    self.shown = None;
                    if !self.resize_focus_suppressed { store.engine.focus_ui_in_host(self.parent); }
                }
            }
            for (tab, profile) in open {
                if !store.identities.contains_key(&tab) {
                    store.next_generation = store.next_generation.checked_add(1).expect("page generations exhausted");
                    let generation = store.next_generation;
                    store.identities.insert(tab, PageIdentity { tab, profile, generation });
                }
            }
        }

        self.store.borrow_mut().prune();
    }

    pub(crate) fn has_native_parent(&self) -> bool { !self.parent.is_null() }
    pub(crate) fn owns_download_fixture_source(&self, source: &crate::window::download_probe::admission::Source) -> bool {
        self.parent as usize == source.parent && self.store.borrow().pages.get(&source.identity.tab)
            .is_some_and(|page| page.owner == self.host && page.identity == source.identity)
    }


    /// Pure source capture after inventory reconciliation and before creation.
    /// Only the explicitly created Today fixture is eligible; no existing page
    /// receives a capability and no storage/profile identifier is reconstructed.
    pub(crate) fn download_probe_source(&self, browser: &Browser, tab: TabId,
        config: &crate::window::download_probe::Config)
        -> Result<crate::window::download_probe::admission::Source, String> {
        if browser.is_private() || browser.is_blank() || self.parent.is_null() {
            return Err("Probe requires a normal native workspace".into());
        }
        let profile = browser.active_user_profile_id();
        let space = browser.space();
        if space.user_profile_id != profile || !space.today.iter().any(|entry| entry.id == tab && entry.url == config.fixture_url()) {
            return Err("Probe source is not the captured Today fixture".into());
        }
        let store = self.store.borrow();
        if store.pages.contains_key(&tab) { return Err("Probe cannot adopt an existing page".into()); }
        let identity = store.identities.get(&tab).copied().filter(|identity| identity.profile == profile)
            .ok_or("Probe source identity is unavailable")?;
        Ok(crate::window::download_probe::admission::Source {
            identity, space: space.id, storage: StorageSelection::for_profile(profile), parent: self.parent as usize,
        })
    }

    pub fn sync(&mut self, browser: &Browser, visible: bool) {
        if crate::quit::is_quitting() { return; }
        {
            let mut store = self.store.borrow_mut();
            for profile in browser.user_profiles().filter(|profile| !browser.is_private() || profile.id() == browser.active_user_profile_id()) {
                let storage = if browser.is_private() { StorageSelection::Current } else { StorageSelection::for_profile(profile.id()) };
                let boosts = profile.boosts.iter().filter(|boost| boost.enabled).map(|boost| engine::SiteBoost {
                    domain: boost.domain().into(), css: boost.stylesheet(), javascript: boost.javascript.clone(),
                }).collect();
                store.engine.set_site_boosts(&storage, boosts);
            }
        }
        self.reconcile_inventory(browser);
        self.store.borrow_mut().prune();
        let selected=browser.active_tab().map(|tab|tab.id);
        if selected!=self.pip_selected {
            let mut store=self.store.borrow_mut();
            let PageStore { engine,pages,.. }=&mut *store;
            if !self.peek_mode && browser.settings.enabled(browser_core::settings::Toggle::AutomaticPictureInPicture) {
                if let Some(page)=self.pip_selected.and_then(|tab|pages.get(&tab)).filter(|page|page.owner==self.host && page.visible && page.state.media.playing && page.state.media.video && !page.state.media.picture_in_picture) {
                    engine.set_picture_in_picture(page.view,true);
                }
            }
            if let Some(page)=selected.and_then(|tab|pages.get(&tab)).filter(|page|page.owner==self.host && page.state.media.picture_in_picture) {
                engine.set_picture_in_picture(page.view,false);
            }
            self.pip_selected=selected;
        }
        self.wanted.clear();
        self.wanted_focus = None;
        self.split = false;
        if visible || self.viewport.retains_library_page() {
            if let Some(active) = browser.active_tab() {
                self.wanted_focus = (!browser_core::omnibox::is_internal_url(&active.url)).then_some(active.id);
                if let Some(split) = browser.active_split() {
                    self.split = true;
                    self.axis = split.axis;
                    for (index, id) in split.panes.iter().enumerate() {
                        if let Some(tab) = browser.all_tabs().find(|tab| tab.id == *id) {
                            let Some(identity) = self.store.borrow().identities.get(id).copied() else { continue; };
                            self.wanted.push(PageIntent { identity, source_space: browser.space().id, storage: if browser.is_private() { StorageSelection::Current } else { StorageSelection::for_profile(identity.profile) }, tab: *id, url: tab.url.clone(), muted: tab.muted,
                                weight: split.weights[index], internal: browser_core::omnibox::is_internal_url(&tab.url) });
                        }
                    }
                } else if self.wanted_focus.is_some() {
                    let identity = self.store.borrow().identities[&active.id];
                    self.wanted.push(PageIntent { identity, source_space: browser.space().id, storage: if browser.is_private() { StorageSelection::Current } else { StorageSelection::for_profile(identity.profile) }, tab: active.id, url: active.url.clone(), muted: active.muted, weight: 1., internal: false });
                }
            }
        }
        self.apply_with_navigation(true);
        self.sync_audio(browser);
        let mut store = self.store.borrow_mut();
        let policies: Vec<_> = store.pages.iter().filter(|(_, page)| page.owner == self.host).map(|(id, page)|
            (page.view, !self.peek_mode && browser.settings.enabled(browser_core::settings::Toggle::PeekPinned) && (browser.is_favorite(*id) || browser.spaces.iter().any(|space| space.pinned.iter().any(|tab| tab.id == *id))))).collect();
        let contexts: Vec<_> = store.pages.iter().filter(|(_, page)| page.owner == self.host).map(|(id, page)| (page.view, engine::page_context::Policy {
            search_name: browser.user_profile(page.identity.profile).map_or("Google", |profile| profile.settings.search.engine.label()).into(),
            can_split: browser.related_tabs(*id).len() < browser_core::splits::MAX_SPLIT_PANES,
            allow_little: !browser.is_private(), can_print: store.engine.supports_printing(),
        })).collect();
        for (view, policy) in contexts { store.engine.set_page_context_policy(view, policy); }
        for (view, pinned) in policies { store.engine.set_peek_policy(view, engine::PeekPolicy { pinned, shift_click: !self.peek_mode && !self.square_top && browser.settings.enabled(browser_core::settings::Toggle::PeekCommandShiftClick), capture_escape: self.peek_mode }); if self.peek_mode { store.engine.set_corner_radius(view, 8.); } }
    }

    /// Browser appearance applies to every retained page owned by this host.
    /// Changing it neither navigates pages nor transfers their ownership.
    pub(crate) fn sync_color_schemes(&mut self, appearance: browser_core::themes::ThemeAppearance) {
        use browser_core::themes::ThemeAppearance;
        let scheme = match appearance { ThemeAppearance::Automatic => None, ThemeAppearance::Light => Some(false), ThemeAppearance::Dark => Some(true) };
        let mut store = self.store.borrow_mut();
        let updates: Vec<_> = store.pages.iter().filter(|(_, page)| page.owner == self.host)
            .filter_map(|(tab, page)| (scheme != page.color_scheme).then_some((*tab, page.view, scheme))).collect();
        for (tab, view, scheme) in updates {
            if store.engine.set_color_scheme(view, scheme) {
                store.pages.get_mut(&tab).unwrap().color_scheme = scheme;
            }
        }
    }

    pub(crate) fn media_identity(&self, tab: TabId) -> Option<PageIdentity> {
        self.store.borrow().pages.get(&tab).filter(|page|page.owner==self.host).map(|page|page.identity)
    }
    pub(crate) fn media_volume(&self, tab: TabId) -> f32 {
        let store=self.store.borrow();
        store.pages.get(&tab).filter(|page|page.owner==self.host)
            .map_or(1.,|page|store.engine.audio_volume(page.view).unwrap_or(page.state.media.volume as f32))
    }
    pub(crate) fn set_media_volume(&mut self, tab: TabId, volume: f32) -> bool {
        if !volume.is_finite() || !(0. ..=1.).contains(&volume) {return false;}
        let mut store=self.store.borrow_mut();
        let Some(view)=store.pages.get(&tab).filter(|page|page.owner==self.host).map(|page|page.view)else{return false;};
        if !store.engine.set_audio_volume(view,volume){return false;}
        store.pages.get_mut(&tab).unwrap().state.media.volume=volume as f64;true
    }

    pub fn media_action(&mut self, tab: TabId, action: engine::MediaAction) -> bool {
        let mut store = self.store.borrow_mut();
        let Some(view) = store.pages.get(&tab).filter(|page|page.owner==self.host).map(|page|page.view) else { return false; };
        store.engine.media_action(view, action)
    }

    /// Applies mute intent even when an occluded window is not rendering.
    pub fn sync_audio(&mut self, browser: &Browser) {
        let mut store = self.store.borrow_mut();
        let PageStore { engine, pages, .. } = &mut *store;
        for intent in &mut self.wanted {
            if let Some(tab) = browser.all_tabs().find(|tab| tab.id == intent.tab) { intent.muted = tab.muted; }
        }
        // Muting a background tab must not activate or load it.
        for tab in browser.all_tabs() {
            if let Some(page) = pages.get_mut(&tab.id) {
                if page.muted != tab.muted || engine.audio_muted(page.view).is_some_and(|actual| actual != tab.muted) {
                    engine.set_audio_muted(page.view, tab.muted);
                    page.muted = tab.muted;
                }
            }
        }
    }

    /// Unloads a tab's page, e.g. a closed pinned tab that stays in the
    /// sidebar. It loads again when the tab is next shown.
    pub fn discard(&mut self, tab: TabId) {
        let mut store = self.store.borrow_mut();
        store.retire(tab);
        store.next_generation = store.next_generation.checked_add(1).expect("page generations exhausted");
        let generation = store.next_generation;
        if let Some(identity) = store.identities.get_mut(&tab) { identity.generation = generation; }
        store.prune();
        self.visible.remove(&tab);
        if self.shown == Some(tab) {
            self.shown = None;
            if !self.resize_focus_suppressed { store.engine.focus_ui_in_host(self.parent); }
        }
    }

    /// Retire geometry before selection sync can reuse a previous surface.
    /// This never changes a document, its host attachment or its storage.
    pub(crate) fn set_viewport(&mut self, viewport: PageViewport) {
        if self.viewport == viewport { return; }
        let keeps_document = self.viewport != PageViewport::Hidden && viewport != PageViewport::Hidden;
        self.viewport = viewport;
        self.viewport_generation = self.viewport_generation.wrapping_add(1);
        if !keeps_document { self.bounds = None; }
        if viewport != PageViewport::Content {
            self.request_ui_focus();
            // Keeping the page visible must not leave its native responder in
            // charge after the user explicitly opens Library's controls.
            if keeps_document && !self.resize_focus_suppressed {
                let mut store = self.store.borrow_mut();
                if store.active_host == self.host { store.engine.focus_ui_in_host(self.parent); }
            }
        }
    }

    pub(crate) fn set_top_toolbar(&mut self, shown: bool) { self.top_toolbar = shown; }

    pub(crate) fn square_top(&mut self) { self.square_top = true; }

    pub(crate) fn viewport_generation(&self) -> u64 { self.viewport_generation }

    /// A deferred normal canvas must not overwrite a newer Library viewport.
    pub(crate) fn set_viewport_bounds(&mut self, generation: u64, bounds: Bounds) -> bool {
        if generation != self.viewport_generation || self.viewport == PageViewport::Hidden { return false; }
        self.set_bounds(bounds);
        true
    }

    /// Called from layout with the content card's bounds.
    pub fn set_bounds(&mut self, bounds: Bounds) {
        if self.bounds == Some(bounds) {
            return;
        }
        self.bounds = Some(bounds);
        self.apply();
    }

    fn apply(&mut self) {
        self.apply_with_navigation(false);
    }

    // Only sync supplies fresh model intent. Capture completion and geometry
    // updates must never navigate using an inactive window's cached URL.
    fn apply_with_navigation(&mut self, navigate: bool) {
        // Geometry/snapshot polls carry cached intent. An outstanding failed
        // create is retried only by sync with a refreshed authoritative model.
        let retry_requires_model = self.creation_pending && !navigate;
        self.creation_pending = retry_requires_model;
        let frames = self.pane_bounds();
        let identities = self.store.borrow().identities.clone();
        let wanted: Vec<_> = self.wanted.iter().zip(frames).filter(|(intent, frame)| identities.get(&intent.tab) == Some(&intent.identity) && !intent.internal && frame.page.width >= 1. && frame.page.height >= 1.)
            .map(|(intent, frame)| (intent.clone(), frame.page)).collect();
        let mut store = self.store.borrow_mut();
        // Capture raw input while the old attachment still owns each document.
        // No pumping/awaiting occurs between this barrier and native transfers.
        store.capture_pending_engine_events();
        let active = store.active_host == self.host;
        let layout: HashMap<_, _> = wanted.iter().map(|(intent, bounds)| (intent.tab, *bounds)).collect();
        if store.layouts.get(&self.host) != Some(&layout) {
            store.layouts.insert(self.host, layout);
            store.mirror_revision = store.mirror_revision.wrapping_add(1);
        }
        store.prune();
        // Decide before borrowing individual pages. Starting capture never pumps
        // CEF, and the native parent is unchanged until ready or the deadline.
        let waiting: HashSet<_> = wanted.iter().filter_map(|(intent, _)| {
            (active && !self.parent.is_null() && !store.prepare_handoff(intent.tab, self.host)).then_some(intent.tab)
        }).collect();
        let PageStore { engine, pages, .. } = &mut *store;
        let ids: HashSet<_> = wanted.iter().map(|(intent, _)| intent.tab).collect();
        let hidden: Vec<_> = self.visible.difference(&ids).copied().collect();
        for tab in hidden {
            if let Some(page) = pages.get_mut(&tab).filter(|page| page.owner == self.host) {
                page.state.status.clear();
                engine.set_visible(page.view, false);
                page.visible = false;
            }
            self.visible.remove(&tab);
        }
        let focused = self.wanted_focus.filter(|id| ids.contains(id));
        if self.shown.is_some() && (focused.is_none() || !self.shown.is_some_and(|id| ids.contains(&id))) {
            self.shown = None;
            if active && !self.resize_focus_suppressed { engine.focus_ui_in_host(self.parent); }
        }
        let mut created = false;
        let mut moved = false;
        for (intent, bounds) in wanted {
            let PageIntent { tab, identity, source_space, storage, url, muted, .. } = intent;
            match pages.get_mut(&tab) {
                Some(page) => {
                    if page.owner != self.host {
                        if waiting.contains(&tab) || !active || self.parent.is_null() || !engine.move_view(page.view, self.parent, bounds) { self.visible.remove(&tab); continue; }
                        page.input_attachment = page.input_attachment.checked_add(1).expect("input attachment generations exhausted");
                        page.owner = self.host;
                        moved = true;
                        self.visible.remove(&tab);
                        created = true;
                    }
                    page.source_space = source_space;
                    if page.muted != muted {
                        engine.set_audio_muted(page.view, muted);
                        page.muted = muted;
                    }
                    if navigate && page.url != url {
                        page.state.status.clear();
                        engine.navigate(page.view, &url);
                        page.url = url;
                    }
                    engine.set_bounds(page.view, bounds);
                    page.frame = bounds;
                }
                None => {
                    if !active { continue; }
                    if retry_requires_model { continue; }
                    let view = match engine.create_page_with_identity(identity, source_space, &storage, self.parent, &url, bounds) {
                        PageCreation::Created(view) => view,
                        PageCreation::Pending => { self.creation_pending = true; continue; }
                        PageCreation::Failed => continue,
                    };
                    created = true;
                    if muted { engine.set_audio_muted(view, true); }
                    pages.insert(tab, Page { input_attachment: 0, identity, source_space, owner: self.host, frame: bounds, visible: false, view, url, state: PageState::default(), muted, color_scheme: None, snapshot: None, snapshot_pending: None, snapshot_origin: None, snapshot_good_origin: None });
                }
            }
            engine.set_top_corners_rounded(pages[&tab].view, !self.split && !self.square_top && !self.top_toolbar);
            let page = pages.get_mut(&tab).unwrap();
            if !page.visible { engine.set_visible(page.view, true); page.visible = true; created = true; }
            self.visible.insert(tab);
        }
        // Creating a native browser may take the responder even when the
        // intended active pane has not changed. Reassert it after all creates.
        if active && !self.resize_focus_suppressed && self.page_focus && (!self.devtools_focused || focused != self.shown) && !self.preserve_ui_focus && (focused != self.shown || created || !self.page_has_focus) {
            if let Some((tab, view)) = focused.and_then(|id| pages.get(&id).filter(|page| page.owner == self.host).map(|page| (id, page.view))) {
                engine.focus_page(view);
                self.shown = Some(tab);
                self.devtools_focused = false;
                self.page_focus = true;
                self.page_has_focus = true;
            } else if self.shown.take().is_some() || created {
                if active && !self.resize_focus_suppressed { engine.focus_ui_in_host(self.parent); }
            }
        }
        if active && (!self.page_focus || self.preserve_ui_focus || self.resize_focus_suppressed) {
            self.page_has_focus = false;
            self.shown = focused.filter(|tab| pages.get(tab).is_some_and(|page| page.owner == self.host));
            if created && !self.resize_focus_suppressed { engine.focus_ui_in_host(self.parent); }
        }
        if moved { store.mirror_revision = store.mirror_revision.wrapping_add(1); }
        // Ownership changes remove target photographs and reveal source ones.
        let completed: Vec<_> = store.handoffs.iter().filter(|(tab, claim)| store.pages.get(*tab).is_some_and(|page| page.owner == claim.target)).map(|(tab, _)| *tab).collect();
        for tab in completed {
            store.cancel_handoff(tab);
            store.mirror_revision = store.mirror_revision.wrapping_add(1);
        }
        store.layout_inspector_probe();
        let (closed, focused) = store.reconcile_devtools(self.host, self.wanted_focus, self.page_focus && !self.preserve_ui_focus && !self.resize_focus_suppressed);
        if closed { self.devtools_focused = false; self.page_has_focus = focused; }
        self.seen_revision = store.mirror_revision;
    }

    fn pane_bounds(&self) -> Vec<crate::split_layout::PaneBounds> {
        let Some(bounds) = self.bounds else { return Vec::new(); };
        let weights: Vec<_> = self.wanted.iter().map(|intent| intent.weight).collect();
        crate::split_layout::layout(bounds, self.axis, &weights, self.split)
    }

    pub fn surface_bounds(&self) -> Vec<Bounds> {
        let mut surfaces: Vec<_> = self.surface_layout().into_iter().map(|(_, bounds)| bounds).collect();
        let store = self.store.borrow();
        if let Some(dock) = store.devtools.filter(|dock| dock.host == self.host) {
            if let Some((_, _, frontend)) = store.layouts.get(&self.host).and_then(|layout| layout.get(&dock.tab)).copied().and_then(devtools::frames) {
                surfaces.push(frontend);
            }
        }
        surfaces
    }

    pub fn surface_layout(&self) -> Vec<(TabId, Bounds)> {
        self.wanted.iter().zip(self.pane_bounds()).filter(|(intent, _)| self.is_visible(intent.tab)).map(|(intent, frame)| {
            let store = self.store.borrow();
            let bounds = if store.devtools.is_some_and(|dock| dock.host == self.host && dock.tab == intent.tab) {
                store.pages.get(&intent.tab).map_or(frame.page, |page| page.frame)
            } else { frame.page };
            (intent.tab, bounds)
        }).collect()
    }

    /// Frozen owner-viewport images, positioned in this host's logical layout.
    /// Images never follow later captures in another host.
    pub fn mirrors(&self) -> Vec<(TabId, Bounds, Arc<Vec<u8>>)> {
        let store = self.store.borrow();
        self.wanted.iter().zip(self.pane_bounds()).filter_map(|(intent, frame)| {
            let page = store.pages.get(&intent.tab)?;
            if page.owner == self.host || intent.internal { return None; }
            store.frozen.get(&(self.host, intent.tab)).map(|png| (intent.tab, frame.page, png.clone()))
        }).collect()
    }

    pub fn mirror_revision(&self) -> u64 { self.store.borrow().mirror_revision }

    pub fn is_visible(&self, tab: TabId) -> bool { self.visible.contains(&tab) && self.view(tab).is_some() }

    pub(crate) fn capture_ui_page_focus_owner(&self, tab: TabId) -> Option<UiPageFocusOwner> {
        let store = self.store.borrow();
        if store.active_host != self.host || store.hosts.get(&self.host) != Some(&self.parent) { return None; }
        Some(UiPageFocusOwner { store: Rc::downgrade(&self.store), host: self.host,
            parent: self.parent, identity: *store.identities.get(&tab)? })
    }

    pub(crate) fn accepts_ui_page_focus_owner(&self, owner: &UiPageFocusOwner) -> bool {
        if !owner.store.ptr_eq(&Rc::downgrade(&self.store)) || owner.host != self.host || owner.parent != self.parent { return false; }
        let store = self.store.borrow();
        store.active_host == self.host && store.hosts.get(&self.host) == Some(&self.parent)
            && store.identities.get(&owner.identity.tab) == Some(&owner.identity)
    }

    /// Read intent without changing native or GPUI focus, draining events, or
    /// retaining a native owner. An invalid native target is never UI intent.
    pub(crate) fn capture_resize_focus(&self) -> ResizePageFocus {
        let target = if !self.page_focus {
            ResizeFocusTarget::Ui
        } else {
            let store = self.store.borrow();
            self.shown.filter(|tab| self.wanted_focus == Some(*tab) && self.visible.contains(tab))
                .and_then(|tab| store.pages.get(&tab))
                .filter(|page| store.active_host == self.host && store.hosts.get(&self.host) == Some(&self.parent)
                    && page.owner == self.host && page.visible)
                .and_then(|page| {
                    let frontend = if self.devtools_focused {
                        let dock = store.devtools.filter(|dock| dock.host == self.host && dock.tab == page.identity.tab && dock.view == page.view)?;
                        let status = store.engine.inspector_status(dock.id).filter(|status|
                            status.id == dock.id && status.inspected == page.view
                                && !matches!(status.phase, engine::InspectorPhase::Detached | engine::InspectorPhase::LoadFailed))?;
                        Some((dock.id, status.generation))
                    } else { None };
                    Some(ResizeFocusTarget::Native {
                        origin: InputOrigin { identity: page.identity, host: self.host, attachment: page.input_attachment,
                            inspector: frontend.map(|(id, _)| id) },
                        view: page.view, frontend,
                    })
                }).unwrap_or(ResizeFocusTarget::Unavailable)
        };
        ResizePageFocus { store: Rc::downgrade(&self.store), host: self.host, parent: self.parent, target }
    }

    pub(crate) fn accepts_native_paste_focus(&self, snapshot: &ResizePageFocus) -> bool {
        if self.resize_focus_suppressed || (!self.page_has_focus && !self.devtools_focused)
            || !snapshot.has_native_focus() { return false; }
        let current = self.capture_resize_focus();
        current.store.ptr_eq(&snapshot.store) && current.host == snapshot.host
            && current.parent == snapshot.parent && current.target == snapshot.target
    }

    pub(crate) fn prepare_native_paste(&mut self) -> Option<(ResizePageFocus, NativePaste)> {
        let snapshot = self.capture_resize_focus();
        if !self.accepts_native_paste_focus(&snapshot) { return None; }
        let ResizeFocusTarget::Native { view, frontend, .. } = snapshot.target else { return None; };
        let mut store = self.store.borrow_mut();
        let effect = if let Some((id, _)) = frontend {
            store.engine.prepare_inspector_paste_and_match_style(id)?
        } else { store.engine.prepare_paste_and_match_style(view)? };
        Some((snapshot, effect))
    }

    /// Restore only the original live native responder. Returns true only when
    /// native focus was issued; UI snapshots are a no-op (the host owns GPUI).
    /// The host must first reject newer UI/popup/inline focus intent.
    pub(crate) fn restore_resize_focus(&mut self, snapshot: ResizePageFocus) -> bool {
        self.restore_resize_focus_inner(snapshot, true)
    }

    /// Validate and resume intent without native focus. This is not safe for
    /// abandoned resizes: retain their exact snapshot under suppression until
    /// activation has reconciled the layout, then explicitly restore it.
    pub(crate) fn resume_resize_focus_intent(&mut self, snapshot: ResizePageFocus) -> bool {
        self.restore_resize_focus_inner(snapshot, false)
    }

    fn restore_resize_focus_inner(&mut self, snapshot: ResizePageFocus, focus_native: bool) -> bool {
        if self.resize_focus_suppressed { return false; }
        let ResizeFocusTarget::Native { origin, view, frontend } = snapshot.target else { return false; };
        if !snapshot.store.ptr_eq(&Rc::downgrade(&self.store)) || snapshot.host != self.host
            || snapshot.parent != self.parent || self.shown != Some(origin.identity.tab)
            || self.wanted_focus != Some(origin.identity.tab) || !self.visible.contains(&origin.identity.tab)
            || crate::quit::is_quitting()
        { return false; }
        let mut store = self.store.borrow_mut();
        if store.active_host != self.host || store.hosts.get(&self.host) != Some(&self.parent)
            || !store.pages.get(&origin.identity.tab).is_some_and(|page|
                page.identity == origin.identity && page.owner == self.host && page.visible
                    && page.input_attachment == origin.attachment && page.view == view)
        { return false; }
        if let Some((id, generation)) = frontend {
            if !store.devtools.is_some_and(|dock| dock.id == id && dock.host == self.host
                && dock.tab == origin.identity.tab && dock.view == view)
                || !store.engine.inspector_status(id).is_some_and(|status|
                    status.id == id && status.inspected == view && status.generation == generation
                        && !matches!(status.phase, engine::InspectorPhase::Detached | engine::InspectorPhase::LoadFailed))
            { return false; }
        }
        if focus_native {
            if let Some((id, _)) = frontend {
                store.engine.focus_inspector(id);
            } else {
                store.engine.focus_page(view);
            }
        }
        self.page_focus = true;
        self.devtools_focused = frontend.is_some();
        self.page_has_focus = focus_native && frontend.is_none();
        true
    }

    /// Keep layout reconciliation from changing responders until the host can
    /// validate its retained resize snapshot after activation.
    pub(crate) fn suppress_resize_focus(&mut self) {
        self.resize_focus_suppressed = true;
        self.request_ui_focus();
    }

    /// Ending suppression retains UI intent. Only explicit snapshot restoration
    /// or a subsequent user activation may request page focus again.
    pub(crate) fn end_resize_focus_suppression(&mut self) {
        self.resize_focus_suppressed = false;
        self.request_ui_focus();
    }

    /// Transfers the native responder to GPUI without hiding the page.
    pub fn focus_ui(&mut self) {
        self.request_ui_focus();
        self.store.borrow_mut().engine.focus_ui_in_host(self.parent);
    }

    /// An overlay owns its own native responder; remember its intent without
    /// pulling focus back into the underlying root window.
    pub fn request_ui_focus(&mut self) {
        self.devtools_focused = false;
        self.page_focus = false;
        self.page_has_focus = false;
    }

    /// Records explicit navigation/selection intent without changing the native
    /// responder before layout or an asynchronous handoff is ready.
    pub fn request_page_focus(&mut self) { if self.resize_focus_suppressed { return; } self.devtools_focused = false; self.page_focus = true; self.page_has_focus = false; }

    /// Gives keyboard focus back to the page on screen.
    pub fn focus_shown(&mut self) {
        if self.resize_focus_suppressed { return; }
        self.devtools_focused = false;
        self.page_focus = true;
        let target = self.wanted_focus.filter(|tab| self.visible.contains(tab)).and_then(|tab| {
            let store = self.store.borrow();
            store.pages.get(&tab).filter(|page| store.active_host == self.host && page.owner == self.host && page.visible)
                .map(|page| (tab, page.view))
        });
        if let Some((tab, view)) = target {
            self.store.borrow_mut().engine.focus_page(view);
            self.shown = Some(tab);
            self.page_has_focus = true;
        }
    }

    /// Transfers one loaded page, retaining its native view ID and cached state.
    pub fn move_to_host(&mut self, tab: TabId, parent: *mut c_void, bounds: Bounds) -> bool {
        if parent.is_null() { return false; }
        let Some(view) = self.view(tab) else { return false; };
        let mut store = self.store.borrow_mut();
        store.capture_pending_engine_events();
        store.engine.move_view(view, parent, bounds)
    }

    pub fn audio_muted(&self, tab: TabId) -> Option<bool> {
        self.view(tab).and_then(|view| self.store.borrow_mut().engine.audio_muted(view))
    }

    pub fn shown(&self) -> Option<TabId> {
        self.shown
    }

    fn view(&self, tab: TabId) -> Option<ViewId> {
        self.store.borrow().pages.get(&tab).filter(|page| page.owner == self.host).map(|page| page.view)
    }

    pub fn go_back(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) {
            self.store.borrow_mut().engine.go_back(view);
        }
    }

    pub fn go_forward(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) {
            self.store.borrow_mut().engine.go_forward(view);
        }
    }

    pub fn undo(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.undo(view); }
    }

    pub fn redo(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.redo(view); }
    }

    pub(crate) fn request_quote(&mut self, tab: TabId, request: u64) -> bool { self.view(tab).is_some_and(|view| self.store.borrow_mut().engine.request_quote(view, request)) }
    pub(crate) fn set_boost_picker(&mut self, tab: TabId, enabled: bool) { if let Some(view)=self.view(tab) { self.store.borrow_mut().engine.set_boost_picker(view,enabled); } }

    pub fn reload(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) {
            self.store.borrow_mut().engine.reload(view);
        }
    }

    pub(crate) fn can_print(&self, tab: TabId) -> bool {
        // Print targets the owned document, not its last painted rectangle.
        // Library's measured viewport can arrive after the parent action tree.
        self.view(tab).is_some() && self.store.borrow().engine.supports_printing()
    }
    pub(crate) fn prepare_print(&mut self, tab: TabId) -> Option<NativePrint> {
        if !self.can_print(tab) { return None; }
        let view = self.view(tab)?;
        self.store.borrow_mut().engine.prepare_print(view)
    }

    pub fn force_reload(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.force_reload(view); }
    }

    pub fn stop_loading(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.stop_loading(view); }
    }

    pub fn zoom(&mut self, tab: TabId, direction: i32) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.zoom(view, direction); }
    }

    pub fn find(&mut self, tab: TabId, text: &str, forward: bool, next: bool) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.find(view, text, forward, next); }
    }

    pub fn stop_finding(&mut self, tab: TabId) {
        if let Some(view) = self.view(tab) { self.store.borrow_mut().engine.stop_finding(view); }
    }

    pub(crate) fn supports_snapshots(&self) -> bool { self.store.borrow().engine.supports_snapshots() }

    /// Captures the existing document; concurrent requests for a tab coalesce.
    pub fn request_snapshot(&mut self, tab: TabId) -> Option<u64> {
        let mut store = self.store.borrow_mut();
        let page = store.pages.get(&tab)?;
        if let Some(request) = page.snapshot_pending { return Some(request); }
        let view = page.view;
        let origin = (page.owner, page.frame);
        store.next_snapshot = store.next_snapshot.checked_add(1)?;
        let request = store.next_snapshot;
        if !store.engine.request_snapshot(view, request) { return None; }
        let page = store.pages.get_mut(&tab)?;
        page.snapshot_pending = Some(request);
        page.snapshot_origin = Some(origin);
        Some(request)
    }

    /// A previous good image remains available while another capture is pending.
    pub fn snapshot(&self, tab: TabId) -> Option<(u64, Arc<Vec<u8>>)> {
        self.store.borrow().pages.get(&tab)?.snapshot.clone()
    }

    pub fn snapshot_pending(&self, tab: TabId, request: u64) -> bool {
        self.store.borrow().pages.get(&tab).is_some_and(|page| page.snapshot_pending == Some(request))
    }

    /// Native-only diagnostic entry point: use this host's actual focused,
    /// visible page, never a mirror or an arbitrary model tab.
    pub fn open_inspector_probe(&mut self) -> Result<engine::InspectorStatus, String> {
        if crate::quit::is_quitting() { return Err("Browser is quitting".into()); }
        let mut store = self.store.borrow_mut();
        let tab = self.shown.filter(|tab| Some(*tab) == self.wanted_focus && self.visible.contains(tab))
            .ok_or("No focused visible page")?;
        let page = store.pages.get(&tab).filter(|page| page.owner == self.host && page.visible)
            .ok_or("Focused page is not owned by this host")?;
        if store.active_host != self.host { return Err("Inspector host is inactive".into()); }
        if store.devtools.is_some() { return Err("Product inspector is already open".into()); }
        let view = page.view;
        let parent = store.hosts.get(&page.owner).copied().filter(|parent| !parent.is_null())
            .ok_or("Focused page has no native GPUI parent")?;
        let frame = store.layouts.get(&self.host).and_then(|layout| layout.get(&tab)).copied()
            .ok_or("Focused page has no current layout")?;
        if frame.width < 2. || frame.height < 1. { return Err("Page is too small for diagnostic dock".into()); }
        if store.inspector_probe.is_some_and(|probe| probe.view != view || probe.host != self.host) { store.close_inspector_probe(); }
        let (_, dock) = inspector_probe_frames(frame);
        let id = store.engine.open_inspector_probe(view, parent, dock)?;
        store.inspector_probe = Some(InspectorProbe { id, tab, view, host: self.host });
        store.layout_inspector_probe();
        store.engine.inspector_status(id).ok_or("Inspector closed during creation".into())
    }

    pub fn inspector_probe_status(&self) -> Option<engine::InspectorStatus> {
        let store = self.store.borrow();
        let probe = store.inspector_probe.filter(|probe| probe.host == self.host)?;
        store.engine.inspector_status(probe.id)
    }

    /// Fixed trusted expression only. It assigns a document-lifetime UUID on
    /// first evaluation so reopen can prove that the inspected page survived.
    /// Submission does not wait: a following snapshot runs concurrently.
    pub fn eval_inspector_probe(&mut self) -> Result<u64, String> {
        let mut store = self.store.borrow_mut();
        let probe = store.inspector_probe.filter(|probe| probe.host == self.host)
            .ok_or("No inspector probe in this host")?;
        store.next_inspector_eval = store.next_inspector_eval.checked_add(1)
            .filter(|id| *id <= 9_007_199_254_740_991).ok_or("Inspector request IDs exhausted")?;
        let id = store.next_inspector_eval;
        let message = serde_json::json!({ "id": id, "method": "Runtime.evaluate", "params": {
            "expression": "JSON.stringify({documentURL:document.URL,uuid:(globalThis.__echoInspectorProbeUUID ??= Array.from(crypto.getRandomValues(new Uint8Array(16)), n => n.toString(16).padStart(2, '0')).join(''))})",
            "returnByValue": true
        }}).to_string();
        if !store.engine.send_inspector_probe_message(probe.id, &message) { return Err("Inspector evaluation rejected".into()); }
        Ok(id)
    }

    pub fn close_inspector_probe(&mut self) {
        let closed = {
            let mut store = self.store.borrow_mut();
            if store.inspector_probe.is_some_and(|probe| probe.host == self.host) {
                store.close_inspector_probe();
                true
            } else { false }
        };
        if closed {
            // Inspector focus events are intentionally separate from tab
            // events. Its responder may have replaced the inspected page's.
            self.page_has_focus = false;
            self.apply();
        }
    }

    pub(crate) fn creation_retry_pending(&self) -> bool { self.creation_pending }

    /// The owner must refresh its authoritative projection before calling this.
    /// Rebuild intent rather than applying a cached URL/profile while waiting
    /// for asynchronous context initialization. No engine pumping occurs here.
    pub(crate) fn retry_pending_creation(&mut self, browser: &Browser) -> bool {
        self.retry_pending_creation_with_visibility(browser, true)
    }

    /// Refresh intent with the host's current sidebar-page visibility so a
    /// deferred engine context cannot surface a page behind Library/creation.
    pub(crate) fn retry_pending_creation_with_visibility(&mut self, browser: &Browser, visible: bool) -> bool {
        if !self.creation_pending { return false; }
        self.sync(browser, visible);
        !self.creation_pending
    }

    pub fn pump_fn(&self) -> fn() {
        self.store.borrow_mut().engine.pump_fn()
    }

    /// Compatibility inspection drain. The native window loop uses the routed
    /// drain below so host provenance survives delivery and deferred dispatch.
    pub fn take_events(&mut self) -> Vec<Event> {
        let events = self.take_routed_events();
        // This compatibility drain is an explicit inspection consumer, unlike
        // the native routed drain whose metadata is acknowledged by the model.
        let mut store = self.store.borrow_mut();
        for event in &events { store.metadata.remove(&event.sequence); }
        events.into_iter().map(|event| event.event).collect()
    }

    pub(crate) fn take_routed_events(&mut self) -> Vec<PageEvent> {
        let mut store = self.store.borrow_mut();
        store.drain_snapshots();
        // Every host polls, even without a browser event. A source may drain the
        // result first; shared revision wakes the target on its following poll.
        let reapply = store.mirror_revision != self.seen_revision
            || store.handoffs.values().any(|claim| claim.target == self.host);
        drop(store);
        if reapply {
            // A deferred transfer must retain the UI/page focus decision that
            // activation made, rather than stealing focus from an open input.
            self.preserve_ui_focus = !self.page_focus;
            self.apply();
            self.preserve_ui_focus = false;
        }
        let mut store = self.store.borrow_mut();
        store.capture_pending_engine_events();
        // take_events processes inspector load callbacks first. This runs after
        // the regular pump, with no borrow retained across pumping or awaiting.
        store.handle_devtools_events(self.host, self.page_focus && !self.resize_focus_suppressed);
        store.layout_inspector_probe();
        let (closed, focused) = store.reconcile_devtools(self.host, self.wanted_focus, self.page_focus && !self.preserve_ui_focus && !self.resize_focus_suppressed);
        if closed { self.devtools_focused = false; self.page_has_focus = focused; }
        let recover: Vec<_> = store.metadata.values().filter_map(|(event, delivered)| {
            let page = store.pages.get(&event.identity.tab).filter(|page| page.identity == event.identity)?;
            (page.owner == self.host && *delivered != Some(self.host)
                && !store.events.iter().any(|queued| queued.sequence == event.sequence)).then(|| event.clone())
        }).collect();
        store.events.extend(recover);
        store.events.sort_by_key(|event| event.sequence);
        let queued = std::mem::take(&mut store.events);
        let mut delivered = Vec::new();
        for queued in queued {
            if queued.is_metadata() && !store.metadata.contains_key(&queued.sequence) { continue; }
            let Some(page) = store.pages.get(&queued.identity.tab).filter(|page| page.identity == queued.identity) else { continue; };
            let owner = page.owner;
            if queued.input_origin.is_some_and(|origin|
                origin.host != owner || origin.attachment != page.input_attachment || !store.hosts.contains_key(&origin.host)) { continue; }
            // Inspector input also requires the same still-open frontend; its
            // inspected page alone does not authorize an obsolete dock event.
            if let Event::InspectorInput(input) = &queued.event {
                if !store.inspector_input_owner(input).is_some_and(|dock| dock.host == owner && dock.view == page.view) { continue; }
            }
            if owner == self.host {
                if let Some((_, host)) = store.metadata.get_mut(&queued.sequence) { *host = Some(self.host); }
                delivered.push(queued);
            }
            else if store.hosts.contains_key(&owner) { store.events.push(queued); }
        }
        delivered
    }

    /// Records what the engine reported so it is not re-sent by `sync`.
    pub fn record(&mut self, event: &Event) {
        let mut store = self.store.borrow_mut();
        let PageStore { pages, .. } = &mut *store;
        match event {
            Event::LoadingProgress { view, progress } => {
                if let Some(page) = pages.values_mut().find(|page| page.view == *view) { page.state.loading_progress = (*progress).min(1000); }
            }
            Event::MediaStateChanged { view, state } => {
                if let Some(page) = pages.values_mut().find(|page| page.view == *view) { page.state.media = state.clone(); }
            }
            Event::AudioStateChanged { view, audible } => {
                if let Some(page) = pages.values_mut().find(|page| page.view == *view) { page.state.audible = *audible; }
            }
            Event::EditFocusChanged { view, editable } => {
                if let Some(page) = pages.values_mut().find(|page| page.view == *view) { page.state.editable = *editable; }
            }
            Event::Focused { view } if !self.resize_focus_suppressed => {
                if let Some(tab) = pages.iter().find_map(|(tab, page)| (page.view == *view && page.owner == self.host).then_some(*tab)).filter(|tab| self.visible.contains(tab)) { self.shown = Some(tab); self.devtools_focused = false; self.page_focus = true; self.page_has_focus = true; }
            }
            Event::UrlChanged { view, url } => {
                if let Some(page) = pages.values_mut().find(|page| page.view == *view) {
                    page.url = url.clone();
                    page.state.status.clear();
                }
            }
            Event::StatusChanged { view, text } => {
                // Hidden pages can still deliver callbacks queued before they
                // were hidden. Never revive their old hover state.
                if let Some(tab) = pages.iter().find_map(|(tab, page)| (page.view == *view && page.owner == self.host).then_some(*tab)).filter(|tab| self.visible.contains(tab)) {
                    pages.get_mut(&tab).unwrap().state.status = text.clone();
                }
            }
            Event::LoadingChanged {
                view,
                loading,
                can_go_back,
                can_go_forward,
            } => {
                if let Some(page) = pages.values_mut().find(|page| page.view == *view) {
                    page.state.loading = *loading;
                    page.state.can_go_back = *can_go_back;
                    page.state.can_go_forward = *can_go_forward;
                    if *loading { page.state.status.clear(); }
                }
            }
            Event::PermissionRequested { .. } | Event::PermissionDismissed { .. } | Event::ContextImage { .. } | Event::PageContext { .. } | Event::PictureInPictureReturnRequested { .. } | Event::QuoteSelection { .. } | Event::OpenInBackgroundTab { .. } | Event::BoostZap { .. } | Event::Focused { .. } | Event::TitleChanged { .. } | Event::Favicon { .. } | Event::OpenInNewTab { .. } | Event::OpenPeek { .. } | Event::Shortcut { .. } | Event::InspectorInput(_) | Event::FindResult { .. } => {}
        }
    }
}

fn inspector_probe_frames(frame: Bounds) -> (Bounds, Bounds) {
    let width = (frame.width / 2.).floor();
    (Bounds { width, ..frame }, Bounds { x: frame.x + width, width: frame.width - width, ..frame })
}

impl PageStore {
    fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(clock) = &self.clock { return clock.now(); }
        Instant::now()
    }

    /// Drain without pumping before a transfer/parking changes owner metadata.
    /// Input keeps its callback-time attachment, even if no host polled first.
    fn capture_pending_engine_events(&mut self) {
        for event in self.engine.take_events() {
            if let Some(event) = self.capture_event(event) { self.events.push(event); }
        }
    }

    fn capture_event(&mut self, event: Event) -> Option<PageEvent> {
        let view = event_view(&event).or_else(|| match &event {
            Event::InspectorInput(input) => self.inspector_input_owner(input).map(|dock| dock.view),
            _ => None,
        })?;
        let page = self.pages.values().find(|page| page.view == view)?;
        let input_origin = matches!(&event, Event::PageContext { .. } | Event::PictureInPictureReturnRequested { .. } | Event::Shortcut { .. } | Event::InspectorInput(_)).then_some(InputOrigin {
            identity: page.identity, host: page.owner, attachment: page.input_attachment,
            inspector: match &event { Event::InspectorInput(input) => self.inspector_input_owner(input).map(|dock| dock.id), _ => None },
        });
        self.next_event = self.next_event.checked_add(1).expect("page event sequence exhausted");
        let event = PageEvent { sequence: self.next_event, event, identity: page.identity, source_space: page.source_space, input_origin };
        if event.is_metadata() { self.metadata.insert(event.sequence, (event.clone(), None)); }
        Some(event)
    }

    fn retire(&mut self, tab: TabId) {
        self.metadata.retain(|_, (event, _)| event.identity.tab != tab);
        if self.devtools.is_some_and(|dock| dock.tab == tab) { self.close_devtools(); }
        if self.inspector_probe.is_some_and(|probe| probe.tab == tab) { self.close_inspector_probe(); }
        self.cancel_handoff(tab);
        if let Some(page) = self.pages.remove(&tab) { self.engine.close(page.view); }
        self.frozen.retain(|(_, entry), _| *entry != tab);
        for layout in self.layouts.values_mut() { layout.remove(&tab); }
        self.mirror_revision = self.mirror_revision.wrapping_add(1);
    }

    fn close_inspector_probe(&mut self) {
        let Some(probe) = self.inspector_probe.take() else { return; };
        self.engine.close_inspector(probe.id);
        if let Some(page) = self.pages.get_mut(&probe.tab).filter(|page| page.view == probe.view && page.owner == probe.host) {
            if let Some(frame) = self.layouts.get(&probe.host).and_then(|layout| layout.get(&probe.tab)).copied() {
                self.engine.set_bounds(page.view, frame);
                page.frame = frame;
            }
        }
    }

    fn layout_inspector_probe(&mut self) {
        let Some(probe) = self.inspector_probe else { return; };
        let frame = self.layouts.get(&probe.host).and_then(|layout| layout.get(&probe.tab)).copied();
        let valid = self.active_host == probe.host && self.hosts.contains_key(&probe.host)
            && self.pages.get(&probe.tab).is_some_and(|page| page.view == probe.view && page.owner == probe.host && page.visible)
            && self.engine.inspector_status(probe.id).is_some();
        let Some(frame) = frame.filter(|frame| valid && frame.width >= 2. && frame.height >= 1.) else {
            self.close_inspector_probe();
            return;
        };
        let (page_frame, dock) = inspector_probe_frames(frame);
        self.engine.set_bounds(probe.view, page_frame);
        self.pages.get_mut(&probe.tab).unwrap().frame = page_frame;
        self.engine.set_inspector_frame(probe.id, dock);
        self.engine.set_inspector_visible(probe.id, true);
    }

    fn cancel_handoff(&mut self, tab: TabId) {
        if let Some(claim) = self.handoffs.remove(&tab) {
            if let Some(page) = self.pages.get_mut(&tab).filter(|page| page.snapshot_pending == Some(claim.request)) {
                page.snapshot_pending = None;
                page.snapshot_origin = None;
            }
        }
    }

    fn prune(&mut self) {
        self.metadata.retain(|_, (event, _)| self.pages.get(&event.identity.tab).is_some_and(|page| page.identity == event.identity));
        let cancelled: Vec<_> = self.handoffs.iter().filter(|(tab, claim)| {
            self.active_host != claim.target
                || !self.hosts.contains_key(&claim.source)
                || !self.layouts.get(&claim.target).is_some_and(|layout| layout.contains_key(*tab))
                || !self.pages.get(*tab).is_some_and(|page| page.owner == claim.source && page.view == claim.view && page.frame == claim.frame)
        }).map(|(tab, _)| *tab).collect();
        for tab in cancelled { self.cancel_handoff(tab); }
        let before = self.frozen.len();
        self.frozen.retain(|(host, tab), _| self.hosts.contains_key(host) && self.pages.contains_key(tab)
            && self.layouts.get(host).is_some_and(|layout| layout.contains_key(tab)));
        if before != self.frozen.len() { self.mirror_revision = self.mirror_revision.wrapping_add(1); }
    }

    /// True means the existing view may now be reparented.
    fn prepare_handoff(&mut self, tab: TabId, target: u64) -> bool {
        // The temporary dock belongs to its original GPUI parent. Restore the
        // full viewport before snapshotting or transferring the inspected view.
        if self.devtools.is_some_and(|dock| dock.tab == tab && dock.host != target) { self.close_devtools(); }
        if self.inspector_probe.is_some_and(|probe| probe.tab == tab && probe.host != target) { self.close_inspector_probe(); }
        let Some(page) = self.pages.get(&tab) else { return true; };
        if page.owner == target || !self.engine.supports_snapshots() { return true; }
        if let Some(claim) = self.handoffs.get(&tab) {
            if claim.target == target {
                if claim.ready || self.now() >= claim.deadline {
                    self.freeze_cached(tab);
                    return true;
                }
                return false;
            }
        }
        let (source, view, frame) = (page.owner, page.view, page.frame);
        let existing = page.snapshot_pending.filter(|_| page.snapshot_origin == Some((source, frame)));
        let request = if let Some(request) = existing { request } else {
            let Some(request) = self.next_snapshot.checked_add(1) else { return true; };
            self.next_snapshot = request;
            if !self.engine.request_snapshot(view, request) { self.freeze_cached(tab); return true; }
            let page = self.pages.get_mut(&tab).unwrap();
            page.snapshot_pending = Some(request);
            page.snapshot_origin = Some((source, frame));
            request
        };
        self.handoffs.insert(tab, Handoff { source, target, view, frame, request,
            deadline: self.now() + HANDOFF_TIMEOUT, ready: false });
        false
    }

    fn freeze_cached(&mut self, tab: TabId) {
        let Some(page) = self.pages.get(&tab) else { return; };
        if page.snapshot_good_origin != Some((page.owner, page.frame))
            || !self.layouts.get(&page.owner).is_some_and(|layout| layout.contains_key(&tab)) { return; }
        if let Some((_, png)) = &page.snapshot {
            let key = (page.owner, tab);
            if !self.frozen.get(&key).is_some_and(|old| Arc::ptr_eq(old, png)) {
                self.frozen.insert(key, png.clone());
                self.mirror_revision = self.mirror_revision.wrapping_add(1);
            }
        }
    }

    fn drain_snapshots(&mut self) {
        self.prune();
        for result in self.engine.take_snapshots() {
            let Some(tab) = self.pages.iter().find_map(|(tab, page)|
                (page.view == result.view && page.snapshot_pending == Some(result.request)).then_some(*tab)) else { continue; };
            let now = self.now();
            let page = self.pages.get_mut(&tab).unwrap();
            page.snapshot_pending = None;
            let origin = page.snapshot_origin.take();
            let image = result.png.map(Arc::new);
            // Preserve the diagnostic cache API independently of handoff images.
            let timely = self.handoffs.get(&tab).is_none_or(|claim| now < claim.deadline);
            if let Some(png) = &image {
                if timely {
                    page.snapshot = Some((result.request, png.clone()));
                    page.snapshot_good_origin = origin;
                }
            }
            if let Some(claim) = self.handoffs.get_mut(&tab).filter(|claim|
                claim.request == result.request && claim.view == result.view
                    && claim.source == page.owner && claim.frame == page.frame
                    && origin == Some((claim.source, claim.frame))) {
                // Commit the image only when prepare_handoff revalidates this
                // claim immediately before reparenting. Merely completing a
                // cancelled/obsolete claim must not replace a host photograph.
                claim.ready = true;
                self.mirror_revision = self.mirror_revision.wrapping_add(1);
            }
        }
    }
}

impl Drop for Pages {
    fn drop(&mut self) { self.release_host(); }
}

#[cfg(test)]
#[path = "pages_resize_focus_tests.rs"]
mod resize_focus_tests;
