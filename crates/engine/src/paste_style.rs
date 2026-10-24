//! Owned focused-frame editing requests, dispatched outside application borrows.
use crate::{
    Browser, Engine, InspectorId, InspectorPhase, RequestContext, Runtime, ViewId, ViewRole,
};
use cef::{ImplBrowser, ImplFrame};
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteDispatch {
    Submitted,
    Busy,
    Unavailable,
}

/// The original browser/context are retained, but logical closure invalidates
/// the request even if downloads retain a hidden native host.
pub struct PasteRequest {
    browser: Browser,
    _context: Rc<RequestContext>,
    runtime: Rc<Runtime>,
    view: Weak<()>,
    finished: bool,
}

impl PasteRequest {
    pub fn try_dispatch(&mut self) -> PasteDispatch {
        if self.finished
            || self.view.upgrade().is_none()
            || self.runtime.lifecycle.require_running().is_err()
        {
            self.finished = true;
            return PasteDispatch::Unavailable;
        }
        if !self.runtime.lifecycle.native_dispatch_allowed() {
            return PasteDispatch::Busy;
        }
        self.finished = true;
        if self.browser.is_valid() == 0 {
            return PasteDispatch::Unavailable;
        }
        let Some(frame) = self.browser.focused_frame() else {
            return PasteDispatch::Unavailable;
        };
        frame.paste_and_match_style();
        PasteDispatch::Submitted
    }
}

impl Engine {
    fn prepare_frame_paste(&self, id: ViewId, role: ViewRole) -> Option<PasteRequest> {
        self._runtime.lifecycle.require_running().ok()?;
        let view = self.views.get(&id).filter(|view| view.role == role)?;
        Some(PasteRequest {
            browser: view.browser.clone(),
            _context: view.context.clone(),
            runtime: self._runtime.clone(),
            view: Rc::downgrade(&view.action_lifetime),
            finished: false,
        })
    }

    pub fn prepare_paste_and_match_style(&self, id: ViewId) -> Option<PasteRequest> {
        self.prepare_frame_paste(id, ViewRole::Page)
    }

    /// Inspector frontend identity remains separate from its inspected page.
    pub fn prepare_inspector_paste_and_match_style(&self, id: InspectorId) -> Option<PasteRequest> {
        let session = self.inspectors.sessions.get(&id).filter(|session| {
            !matches!(
                session.status.phase,
                InspectorPhase::Detached | InspectorPhase::LoadFailed
            )
        })?;
        self.prepare_frame_paste(session.frontend, ViewRole::Inspector(id))
    }
}
