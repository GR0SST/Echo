//! Permission continuations remain on CEF's UI thread. The UI sees only owned
//! request descriptions and queues decisions; native continuations run while
//! pumping, outside application/window/page borrows.
use crate::{site_data, site_info::web_origin, EventQueue, PermissionKind, RawEvent};
use cef::*;
use std::{cell::RefCell, collections::HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PermissionRequestId(u64);
impl PermissionRequestId {
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequest {
    pub id: PermissionRequestId,
    pub origin: String,
    pub top_origin: String,
    pub permissions: Vec<PermissionKind>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Block,
    Dismiss,
}
#[derive(Clone, Debug)]
struct Scope {
    browser: i32,
    epoch: u64,
    frame: Option<String>,
    cef_prompt: Option<u64>,
}
impl Scope {
    fn matches(
        &self,
        browser: i32,
        epoch: Option<u64>,
        top: Option<&str>,
        request: &PermissionRequest,
        frame_origin: Option<&str>,
    ) -> bool {
        browser == self.browser
            && epoch == Some(self.epoch)
            && top == Some(request.top_origin.as_str())
            && (self.frame.is_none() || frame_origin == Some(request.origin.as_str()))
    }
}
struct Pending<T> {
    request: PermissionRequest,
    scope: Scope,
    payload: T,
    decision: Option<PermissionDecision>,
}
struct Requests<T> {
    next: u64,
    entries: HashMap<PermissionRequestId, Pending<T>>,
    cancelled: Vec<Pending<T>>,
}
impl<T> Default for Requests<T> {
    fn default() -> Self {
        Self {
            next: 0,
            entries: HashMap::new(),
            cancelled: Vec::new(),
        }
    }
}
impl<T> Requests<T> {
    fn insert(
        &mut self,
        origin: String,
        top_origin: String,
        permissions: Vec<PermissionKind>,
        scope: Scope,
        payload: T,
    ) -> Option<PermissionRequest> {
        let next = self.next.checked_add(1)?;
        self.next = next;
        let request = PermissionRequest {
            id: PermissionRequestId(next),
            origin,
            top_origin,
            permissions,
        };
        self.entries.insert(
            request.id,
            Pending {
                request: request.clone(),
                scope,
                payload,
                decision: None,
            },
        );
        Some(request)
    }
    fn answer(
        &mut self,
        browser: i32,
        id: PermissionRequestId,
        decision: PermissionDecision,
    ) -> bool {
        let Some(pending) = self
            .entries
            .get_mut(&id)
            .filter(|p| p.scope.browser == browser && p.decision.is_none())
        else {
            return false;
        };
        pending.decision = Some(decision);
        true
    }
    fn cancel(&mut self, matches: impl Fn(&Scope) -> bool) -> Vec<(i32, PermissionRequestId)> {
        let ids: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(&id, p)| matches(&p.scope).then_some(id))
            .collect();
        let mut closed = Vec::new();
        for id in ids {
            let mut pending = self.entries.remove(&id).unwrap();
            closed.push((pending.scope.browser, id));
            pending.decision = Some(PermissionDecision::Dismiss);
            self.cancelled.push(pending);
        }
        closed
    }
    fn ready(&mut self) -> Vec<Pending<T>> {
        let ids: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(&id, p)| p.decision.is_some().then_some(id))
            .collect();
        let mut ready = std::mem::take(&mut self.cancelled);
        ready.extend(ids.into_iter().filter_map(|id| self.entries.remove(&id)));
        ready
    }
}
struct Payload {
    browser: Browser,
    frame: Option<Frame>,
    callback: Continuation,
    events: EventQueue,
}
enum Continuation {
    Media(MediaAccessCallback, u32),
    Prompt(PermissionPromptCallback),
}
thread_local! { static REQUESTS: RefCell<Requests<Payload>> = RefCell::new(Requests::default()); }

pub(crate) fn pending(browser: i32, id: PermissionRequestId) -> bool {
    REQUESTS.with_borrow(|s| {
        s.entries
            .get(&id)
            .is_some_and(|p| p.scope.browser == browser)
    })
}
pub(crate) fn answer(browser: i32, id: PermissionRequestId, decision: PermissionDecision) -> bool {
    REQUESTS.with_borrow_mut(|s| s.answer(browser, id, decision))
}
pub(crate) fn cancel_browser(browser: i32) {
    cancel(|scope| scope.browser == browser);
}
pub(crate) fn navigated(browser: i32, frame: &Frame) {
    let main = frame.is_main() != 0;
    let frame = CefString::from(&frame.identifier()).to_string();
    cancel(|scope| scope.browser == browser && (main || scope.frame.as_ref() == Some(&frame)));
}
fn cancel(matches: impl Fn(&Scope) -> bool) {
    let closed = REQUESTS.with_borrow_mut(|s| {
        let closed = s.cancel(matches);
        closed
            .into_iter()
            .map(|(browser, id)| {
                let events = s
                    .cancelled
                    .iter()
                    .find(|p| p.request.id == id)
                    .unwrap()
                    .payload
                    .events
                    .clone();
                (browser, id, events)
            })
            .collect::<Vec<_>>()
    });
    for (browser, id, events) in closed {
        events.push((browser, RawEvent::PermissionDismissed(id)));
    }
}
pub(crate) fn destroyed(browser: i32) {
    // CEF already cancelled these callbacks. Release their handles without
    // invoking a continuation after OnBeforeClose, and outside the store borrow.
    let removed = REQUESTS.with_borrow_mut(|s| {
        let ids: Vec<_> = s
            .entries
            .iter()
            .filter_map(|(&id, p)| (p.scope.browser == browser).then_some(id))
            .collect();
        let mut removed: Vec<_> = ids
            .into_iter()
            .filter_map(|id| s.entries.remove(&id))
            .collect();
        let mut keep = Vec::new();
        for p in std::mem::take(&mut s.cancelled) {
            if p.scope.browser == browser {
                removed.push(p)
            } else {
                keep.push(p)
            }
        }
        s.cancelled = keep;
        removed
    });
    drop(removed);
}
pub(crate) fn pump() {
    let ready = REQUESTS.with_borrow_mut(Requests::ready);
    for pending in ready {
        let Payload {
            browser,
            frame,
            callback,
            events,
        } = pending.payload;
        let top = browser
            .main_frame()
            .and_then(|f| web_origin(&CefString::from(&f.url()).to_string()));
        let frame_origin = frame
            .as_ref()
            .filter(|f| f.is_valid() != 0)
            .and_then(|f| web_origin(&CefString::from(&f.url()).to_string()));
        let valid = pending.scope.matches(
            browser.identifier(),
            site_data::document_epoch(browser.identifier()),
            top.as_deref(),
            &pending.request,
            frame_origin.as_deref(),
        );
        let decision = if valid {
            pending.decision.unwrap_or(PermissionDecision::Dismiss)
        } else {
            PermissionDecision::Dismiss
        };
        if decision != PermissionDecision::Dismiss {
            if let Some(context) = browser.host().and_then(|h| h.request_context()) {
                let origin = CefString::from(pending.request.origin.as_str());
                let top = CefString::from(pending.request.top_origin.as_str());
                for permission in &pending.request.permissions {
                    context.set_content_setting(
                        Some(&origin),
                        Some(&top),
                        permission.cef_type(),
                        if decision == PermissionDecision::Allow {
                            ContentSettingValues::ALLOW
                        } else {
                            ContentSettingValues::BLOCK
                        },
                    );
                }
            }
        }
        // Removing the request precedes continuation: CEF can dismiss it
        // synchronously, enqueue another request, or show an OS permission sheet.
        match callback {
            Continuation::Media(callback, mask) => match decision {
                PermissionDecision::Allow => callback.cont(mask),
                PermissionDecision::Block => callback.cont(0),
                PermissionDecision::Dismiss => callback.cancel(),
            },
            Continuation::Prompt(callback) => callback.cont(match decision {
                PermissionDecision::Allow => PermissionRequestResult::ACCEPT,
                PermissionDecision::Block => PermissionRequestResult::DENY,
                PermissionDecision::Dismiss => PermissionRequestResult::IGNORE,
            }),
        }
        events.push((
            browser.identifier(),
            RawEvent::PermissionDismissed(pending.request.id),
        ));
    }
}
fn media_kinds(mask: u32) -> Option<Vec<PermissionKind>> {
    let flags = [
        (
            MediaAccessPermissionTypes::DEVICE_VIDEO_CAPTURE.get_raw(),
            PermissionKind::Camera,
        ),
        (
            MediaAccessPermissionTypes::DEVICE_AUDIO_CAPTURE.get_raw(),
            PermissionKind::Microphone,
        ),
    ];
    kinds(mask, &flags)
}
fn prompt_kinds(mask: u32) -> Option<Vec<PermissionKind>> {
    let permissions=kinds(mask,&[
        (PermissionRequestTypes::CAMERA_STREAM.get_raw(),PermissionKind::Camera),
        (PermissionRequestTypes::MIC_STREAM.get_raw(),PermissionKind::Microphone),
        (PermissionRequestTypes::GEOLOCATION.get_raw(),PermissionKind::Location),
        (PermissionRequestTypes::NOTIFICATIONS.get_raw(),PermissionKind::Notifications),
    ])?;
    (permissions.len()==1 || permissions==[PermissionKind::Camera,PermissionKind::Microphone]).then_some(permissions)
}

fn kinds(mask: u32, flags: &[(u32, PermissionKind)]) -> Option<Vec<PermissionKind>> {
    let known = flags.iter().fold(0, |all, (flag, _)| all | flag);
    if mask == 0 || mask & !known != 0 {
        return None;
    }
    Some(
        flags
            .iter()
            .filter_map(|(flag, kind)| (mask & flag != 0).then_some(*kind))
            .collect(),
    )
}
fn enqueue(
    browser: &Browser,
    frame: Option<&Frame>,
    origin: Option<&CefString>,
    permissions: Vec<PermissionKind>,
    cef_prompt: Option<u64>,
    callback: Continuation,
    events: &EventQueue,
) -> bool {
    let origin = origin.and_then(|o| web_origin(&o.to_string()));
    let top = browser
        .main_frame()
        .and_then(|f| web_origin(&CefString::from(&f.url()).to_string()));
    let (Some(origin), Some(top), Some(epoch)) =
        (origin, top, site_data::document_epoch(browser.identifier()))
    else {
        return false;
    };
    // Cross-origin embedded requests need a distinct per-frame settings UI.
    if origin != top {
        return false;
    }
    let scope = Scope {
        browser: browser.identifier(),
        epoch,
        frame: frame.map(|f| CefString::from(&f.identifier()).to_string()),
        cef_prompt,
    };
    let request = REQUESTS.with_borrow_mut(|s| {
        s.insert(
            origin,
            top,
            permissions,
            scope,
            Payload {
                browser: browser.clone(),
                frame: frame.cloned(),
                callback,
                events: events.clone(),
            },
        )
    });
    if let Some(request) = request {
        events.push((browser.identifier(), RawEvent::PermissionRequested(request)));
        true
    } else {
        false
    }
}
wrap_permission_handler! {
    pub(crate) struct Handler { events: EventQueue }
    impl PermissionHandler {
        fn on_request_media_access_permission(&self,browser:Option<&mut Browser>,frame:Option<&mut Frame>,requesting_origin:Option<&CefString>,requested_permissions:u32,callback:Option<&mut MediaAccessCallback>)->i32 {
            let Some(callback)=callback else{return 1;};
            let (Some(browser),Some(kinds))=(browser,media_kinds(requested_permissions)) else {callback.cont(0);return 1;};
            // Alloy delegates media policy to the client, including previously
            // stored settings. Never bypass an explicit per-profile block.
            let origin=requesting_origin.and_then(|o|web_origin(&o.to_string()));
            let top=browser.main_frame().and_then(|f|web_origin(&CefString::from(&f.url()).to_string()));
            if let (Some(origin),Some(top),Some(context))=(&origin,&top,browser.host().and_then(|h|h.request_context())) {
                if origin==top {
                    let origin=CefString::from(origin.as_str());
                    let values:Vec<_>=kinds.iter().map(|k|context.content_setting(Some(&origin),Some(&origin),k.cef_type())).collect();
                    if values.iter().any(|v|*v==ContentSettingValues::BLOCK) {callback.cont(0);return 1;}
                    if values.iter().all(|v|*v==ContentSettingValues::ALLOW) {callback.cont(requested_permissions);return 1;}
                }
            }
            if !enqueue(browser,frame.as_deref(),requesting_origin,kinds,None,Continuation::Media(callback.clone(),requested_permissions),&self.events) {callback.cont(0);}
            1
        }
        fn on_show_permission_prompt(&self,browser:Option<&mut Browser>,prompt_id:u64,requesting_origin:Option<&CefString>,requested_permissions:u32,callback:Option<&mut PermissionPromptCallback>)->i32 {
            let Some(callback)=callback else{return 1;};
            let (Some(browser),Some(kinds))=(browser,prompt_kinds(requested_permissions)) else {callback.cont(PermissionRequestResult::IGNORE);return 1;};
            if !enqueue(browser,None,requesting_origin,kinds,Some(prompt_id),Continuation::Prompt(callback.clone()),&self.events) {callback.cont(PermissionRequestResult::IGNORE);}
            1
        }
        fn on_dismiss_permission_prompt(&self,browser:Option<&mut Browser>,prompt_id:u64,_result:PermissionRequestResult) {
            let Some(browser)=browser else{return;};
            let removed=REQUESTS.with_borrow_mut(|s| {
                let ids:Vec<_>=s.entries.iter().filter_map(|(&id,p)|(p.scope.browser==browser.identifier()&&p.scope.cef_prompt==Some(prompt_id)).then_some(id)).collect();
                ids.into_iter().filter_map(|id|s.entries.remove(&id)).collect::<Vec<_>>()
            });
            for p in &removed {self.events.push((browser.identifier(),RawEvent::PermissionDismissed(p.request.id)));}
            drop(removed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope(browser: i32) -> Scope {
        Scope {
            browser,
            epoch: 7,
            frame: Some("main".into()),
            cef_prompt: Some(10),
        }
    }
    fn insert(
        store: &mut Requests<&'static str>,
        browser: i32,
        payload: &'static str,
    ) -> PermissionRequest {
        store
            .insert(
                "https://site.example".into(),
                "https://site.example".into(),
                vec![PermissionKind::Camera],
                scope(browser),
                payload,
            )
            .unwrap()
    }
    #[test]
    fn device_access_never_grants_desktop_capture_or_unknown_permission_bits() {
        let camera = MediaAccessPermissionTypes::DEVICE_VIDEO_CAPTURE.get_raw();
        let mic = MediaAccessPermissionTypes::DEVICE_AUDIO_CAPTURE.get_raw();
        assert_eq!(
            media_kinds(camera | mic),
            Some(vec![PermissionKind::Camera, PermissionKind::Microphone])
        );
        for mask in [
            0,
            1 << 31,
            camera | MediaAccessPermissionTypes::DESKTOP_VIDEO_CAPTURE.get_raw(),
        ] {
            assert!(media_kinds(mask).is_none());
        }
        assert_eq!(
            prompt_kinds(PermissionRequestTypes::NOTIFICATIONS.get_raw()),
            Some(vec![PermissionKind::Notifications])
        );
        assert_eq!(
            prompt_kinds(PermissionRequestTypes::GEOLOCATION.get_raw()),
            Some(vec![PermissionKind::Location])
        );
    }
    #[test]
    fn decisions_are_bound_to_the_request_and_browser_and_are_consumed_once() {
        let mut store = Requests::default();
        let request = insert(&mut store, 3, "first callback");
        assert!(!store.answer(4, request.id, PermissionDecision::Allow));
        assert!(!store.answer(3, PermissionRequestId(99), PermissionDecision::Allow));
        assert!(store.answer(3, request.id, PermissionDecision::Block));
        assert!(!store.answer(3, request.id, PermissionDecision::Allow));
        let ready = store.ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].payload, "first callback");
        assert_eq!(ready[0].decision, Some(PermissionDecision::Block));
        assert!(store.ready().is_empty());
    }
    #[test]
    fn navigation_or_transfer_cancels_even_an_already_queued_allow() {
        let mut store = Requests::default();
        let first = insert(&mut store, 3, "old callback");
        let other = insert(&mut store, 4, "other browser");
        assert!(store.answer(3, first.id, PermissionDecision::Allow));
        assert_eq!(store.cancel(|s| s.browser == 3), vec![(3, first.id)]);
        let replacement = insert(&mut store, 3, "replacement");
        assert_ne!(first.id, replacement.id);
        assert!(!store.answer(3, first.id, PermissionDecision::Allow));
        let ready = store.ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].decision, Some(PermissionDecision::Dismiss));
        assert!(store.entries.contains_key(&other.id));
        assert!(store.entries.contains_key(&replacement.id));
    }
    #[test]
    fn a_taken_decision_still_requires_the_same_document_and_requesting_frame() {
        let mut store = Requests::default();
        let request = insert(&mut store, 3, "callback");
        let scope = &store.entries[&request.id].scope;
        assert!(scope.matches(
            3,
            Some(7),
            Some("https://site.example"),
            &request,
            Some("https://site.example")
        ));
        assert!(!scope.matches(
            3,
            Some(8),
            Some("https://site.example"),
            &request,
            Some("https://site.example")
        ));
        assert!(!scope.matches(
            3,
            Some(7),
            Some("https://other.example"),
            &request,
            Some("https://site.example")
        ));
        assert!(!scope.matches(3, Some(7), Some("https://site.example"), &request, None));
        assert!(!scope.matches(
            4,
            Some(7),
            Some("https://site.example"),
            &request,
            Some("https://site.example")
        ));
    }
    #[test]
    fn token_exhaustion_does_not_replace_a_live_callback() {
        let mut store = Requests::default();
        let request = insert(&mut store, 3, "live");
        store.next = u64::MAX;
        assert!(store
            .insert(
                "https://site.example".into(),
                "https://site.example".into(),
                vec![PermissionKind::Location],
                scope(3),
                "overflow"
            )
            .is_none());
        assert_eq!(store.entries.len(), 1);
        assert!(store.entries.contains_key(&request.id));
    }
}
