//! One-shot native printing outside application/model/engine borrows.
use crate::{Browser, Engine, RequestContext, Runtime, ViewId, ViewRole};
use cef::{ImplBrowser, ImplBrowserHost};
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintDispatch {
    Submitted,
    Busy,
    Unavailable,
}

/// Retains the exact context and initialized runtime, but not the Engine or its
/// view map. Logical view closure invalidates this request, even if a download
/// keeps its native browser parked. Main thread only; native dispatch may reenter.
pub struct PrintRequest {
    // Native handles must drop before their context/runtime owners.
    browser: Browser,
    _context: Rc<RequestContext>,
    runtime: Rc<Runtime>,
    view: Weak<()>,
    finished: bool,
}
impl PrintRequest {
    pub fn try_dispatch(&mut self) -> PrintDispatch {
        if self.finished
            || self.view.upgrade().is_none()
            || self.runtime.lifecycle.require_running().is_err()
        {
            self.finished = true;
            return PrintDispatch::Unavailable;
        }
        if !self.runtime.lifecycle.native_dispatch_allowed() {
            return PrintDispatch::Busy;
        }
        self.finished = true;
        if self.browser.is_valid() == 0 {
            return PrintDispatch::Unavailable;
        }
        let Some(host) = self.browser.host() else {
            return PrintDispatch::Unavailable;
        };
        // No lifecycle ledger, Engine, page or application borrow survives here.
        // Submitted means CEF received Print; it does not mean paper was printed.
        host.print();
        PrintDispatch::Submitted
    }
}
impl Engine {
    pub fn supports_printing(&self) -> bool {
        // CEF's Linux path requires a CefPrintHandler, not installed yet.
        cfg!(any(target_os = "macos", target_os = "windows"))
    }
    pub fn prepare_print(&self, id: ViewId) -> Option<PrintRequest> {
        if !self.supports_printing() {
            return None;
        }
        self._runtime.lifecycle.require_running().ok()?;
        let view = self
            .views
            .get(&id)
            .filter(|view| view.role == ViewRole::Page)?;
        Some(PrintRequest {
            browser: view.browser.clone(),
            _context: view.context.clone(),
            runtime: self._runtime.clone(),
            view: Rc::downgrade(&view.action_lifetime),
            finished: false,
        })
    }
}
