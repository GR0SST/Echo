//! Asynchronous screenshots of existing windowed browsers via the local CEF
//! DevTools transport. Callbacks own only a queue; decoding happens on polling.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use cef::*;

use crate::ViewId;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Completion of an accepted screenshot request. `None` means capture failed,
/// the DevTools agent detached, or the request timed out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageSnapshot {
    pub view: ViewId,
    pub request: u64,
    pub png: Option<Vec<u8>>,
}

enum Reply {
    Result {
        success: bool,
        bytes: Option<Vec<u8>>,
    },
    Detached,
}

type Queue = Arc<Mutex<Vec<(ViewId, i32, Reply)>>>;

struct Pending {
    request: u64,
    message_id: i32,
    deadline: Instant,
    // Dropping this unregisters the observer before the browser is closed.
    _registration: Option<Registration>,
}

#[derive(Default)]
pub(crate) struct Snapshots {
    queue: Queue,
    pending: HashMap<ViewId, Pending>,
    ready: Vec<PageSnapshot>,
    message_ids: crate::devtools::MessageIds,
}

impl Snapshots {
    pub(crate) fn request(&mut self, view: ViewId, request: u64, browser: &Browser, ids: &crate::devtools::MessageIds) -> bool {
        self.message_ids = ids.clone();
        self.collect(Instant::now());
        if self.pending.contains_key(&view) {
            return false;
        }
        let Some(host) = browser.host() else {
            return false;
        };
        let Some(mut params) = dictionary_value_create() else {
            return false;
        };
        if params.set_string(Some(&"format".into()), Some(&"png".into())) == 0
            || params.set_bool(Some(&"fromSurface".into()), 1) == 0
            || params.set_bool(Some(&"captureBeyondViewport".into()), 0) == 0
        {
            return false;
        }
        let Some(message_id) = self.begin(view, request, Instant::now()) else {
            return false;
        };
        let mut observer = SnapshotObserver::new(self.queue.clone(), view, message_id);
        let Some(registration) = host.add_dev_tools_message_observer(Some(&mut observer)) else {
            self.pending.remove(&view);
            return false;
        };
        self.pending.get_mut(&view).unwrap()._registration = Some(registration);
        // Install pending state first: even a synchronous callback only queues
        // owned bytes, which will be correlated after this method returns.
        let submitted = host.execute_dev_tools_method(
            message_id,
            Some(&"Page.captureScreenshot".into()),
            Some(&mut params),
        );
        self.submitted(view, message_id, submitted)
    }

    fn begin(&mut self, view: ViewId, request: u64, now: Instant) -> Option<i32> {
        if self.pending.contains_key(&view) {
            return None;
        }
        let message_id = self.next_message_id()?;
        self.pending.insert(
            view,
            Pending {
                request,
                message_id,
                deadline: now + TIMEOUT,
                _registration: None,
            },
        );
        Some(message_id)
    }

    fn submitted(&mut self, view: ViewId, message_id: i32, submitted: i32) -> bool {
        if submitted != message_id {
            self.pending.remove(&view);
            return false;
        }
        true
    }

    fn next_message_id(&mut self) -> Option<i32> {
        // Never reuse an ID, even after submission failure or timeout: a late
        // result must not complete a retry (including a reused caller request).
        self.message_ids.next()
    }

    fn collect(&mut self, now: Instant) {
        let expired: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(&view, pending)| (now >= pending.deadline).then_some(view))
            .collect();
        for view in expired {
            self.finish(view, None);
        }
        let replies = std::mem::take(&mut *self.queue.lock().unwrap());
        for (view, message_id, reply) in replies {
            if !self
                .pending
                .get(&view)
                .is_some_and(|pending| pending.message_id == message_id)
            {
                continue;
            }
            let png = match reply {
                Reply::Result { success, bytes } => decode_result(success, bytes.as_deref()),
                Reply::Detached => None,
            };
            self.finish(view, png);
        }
    }

    fn finish(&mut self, view: ViewId, png: Option<Vec<u8>>) {
        if let Some(pending) = self.pending.remove(&view) {
            self.ready.push(PageSnapshot {
                view,
                request: pending.request,
                png,
            });
        }
    }

    pub(crate) fn take(&mut self) -> Vec<PageSnapshot> {
        self.collect(Instant::now());
        std::mem::take(&mut self.ready)
    }

    pub(crate) fn close(&mut self, view: ViewId) {
        self.pending.remove(&view);
        self.ready.retain(|snapshot| snapshot.view != view);
        self.queue.lock().unwrap().retain(|(id, _, _)| *id != view);
    }
}

fn decode_result(success: bool, result: Option<&[u8]>) -> Option<Vec<u8>> {
    if !success {
        return None;
    }
    // CEF supplies the result dictionary itself, not the protocol envelope.
    let result: serde_json::Value = serde_json::from_slice(result?).ok()?;
    let data = result.get("data")?.as_str()?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    png.starts_with(b"\x89PNG\r\n\x1a\n").then_some(png)
}

wrap_dev_tools_message_observer! {
    struct SnapshotObserver {
        queue: Queue,
        view: ViewId,
        message_id: i32,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: i32,
            success: i32,
            result: Option<&[u8]>,
        ) {
            if message_id == self.message_id {
                self.queue.lock().unwrap().push((self.view, self.message_id, Reply::Result {
                    success: success != 0,
                    bytes: result.map(<[u8]>::to_vec),
                }));
            }
        }

        fn on_dev_tools_agent_detached(&self, _browser: Option<&mut Browser>) {
            self.queue.lock().unwrap().push((self.view, self.message_id, Reply::Detached));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aX1cAAAAASUVORK5CYII=";

    fn result() -> Vec<u8> {
        format!(r#"{{"data":"{PNG}"}}"#).into_bytes()
    }

    fn pending(state: &mut Snapshots, view: ViewId, request: u64, now: Instant) -> i32 {
        state.begin(view, request, now).unwrap()
    }

    fn reply(state: &Snapshots, view: ViewId, message_id: i32) {
        state.queue.lock().unwrap().push((
            view,
            message_id,
            Reply::Result {
                success: true,
                bytes: Some(result()),
            },
        ));
    }

    #[test]
    fn busy_and_submission_failure_preserve_correlation() {
        let mut state = Snapshots::default();
        let view = ViewId::from_raw(1);
        let now = Instant::now();
        let old = pending(&mut state, view, 1, now);
        assert_eq!(state.begin(view, 2, now), None);
        assert_eq!(state.pending[&view].request, 1);
        assert!(!state.submitted(view, old, 0));
        assert!(state.ready.is_empty());
        let new = pending(&mut state, view, 1, now);
        assert_ne!(old, new);
        reply(&state, view, old);
        state.collect(now);
        assert!(state.ready.is_empty());
        assert!(state.submitted(view, new, new));
        reply(&state, view, new);
        state.collect(now);
        assert_eq!(state.ready.len(), 1);
        assert!(state.ready[0].png.is_some());
        let id = pending(&mut state, view, 3, now);
        assert!(!state.submitted(view, id, id + 1));
        assert!(state.pending.is_empty());
    }

    #[test]
    fn decodes_result_dictionary_and_rejects_failures() {
        assert_eq!(
            decode_result(true, Some(&result())),
            Some(
                base64::engine::general_purpose::STANDARD
                    .decode(PNG)
                    .unwrap()
            )
        );
        for bytes in [
            None,
            Some(b"invalid".as_slice()),
            Some(br#"{}"#.as_slice()),
            Some(br#"{"data":3}"#.as_slice()),
            Some(br#"{"data":"%%%"}"#.as_slice()),
            Some(br#"{"data":""}"#.as_slice()),
            Some(br#"{"data":"aGVsbG8="}"#.as_slice()),
            Some(br#"{"result":{"data":"iVBORw0KGgo="}}"#.as_slice()),
        ] {
            assert_eq!(decode_result(true, bytes), None);
        }
        assert_eq!(decode_result(false, Some(&result())), None);
        assert_eq!(
            decode_result(false, Some(br#"{"code":-32000,"message":"failed"}"#)),
            None
        );
    }

    #[test]
    fn timeout_and_late_result_do_not_complete_retry_with_same_caller_id() {
        let mut state = Snapshots::default();
        let view = ViewId::from_raw(1);
        let now = Instant::now();
        let old = pending(&mut state, view, u64::MAX, now);
        state.collect(now + TIMEOUT);
        assert_eq!(
            state.ready,
            vec![PageSnapshot {
                view,
                request: u64::MAX,
                png: None
            }]
        );
        state.ready.clear();
        let new = pending(&mut state, view, u64::MAX, now + TIMEOUT);
        reply(&state, view, old);
        state
            .queue
            .lock()
            .unwrap()
            .push((view, old, Reply::Detached));
        state.collect(now + TIMEOUT);
        assert!(state.ready.is_empty());
        assert!(state.pending.contains_key(&view));
        reply(&state, view, new);
        reply(&state, view, new);
        state.collect(now + TIMEOUT);
        assert_eq!(state.ready.len(), 1);
        assert!(state.ready[0].png.is_some());
        assert!(state.pending.is_empty());
    }

    #[test]
    fn detach_and_protocol_failure_allow_retry_and_close_discards_results() {
        let mut state = Snapshots::default();
        let view = ViewId::from_raw(2);
        let now = Instant::now();
        let id = pending(&mut state, view, 10, now);
        state
            .queue
            .lock()
            .unwrap()
            .push((view, id, Reply::Detached));
        state.collect(now);
        assert_eq!(
            state.ready,
            vec![PageSnapshot {
                view,
                request: 10,
                png: None
            }]
        );
        let id = pending(&mut state, view, 11, now);
        state.queue.lock().unwrap().push((
            view,
            id,
            Reply::Result {
                success: false,
                bytes: None,
            },
        ));
        state.collect(now);
        assert_eq!(
            state.ready[1],
            PageSnapshot {
                view,
                request: 11,
                png: None
            }
        );
        let id = pending(&mut state, view, 12, now);
        reply(&state, view, id);
        state.close(view);
        // A callback already in flight when registration was released is stale.
        reply(&state, view, id);
        assert!(state.take().is_empty());
        assert!(state.pending.is_empty());
    }

    #[test]
    fn views_are_independent() {
        let mut state = Snapshots::default();
        let now = Instant::now();
        let a = ViewId::from_raw(1);
        let b = ViewId::from_raw(2);
        let id = pending(&mut state, a, 7, now);
        pending(&mut state, b, 7, now);
        reply(&state, b, id);
        state.collect(now);
        assert!(state.ready.is_empty());
        reply(&state, a, id);
        state.collect(now);
        assert_eq!(state.ready.len(), 1);
        assert!(state.pending.contains_key(&b));
    }

    #[test]
    fn screenshot_ids_share_the_frontend_allocator() {
        let ids = crate::devtools::MessageIds::default();
        let mut state = Snapshots { message_ids: ids.clone(), ..Default::default() };
        let now = Instant::now();
        let first = ids.next().unwrap();
        let screenshot = pending(&mut state, ViewId::from_raw(1), 7, now);
        let second = ids.next().unwrap();
        assert!(first < screenshot && screenshot < second);
        state.close(ViewId::from_raw(1));
        let retry = pending(&mut state, ViewId::from_raw(1), 7, now);
        assert!(retry > second);
    }
}
