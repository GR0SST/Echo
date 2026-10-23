//! Authenticated local DevTools frontend transport.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cef::*;
use serde_json::Value;

use crate::{Shortcut, ViewId};

pub const INSPECTOR_FRONTEND_URL: &str = "devtools://devtools/bundled/devtools_app.html";

/// Separate from tab/page identity. Never add this to browser sessions/history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InspectorId(pub(crate) u64);

impl InspectorId {
    /// For alternative engines, e.g. test fakes. Native operations still
    /// validate the identifier against their live inspector sessions.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

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
    /// Observed supported main panel, never inferred from a request.
    pub selected_panel: Option<InspectorPanel>,
    pub phase: InspectorPhase,
    pub url: String,
}

/// Inspector-only lifecycle/input events. Protocol is reserved for trusted
/// native diagnostics; renderer connection traffic is dispatched internally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InspectorEvent {
    /// The original frontend requested closure; the parent owns layout/close.
    CloseRequested {
        inspector: InspectorId,
    },
    /// A supported main-tab selection reported by the original frontend.
    /// None invalidates a previous observation (unsupported/ambiguous panel).
    PanelChanged {
        inspector: InspectorId,
        panel: Option<InspectorPanel>,
    },
    Focused {
        inspector: InspectorId,
    },
    Shortcut {
        inspector: InspectorId,
        shortcut: Shortcut,
    },
    FocusRequested {
        inspector: InspectorId,
    },
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
pub(crate) struct Protocol {
    pub(crate) pending: HashMap<i32, (Value, Option<Value>)>,
}

impl Protocol {
    pub(crate) fn submit(&mut self, ids: &MessageIds, message: &str) -> Option<(i32, Vec<u8>)> {
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
        if bytes.len() > MAX_MESSAGE {
            return None;
        }
        let mut value: Value = serde_json::from_slice(bytes).ok()?;
        let object = value.as_object_mut()?;
        if object.get("sessionId").is_some_and(|v| !v.is_string())
            || object.get("params").is_some_and(|v| !v.is_object())
        {
            return None;
        }
        if let Some(id) = object.get("id") {
            if object.get("result").is_some_and(Value::is_object)
                == object.get("error").is_some_and(Value::is_object)
            {
                return None;
            }
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
    Frontend(BridgeMessage),
    Revoke,
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
    pub(crate) renderer_protocol: Protocol,
    pub(crate) bridge: BridgeState,
}

pub(crate) struct Inspectors<Binding = NativeBinding> {
    pub(crate) sessions: HashMap<InspectorId, Session<Binding>>,
    pub(crate) queue: Queue,
    pub(crate) ready: Vec<InspectorEvent>,
    next: u64,
    pub(crate) actions: Vec<(InspectorId, u64, BridgeAction)>,
}

impl<Binding> Default for Inspectors<Binding> {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            queue: Queue::default(),
            ready: Vec::new(),
            next: 0,
            actions: Vec::new(),
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
            selected_panel: None,
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
                renderer_protocol: Protocol::default(),
                bridge: BridgeState::default(),
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
        } else if session.status.phase == InspectorPhase::FrontendReady && expected_frontend(&url) {
            InspectorPhase::FrontendReady
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
        if session.bridge.active {
            self.actions.push((
                id,
                session.bridge.epoch,
                BridgeAction::Deliver("revoke", String::new()),
            ));
        }
        session.bridge.revoke();
        session.renderer_protocol.pending.clear();
        session.status.selected_panel = None;
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
            if session.status.phase == InspectorPhase::LoadFailed
                && matches!(&backend, Backend::Frontend(_) | Backend::Revoke)
            {
                continue;
            }
            match backend {
                Backend::Frontend(input) => {
                    if input.generation != id.0 {
                        continue;
                    }
                    let epoch = input.epoch;
                    let Some(action) = session.bridge.accept(&input) else {
                        continue;
                    };
                    if input.kind == "context" || input.kind == "release" {
                        session.renderer_protocol.pending.clear();
                        let previous = session.status.clone();
                        session.status.phase = InspectorPhase::Opening;
                        session.status.selected_panel = None;
                        if previous != session.status {
                            self.ready
                                .push(InspectorEvent::Status(session.status.clone()));
                        }
                    }
                    if input.kind == "selected" {
                        let selected = match input.payload.as_str() {
                            "elements" => Some(InspectorPanel::Elements),
                            "console" => Some(InspectorPanel::Console),
                            "network" => Some(InspectorPanel::Network),
                            _ => None,
                        };
                        if session.status.selected_panel != selected {
                            session.status.selected_panel = selected;
                            self.ready.push(InspectorEvent::PanelChanged {
                                inspector: id,
                                panel: selected,
                            });
                            self.ready
                                .push(InspectorEvent::Status(session.status.clone()));
                        }
                    }
                    if input.kind == "focus" {
                        self.ready
                            .push(InspectorEvent::FocusRequested { inspector: id });
                    }
                    if input.kind == "close" {
                        self.ready
                            .push(InspectorEvent::CloseRequested { inspector: id });
                    }
                    if let Some(action) = action {
                        self.actions.push((id, epoch, action));
                    }
                    if session.bridge.is_ready()
                        && session.status.phase != InspectorPhase::FrontendReady
                    {
                        session.status.phase = InspectorPhase::FrontendReady;
                        self.ready
                            .push(InspectorEvent::Status(session.status.clone()));
                        self.actions.push((
                            id,
                            epoch,
                            BridgeAction::Deliver("panel", panel_name(session.status.panel).into()),
                        ));
                        if std::mem::take(&mut session.bridge.pending_inspect) {
                            self.actions.push((
                                id,
                                epoch,
                                BridgeAction::Deliver("inspect", String::new()),
                            ));
                        }
                    }
                }
                Backend::Revoke => {
                    let initial_pending =
                        session.bridge.epoch == 0 && session.bridge.pending_inspect;
                    session.bridge.revoke();
                    session.bridge.pending_inspect = initial_pending;
                    let previous = session.status.clone();
                    session.status.selected_panel = None;
                    session.renderer_protocol.pending.clear();
                    session.status.phase = InspectorPhase::Opening;
                    if previous != session.status {
                        self.ready
                            .push(InspectorEvent::Status(session.status.clone()));
                    }
                }
                Backend::Message(bytes) => {
                    if bytes.len() > MAX_MESSAGE {
                        continue;
                    }
                    if let Some(message) = session.renderer_protocol.receive(&bytes) {
                        if session.bridge.active {
                            let reply: Value = serde_json::from_str(&message).unwrap();
                            if reply.get("id").is_some()
                                && reply.get("result").is_some_and(Value::is_object)
                            {
                                if session.bridge.proof.is_none() && !session.bridge.roundtrip {
                                    session.bridge.proof = Some(
                                        serde_json::to_string(&serde_json::json!([
                                            reply["id"],
                                            reply.get("sessionId").unwrap_or(&Value::Null)
                                        ]))
                                        .unwrap(),
                                    );
                                }
                            }
                            self.actions.push((
                                id,
                                session.bridge.epoch,
                                BridgeAction::Deliver("protocol", message),
                            ));
                            if session.bridge.is_ready()
                                && session.status.phase != InspectorPhase::FrontendReady
                            {
                                session.status.phase = InspectorPhase::FrontendReady;
                                self.ready
                                    .push(InspectorEvent::Status(session.status.clone()));
                                self.actions.push((
                                    id,
                                    session.bridge.epoch,
                                    BridgeAction::Deliver(
                                        "panel",
                                        panel_name(session.status.panel).into(),
                                    ),
                                ));
                                if std::mem::take(&mut session.bridge.pending_inspect) {
                                    self.actions.push((
                                        id,
                                        session.bridge.epoch,
                                        BridgeAction::Deliver("inspect", String::new()),
                                    ));
                                }
                            }
                        }
                    }
                    if let Some(message) = session.protocol.receive(&bytes) {
                        self.ready.push(InspectorEvent::Protocol {
                            inspector: id,
                            message,
                        });
                    }
                }
                Backend::Detached => {
                    session.status.phase = InspectorPhase::Detached;
                    session.status.selected_panel = None;
                    session.protocol.pending.clear();
                    session.renderer_protocol.pending.clear();
                    if session.bridge.active {
                        self.actions.push((
                            id,
                            session.bridge.epoch,
                            BridgeAction::Deliver("revoke", String::new()),
                        ));
                    }
                    session.bridge.revoke();
                    self.ready.push(InspectorEvent::Detached { inspector: id });
                }
            }
        }
        std::mem::take(&mut self.ready)
    }

    pub(crate) fn close(&mut self, id: InspectorId) -> Option<ViewId> {
        let session = self.sessions.remove(&id)?;
        self.actions.retain(|(inspector, _, _)| *inspector != id);
        self.ready.retain(|event| match event {
            InspectorEvent::Status(status) => status.id != id,
            InspectorEvent::Protocol { inspector, .. }
            | InspectorEvent::Detached { inspector }
            | InspectorEvent::LoadFailed { inspector, .. }
            | InspectorEvent::CloseRequested { inspector }
            | InspectorEvent::PanelChanged { inspector, .. }
            | InspectorEvent::Shortcut { inspector, .. }
            | InspectorEvent::Focused { inspector }
            | InspectorEvent::FocusRequested { inspector } => *inspector != id,
        });
        self.queue
            .lock()
            .unwrap()
            .retain(|(inspector, _)| *inspector != id);
        Some(session.frontend)
    }
}

/// Exact URL is only one capability check. Both IPC directions also require
/// native role, generation, main-frame identity and live document epoch.
pub(crate) fn expected_frontend(url: &str) -> bool {
    url == INSPECTOR_FRONTEND_URL
}

wrap_dev_tools_message_observer! {
    pub(crate) struct InspectorObserver { queue: Queue, inspector: InspectorId }
    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            if let Some(message) = message {
                if message.len() <= MAX_MESSAGE { enqueue(&self.queue, self.inspector, Backend::Message(message.to_vec())); } else { enqueue(&self.queue, self.inspector, Backend::Detached); }
            }
            // Do not consume: screenshot observers still need method callbacks.
            0
        }
        fn on_dev_tools_agent_detached(&self, _browser: Option<&mut Browser>) {
            enqueue(&self.queue, self.inspector, Backend::Detached);
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
    fn shared_context_pools_keep_inspectors_and_protocol_callbacks_independent() {
        // A shared native context is opaque to this state machine. Closing the
        // source pool must not detach the fork's same-numbered inspector.
        let context = std::rc::Rc::new("shared request context");
        let mut source = Inspectors::default();
        let mut fork = Inspectors::default();
        let source_id = insert(&mut source, 1, context.clone());
        let fork_id = insert(&mut fork, 1, context.clone());
        assert_eq!(source_id, fork_id);
        source.take();
        fork.take();
        let source_ids = MessageIds::default();
        let fork_ids = MessageIds::default();
        let request = r#"{"id":7,"method":"Runtime.enable"}"#;
        let (source_wire, _) = source.submit(source_id, &source_ids, request).unwrap();
        let (fork_wire, _) = fork.submit(fork_id, &fork_ids, request).unwrap();
        assert_eq!(source_wire, fork_wire);
        let source_callbacks = source.queue.clone();
        source.close(source_id);
        drop(source);
        source_callbacks.lock().unwrap().push((source_id, Backend::Detached));
        assert!(fork.take().is_empty());
        assert_eq!(fork.sessions[&fork_id].status.phase, InspectorPhase::Opening);
        assert!(std::rc::Rc::ptr_eq(&fork.sessions[&fork_id]._binding, &context));
        fork.queue.lock().unwrap().push((fork_id, Backend::Message(
            format!(r#"{{"id":{fork_wire},"result":{{}}}}"#).into_bytes(),
        )));
        let events = fork.take();
        assert_eq!(events, vec![InspectorEvent::Protocol {
            inspector: fork_id,
            message: r#"{"id":7,"result":{}}"#.into(),
        }]);
        assert!(fork.sessions[&fork_id].protocol.pending.is_empty());
        fork.close(fork_id);
        assert_eq!(std::rc::Rc::strong_count(&context), 1);
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
            "devtools://devtools/bundled/inspector.html",
            "devtools://devtools/bundled/devtools_app.html?can_dock=true",
            "devtools://devtools/bundled/devtools_app.html?ws=evil",
            "devtools://devtools/bundled/devtools_app.html#elements",
            "https://devtools/bundled/inspector.html",
            "devtools://devtools/bundled/inspector.html?ws=evil",
            "devtools://devtools.evil/bundled/inspector.html",
            "about:blank",
        ] {
            assert!(!expected_frontend(url));
        }
    }
}

pub(crate) const MAX_MESSAGE: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct BridgeMessage {
    pub(crate) generation: u64,
    pub(crate) epoch: u64,
    pub(crate) frame: String,
    pub(crate) kind: String,
    pub(crate) payload: String,
}
impl BridgeMessage {
    pub(crate) fn parse(message: &ProcessMessage) -> Option<Self> {
        if CefString::from(&message.name()).to_string() != crate::devtools_renderer::IPC {
            return None;
        }
        let args = message.argument_list()?;
        if args.size() != 5 || (0..5).any(|i| args.get_type(i) != ValueType::STRING) {
            return None;
        }
        let values: Vec<_> = (0..5)
            .map(|i| CefString::from(&args.string(i)).to_string())
            .collect();
        Self::from_strings(&values)
    }
    fn from_strings(values: &[String]) -> Option<Self> {
        if values.len() != 5 {
            return None;
        }
        if values[0].len() > 20
            || values[1].len() > 20
            || values[2].len() > 32
            || values[3].len() > MAX_MESSAGE
            || values[4].len() > 512
        {
            return None;
        }
        let generation = values[0].parse().ok()?;
        let epoch = values[1].parse().ok()?;
        if generation == 0 || epoch == 0 || values[4].is_empty() {
            return None;
        }
        Some(Self {
            generation,
            epoch,
            kind: values[2].clone(),
            payload: values[3].clone(),
            frame: values[4].clone(),
        })
    }
}
pub(crate) fn bridge_message(
    generation: u64,
    epoch: u64,
    kind: &str,
    payload: &str,
    frame: &str,
) -> Option<ProcessMessage> {
    if payload.len() > MAX_MESSAGE {
        return None;
    }
    let message = process_message_create(Some(&CefString::from(crate::devtools_renderer::IPC)))?;
    let args = message.argument_list()?;
    for (i, value) in [
        generation.to_string(),
        epoch.to_string(),
        kind.into(),
        payload.into(),
        frame.into(),
    ]
    .iter()
    .enumerate()
    {
        args.set_string(i, Some(&CefString::from(value.as_str())));
    }
    Some(message)
}
pub(crate) fn renderer_kind(kind: &str, payload: &str) -> bool {
    match kind {
        "context" | "release" | "loaded" | "connected" | "ready" | "close" | "focus" => {
            payload.is_empty()
        }
        "selected" => {
            !payload.is_empty()
                && payload.len() <= 128
                && payload
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }
        "roundtrip" => {
            payload.len() <= 1024
                && serde_json::from_str::<Value>(payload)
                    .ok()
                    .is_some_and(|v| {
                        v.as_array().is_some_and(|a| {
                            a.len() == 2 && a[0].is_u64() && (a[1].is_null() || a[1].is_string())
                        })
                    })
        }
        "protocol" => {
            payload.len() <= MAX_MESSAGE
                && serde_json::from_str::<Value>(payload)
                    .ok()
                    .is_some_and(|value| {
                        value.is_object()
                            && value
                                .get("id")
                                .and_then(Value::as_u64)
                                .is_some_and(|id| id <= 9_007_199_254_740_991)
                            && value
                                .get("method")
                                .and_then(Value::as_str)
                                .is_some_and(|method| !method.is_empty())
                            && value.get("params").is_none_or(Value::is_object)
                            && value.get("sessionId").is_none_or(Value::is_string)
                    })
        }
        _ => false,
    }
}
pub(crate) fn browser_kind(kind: &str, payload: &str) -> bool {
    match kind {
        "protocol" => {
            payload.len() <= MAX_MESSAGE
                && serde_json::from_str::<Value>(payload)
                    .ok()
                    .is_some_and(|v| v.is_object())
        }
        "panel" => ["elements", "console", "network"].contains(&payload),
        "inspect" | "revoke" => payload.is_empty(),
        _ => false,
    }
}
pub(crate) fn panel_name(panel: InspectorPanel) -> &'static str {
    match panel {
        InspectorPanel::Elements => "elements",
        InspectorPanel::Console => "console",
        InspectorPanel::Network => "network",
    }
}
pub(crate) enum BridgeAction {
    Submit(String),
    Deliver(&'static str, String),
}
#[derive(Default)]
pub(crate) struct BridgeState {
    pub(crate) epoch: u64,
    pub(crate) active: bool,
    loaded: bool,
    connected: bool,
    pub(crate) tested: bool,
    pub(crate) pending_inspect: bool,
    roundtrip: bool,
    proof: Option<String>,
    frame: String,
}
impl BridgeState {
    pub(crate) fn revoke(&mut self) {
        self.active = false;
        self.loaded = false;
        self.connected = false;
        self.tested = false;
        self.roundtrip = false;
        self.proof = None;
        self.pending_inspect = false;
    }
    pub(crate) fn toggle_inspect(&mut self) -> bool {
        if self.is_ready() {
            return true;
        }
        self.pending_inspect = !self.pending_inspect;
        false
    }
    pub(crate) fn is_ready(&self) -> bool {
        self.active && self.loaded && self.connected && self.roundtrip && self.tested
    }
    fn accept(&mut self, message: &BridgeMessage) -> Option<Option<BridgeAction>> {
        if !renderer_kind(&message.kind, &message.payload) {
            return None;
        }
        if message.kind == "context" {
            if message.epoch <= self.epoch {
                return None;
            }
            let pending = !self.active && self.pending_inspect;
            self.revoke();
            self.pending_inspect = pending;
            self.epoch = message.epoch;
            self.active = true;
            self.frame = message.frame.clone();
            return Some(None);
        }
        if !self.active || self.epoch != message.epoch || self.frame != message.frame {
            return None;
        }
        match message.kind.as_str() {
            "release" => self.revoke(),
            "roundtrip" if self.proof.as_deref() == Some(message.payload.as_str()) => {
                self.roundtrip = true;
                self.proof = None;
            }
            "selected" | "close" | "focus" => {}
            "loaded" => self.loaded = true,
            "connected" if self.loaded => self.connected = true,
            "ready" if self.connected => self.tested = true,
            "protocol" => return Some(Some(BridgeAction::Submit(message.payload.clone()))),
            _ => return None,
        }
        Some(None)
    }
}

/// The caller is a per-browser native client carrying an immutable inspector
/// role. A URL or a renderer-supplied browser ID cannot acquire that role.
pub(crate) fn receive_frontend(
    id: InspectorId,
    queue: &Queue,
    browser: Option<&mut Browser>,
    frame: Option<&mut Frame>,
    source: ProcessId,
    message: Option<&mut ProcessMessage>,
) -> i32 {
    if source != ProcessId::RENDERER {
        return 0;
    }
    let (Some(browser), Some(frame), Some(message)) = (browser, frame, message) else {
        return 0;
    };
    let Some(input) = BridgeMessage::parse(message) else {
        return 0;
    };
    let identifier = CefString::from(&frame.identifier()).to_string();
    if frame.is_main() == 0
        || input.frame != identifier
        || input.generation != id.0
        || !expected_frontend(&CefString::from(&frame.url()).to_string())
        || !browser
            .main_frame()
            .is_some_and(|main| CefString::from(&main.identifier()).to_string() == identifier)
        || !renderer_kind(&input.kind, &input.payload)
    {
        return 0;
    }
    enqueue(queue, id, Backend::Frontend(input));
    1
}
fn weight(message: &Backend) -> usize {
    match message {
        Backend::Message(bytes) => bytes.len(),
        Backend::Frontend(input) => input.payload.len() + 1024,
        _ => 64,
    }
}
fn enqueue(queue: &Queue, id: InspectorId, message: Backend) {
    let mut queue = queue.lock().unwrap();
    if queue.len() >= 8192
        || queue
            .iter()
            .map(|(_, message)| weight(message))
            .sum::<usize>()
            + weight(&message)
            > 32 * 1024 * 1024
    {
        // Overflow is terminal: never continue with a silently truncated CDP
        // stream or claim readiness after losing a lifecycle notification.
        queue.retain(|(inspector, _)| *inspector != id);
        queue.push((id, Backend::Detached));
    } else {
        queue.push((id, message));
    }
}
/// Cancel foreign destinations without revoking the still-live document.
/// Only an accepted replacement of the main document starts a new epoch.
fn inspector_navigation(queue: &Queue, inspector: InspectorId, url: &str) -> i32 {
    if !expected_frontend(url) {
        return 1;
    }
    enqueue(queue, inspector, Backend::Revoke);
    0
}

wrap_request_handler! {
    pub(crate) struct InspectorRequestHandler { inspector: InspectorId, queue: Queue }
    impl RequestHandler {
        fn on_before_browse(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, request: Option<&mut Request>, _gesture: i32, _redirect: i32) -> i32 {
            if frame.is_some_and(|f| f.is_main() != 0) {
                let url = request.map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
                return inspector_navigation(&self.queue, self.inspector, &url);
            }
            0
        }
        fn on_render_process_terminated(&self, _browser: Option<&mut Browser>, _status: TerminationStatus, _code: i32, _text: Option<&CefString>) {
            enqueue(&self.queue, self.inspector, Backend::Detached);
        }
    }
}

#[cfg(test)]
mod bridge_tests {
    use super::*;
    fn input(id: InspectorId, epoch: u64, kind: &str, payload: &str) -> BridgeMessage {
        BridgeMessage {
            generation: id.0,
            epoch,
            frame: "frame-token-not-an-integer".into(),
            kind: kind.into(),
            payload: payload.into(),
        }
    }
    fn session() -> (Inspectors<()>, InspectorId) {
        let mut state = Inspectors::default();
        let id = state.allocate().unwrap();
        state.insert(
            id,
            ViewId::from_raw(1),
            ViewId::from_raw(2),
            InspectorPanel::Console,
            (),
        );
        state.take();
        (state, id)
    }
    fn push(state: &mut Inspectors<()>, id: InspectorId, epoch: u64, kind: &str, payload: &str) {
        enqueue(
            &state.queue,
            id,
            Backend::Frontend(input(id, epoch, kind, payload)),
        );
    }
    #[test]
    fn replies_flow_before_connection_ready_but_readiness_requires_delivery_proof() {
        let (mut state, id) = session();
        push(&mut state, id, 1, "context", "");
        push(&mut state, id, 1, "loaded", "");
        push(
            &mut state,
            id,
            1,
            "protocol",
            r#"{"id":1,"method":"Runtime.enable"}"#,
        );
        assert!(state.take().is_empty());
        assert!(matches!(
            state.actions.pop(),
            Some((_, 1, BridgeAction::Submit(_)))
        ));
        let ids = MessageIds::default();
        let (wire, _) = state
            .sessions
            .get_mut(&id)
            .unwrap()
            .renderer_protocol
            .submit(&ids, r#"{"id":1,"method":"Runtime.enable"}"#)
            .unwrap();
        // A snapshot response cannot produce proof or consume this request.
        let snapshot = ids.next().unwrap();
        enqueue(
            &state.queue,
            id,
            Backend::Message(format!(r#"{{"id":{snapshot},"result":{{}}}}"#).into_bytes()),
        );
        state.take();
        assert!(state.sessions[&id].bridge.proof.is_none());
        enqueue(
            &state.queue,
            id,
            Backend::Message(format!(r#"{{"id":{wire},"result":{{}}}}"#).into_bytes()),
        );
        state.take();
        assert!(matches!(
            state.actions.pop(),
            Some((_, 1, BridgeAction::Deliver("protocol", _)))
        ));
        assert!(!state.sessions[&id].bridge.is_ready());
        push(&mut state, id, 1, "roundtrip", "[999,null]");
        push(&mut state, id, 1, "connected", "");
        push(&mut state, id, 1, "ready", "");
        assert!(state.take().is_empty());
        assert!(!state.sessions[&id].bridge.is_ready());
        push(&mut state, id, 1, "roundtrip", "[1,null]");
        assert!(
            matches!(state.take().as_slice(), [InspectorEvent::Status(s)] if s.phase == InspectorPhase::FrontendReady)
        );
        assert!(
            matches!(state.actions.pop(), Some((_, 1, BridgeAction::Deliver("panel", panel))) if panel == "console")
        );
        state.loading(id, INSPECTOR_FRONTEND_URL.into(), false);
        assert_eq!(
            state.sessions[&id].status.phase,
            InspectorPhase::FrontendReady
        );
    }
    #[test]
    fn wrong_generation_frame_epoch_and_revoked_contexts_cannot_submit() {
        let (mut state, id) = session();
        let mut wrong = input(id, 1, "context", "");
        wrong.generation += 1;
        enqueue(&state.queue, id, Backend::Frontend(wrong));
        state.take();
        assert!(!state.sessions[&id].bridge.active);
        push(&mut state, id, 2, "context", "");
        state.take();
        let command = r#"{"id":1,"method":"Runtime.enable"}"#;
        for mut forged in [
            input(id, 1, "protocol", command),
            input(id, 2, "protocol", command),
        ] {
            if forged.epoch == 2 {
                forged.frame = "different-main-frame".into();
            }
            enqueue(&state.queue, id, Backend::Frontend(forged));
        }
        state.take();
        assert!(state.actions.is_empty());
        push(&mut state, id, 2, "release", "");
        push(&mut state, id, 2, "context", ""); // Cannot resurrect released context.
        push(&mut state, id, 2, "protocol", command);
        state.take();
        assert!(state.actions.is_empty());
        assert!(!state.sessions[&id].bridge.active);
        push(&mut state, id, 3, "context", "");
        push(&mut state, id, 3, "protocol", command);
        state.take();
        assert_eq!(state.actions.len(), 1);
        state.actions.clear();
        enqueue(&state.queue, id, Backend::Revoke);
        push(&mut state, id, 3, "protocol", command);
        state.take();
        assert!(state.actions.is_empty());
        enqueue(&state.queue, id, Backend::Detached);
        push(&mut state, id, 4, "context", "");
        state.take();
        assert_eq!(state.sessions[&id].status.phase, InspectorPhase::Detached);
        assert!(!state.sessions[&id].bridge.active);
    }
    #[test]
    fn protocol_correlation_is_cleared_between_documents_and_probe_ids_are_independent() {
        let (mut state, id) = session();
        let ids = MessageIds::default();
        let command = r#"{"id":1,"method":"Runtime.enable"}"#;
        let (probe, _) = state.submit(id, &ids, command).unwrap();
        push(&mut state, id, 1, "context", "");
        state.take();
        let (old, _) = state
            .sessions
            .get_mut(&id)
            .unwrap()
            .renderer_protocol
            .submit(&ids, command)
            .unwrap();
        push(&mut state, id, 1, "release", "");
        push(&mut state, id, 2, "context", "");
        state.take();
        let (new, _) = state
            .sessions
            .get_mut(&id)
            .unwrap()
            .renderer_protocol
            .submit(&ids, command)
            .unwrap();
        assert_ne!(new, old);
        assert_ne!(new, probe);
        enqueue(
            &state.queue,
            id,
            Backend::Message(format!(r#"{{"id":{old},"result":{{}}}}"#).into_bytes()),
        );
        assert!(state.take().is_empty());
        assert!(state.actions.is_empty());
        enqueue(
            &state.queue,
            id,
            Backend::Message(format!(r#"{{"id":{probe},"result":{{}}}}"#).into_bytes()),
        );
        assert!(matches!(
            state.take().as_slice(),
            [InspectorEvent::Protocol { .. }]
        ));
        assert!(state.actions.is_empty());
        enqueue(
            &state.queue,
            id,
            Backend::Message(format!(r#"{{"id":{new},"error":{{"code":-1}}}}"#).into_bytes()),
        );
        state.take();
        assert!(state.sessions[&id].bridge.proof.is_none());
        assert!(matches!(
            state.actions.pop(),
            Some((_, 2, BridgeAction::Deliver("protocol", _)))
        ));
    }
    #[test]
    fn schema_direction_size_and_queue_overflow_fail_closed() {
        let values = ["1", "1", "loaded", "", "frame-token"].map(str::to_string);
        assert!(BridgeMessage::from_strings(&values).is_some());
        for index in [0, 1, 4] {
            let mut invalid = values.clone();
            invalid[index] = String::new();
            assert!(BridgeMessage::from_strings(&invalid).is_none());
        }
        assert!(BridgeMessage::from_strings(&values[..4]).is_none());
        assert!(!renderer_kind("panel", "console"));
        assert!(!browser_kind("connected", ""));
        assert!(!renderer_kind("ready", "true"));
        for payload in [
            "null",
            "[]",
            r#"{"id":1,"method":"x","params":[]}"#,
            r#"{"id":1,"method":"x","sessionId":3}"#,
        ] {
            assert!(!renderer_kind("protocol", payload));
        }
        assert!(!renderer_kind("protocol", &" ".repeat(MAX_MESSAGE + 1)));
        let (mut state, id) = session();
        push(&mut state, id, 1, "context", "");
        state.take();
        // Test the byte bound without allocating thousands of huge messages.
        enqueue(&state.queue, id, Backend::Message(vec![0; MAX_MESSAGE]));
        enqueue(&state.queue, id, Backend::Message(vec![0; MAX_MESSAGE]));
        push(&mut state, id, 1, "ready", "");
        assert!(matches!(
            state.take().as_slice(),
            [InspectorEvent::Detached { .. }]
        ));
        assert!(!state.sessions[&id].bridge.active);
    }
}

#[cfg(test)]
mod frontend_event_tests {
    use super::*;
    #[test]
    fn actual_selection_and_close_requests_are_epoch_scoped_and_never_requested_fallbacks() {
        let mut state = Inspectors::default();
        let id = state.allocate().unwrap();
        state.insert(
            id,
            ViewId::from_raw(1),
            ViewId::from_raw(2),
            InspectorPanel::Elements,
            (),
        );
        state.take();
        let message = |epoch, kind: &str, payload: &str| BridgeMessage {
            generation: id.0,
            epoch,
            frame: "main".into(),
            kind: kind.into(),
            payload: payload.into(),
        };
        enqueue(
            &state.queue,
            id,
            Backend::Frontend(message(1, "context", "")),
        );
        enqueue(
            &state.queue,
            id,
            Backend::Frontend(message(1, "selected", "network")),
        );
        enqueue(&state.queue, id, Backend::Frontend(message(1, "close", "")));
        enqueue(&state.queue, id, Backend::Frontend(message(1, "focus", "")));
        let events = state.take();
        assert!(events.contains(&InspectorEvent::PanelChanged {
            inspector: id,
            panel: Some(InspectorPanel::Network)
        }));
        assert!(events.contains(&InspectorEvent::CloseRequested { inspector: id }));
        assert!(events.contains(&InspectorEvent::FocusRequested { inspector: id }));
        assert_eq!(state.sessions[&id].status.panel, InspectorPanel::Elements);
        assert_eq!(
            state.sessions[&id].status.selected_panel,
            Some(InspectorPanel::Network)
        );
        enqueue(
            &state.queue,
            id,
            Backend::Frontend(message(1, "selected", "settings-preferences")),
        );
        assert!(state.take().contains(&InspectorEvent::PanelChanged {
            inspector: id,
            panel: None
        }));
        enqueue(&state.queue, id, Backend::Revoke);
        enqueue(&state.queue, id, Backend::Frontend(message(1, "close", "")));
        enqueue(
            &state.queue,
            id,
            Backend::Frontend(message(1, "selected", "console")),
        );
        assert!(state.take().is_empty());
        assert_eq!(state.sessions[&id].status.selected_panel, None);
    }
}

#[cfg(test)]
mod inspect_intent_tests {
    use super::*;
    #[test]
    fn pre_ready_inspect_calls_coalesce_by_parity_run_once_and_revoke() {
        let mut state = Inspectors::default();
        let id = state.allocate().unwrap();
        state.insert(
            id,
            ViewId::from_raw(1),
            ViewId::from_raw(2),
            InspectorPanel::Elements,
            (),
        );
        state.take();
        let message = |epoch, kind: &str| BridgeMessage {
            generation: id.0,
            epoch,
            frame: "main".into(),
            kind: kind.into(),
            payload: String::new(),
        };
        assert!(!state.sessions.get_mut(&id).unwrap().bridge.toggle_inspect());
        enqueue(&state.queue, id, Backend::Revoke); // Initial on_before_browse.
        enqueue(&state.queue, id, Backend::Frontend(message(1, "context")));
        state.take();
        assert!(state.sessions[&id].bridge.pending_inspect);
        for _ in 0..2 {
            assert!(!state.sessions.get_mut(&id).unwrap().bridge.toggle_inspect());
        }
        assert!(state.sessions[&id].bridge.pending_inspect);
        {
            let bridge = &mut state.sessions.get_mut(&id).unwrap().bridge;
            bridge.loaded = true;
            bridge.connected = true;
            bridge.roundtrip = true;
        }
        enqueue(&state.queue, id, Backend::Frontend(message(1, "ready")));
        state.take();
        assert_eq!(
            state
                .actions
                .iter()
                .filter(|(_, _, action)| matches!(action, BridgeAction::Deliver("inspect", _)))
                .count(),
            1
        );
        assert!(!state.sessions[&id].bridge.pending_inspect);
        state.actions.clear();
        enqueue(&state.queue, id, Backend::Frontend(message(1, "ready")));
        state.take();
        assert!(state.actions.is_empty());
        assert!(state.sessions.get_mut(&id).unwrap().bridge.toggle_inspect()); // Ready: send immediately.
        enqueue(&state.queue, id, Backend::Frontend(message(1, "release")));
        state.take();
        assert!(!state.sessions.get_mut(&id).unwrap().bridge.toggle_inspect());
        assert!(!state.sessions.get_mut(&id).unwrap().bridge.toggle_inspect());
        assert!(!state.sessions[&id].bridge.pending_inspect); // Two early calls cancel.
        assert!(!state.sessions.get_mut(&id).unwrap().bridge.toggle_inspect());
        enqueue(&state.queue, id, Backend::Revoke);
        state.take();
        assert!(!state.sessions[&id].bridge.pending_inspect);
        assert!(!state.sessions.get_mut(&id).unwrap().bridge.toggle_inspect());
        enqueue(&state.queue, id, Backend::Detached);
        state.take();
        assert!(!state.sessions[&id].bridge.pending_inspect);
        state.close(id);
        assert!(!state.sessions.contains_key(&id));
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    #[test]
    fn cancelled_foreign_navigation_preserves_ready_context_transport_and_reports() {
        let mut state = Inspectors::default();
        let id = state.allocate().unwrap();
        state.insert(
            id,
            ViewId::from_raw(1),
            ViewId::from_raw(2),
            InspectorPanel::Elements,
            (),
        );
        state.take();
        let input = |kind: &str, payload: &str| BridgeMessage {
            generation: id.0,
            epoch: 1,
            frame: "main".into(),
            kind: kind.into(),
            payload: payload.into(),
        };
        enqueue(&state.queue, id, Backend::Frontend(input("context", "")));
        state.take();
        {
            let session = state.sessions.get_mut(&id).unwrap();
            session.bridge.loaded = true;
            session.bridge.connected = true;
            session.bridge.tested = true;
            session.bridge.roundtrip = true;
            session.status.phase = InspectorPhase::FrontendReady;
        }
        for foreign in [
            "https://example.com",
            "devtools://devtools/bundled/inspector.html?ws=evil",
            "about:blank",
        ] {
            assert_eq!(inspector_navigation(&state.queue, id, foreign), 1);
            assert!(state.queue.lock().unwrap().is_empty());
        }
        enqueue(
            &state.queue,
            id,
            Backend::Frontend(input("protocol", r#"{"id":9,"method":"Runtime.enable"}"#)),
        );
        enqueue(
            &state.queue,
            id,
            Backend::Frontend(input("selected", "network")),
        );
        enqueue(&state.queue, id, Backend::Frontend(input("close", "")));
        let events = state.take();
        assert!(state.sessions[&id].bridge.is_ready());
        assert_eq!(
            state.sessions[&id].status.phase,
            InspectorPhase::FrontendReady
        );
        assert!(matches!(
            state.actions.as_slice(),
            [(_, 1, BridgeAction::Submit(_))]
        ));
        assert!(events.contains(&InspectorEvent::PanelChanged {
            inspector: id,
            panel: Some(InspectorPanel::Network)
        }));
        assert!(events.contains(&InspectorEvent::CloseRequested { inspector: id }));
        assert_eq!(
            inspector_navigation(&state.queue, id, INSPECTOR_FRONTEND_URL),
            0
        );
        assert!(
            state.take().iter().any(
                |e| matches!(e, InspectorEvent::Status(s) if s.phase == InspectorPhase::Opening)
            )
        );
        assert!(!state.sessions[&id].bridge.is_ready());
    }
}
