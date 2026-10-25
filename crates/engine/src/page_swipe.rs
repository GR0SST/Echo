//! Precise finger scroll lifecycle and conservative document scroll policy.
use crate::{EventQueue, RawEvent};
use cef::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    time::{Duration, Instant},
};
fn trace(message: std::fmt::Arguments<'_>) {
    if std::env::var_os("BROWSER_SCRIPT").is_some() {
        eprintln!("[page-swipe] {message}");
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSwipe {
    pub back: bool,
    pub progress: f32,
}
#[derive(Default)]
struct Drag {
    x: f64,
    y: f64,
    horizontal: Option<bool>,
    samples: Vec<(Instant, f64)>,
    ended: bool,
}
impl Drag {
    fn update(&mut self, dx: f64, dy: f64, now: Instant) {
        if !dx.is_finite() || !dy.is_finite() {
            self.horizontal = Some(false);
            return;
        }
        self.x += dx;
        self.y += dy;
        if self.horizontal.is_none() && (self.x.abs() > 6. || self.y.abs() > 6.) {
            self.horizontal = Some(self.x.abs() > self.y.abs() * 1.2);
        }
        self.samples.push((now, dx));
        self.samples
            .retain(|(at, _)| now.saturating_duration_since(*at) <= Duration::from_millis(80));
    }
    fn state(&self, back: bool, forward: bool) -> Option<PageSwipe> {
        (self.horizontal == Some(true) && if self.x > 0. { back } else { forward }).then_some(
            PageSwipe {
                back: self.x > 0.,
                progress: (self.x.abs() / 96.).min(1.) as f32,
            },
        )
    }
    fn commit(&self, now: Instant) -> bool {
        let recent: f64 = self
            .samples
            .iter()
            .filter(|(at, _)| now.saturating_duration_since(*at) <= Duration::from_millis(80))
            .map(|(_, dx)| *dx)
            .sum();
        self.x.abs() >= 96.
            || (self.x.abs() >= 32.
                && recent.signum() == self.x.signum()
                && recent.abs() / 0.08 > 700.)
    }
}
struct Pending {
    id: i32,
    token: i32,
    started: Instant,
    _registration: Option<Registration>,
}
struct Binding {
    browser: Browser,
    events: EventQueue,
    ids: crate::devtools::MessageIds,
    token: i32,
    allowed: Option<bool>,
    drag: Option<Drag>,
    back: bool,
    forward: bool,
    feedback: Option<PageSwipe>,
    pending: Option<Pending>,
}
impl Binding {
    fn feedback(&mut self, id: i32, state: Option<PageSwipe>) {
        if self.feedback != state {
            self.feedback = state;
            self.events.push((id, RawEvent::PageSwipe(state)));
        }
    }
}
type Replies = std::sync::Arc<std::sync::Mutex<Vec<(i32, i32, i32, bool)>>>;
fn replies() -> Replies {
    static QUEUE: std::sync::OnceLock<Replies> = std::sync::OnceLock::new();
    QUEUE.get_or_init(Default::default).clone()
}
thread_local! {static BINDINGS:RefCell<HashMap<i32,Binding>>=RefCell::default();}
pub(crate) fn register(browser: Browser, events: EventQueue, ids: crate::devtools::MessageIds) {
    BINDINGS.with_borrow_mut(|all| {
        all.insert(
            browser.identifier(),
            Binding {
                browser,
                events,
                ids,
                token: 0,
                allowed: None,
                drag: None,
                back: false,
                forward: false,
                feedback: None,
                pending: None,
            },
        );
    });
}
pub(crate) fn active_token(id: i32) -> Option<i32> {
    BINDINGS.with_borrow(|all| all.get(&id).filter(|b| b.drag.is_some()).map(|b| b.token))
}
pub(crate) fn awaiting_policy(id: i32) -> bool {
    BINDINGS.with_borrow(|all| {
        all.get(&id)
            .is_some_and(|b| b.drag.is_some() && b.allowed.is_none())
    })
}
pub(crate) fn eligible(id: i32) -> bool {
    BINDINGS.with_borrow(|all| {
        all.get(&id).is_some_and(|b| {
            b.allowed == Some(true)
                && b.drag
                    .as_ref()
                    .and_then(|d| d.state(b.back, b.forward))
                    .is_some()
        })
    })
}
pub(crate) fn deny(id: i32, token: i32) {
    let pending = BINDINGS.with_borrow_mut(|all| {
        let b = all.get_mut(&id).filter(|b| b.token == token)?;
        b.allowed = Some(false);
        b.feedback(id, None);
        b.pending.take()
    });
    drop(pending);
}
pub(crate) fn cancel(id: i32) {
    let pending = BINDINGS.with_borrow_mut(|all| {
        let b = all.get_mut(&id)?;
        b.drag = None;
        b.allowed = None;
        b.feedback(id, None);
        b.pending.take()
    });
    drop(pending);
    #[cfg(target_os = "macos")]
    crate::mac::cancel_pending_scroll(id);
}
pub(crate) fn retarget(id: i32, events: EventQueue) {
    cancel(id);
    BINDINGS.with_borrow_mut(|all| {
        if let Some(b) = all.get_mut(&id) {
            b.events = events;
        }
    });
}
pub(crate) fn forget(id: i32) {
    cancel(id);
    let old = BINDINGS.with_borrow_mut(|all| all.remove(&id));
    drop(old);
}
fn finish(id: i32) {
    let navigate = BINDINGS.with_borrow_mut(|all| {
        let b = all.get_mut(&id)?;
        let drag = b.drag.as_ref()?;
        if !drag.ended || b.allowed.is_none() {
            return None;
        }
        let direction = if b.allowed == Some(true) && drag.commit(Instant::now()) {
            drag.state(b.back, b.forward).map(|s| s.back)
        } else {
            None
        };
        b.feedback(id, None);
        b.drag = None;
        direction.map(|back| (b.browser.clone(), back))
    });
    if let Some((browser, back)) = navigate {
        if back && browser.can_go_back() != 0 {
            browser.go_back();
        } else if !back && browser.can_go_forward() != 0 {
            browser.go_forward();
        }
    }
}
/// Only a live physical finger stream can start a gesture; momentum is excluded
/// by the native bridge. Site policy is read before forwarding queued input.
pub(crate) fn scroll(id: i32, x: f64, y: f64, dx: f64, dy: f64, phase: u8) -> bool {
    if phase == 8 {
        cancel(id);
        return false;
    }
    if phase == 1 {
        cancel(id);
    }
    let (query, eligible) = BINDINGS.with_borrow_mut(|all| {
        let Some(b) = all.get_mut(&id) else {
            return (None, false);
        };
        let query = if phase == 1 {
            b.token = b.token.wrapping_add(1);
            b.allowed = None;
            b.drag = Some(Drag::default());
            b.back = b.browser.can_go_back() != 0;
            b.forward = b.browser.can_go_forward() != 0;
            if b.back || b.forward {
                Some(b.token)
            } else {
                b.allowed = Some(false);
                None
            }
        } else {
            None
        };
        let Some(drag) = b.drag.as_mut() else {
            return (query, false);
        };
        drag.update(dx, dy, Instant::now());
        if phase == 4 {
            drag.ended = true;
        }
        let state = drag.state(b.back, b.forward);
        let eligible = b.allowed == Some(true) && state.is_some();
        if b.allowed == Some(true) {
            b.feedback(id, state);
        }
        (query, eligible)
    });
    if let Some(token) = query {
        if !query_policy(id, token, x, y) {
            deny(id, token);
        }
    }
    finish(id);
    eligible
}
fn query_policy(id: i32, token: i32, x: f64, y: f64) -> bool {
    if !x.is_finite() || !y.is_finite() {
        return false;
    }
    let Some((browser, ids)) =
        BINDINGS.with_borrow(|all| all.get(&id).map(|b| (b.browser.clone(), b.ids.clone())))
    else {
        return false;
    };
    let (Some(host), Some(message_id), Some(mut params)) =
        (browser.host(), ids.next(), dictionary_value_create())
    else {
        return false;
    };
    let code = include_str!("page_swipe_policy.js")
        .replace("POINT_X", &x.to_string())
        .replace("POINT_Y", &y.to_string());
    params.set_string(
        Some(&"expression".into()),
        Some(&CefString::from(code.as_str())),
    );
    for name in [
        "returnByValue",
        "includeCommandLineAPI",
        "silent",
        "disableBreaks",
    ] {
        params.set_bool(Some(&name.into()), 1);
    }
    let mut observer = PolicyObserver::new(replies(), id, token, message_id);
    let Some(registration) = host.add_dev_tools_message_observer(Some(&mut observer)) else {
        return false;
    };
    let mut registration = Some(registration);
    let admitted = BINDINGS.with_borrow_mut(|all| {
        let Some(b) = all
            .get_mut(&id)
            .filter(|b| b.token == token && b.drag.is_some())
        else {
            return false;
        };
        b.pending = Some(Pending {
            id: message_id,
            token,
            started: Instant::now(),
            _registration: registration.take(),
        });
        true
    });
    if !admitted {
        return false;
    }
    if host.execute_dev_tools_method(
        message_id,
        Some(&"Runtime.evaluate".into()),
        Some(&mut params),
    ) != message_id
    {
        replies()
            .lock()
            .unwrap()
            .push((id, token, message_id, false));
    }
    true
}
fn decode_policy(success: bool, result: Option<&[u8]>) -> bool {
    success
        && result
            .filter(|b| b.len() <= 65536)
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
            .filter(|v| v.get("exceptionDetails").is_none())
            .and_then(|v| {
                v.get("result")
                    .and_then(|v| v.get("value"))
                    .and_then(|v| v.as_bool())
            })
            .unwrap_or(false)
}
wrap_dev_tools_message_observer! {
 struct PolicyObserver{queue:Replies,browser:i32,token:i32,message:i32}
 impl DevToolsMessageObserver{
  fn on_dev_tools_method_result(&self,_browser:Option<&mut Browser>,message_id:i32,success:i32,result:Option<&[u8]>){if message_id==self.message{self.queue.lock().unwrap().push((self.browser,self.token,self.message,decode_policy(success!=0,result)));}}
  fn on_dev_tools_agent_detached(&self,_browser:Option<&mut Browser>){self.queue.lock().unwrap().push((self.browser,self.token,self.message,false));}
 }
}
pub(crate) fn poll() {
    let queue = replies();
    let mut results = std::mem::take(&mut *queue.lock().unwrap());
    BINDINGS.with_borrow(|all| {
        for (id, b) in all {
            if let Some(p) = b
                .pending
                .as_ref()
                .filter(|p| p.started.elapsed() >= Duration::from_millis(500))
            {
                results.push((*id, p.token, p.id, false));
            }
        }
    });
    for (id, token, message, allowed) in results {
        let pending = BINDINGS.with_borrow_mut(|all| {
            let b = all.get_mut(&id).filter(|b| {
                b.token == token
                    && b.drag.is_some()
                    && b.pending
                        .as_ref()
                        .is_some_and(|p| p.id == message && p.token == token)
            })?;
            let pending = b.pending.take();
            b.allowed = Some(allowed);
            let state = if allowed {
                b.drag.as_ref().and_then(|d| d.state(b.back, b.forward))
            } else {
                None
            };
            b.feedback(id, state);
            pending
        });
        if pending.is_none() {
            continue;
        }
        drop(pending);
        trace(format_args!(
            "policy browser={id} token={token} allowed={allowed}"
        ));
        #[cfg(target_os = "macos")]
        crate::mac::flush_pending_scroll(id, token);
        finish(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_requires_a_successful_boolean_and_rejects_exceptions() {
        let yes = br#"{"result":{"type":"boolean","value":true}}"#;
        assert!(decode_policy(true, Some(yes)));
        assert!(!decode_policy(false, Some(yes)));
        for bad in [
            &br#"{"result":{"value":"true"}}"#[..],
            br#"{"result":{"value":true},"exceptionDetails":{}}"#,
            b"null",
            b"invalid",
        ] {
            assert!(!decode_policy(true, Some(bad)));
        }
        assert!(!decode_policy(true, None));
        assert!(!decode_policy(true, Some(&vec![b' '; 65537])));
    }
    #[test]
    fn short_vertical_reversed_and_momentum_without_a_begin_do_not_commit() {
        let now = Instant::now();
        let mut d = Drag::default();
        d.update(0., 50., now);
        d.update(200., 0., now);
        assert_eq!(d.state(true, true), None);
        let mut d = Drag::default();
        d.update(25., 0., now);
        assert!(!d.commit(now));
        d.update(-20., 0., now);
        assert!(!d.commit(now));
    }
    #[test]
    fn threshold_flick_and_history_edges_are_directional() {
        let now = Instant::now();
        let mut d = Drag::default();
        d.update(100., 0., now);
        assert!(d.commit(now));
        assert_eq!(d.state(false, true), None);
        assert!(d.state(true, false).unwrap().back);
        let mut d = Drag::default();
        d.update(-60., 0., now);
        assert!(d.commit(now));
        assert!(!d.state(true, true).unwrap().back);
        assert!(!d.commit(now + Duration::from_millis(100)));
    }
}
