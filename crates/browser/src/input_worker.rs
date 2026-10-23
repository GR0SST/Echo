//! Scalar-only worker delivery: posting and root-close revocation serialize.
//! No App, Entity, view/window pointer or CEF owner may be captured here.
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Completion {
    Pending,
    Posted,
    Cancelled,
    Failed,
}
struct State {
    live: bool,
    completion: Completion,
}
pub(super) struct PostGate(Mutex<State>);
impl PostGate {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(State {
            live: true,
            completion: Completion::Pending,
        })))
    }
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|poisoned| {
            let mut state = poisoned.into_inner();
            state.live = false;
            state
        })
    }
    /// The lock covers BOTH admission and the actual enqueue. No check-then-
    /// post gap can cross completed revocation. This callback must only enqueue
    /// an NSEvent; never synchronously dispatch, pump, await or enter GPUI/CEF.
    pub(super) fn post(&self, enqueue: impl FnOnce()) -> bool {
        let state = self.state();
        if !state.live {
            return false;
        }
        enqueue();
        drop(state);
        true
    }
    /// GPUI invokes this before dropping the removed native window. If a
    /// worker already owns admission, its enqueue finishes before revocation
    /// returns and the native parent can be destroyed. Later posts are denied.
    pub(super) fn revoke(&self) {
        self.state().live = false;
    }
    pub(super) fn status(&self) -> (bool, Completion) {
        let state = self.state();
        (state.live, state.completion)
    }
    pub(super) fn finish(&self, completion: Completion) {
        self.state().completion = completion;
    }
}

/// Completion is published only AFTER the worker has stopped posting. The
/// failure default also releases an async waiter if the worker panics.
struct Finish {
    gate: Arc<PostGate>,
    completion: Completion,
}
impl Drop for Finish {
    fn drop(&mut self) {
        self.gate.finish(self.completion);
    }
}
pub(super) fn spawn(
    gate: Arc<PostGate>,
    work: impl FnOnce(&PostGate) -> bool + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut finish = Finish {
            gate,
            completion: Completion::Failed,
        };
        let posted = work(&finish.gate);
        finish.completion = if posted {
            Completion::Posted
        } else {
            Completion::Cancelled
        };
    })
}

#[cfg(test)]
#[path = "input_worker_tests.rs"]
mod tests;
