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
struct Native {
    api: Api,
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
            Ok(Rc::new(Self {
                api,
                _library: library,
            }))
        }
    }
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
