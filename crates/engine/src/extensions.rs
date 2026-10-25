//! Opt-in source-built CEF bridge. Stock CEF cleanly reports unavailable.
//! No Chrome window, toolbar, DevTools default context or renderer transport.
use crate::{Engine, Runtime, ViewId, ViewRole};
use cef::ImplBrowser;
use serde_json::Value;
use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_void},
    rc::Rc,
};

type ManagementCallback = unsafe extern "C" fn(*mut c_void, u32, u64, *const c_char);
type PopupCallback = unsafe extern "C" fn(*mut c_void, u32, u64, usize, *const c_char, i32, i32);
type LoadCallback = unsafe extern "C" fn(*mut c_void, u32, *const c_char);
#[repr(C)]
#[derive(Clone, Copy)]
struct Api {
    size: u32,
    version: u32,
    load: unsafe extern "C" fn(i32, *const c_char, LoadCallback, *mut c_void) -> u32,
    has: unsafe extern "C" fn(i32, *const c_char) -> i32,
    invoke: unsafe extern "C" fn(i32, *const c_char, PopupCallback, *mut c_void, *mut u64) -> u32,
    resize: unsafe extern "C" fn(i32, u64, i32, i32) -> u32,
    close: unsafe extern "C" fn(i32, u64) -> u32,
    watch: unsafe extern "C" fn(i32, ManagementCallback, *mut c_void) -> u32,
    unwatch: unsafe extern "C" fn(i32) -> u32,
    set_enabled: unsafe extern "C" fn(i32, *const c_char, i32, u64) -> u32,
    install_store: unsafe extern "C" fn(i32, *const c_char, *mut u64) -> u32,
    decide_store: unsafe extern "C" fn(i32, u64, i32) -> u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Api2 {
    size: u32,
    version: u32,
    remove: unsafe extern "C" fn(i32, *const c_char, u64) -> u32,
    set_shortcut: unsafe extern "C" fn(i32, *const c_char, *const c_char, *const c_char, u64) -> u32,
    dispatch_key: unsafe extern "C" fn(i32, i32, u32) -> u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Api3 {
    size: u32,
    version: u32,
    watch: unsafe extern "C" fn(i32, ManagementCallback, *mut c_void) -> u32,
    unwatch: unsafe extern "C" fn(i32) -> u32,
    sync: unsafe extern "C" fn(i32, *const c_char) -> u32,
    complete: unsafe extern "C" fn(i32, u64, *const c_char) -> u32,
    pending: unsafe extern "C" fn(i32, u64) -> u32,
}
struct Native {
    api: Api,
    ui: Option<Api2>,
    model: Option<Api3>,
    _library: libloading::Library,
}
impl Native {
    fn load() -> Result<Rc<Self>, String> {
        // Open only the framework already used by this engine. No environment
        // override, external search path, or alternate CEF is loaded here.
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        let path = exe
            .parent()
            .and_then(|p| p.parent())
            .ok_or("Missing bundle")?
            .join("Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework");
        #[cfg(target_os = "windows")]
        let path = exe
            .parent()
            .ok_or("Missing executable directory")?
            .join("libcef.dll");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let path = exe
            .parent()
            .ok_or("Missing executable directory")?
            .join("libcef.so");
        unsafe {
            let library = libloading::Library::new(path)
                .map_err(|e| format!("Extension bridge unavailable: {e}"))?;
            let getter = library
                .get::<unsafe extern "C" fn(u32, *mut Api) -> u32>(
                    b"cef_echo_extensions_get_api_v1\0",
                )
                .map_err(|_| "This CEF distribution has no Echo extension bridge")?;
            let mut api = std::mem::MaybeUninit::<Api>::uninit();
            status(getter(std::mem::size_of::<Api>() as u32, api.as_mut_ptr()))?;
            let api = api.assume_init();
            if api.version != 1 || api.size as usize != std::mem::size_of::<Api>() {
                return Err("Unsupported extension bridge ABI".into());
            }
            let ui = library.get::<unsafe extern "C" fn(u32, *mut Api2) -> u32>(
                b"cef_echo_extensions_get_api_v2\0").ok().and_then(|getter| {
                    let mut ui = std::mem::MaybeUninit::<Api2>::uninit();
                    if getter(std::mem::size_of::<Api2>() as u32, ui.as_mut_ptr()) != 0 { return None; }
                    let ui = ui.assume_init();
                    (ui.version == 2 && ui.size as usize == std::mem::size_of::<Api2>()).then_some(ui)
                });
            let model = library.get::<unsafe extern "C" fn(u32, *mut Api3) -> u32>(
                b"cef_echo_extensions_get_api_v3\0").ok().and_then(|getter| {
                    let mut api = std::mem::MaybeUninit::<Api3>::uninit();
                    if getter(std::mem::size_of::<Api3>() as u32, api.as_mut_ptr()) != 0 { return None; }
                    let api = api.assume_init();
                    (api.version == 3 && api.size as usize == std::mem::size_of::<Api3>()).then_some(api)
                });
            Ok(Rc::new(Self { api, ui, model, _library: library }))
        }
    }
}

/// A native extension mutation, still bound to the exact submitting page and
/// profile. GPUI must validate opaque model keys before editing its model.
#[derive(Clone, Debug)]
pub struct ExtensionModelRequest {
    pub owner: ViewId,
    pub token: u64,
    pub data: Value,
}
#[derive(Default)]
struct ModelEvents {
    closed: bool,
    pending: Vec<(u64, Value)>,
}
unsafe extern "C" fn model_event(data: *mut c_void, event: u32, token: u64, json: *const c_char) {
    let state = unsafe { &*(data as *const RefCell<ModelEvents>) };
    if event == 11 {
        { let mut state = state.borrow_mut(); state.closed = true; state.pending.clear(); }
        drop(unsafe { Rc::from_raw(data as *const RefCell<ModelEvents>) });
    } else if event == 17 {
        // Native bounds this queue to 128 outstanding operations and 30 seconds.
        if let Ok(value) = serde_json::from_str(&unsafe { copied_text(json) }) {
            state.borrow_mut().pending.push((token, value));
        }
    }
}
struct ModelEndpoint {
    _native: Rc<Native>,
    api: Api3,
    runtime: Rc<Runtime>,
    context: Rc<cef::RequestContext>,
    owner: i32,
    view: ViewId,
    events: Rc<RefCell<ModelEvents>>,
    last_snapshot: Option<String>,
}
impl Drop for ModelEndpoint {
    fn drop(&mut self) {
        if !self.events.borrow().closed && self.runtime.lifecycle.require_running().is_ok() {
            // Unwatch synchronously cancels pending native requests and emits
            // CLOSED before the callback's final Rc is released.
            let _ = unsafe { (self.api.unwatch)(self.owner) };
        }
    }
}
#[derive(Default)]
pub(crate) struct ModelTransport {
    native: RefCell<Option<Option<Rc<Native>>>>,
    endpoints: Vec<ModelEndpoint>,
}
impl ModelTransport {
    fn native(&self) -> Option<Rc<Native>> {
        self.native.borrow_mut().get_or_insert_with(||
            Native::load().ok().filter(|native| native.model.is_some())).clone()
    }
}
impl Engine {
    pub fn extension_browser_id(&self, view: ViewId) -> Option<i32> {
        self.views.get(&view).filter(|page| page.role == ViewRole::Page)
            .map(|page| page.browser.identifier())
    }
    /// Publishes a complete authoritative profile without pumping CEF or holding
    /// a GPUI borrow in a native callback. Stock CEF returns false.
    pub fn has_extension_context_view(&self, context: &crate::StorageContext) -> bool {
        self.extension_model.native().is_some() && self.check_storage_context(context).is_ok() && self.views.values().any(|page|
            page.role == ViewRole::Page && Rc::ptr_eq(&page.context,&context.context))
    }
    pub fn extension_model_available(&self, context: &crate::StorageContext) -> bool {
        self.extension_model.native().is_some() && self.check_storage_context(context).is_ok() && context.is_ready()
    }
    /// Force a fresh native projection after an accepted renderer metadata
    /// event, even if the intended GPUI URL already matches the committed URL.
    pub fn invalidate_extension_tab_metadata(&mut self) {
        for endpoint in &mut self.extension_model.endpoints { endpoint.last_snapshot = None; }
    }
    pub fn sync_extension_tabs(&mut self, context: &crate::StorageContext, owner_view: ViewId,
        snapshot: &crate::ExtensionTabsSnapshot) -> Result<bool, String> {
        crate::shutdown::check_thread()?;
        self.check_storage_context(context)?;
        snapshot.validate()?;
        self.extension_model.endpoints.retain(|endpoint| !endpoint.events.borrow().closed);
        let mut index = self.extension_model.endpoints.iter()
            .position(|endpoint| Rc::ptr_eq(&endpoint.context, &context.context));
        if index.is_none() {
            let Some(page) = self.views.get(&owner_view).filter(|page|
                page.role == ViewRole::Page && Rc::ptr_eq(&page.context, &context.context)) else { return Ok(false); };
            let view = owner_view;
            let Some(native) = self.extension_model.native() else { return Ok(false); };
            let Some(api) = native.model else { return Ok(false); };
            let owner = page.browser.identifier();
            let events = Rc::new(RefCell::new(ModelEvents::default()));
            let data = Rc::into_raw(events.clone()) as *mut c_void;
            let result = unsafe { (api.watch)(owner, model_event, data) };
            if let Err(error) = status(result) {
                drop(unsafe { Rc::from_raw(data as *const RefCell<ModelEvents>) });
                return Err(error);
            }
            index = Some(self.extension_model.endpoints.len());
            self.extension_model.endpoints.push(ModelEndpoint { _native: native, api,
                runtime: self._runtime.clone(), context: context.context.clone(), owner,
                view, events, last_snapshot: None });
        }
        let endpoint = &mut self.extension_model.endpoints[index.unwrap()];
        let json = serde_json::to_string(snapshot).map_err(|error| error.to_string())?;
        if endpoint.last_snapshot.as_ref() != Some(&json) {
            let wire = text(&json)?;
            status(unsafe { (endpoint.api.sync)(endpoint.owner, wire.as_ptr()) })?;
            endpoint.last_snapshot = Some(json);
        }
        Ok(true)
    }
    pub fn take_extension_model_requests(&mut self) -> Vec<ExtensionModelRequest> {
        self.extension_model.endpoints.iter().filter(|endpoint|!endpoint.events.borrow().closed).flat_map(|endpoint| {
            let events = std::mem::take(&mut endpoint.events.borrow_mut().pending);
            events.into_iter().map(|(token,data)| ExtensionModelRequest { owner: endpoint.view, token, data })
        }).collect()
    }
    /// Complete only after publishing the resulting snapshot. A closed or
    /// replaced owner fails without redirecting work to another profile.
    pub fn complete_extension_model_request(&self, request: &ExtensionModelRequest,
        result: Value) -> Result<(), String> {
        crate::shutdown::check_thread()?;
        self._runtime.lifecycle.require_running()?;
        let endpoint = self.extension_model.endpoints.iter().find(|endpoint|
            endpoint.view == request.owner && !endpoint.events.borrow().closed)
            .ok_or("Extension model owner closed")?;
        let json = text(&serde_json::to_string(&result).map_err(|error| error.to_string())?)?;
        status(unsafe { (endpoint.api.complete)(endpoint.owner, request.token, json.as_ptr()) })
    }
    pub fn extension_model_request_pending(&self, request: &ExtensionModelRequest) -> bool {
        if crate::shutdown::check_thread().is_err() || self._runtime.lifecycle.require_running().is_err() { return false; }
        self.extension_model.endpoints.iter().find(|endpoint|
            endpoint.view == request.owner && !endpoint.events.borrow().closed)
            .is_some_and(|endpoint| unsafe { (endpoint.api.pending)(endpoint.owner,request.token) == 0 })
    }
}
pub(crate) fn ui_available() -> bool {
    Native::load().is_ok_and(|native| native.ui.is_some())
}
fn status(code: u32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("CEF extension operation failed ({code})"))
    }
}
fn text(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| "Extension argument contains NUL".into())
}
unsafe fn copied_text(value: *const c_char) -> String {
    if value.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned()
    }
}

/// Actual profile snapshots, operation results and permission continuations.
/// InstallPrompt must be presented with all supplied permission details before
/// calling approve_install. Merely receiving it never approves installation.
#[derive(Debug, Clone)]
pub enum ExtensionEvent {
    Catalog(Value),
    OperationComplete { request: u64, result: Value },
    InstallPrompt { request: u64, prompt: Value },
    InstallComplete { request: u64, result: Value },
    ActionRequested { id: String },
    OpenUrlRequested { url: String },
    Closed,
    InvalidPayload(String),
}
#[derive(Default)]
struct Events {
    closed: bool,
    pending: Vec<ExtensionEvent>,
}
unsafe extern "C" fn management(data: *mut c_void, event: u32, request: u64, json: *const c_char) {
    // Native registration owns exactly one Rc until WATCH_CLOSED.
    let state = unsafe { &*(data as *const RefCell<Events>) };
    let mut state = state.borrow_mut();
    if event == 11 {
        state.closed = true;
        state.pending.push(ExtensionEvent::Closed);
        drop(state);
        drop(unsafe { Rc::from_raw(data as *const RefCell<Events>) });
        return;
    }
    let payload = serde_json::from_str::<Value>(&unsafe { copied_text(json) });
    let value = match payload {
        Ok(value) => match event {
            10 => ExtensionEvent::Catalog(value),
            12 => ExtensionEvent::OperationComplete {
                request,
                result: value,
            },
            13 => ExtensionEvent::InstallPrompt {
                request,
                prompt: value,
            },
            14 => ExtensionEvent::InstallComplete {
                request,
                result: value,
            },
            15 => ExtensionEvent::ActionRequested { id: value["id"].as_str().unwrap_or_default().into() },
            16 => ExtensionEvent::OpenUrlRequested { url: value["url"].as_str().unwrap_or_default().into() },
            _ => ExtensionEvent::InvalidPayload(format!("Unknown extension event {event}")),
        },
        Err(error) => ExtensionEvent::InvalidPayload(error.to_string()),
    };
    state.pending.push(value);
}

/// A borrowed native view is valid only until its synchronous Closed callback.
#[derive(Debug)]
pub struct ExtensionPopupEvent {
    pub kind: u32,
    pub popup: u64,
    pub native_view: usize,
    pub title: String,
    pub width: i32,
    pub height: i32,
}
struct PopupState {
    closed: bool,
    callback: Box<dyn FnMut(ExtensionPopupEvent)>,
}
unsafe extern "C" fn popup(
    data: *mut c_void,
    kind: u32,
    id: u64,
    view: usize,
    title: *const c_char,
    width: i32,
    height: i32,
) {
    let state = unsafe { &*(data as *const RefCell<PopupState>) };
    let mut state = state.borrow_mut();
    if kind == 4 {
        state.closed = true;
    }
    // Never unwind across CEF's C ABI. Native attachment callbacks must detach
    // synchronously on Closed, and must not reenter CEF.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (state.callback)(ExtensionPopupEvent {
            kind,
            popup: id,
            native_view: view,
            title: unsafe { copied_text(title) },
            width,
            height,
        })
    }));
    drop(state);
    if kind == 4 {
        drop(unsafe { Rc::from_raw(data as *const RefCell<PopupState>) });
    }
}

/// Main-thread profile bridge bound to a live Alloy page. Drop before shutdown.
pub struct ExtensionBridge {
    native: Rc<Native>,
    runtime: Rc<Runtime>,
    owner: i32,
    events: Rc<RefCell<Events>>,
}
impl Engine {
    pub fn extension_bridge(&self, view: ViewId) -> Result<ExtensionBridge, String> {
        crate::shutdown::check_thread()?;
        self._runtime.lifecycle.require_running()?;
        let page = self
            .views
            .get(&view)
            .filter(|p| p.role == ViewRole::Page)
            .ok_or("Extension owner page is missing")?;
        let native = Native::load()?;
        let events = Rc::new(RefCell::new(Events::default()));
        let data = Rc::into_raw(events.clone()) as *mut c_void;
        let result = unsafe { (native.api.watch)(page.browser.identifier(), management, data) };
        if let Err(error) = status(result) {
            drop(unsafe { Rc::from_raw(data as *const RefCell<Events>) });
            return Err(error);
        }
        Ok(ExtensionBridge {
            native,
            runtime: self._runtime.clone(),
            owner: page.browser.identifier(),
            events,
        })
    }
}
impl ExtensionBridge {
    fn check(&self) -> Result<(), String> {
        crate::shutdown::check_thread()?;
        self.runtime.lifecycle.require_running()?;
        if self.events.borrow().closed {
            Err("Extension owner page closed".into())
        } else {
            Ok(())
        }
    }
    pub fn load_unpacked(&self, path: &str, callback: Box<dyn FnOnce(Result<String, String>)>) -> Result<(), String> {
        self.check()?;
        let path = text(path)?;
        type Completion = Box<dyn FnOnce(Result<String, String>)>;
        unsafe extern "C" fn loaded(data: *mut c_void, result: u32, value: *const c_char) {
            let callback = unsafe { Box::from_raw(data as *mut Completion) };
            let value = unsafe { copied_text(value) };
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                callback(if result == 0 { Ok(value) } else { Err(value) });
            }));
        }
        let data = Box::into_raw(Box::new(callback)) as *mut c_void;
        let result = unsafe { (self.native.api.load)(self.owner, path.as_ptr(), loaded, data) };
        if result != 0 { drop(unsafe { Box::from_raw(data as *mut Completion) }); }
        status(result)
    }
    pub fn take_events(&self) -> Vec<ExtensionEvent> {
        std::mem::take(&mut self.events.borrow_mut().pending)
    }
    pub fn set_enabled(&self, id: &str, enabled: bool, request: u64) -> Result<(), String> {
        self.check()?;
        let id = text(id)?;
        status(unsafe {
            (self.native.api.set_enabled)(self.owner, id.as_ptr(), enabled.into(), request)
        })
    }
    pub fn remove(&self, id: &str, request: u64) -> Result<(), String> {
        self.check()?;
        let ui = self.native.ui.ok_or("This CEF has no extension management API")?;
        let id = text(id)?;
        status(unsafe { (ui.remove)(self.owner, id.as_ptr(), request) })
    }
    pub fn set_shortcut(&self, id: &str, command: &str, shortcut: &str, request: u64) -> Result<(), String> {
        self.check()?;
        let ui = self.native.ui.ok_or("This CEF has no extension shortcut API")?;
        let (id, command, shortcut) = (text(id)?, text(command)?, text(shortcut)?);
        status(unsafe { (ui.set_shortcut)(self.owner, id.as_ptr(), command.as_ptr(), shortcut.as_ptr(), request) })
    }
    /// Starts Chromium's metadata, policy, permission and verified CRX flow.
    pub fn install_web_store(&self, item_id: &str) -> Result<u64, String> {
        self.check()?;
        let item = text(item_id)?;
        let mut request = 0;
        status(unsafe {
            (self.native.api.install_store)(self.owner, item.as_ptr(), &mut request)
        })?;
        Ok(request)
    }
    pub fn approve_install(&self, request: u64, accepted: bool) -> Result<(), String> {
        self.check()?;
        status(unsafe { (self.native.api.decide_store)(self.owner, request, accepted.into()) })
    }
    pub fn cancel_install(&self, request: u64) -> Result<(), String> {
        self.check()?;
        status(unsafe { (self.native.api.decide_store)(self.owner, request, -1) })
    }
    /// # Safety
    /// The callback must attach only the borrowed native view supplied by CEF,
    /// detach it synchronously on kind=4 (Closed), never retain it afterward,
    /// and never reenter CEF. Keep all native handling on this UI thread.
    pub unsafe fn invoke_action(
        &self,
        id: &str,
        callback: impl FnMut(ExtensionPopupEvent) + 'static,
    ) -> Result<ExtensionPopup, String> {
        self.check()?;
        let id = text(id)?;
        let state = Rc::new(RefCell::new(PopupState {
            closed: false,
            callback: Box::new(callback),
        }));
        let data = Rc::into_raw(state.clone()) as *mut c_void;
        let mut token = 0;
        let result =
            unsafe { (self.native.api.invoke)(self.owner, id.as_ptr(), popup, data, &mut token) };
        if let Err(error) = status(result) {
            drop(unsafe { Rc::from_raw(data as *const RefCell<PopupState>) });
            return Err(error);
        }
        Ok(ExtensionPopup {
            native: self.native.clone(),
            runtime: self.runtime.clone(),
            owner: self.owner,
            token,
            state,
        })
    }
}
impl Drop for ExtensionBridge {
    fn drop(&mut self) {
        if self.check().is_ok() {
            unsafe {
                (self.native.api.unwatch)(self.owner);
            }
        }
    }
}
pub struct ExtensionPopup {
    native: Rc<Native>,
    runtime: Rc<Runtime>,
    owner: i32,
    token: u64,
    state: Rc<RefCell<PopupState>>,
}
impl ExtensionPopup {
    pub fn resize(&self, width: i32, height: i32) -> Result<(), String> {
        crate::shutdown::check_thread()?;
        self.runtime.lifecycle.require_running()?;
        if self.state.borrow().closed {
            return Err("Extension popup closed".into());
        }
        status(unsafe { (self.native.api.resize)(self.owner, self.token, width, height) })
    }
}
impl Drop for ExtensionPopup {
    fn drop(&mut self) {
        if !self.state.borrow().closed && self.runtime.lifecycle.require_running().is_ok() {
            unsafe {
                (self.native.api.close)(self.owner, self.token);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_copies_prompt_and_balances_native_registration_on_close() {
        let state = Rc::new(RefCell::new(Events::default()));
        let pointer = Rc::into_raw(state.clone()) as *mut c_void;
        let json =
            CString::new(r#"{"permissions":[{"message":"Read site data","details":"All sites"}]}"#)
                .unwrap();
        unsafe {
            management(pointer, 13, 7, json.as_ptr());
        }
        drop(json);
        assert!(
            matches!(&state.borrow().pending[0], ExtensionEvent::InstallPrompt { request:7, prompt } if prompt["permissions"][0]["details"] == "All sites")
        );
        unsafe {
            management(pointer, 11, 0, std::ptr::null());
        }
        assert_eq!(Rc::strong_count(&state), 1);
        assert!(state.borrow().closed);
    }
    #[test]
    fn malformed_native_payload_is_reported_without_panicking() {
        let state = Rc::new(RefCell::new(Events::default()));
        let pointer = Rc::into_raw(state.clone()) as *mut c_void;
        let json = CString::new("{").unwrap();
        unsafe {
            management(pointer, 10, 0, json.as_ptr());
            management(pointer, 11, 0, std::ptr::null());
        }
        assert!(matches!(
            state.borrow().pending[0],
            ExtensionEvent::InvalidPayload(_)
        ));
        assert!(text("bad\0argument").is_err());
    }
}

thread_local! {
    static KEY_API: std::cell::OnceCell<Option<Rc<Native>>> = const { std::cell::OnceCell::new() };
}
pub(crate) fn dispatch_key(owner: i32, key: i32, modifiers: u32) -> bool {
    KEY_API.with(|cache| cache.get_or_init(|| Native::load().ok()).as_ref()
        .and_then(|native| native.ui).is_some_and(|ui| unsafe {
            (ui.dispatch_key)(owner, key, modifiers) == 0
        }))
}

/// Window-independent profile operations; fakes exercise the same UI boundary.
pub trait ExtensionService {
    fn load_unpacked(&self, _path: &str, _callback: Box<dyn FnOnce(Result<String, String>)>) -> Result<(), String> { Err("Unpacked extension loading is unavailable".into()) }
    fn take_events(&self) -> Vec<ExtensionEvent>;
    fn set_enabled(&self, id: &str, enabled: bool, request: u64) -> Result<(), String>;
    fn remove(&self, id: &str, request: u64) -> Result<(), String>;
    fn set_shortcut(&self, id: &str, command: &str, shortcut: &str, request: u64) -> Result<(), String>;
    fn approve_install(&self, request: u64, accepted: bool) -> Result<(), String>;
    /// # Safety
    /// Same borrowed-view and synchronous detachment contract as invoke_action.
    unsafe fn invoke_action(&self, id: &str, callback: Box<dyn FnMut(ExtensionPopupEvent)>) -> Result<ExtensionPopup, String>;
}
impl ExtensionService for ExtensionBridge {
    fn load_unpacked(&self, path: &str, callback: Box<dyn FnOnce(Result<String, String>)>) -> Result<(), String> { self.load_unpacked(path, callback) }
    fn take_events(&self) -> Vec<ExtensionEvent> { self.take_events() }
    fn set_enabled(&self, id: &str, enabled: bool, request: u64) -> Result<(), String> { self.set_enabled(id, enabled, request) }
    fn remove(&self, id: &str, request: u64) -> Result<(), String> { self.remove(id, request) }
    fn set_shortcut(&self, id: &str, command: &str, shortcut: &str, request: u64) -> Result<(), String> { self.set_shortcut(id, command, shortcut, request) }
    fn approve_install(&self, request: u64, accepted: bool) -> Result<(), String> { self.approve_install(request, accepted) }
    unsafe fn invoke_action(&self, id: &str, callback: Box<dyn FnMut(ExtensionPopupEvent)>) -> Result<ExtensionPopup, String> {
        unsafe { self.invoke_action(id, callback) }
    }
}
