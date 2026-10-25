//! Explicit Save As transfers have their own native browser registration. The
//! source document is never navigated and a chooser path cannot be attributed
//! to an unrelated concurrent download from the same page.
use crate::{
    Bounds, DownloadEvent, DownloadOwner, Engine, RequestContext, Runtime, Shortcuts, ViewId,
    ViewRole,
};
use cef::{CefString, ImplBrowser, ImplBrowserHost, ImplRequestContext};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::{Rc, Weak},
};

thread_local! { static INITIATORS: RefCell<HashMap<u64, Engine>> = RefCell::new(HashMap::new()); }

pub struct FileDownloadRequest {
    runtime: Weak<Runtime>,
    context: Rc<RequestContext>,
    shortcuts: Shortcuts,
    source: Weak<()>,
    browser: i32,
    epoch: u64,
    url: String,
}
impl FileDownloadRequest {
    /// Run on the CEF/UI thread outside application, page and engine borrows.
    pub fn start(self, owner: u64) -> Result<(), String> {
        crate::shutdown::check_thread()?;
        let runtime = self
            .runtime
            .upgrade()
            .ok_or("Download runtime has closed")?;
        let current = || {
            self.source.upgrade().is_some()
                && crate::site_data::document_epoch(self.browser) == Some(self.epoch)
        };
        runtime.lifecycle.require_running()?;
        if !current() {
            return Err("Download source document has changed".into());
        }
        if INITIATORS.with_borrow(|sources| sources.contains_key(&owner)) {
            return Err("Download source already registered".into());
        }
        let host = Rc::new(crate::host::RetiredHost::new()?);
        runtime.lifecycle.require_running()?;
        if !current() {
            return Err("Download source document has changed".into());
        }
        let mut engine =
            Engine::with_context(runtime.clone(), self.context.clone(), self.shortcuts);
        let view = engine
            .create_content_view_owned(
                host.parent(),
                "about:blank",
                Bounds {
                    width: 16.,
                    height: 16.,
                    ..Default::default()
                },
                0.,
                ViewRole::Page,
                self.context,
                Some(DownloadOwner::continue_normal_after_source_close(owner)),
                Some(host),
                None,
                true,
            )
            .ok_or("Cannot create the owned download initiator")?;
        runtime.lifecycle.require_running()?;
        if self.source.upgrade().is_none()
            || crate::site_data::document_epoch(self.browser) != Some(self.epoch)
        {
            return Err("Download source document has changed".into());
        }
        let browser = engine.views.get(&view).unwrap().browser.clone();
        let host = browser.host().ok_or("Download browser is unavailable")?;
        // Install ownership before CEF, which may synchronously deliver input.
        // No native call or Engine drop occurs under this thread-local borrow.
        INITIATORS.with_borrow_mut(|sources| {
            sources.insert(owner, engine);
        });
        host.start_download(Some(&CefString::from(self.url.as_str())));
        Ok(())
    }
}

impl Engine {
    /// Preparation is pure. The request only weakly retains Runtime: an open
    /// chooser must never become an extra shutdown-owner barrier.
    pub fn prepare_file_download(&self, id: ViewId, url: &str) -> Option<FileDownloadRequest> {
        self._runtime.lifecycle.require_running().ok()?;
        let parsed = url::Url::parse(url).ok()?;
        if !matches!(parsed.scheme(), "http" | "https" | "blob" | "data") {
            return None;
        }
        let view = self
            .views
            .get(&id)
            .filter(|view| view.role == ViewRole::Page)?;
        if CefString::from(&view.context.cache_path())
            .to_string()
            .is_empty()
        {
            return None;
        }
        Some(FileDownloadRequest {
            runtime: Rc::downgrade(&self._runtime),
            context: view.context.clone(),
            shortcuts: self.shortcuts.clone(),
            source: Rc::downgrade(&view.action_lifetime),
            browser: view.browser.identifier(),
            epoch: crate::site_data::document_epoch(view.browser.identifier())?,
            url: url.to_owned(),
        })
    }
    /// A genuine copied event proves association. Logical retirement then uses
    /// the existing parking/continuation machinery and native lease owns the host.
    pub(crate) fn retire_file_download_initiators(events: &[DownloadEvent]) {
        let owners: HashSet<_> = events
            .iter()
            .map(|event| match event {
                DownloadEvent::DestinationRequested(s) | DownloadEvent::Updated(s) => {
                    s.provenance.owner()
                }
            })
            .collect();
        let retired = INITIATORS.with_borrow_mut(|sources| {
            owners
                .into_iter()
                .filter_map(|id| sources.remove(&id))
                .collect::<Vec<_>>()
        });
        drop(retired);
    }
    pub(crate) fn file_download_initiators_pending() -> bool {
        INITIATORS.with_borrow(|sources| !sources.is_empty())
    }
    /// Called outside App borrows before the native shutdown-owner barrier.
    pub fn release_file_download_initiators() {
        let retired = INITIATORS.with_borrow_mut(std::mem::take);
        drop(retired);
    }
}
