//! Local DevTools transport and bundled-frontend loading probe.
//!
//! This deliberately does not claim to install an InspectorFrontendHost. The
//! parent must first prove that the version-matched bundled URL is available
//! in an ordinary Alloy content browser. No network frontend or native DevTools
//! window is substituted if that probe fails.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cef::*;
use serde_json::Value;

use crate::ViewId;

pub const INSPECTOR_FRONTEND_URL: &str = "devtools://devtools/bundled/inspector.html";

/// Separate from tab/page identity. Never add this to browser sessions/history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InspectorId(pub(crate) u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorPanel {
    Elements,
    Console,
    Network,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorPhase {
    Opening,
    /// The expected URL stopped loading. This is not proof the app booted.
    FrontendLoaded,
    /// Reserved for a verified renderer host handshake, not a load callback.
    FrontendReady,
    LoadFailed,
    Detached,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorStatus {
    pub id: InspectorId,
    pub inspected: ViewId,
    pub generation: u64,
    pub panel: InspectorPanel,
    pub phase: InspectorPhase,
    pub url: String,
}

/// Probe output only; feed protocol bytes into InspectorFrontendAPI.dispatchMessage
/// after a version-matched host adapter has been installed, never into page JS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InspectorEvent {
    Status(InspectorStatus),
    Protocol {
        inspector: InspectorId,
        message: String,
    },
    Detached {
        inspector: InspectorId,
    },
    LoadFailed {
        inspector: InspectorId,
        url: String,
        code: i32,
        text: String,
    },
}

/// All clients on a CEF agent share this monotonically increasing namespace.
/// IDs are never reused after timeout, detach, close or failed submission.
#[derive(Clone, Default)]
pub(crate) struct MessageIds(Arc<Mutex<i32>>);

impl MessageIds {
    pub(crate) fn next(&self) -> Option<i32> {
        let mut last = self.0.lock().unwrap();
        *last = last.checked_add(1)?;
        Some(*last)
    }
}

#[derive(Default)]
struct Protocol {
    pending: HashMap<i32, (Value, Option<Value>)>,
}

impl Protocol {
    fn submit(&mut self, ids: &MessageIds, message: &str) -> Option<(i32, Vec<u8>)> {
        let mut value: Value = serde_json::from_str(message).ok()?;
        let object = value.as_object_mut()?;
        let original = object.get("id")?.as_u64()?;
        if original > 9_007_199_254_740_991 || object.get("method")?.as_str()?.is_empty() {
            return None;
        }
        let session = object.get("sessionId").cloned();
        if session
            .as_ref()
            .is_some_and(|value| value.as_str().is_none())
        {
            return None;
        }
        // Bound unacknowledged requests and reject ambiguous duplicate IDs.
        if self.pending.len() >= 8192
            || self
                .pending
                .values()
                .any(|(id, child)| id == &Value::from(original) && child == &session)
        {
            return None;
        }
        let id = ids.next()?;
        object.insert("id".into(), id.into());
        self.pending.insert(id, (original.into(), session));
        Some((id, serde_json::to_vec(&value).ok()?))
    }

    fn receive(&mut self, bytes: &[u8]) -> Option<String> {
        let mut value: Value = serde_json::from_slice(bytes).ok()?;
        let object = value.as_object_mut()?;
        if let Some(id) = object.get("id") {
            let id = i32::try_from(id.as_i64()?).ok()?;
            let (original, session) = self.pending.get(&id)?;
            // Chromium reports a vanished child session as an error without
            // sessionId. The globally unique wire ID still identifies its
            // request. Explicitly wrong sessions and unscoped success replies
            // must continue to fail correlation.
            let unscoped_error = !object.contains_key("sessionId")
                && object.get("error").is_some_and(Value::is_object);
            if object.get("sessionId") != session.as_ref() && !unscoped_error {
                return None;
            }
            let original = original.clone();
            let session = session.clone();
            self.pending.remove(&id);
            object.insert("id".into(), original);
            if let Some(session) = session {
                object.insert("sessionId".into(), session);
            }
        } else if object.get("method")?.as_str()?.is_empty() {
            return None;
        }
        serde_json::to_string(&value).ok()
    }
}

pub(crate) enum Backend {
    Message(Vec<u8>),
    Detached,
}
pub(crate) type Queue = Arc<Mutex<Vec<(InspectorId, Backend)>>>;

pub(crate) struct NativeBinding {
    // Retain the actual inspected context for the whole inspector lifetime.
    pub(crate) _context: RequestContext,
    pub(crate) _registration: Registration,
}

pub(crate) struct Session<Binding> {
    pub(crate) status: InspectorStatus,
    pub(crate) frontend: ViewId,
    pub(crate) _binding: Binding,
    protocol: Protocol,
}

pub(crate) struct Inspectors<Binding = NativeBinding> {
    pub(crate) sessions: HashMap<InspectorId, Session<Binding>>,
    pub(crate) queue: Queue,
    pub(crate) ready: Vec<InspectorEvent>,
    next: u64,
}

impl<Binding> Default for Inspectors<Binding> {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            queue: Queue::default(),
            ready: Vec::new(),
            next: 0,
        }
    }
}

impl<Binding> Inspectors<Binding> {
    pub(crate) fn allocate(&mut self) -> Option<InspectorId> {
        self.next = self.next.checked_add(1)?;
        Some(InspectorId(self.next))
    }

    pub(crate) fn existing(
        &mut self,
        inspected: ViewId,
        panel: InspectorPanel,
    ) -> Option<InspectorId> {
        let session = self
            .sessions
            .values_mut()
            .find(|session| session.status.inspected == inspected)?;
        session.status.panel = panel;
        self.ready
            .push(InspectorEvent::Status(session.status.clone()));
        Some(session.status.id)
    }

    pub(crate) fn insert(
        &mut self,
        id: InspectorId,
        inspected: ViewId,
        frontend: ViewId,
        panel: InspectorPanel,
        binding: Binding,
    ) {
        let status = InspectorStatus {
            id,
            inspected,
            generation: id.0,
            panel,
            phase: InspectorPhase::Opening,
            url: String::new(),
        };
        self.ready.push(InspectorEvent::Status(status.clone()));
        self.sessions.insert(
            id,
            Session {
                status,
                frontend,
                _binding: binding,
                protocol: Protocol::default(),
            },
        );
    }

    pub(crate) fn loading(&mut self, id: InspectorId, url: String, loading: bool) {
        let Some(session) = self.sessions.get_mut(&id) else {
            return;
        };
        session.status.phase = if matches!(
            session.status.phase,
            InspectorPhase::Detached | InspectorPhase::LoadFailed
        ) {
            session.status.phase
        } else if !loading && expected_frontend(&url) {
            InspectorPhase::FrontendLoaded
        } else {
            InspectorPhase::Opening
        };
        session.status.url = url;
        self.ready
            .push(InspectorEvent::Status(session.status.clone()));
    }

    pub(crate) fn load_failed(&mut self, id: InspectorId, url: String, code: i32, text: String) {
        let Some(session) = self.sessions.get_mut(&id) else {
            return;
        };
        session.status.phase = InspectorPhase::LoadFailed;
        session.status.url = url.clone();
        self.ready.push(InspectorEvent::LoadFailed {
            inspector: id,
            url,
            code,
            text,
        });
    }

    pub(crate) fn submit(
        &mut self,
        id: InspectorId,
        ids: &MessageIds,
        message: &str,
    ) -> Option<(i32, Vec<u8>)> {
        if message.len() > 16 * 1024 * 1024 {
            return None;
        }
        let session = self.sessions.get_mut(&id)?;
        if session.status.phase == InspectorPhase::Detached {
            return None;
        }
        session.protocol.submit(ids, message)
    }

    pub(crate) fn failed(&mut self, id: InspectorId, message_id: i32) {
        if let Some(session) = self.sessions.get_mut(&id) {
            session.protocol.pending.remove(&message_id);
        }
    }

    pub(crate) fn take(&mut self) -> Vec<InspectorEvent> {
        let messages = std::mem::take(&mut *self.queue.lock().unwrap());
        for (id, backend) in messages {
            let Some(session) = self.sessions.get_mut(&id) else {
                continue;
            };
            if session.status.phase == InspectorPhase::Detached {
                continue;
            }
            match backend {
                Backend::Message(bytes) => {
                    if let Some(message) = session.protocol.receive(&bytes) {
                        self.ready.push(InspectorEvent::Protocol {
                            inspector: id,
                            message,
                        });
                    }
                }
                Backend::Detached => {
                    session.status.phase = InspectorPhase::Detached;
                    session.protocol.pending.clear();
                    self.ready.push(InspectorEvent::Detached { inspector: id });
                }
            }
        }
        std::mem::take(&mut self.ready)
    }

    pub(crate) fn close(&mut self, id: InspectorId) -> Option<ViewId> {
        let session = self.sessions.remove(&id)?;
        self.ready.retain(|event| match event {
            InspectorEvent::Status(status) => status.id != id,
            InspectorEvent::Protocol { inspector, .. }
            | InspectorEvent::Detached { inspector }
            | InspectorEvent::LoadFailed { inspector, .. } => *inspector != id,
        });
        self.queue
            .lock()
            .unwrap()
            .retain(|(inspector, _)| *inspector != id);
        Some(session.frontend)
    }
}

/// Exact URL only: query strings, lookalike hosts and subframes must not acquire
/// frontend privileges. Future renderer IPC must ALSO check the browser role
/// and main-frame identity; URL alone is never authorization.
pub(crate) fn expected_frontend(url: &str) -> bool {
    url == INSPECTOR_FRONTEND_URL
}

wrap_dev_tools_message_observer! {
    pub(crate) struct InspectorObserver { queue: Queue, inspector: InspectorId }
    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            if let Some(message) = message {
                self.queue.lock().unwrap().push((self.inspector, Backend::Message(message.to_vec())));
            }
            // Do not consume: screenshot observers still need method callbacks.
            0
        }
        fn on_dev_tools_agent_detached(&self, _browser: Option<&mut Browser>) {
            self.queue.lock().unwrap().push((self.inspector, Backend::Detached));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert<B>(state: &mut Inspectors<B>, page: u64, binding: B) -> InspectorId {
        let id = state.allocate().unwrap();
        state.insert(
            id,
            ViewId::from_raw(page),
            ViewId::from_raw(page + 100),
            InspectorPanel::Elements,
            binding,
        );
        id
    }

    #[test]
    fn lifecycle_coalesces_panel_requests_and_close_drops_only_frontend_binding() {
        let mut state = Inspectors::default();
        let binding = std::rc::Rc::new("private context and observer lifetime");
        let id = insert(&mut state, 1, binding.clone());
        assert_eq!(std::rc::Rc::strong_count(&binding), 2);
        assert_eq!(
            state.existing(ViewId::from_raw(1), InspectorPanel::Console),
            Some(id)
        );
        assert_eq!(
            state.existing(ViewId::from_raw(1), InspectorPanel::Network),
            Some(id)
        );
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[&id].status.panel, InspectorPanel::Network);
        state.loading(id, INSPECTOR_FRONTEND_URL.into(), false);
        assert_eq!(
            state.sessions[&id].status.phase,
            InspectorPhase::FrontendLoaded
        );
        assert_ne!(
            state.sessions[&id].status.phase,
            InspectorPhase::FrontendReady
        );
        assert_eq!(state.close(id), Some(ViewId::from_raw(101)));
        assert_eq!(std::rc::Rc::strong_count(&binding), 1);
        assert!(state.take().is_empty());
        assert_eq!(state.close(id), None);
        // In-flight callbacks may arrive after dropping the registration.
        state.queue.lock().unwrap().push((id, Backend::Detached));
        let reopened = insert(&mut state, 1, binding.clone());
        assert_ne!(id, reopened);
        assert!(
            state.take().iter().all(
                |event| matches!(event, InspectorEvent::Status(status) if status.id == reopened)
            )
        );
        state.load_failed(
            reopened,
            INSPECTOR_FRONTEND_URL.into(),
            -302,
            "Unknown scheme".into(),
        );
        state.loading(reopened, INSPECTOR_FRONTEND_URL.into(), false);
        assert_eq!(
            state.sessions[&reopened].status.phase,
            InspectorPhase::LoadFailed
        );
        assert!(
            state
                .take()
                .iter()
                .any(|event| matches!(event, InspectorEvent::LoadFailed { code: -302, .. }))
        );
    }

    #[test]
    fn regular_and_private_bindings_and_protocol_queues_stay_isolated() {
        // The state machine treats the native binding as opaque. Production
        // supplies the inspected RequestContext plus Registration; tests use
        // distinct retained identities without initializing native CEF.
        let regular_context = std::rc::Rc::new("regular");
        let private_context = std::rc::Rc::new("private");
        let mut state = Inspectors::default();
        let regular = insert(&mut state, 1, regular_context.clone());
        let private = insert(&mut state, 2, private_context.clone());
        assert!(std::rc::Rc::ptr_eq(
            &state.sessions[&private]._binding,
            &private_context
        ));
        assert!(!std::rc::Rc::ptr_eq(
            &state.sessions[&private]._binding,
            &regular_context
        ));
        state.take();
        let ids = MessageIds::default();
        let (regular_id, _) = state
            .submit(regular, &ids, r#"{"id":1,"method":"Runtime.evaluate"}"#)
            .unwrap();
        let (private_id, _) = state
            .submit(private, &ids, r#"{"id":1,"method":"Runtime.evaluate"}"#)
            .unwrap();
        let private_reply =
            format!(r#"{{"id":{private_id},"result":{{"private":"secret"}}}}"#).into_bytes();
        state
            .queue
            .lock()
            .unwrap()
            .push((regular, Backend::Message(private_reply.clone())));
        assert!(state.take().is_empty());
        state
            .queue
            .lock()
            .unwrap()
            .push((private, Backend::Message(private_reply)));
        assert!(
            matches!(state.take().as_slice(), [InspectorEvent::Protocol { inspector, message }] if *inspector == private && message.contains("secret"))
        );
        state.failed(regular, regular_id);
        state.queue.lock().unwrap().push((
            regular,
            Backend::Message(format!(r#"{{"id":{regular_id},"result":{{}}}}"#).into_bytes()),
        ));
        assert!(state.take().is_empty());
        state.close(private);
        assert_eq!(std::rc::Rc::strong_count(&private_context), 1);
        assert_eq!(std::rc::Rc::strong_count(&regular_context), 2);
        assert!(state.sessions.contains_key(&regular));
    }

    #[test]
    fn detach_clears_pending_work_and_rejects_late_messages() {
        let mut state = Inspectors::default();
        let id = insert(&mut state, 1, ());
        state.take();
        let ids = MessageIds::default();
        state
            .submit(id, &ids, r#"{"id":1,"method":"Runtime.enable"}"#)
            .unwrap();
        state.queue.lock().unwrap().extend([
            (id, Backend::Detached),
            (
                id,
                Backend::Message(br#"{"method":"Runtime.consoleAPICalled"}"#.to_vec()),
            ),
            (id, Backend::Detached),
        ]);
        assert_eq!(
            state.take(),
            vec![InspectorEvent::Detached { inspector: id }]
        );
        assert!(state.sessions[&id].protocol.pending.is_empty());
        assert!(
            state
                .submit(id, &ids, r#"{"id":2,"method":"Runtime.enable"}"#)
                .is_none()
        );
        state.loading(id, INSPECTOR_FRONTEND_URL.into(), false);
        assert_eq!(state.sessions[&id].status.phase, InspectorPhase::Detached);
    }

    #[test]
    fn shared_ids_isolate_snapshot_replies_and_preserve_child_sessions() {
        let ids = MessageIds::default();
        let snapshot = ids.next().unwrap();
        let mut protocol = Protocol::default();
        let (wire, bytes) = protocol
            .submit(
                &ids,
                r#"{"id":1,"method":"Runtime.enable","sessionId":"child","params":{}}"#,
            )
            .unwrap();
        assert_ne!(wire, snapshot);
        let sent: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(sent["sessionId"], "child");
        assert_eq!(sent["id"], wire);
        assert!(
            protocol
                .receive(format!(r#"{{"id":{snapshot},"result":{{}}}}"#).as_bytes())
                .is_none()
        );
        assert!(
            protocol
                .receive(format!(r#"{{"id":{wire},"result":{{}}}}"#).as_bytes())
                .is_none()
        );
        let response = protocol
            .receive(format!(r#"{{"id":{wire},"sessionId":"child","result":{{}}}}"#).as_bytes())
            .unwrap();
        let response: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["id"], 1);
        assert_eq!(response["sessionId"], "child");
        assert!(protocol.pending.is_empty());
        let event =
            br#"{"method":"Runtime.consoleAPICalled","sessionId":"child","params":{"type":"log"}}"#;
        assert_eq!(
            serde_json::from_str::<Value>(&protocol.receive(event).unwrap()).unwrap(),
            serde_json::from_slice::<Value>(event).unwrap()
        );
    }

    #[test]
    fn missing_child_session_error_restores_routing_and_releases_pending_request() {
        let ids = MessageIds::default();
        let mut protocol = Protocol::default();
        let (gone, _) = protocol
            .submit(
                &ids,
                r#"{"id":7,"method":"Runtime.enable","sessionId":"gone"}"#,
            )
            .unwrap();
        let (live, _) = protocol
            .submit(
                &ids,
                r#"{"id":7,"method":"Runtime.enable","sessionId":"live"}"#,
            )
            .unwrap();

        // Only the sessionless error exception may correlate by wire ID.
        for invalid in [
            format!(r#"{{"id":{gone},"result":{{}}}}"#),
            format!(r#"{{"id":{gone},"sessionId":"live","error":{{"code":-32001}}}}"#),
            format!(r#"{{"id":{gone},"error":null}}"#),
        ] {
            assert!(protocol.receive(invalid.as_bytes()).is_none());
        }
        assert_eq!(protocol.pending.len(), 2);
        let error = format!(
            r#"{{"id":{gone},"error":{{"code":-32001,"message":"Session with given id not found."}}}}"#
        );
        let reply: Value =
            serde_json::from_str(&protocol.receive(error.as_bytes()).unwrap()).unwrap();
        assert_eq!(
            reply,
            serde_json::json!({
                "id": 7, "sessionId": "gone",
                "error": { "code": -32001, "message": "Session with given id not found." }
            })
        );
        assert!(!protocol.pending.contains_key(&gone));
        assert!(protocol.pending.contains_key(&live));
        assert!(protocol.receive(error.as_bytes()).is_none());
        // Reusing the frontend ID is now accepted; no pending slot leaked.
        assert!(
            protocol
                .submit(
                    &ids,
                    r#"{"id":7,"method":"Runtime.enable","sessionId":"gone"}"#
                )
                .is_some()
        );
        let reply = format!(r#"{{"id":{live},"sessionId":"live","result":{{}}}}"#);
        assert!(protocol.receive(reply.as_bytes()).is_some());
    }

    #[test]
    fn duplicate_ids_errors_malformed_requests_and_exhaustion() {
        let ids = MessageIds::default();
        let mut protocol = Protocol::default();
        let (id, _) = protocol
            .submit(&ids, r#"{"id":5,"method":"Missing.method"}"#)
            .unwrap();
        assert!(
            protocol
                .submit(&ids, r#"{"id":5,"method":"Runtime.enable"}"#)
                .is_none()
        );
        assert!(
            protocol
                .submit(
                    &ids,
                    r#"{"id":5,"method":"Runtime.enable","sessionId":"other"}"#
                )
                .is_some()
        );
        let error = format!(r#"{{"id":{id},"error":{{"code":-32601,"message":"unknown"}}}}"#);
        assert!(
            protocol
                .receive(error.as_bytes())
                .unwrap()
                .contains("-32601")
        );
        assert!(protocol.receive(error.as_bytes()).is_none());
        for bad in [
            "null",
            "[]",
            "{}",
            r#"{"id":-1,"method":"x"}"#,
            r#"{"id":1,"method":""}"#,
            r#"{"id":1,"method":"x","sessionId":1}"#,
        ] {
            assert!(protocol.submit(&ids, bad).is_none(), "{bad}");
        }
        *ids.0.lock().unwrap() = i32::MAX;
        assert!(ids.next().is_none());
        assert!(protocol.submit(&ids, r#"{"id":8,"method":"x"}"#).is_none());
    }

    #[test]
    fn new_session_does_not_accept_old_replies_and_probe_url_is_strict() {
        let ids = MessageIds::default();
        let mut old = Protocol::default();
        let (old_id, _) = old
            .submit(&ids, r#"{"id":1,"method":"Runtime.enable"}"#)
            .unwrap();
        drop(old);
        let mut new = Protocol::default();
        let (new_id, _) = new
            .submit(&ids, r#"{"id":1,"method":"Runtime.enable"}"#)
            .unwrap();
        assert_ne!(old_id, new_id);
        assert!(
            new.receive(format!(r#"{{"id":{old_id},"result":{{}}}}"#).as_bytes())
                .is_none()
        );
        assert!(expected_frontend(INSPECTOR_FRONTEND_URL));
        for url in [
            "https://devtools/bundled/inspector.html",
            "devtools://devtools/bundled/inspector.html?ws=evil",
            "devtools://devtools.evil/bundled/inspector.html",
            "about:blank",
        ] {
            assert!(!expected_frontend(url));
        }
    }
}
