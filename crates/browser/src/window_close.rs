//! Ordinary window closure only. App termination stays owned by quit.rs.
use engine::{DownloadRetirementPreparation, Engine};
use gpui::{AnyWindowHandle, App, AppContext, Global};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Preparing,
    Ready,
    Failed(String),
}
struct Entry {
    root: AnyWindowHandle,
    phase: Phase,
}
#[derive(Default)]
struct State(Vec<Entry>);
struct CloseState(Rc<RefCell<State>>);
impl Global for CloseState {}

/// Claim before release_host: duplicate/reentrant native requests cannot release
/// the host twice or start another task. Failed entries deliberately stay sealed.
pub(super) fn claim(root: AnyWindowHandle, cx: &mut App) -> bool {
    if crate::quit::is_quitting() {
        return false;
    }
    if !cx.has_global::<CloseState>() {
        cx.set_global(CloseState(Rc::default()));
    }
    let mut state = cx.global::<CloseState>().0.borrow_mut();
    if state
        .0
        .iter()
        .any(|entry| entry.root.window_id() == root.window_id())
    {
        return false;
    }
    state.0.push(Entry {
        root,
        phase: Phase::Preparing,
    });
    true
}

fn observe(phase: &mut Phase, result: Result<DownloadRetirementPreparation, String>) -> bool {
    if *phase != Phase::Preparing {
        return false;
    }
    match result {
        Ok(DownloadRetirementPreparation::Ready) => {
            *phase = Phase::Ready;
            true
        }
        Ok(DownloadRetirementPreparation::Pending) => false,
        Err(error) => {
            *phase = Phase::Failed(error);
            false
        }
    }
}

pub(super) fn prepare(root: AnyWindowHandle, cx: &mut App) {
    prepare_with(root, cx, Engine::prepare_download_retirements, Engine::pump);
}
fn prepare_with(
    root: AnyWindowHandle,
    cx: &mut App,
    prepare: fn() -> Result<DownloadRetirementPreparation, String>,
    pump: fn(),
) {
    let state = cx.global::<CloseState>().0.clone();
    cx.spawn(async move |cx| {
        loop {
            // App::spawn enqueues this task on the main executor; it never
            // polls inline. That boundary ends release_host's borrows without
            // delaying an ordinary Ready close on a timer.
            if crate::quit::is_quitting() {
                break;
            }
            let result = prepare(); // No App, coordinator, Pages or Engine borrow.
            let (ready, failure) = {
                let mut state = state.borrow_mut();
                let Some(entry) = state
                    .0
                    .iter_mut()
                    .find(|entry| entry.root.window_id() == root.window_id())
                else {
                    break;
                };
                let ready = observe(&mut entry.phase, result);
                let failure = if let Phase::Failed(error) = &entry.phase {
                    Some(error.clone())
                } else {
                    None
                };
                (ready, failure)
            };
            if let Some(error) = failure {
                eprintln!("[window-close] parent retained: {error}");
                break; // Errors are never Pending, retries, or cancellation.
            }
            if crate::quit::is_quitting() {
                break;
            }
            if ready {
                let _ = cx.update(|cx| {
                    if crate::quit::is_quitting() {
                        return;
                    }
                    if let Some(handle)=root.downcast::<crate::window::BrowserWindow>() {
                        let _=handle.update(cx, |view,_,_|view.forget_restoration());
                    }
                    let _ = root.update(cx, |_, window, _| window.remove_window());
                });
                break;
            }
            // Only explicit Pending authorizes another CEF pass. Do not drain
            // download events here: the central probe/model owns that stream.
            pump();
            cx.background_executor()
                .timer(Duration::from_millis(7))
                .await;
        }
    })
    .detach();
}

#[cfg(test)]
#[path = "window_close_tests.rs"]
mod tests;
