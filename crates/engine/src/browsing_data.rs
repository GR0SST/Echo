//! Profile-bound native Chromium deletion. No renderer, DevTools or WebUI transport.
use crate::{Runtime, StorageContext};
use cef::ImplRequestContext;
use std::{cell::Cell, ffi::c_void, rc::Rc};
type Callback = unsafe extern "C" fn(*mut c_void, u64);
type CountCallback = unsafe extern "C" fn(*mut c_void, *const std::ffi::c_char);
#[repr(C)]
#[derive(Clone, Copy)]
struct Api {
    size: u32,
    version: u32,
    clear: unsafe extern "C" fn(*mut c_void, u32, i64, Callback, *mut c_void) -> u32,
    count: unsafe extern "C" fn(*mut c_void, u32, i64, CountCallback, *mut c_void) -> u32,
}
struct Native {
    api: Api,
    _library: libloading::Library,
}
impl Native {
    fn load() -> Result<Rc<Self>, String> {
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
            let library =
                libloading::Library::new(path).map_err(|_| "Browsing-data bridge unavailable")?;
            let getter = library
                .get::<unsafe extern "C" fn(u32, *mut Api) -> u32>(
                    b"cef_echo_browsing_data_get_api_v1\0",
                )
                .map_err(|_| "This CEF has no browsing-data bridge")?;
            let mut api = std::mem::MaybeUninit::<Api>::uninit();
            if getter(std::mem::size_of::<Api>() as u32, api.as_mut_ptr()) != 0 {
                return Err("Browsing-data ABI rejected".into());
            }
            let api = api.assume_init();
            if api.version != 1 || api.size as usize != std::mem::size_of::<Api>() {
                return Err("Unsupported browsing-data ABI".into());
            }
            Ok(Rc::new(Self {
                api,
                _library: library,
            }))
        }
    }
}
/// Poll on the UI thread. Dropping a ticket never cancels an accepted deletion.
/// Its callback retains the exact storage/runtime until the native remover replies.
pub struct BrowsingDataRequest {
    result: Rc<Cell<Option<u64>>>,
}
impl BrowsingDataRequest {
    pub fn take(&self) -> Option<Result<(), u64>> {
        self.result
            .take()
            .map(|mask| if mask == 0 { Ok(()) } else { Err(mask) })
    }
}
struct Pending {
    result: Rc<Cell<Option<u64>>>,
    _runtime: Rc<Runtime>,
    _context: Rc<cef::RequestContext>,
    _native: Rc<Native>,
}
unsafe extern "C" fn completed(data: *mut c_void, failed: u64) {
    // C ABI guarantees exactly one UI-thread callback only after acceptance.
    let pending = unsafe { Box::from_raw(data.cast::<Pending>()) };
    pending.result.set(Some(failed));
}
impl StorageContext {
    /// Bits: history=1, site data=2, cache=4, downloads=8, form data=16,
    /// site settings=32, hosted-app data=64. Since is Unix microseconds (-1=all).
    pub fn clear_browsing_data(
        &self,
        categories: u32,
        since: i64,
    ) -> Result<BrowsingDataRequest, String> {
        if !self.is_ready() || cef::currently_on(cef::ThreadId::UI) == 0 {
            return Err("Profile context is not ready".into());
        }
        if categories == 0 || categories & !127 != 0 || since < -1 {
            return Err("Invalid browsing-data selection".into());
        }
        let native = Native::load()?;
        let result = Rc::new(Cell::new(None));
        let pending = Box::new(Pending {
            result: result.clone(),
            _runtime: self.runtime.clone(),
            _context: self.context.clone(),
            _native: native.clone(),
        });
        let raw = Box::into_raw(pending);
        // CEF's generated CppToC_Unwrap consumes one incoming C reference.
        // Transfer an extra clone; the registry's borrowed reference stays live.
        let transferred: cef::RequestContext = self.context.as_ref().clone();
        let status = unsafe {
            (native.api.clear)(
                ImplRequestContext::get_raw(&transferred).cast(),
                categories,
                since,
                completed,
                raw.cast(),
            )
        };
        // Sequence/argument rejection occurs before unwrapping. All other
        // v1 statuses have consumed the transferred reference, including 2/4.
        if matches!(status, 1 | 3) {
            drop(transferred);
        } else {
            std::mem::forget(transferred);
        }
        if status != 0 {
            unsafe {
                drop(Box::from_raw(raw));
            }
            return Err(format!("Browsing-data removal was rejected ({status})"));
        }
        Ok(BrowsingDataRequest { result })
    }
}

/// Native localized counter text, bound to one immutable context and start time.
pub struct BrowsingDataCounters {
    values: Rc<std::cell::RefCell<[Option<String>; 7]>>,
}
impl BrowsingDataCounters {
    pub fn snapshot(&self) -> [Option<String>; 7] {
        self.values.borrow().clone()
    }
}
struct CounterPending {
    values: Rc<std::cell::RefCell<[Option<String>; 7]>>,
    index: usize,
    _runtime: Rc<Runtime>,
    _context: Rc<cef::RequestContext>,
    _native: Rc<Native>,
}
unsafe extern "C" fn counted(data: *mut c_void, text: *const std::ffi::c_char) {
    let pending = unsafe { Box::from_raw(data.cast::<CounterPending>()) };
    if !text.is_null() {
        let text = unsafe { std::ffi::CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned();
        pending.values.borrow_mut()[pending.index] = Some(text);
    }
}
impl StorageContext {
    pub fn browsing_data_counters(&self, since: i64) -> Result<BrowsingDataCounters, String> {
        if !self.is_ready() || cef::currently_on(cef::ThreadId::UI) == 0 || since < -1 {
            return Err("Profile counters are not ready".into());
        }
        let native = Native::load()?;
        let values = Rc::new(std::cell::RefCell::new(std::array::from_fn(|_| None)));
        for index in 0..7 {
            if matches!(index, 0 | 3) {
                values.borrow_mut()[index] = Some(String::new());
                continue;
            }
            #[cfg(debug_assertions)]
            if std::env::var("BROWSER_BROWSING_DATA_FIXTURE")
                .ok()
                .as_deref()
                == Some("1")
            {
                eprintln!("[browsing-data-counter] start category={}", 1 << index);
            }
            let pending = Box::into_raw(Box::new(CounterPending {
                values: values.clone(),
                index,
                _runtime: self.runtime.clone(),
                _context: self.context.clone(),
                _native: native.clone(),
            }));
            let transferred: cef::RequestContext = self.context.as_ref().clone();
            let status = unsafe {
                (native.api.count)(
                    ImplRequestContext::get_raw(&transferred).cast(),
                    1 << index,
                    since,
                    counted,
                    pending.cast(),
                )
            };
            if matches!(status, 1 | 3) {
                drop(transferred);
            } else {
                std::mem::forget(transferred);
            }
            if status != 0 {
                unsafe {
                    drop(Box::from_raw(pending));
                }
                values.borrow_mut()[index] = Some(String::new());
            }
        }
        Ok(BrowsingDataCounters { values })
    }
}
