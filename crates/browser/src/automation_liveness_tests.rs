//! Runs the real automation command loop with native leaf calls intercepted.
//! Fake pointers are compared only, never sent to AppKit.
use super::*;
use gpui::{Entity, TestAppContext};
use std::{cell::RefCell, rc::Rc};

type Hook = Rc<dyn Fn(&str)>;
thread_local! { static NATIVE: RefCell<Option<Hook>> = const { RefCell::new(None) }; }
pub(super) fn native_call(name: &str) -> bool {
    let hook = NATIVE.with(|slot| slot.borrow().clone());
    if let Some(hook) = hook {
        hook(name);
        true
    } else {
        false
    }
}
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        NATIVE.with(|slot| slot.borrow_mut().take());
    }
}
fn hook(log: Rc<RefCell<Vec<String>>>) -> Reset {
    NATIVE.with(|slot| {
        *slot.borrow_mut() = Some(Rc::new(move |name| log.borrow_mut().push(name.into())))
    });
    Reset
}
fn source(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, gpui::AnyWindowHandle) {
    cx.update(crate::quit::reset_for_test);
    cx.update(|cx| {
        let root = cx
            .open_window(Default::default(), |window, cx| {
                cx.new(|cx| BrowserWindow::new(crate::demo_browser(), None, window, cx))
            })
            .unwrap();
        let entity = root.root(cx).unwrap();
        entity.update(cx, |view, _| view.native_view = Some(1usize as *mut c_void));
        (entity, root.into())
    })
}
fn remove(root: gpui::AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update(|cx| {
        root.update(cx, |_, window, _| window.remove_window())
            .unwrap()
    });
}
#[gpui::test]
fn retained_entity_is_not_a_live_native_root(cx: &mut TestAppContext) {
    let (retained, root) = source(cx);
    let weak = retained.downgrade();
    let mut app = cx.to_async();
    let mut native_calls = 0;
    assert_eq!(
        with_live_source(&weak, 1usize as *mut AnyObject, &mut app, |_| {
            native_calls += 1;
            7
        }),
        Some(7)
    );
    remove(root, cx);
    assert!(
        weak.upgrade().is_some(),
        "regression requires an Entity surviving root removal"
    );
    // Covers key/click/screenshot/activate dispatch without a focus requirement.
    for _ in [
        "key", "click", "shot", "activate", "views", "resize", "compose",
    ] {
        assert!(
            with_live_source(&weak, 1usize as *mut AnyObject, &mut app, |_| {
                native_calls += 1;
            })
            .is_none()
        );
    }
    assert_eq!(native_calls, 1);
    retained.update(cx, |view, _| view.native_view = None);
}

#[gpui::test]
fn native_dispatch_can_reenter_and_remove_root_before_next_effect(cx: &mut TestAppContext) {
    let (retained, root) = source(cx);
    let weak = retained.downgrade();
    let mut app = cx.to_async();
    let mut reentrant = cx.to_async();
    assert_eq!(
        with_live_source(&weak, 1usize as *mut AnyObject, &mut app, |_| {
            // Would panic on the shared App RefCell if the liveness check retained
            // its update borrow across the native effect.
            root.update(&mut reentrant, |_, window, _| window.remove_window())
                .unwrap();
            1
        }),
        Some(1)
    );
    assert!(weak.upgrade().is_some());
    assert!(
        with_live_source(&weak, 1usize as *mut AnyObject, &mut app, |_| panic!(
            "stale native call"
        ))
        .is_none()
    );
    retained.update(cx, |view, _| view.native_view = None);
}

fn close_during_gap(script: &str, first: &str, cx: &mut TestAppContext) {
    let (retained, root) = source(cx);
    let weak = retained.downgrade();
    let log = Rc::new(RefCell::new(Vec::new()));
    let _reset = hook(log.clone());
    cx.update(|cx| start(script.into(), weak.clone(), 1usize as *mut c_void, cx));
    cx.run_until_parked();
    assert_eq!(&*log.borrow(), &["make-key", first]);
    // Both type's 15ms and swipe's 16ms await are pending. Removing only the
    // root while retaining its Entity reproduces the reviewed stale pointer.
    remove(root, cx);
    assert!(weak.upgrade().is_some());
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(
        &*log.borrow(),
        &["make-key", first],
        "no later input, screenshot or activation may touch native state"
    );
    retained.update(cx, |view, _| view.native_view = None);
}
#[gpui::test]
fn type_rechecks_after_each_character_without_require_focus(cx: &mut TestAppContext) {
    close_during_gap("type ab; click 1 1; shot ignored.png; activate", "key", cx);
}
#[gpui::test]
fn swipe_rechecks_after_each_async_step_without_require_focus(cx: &mut TestAppContext) {
    close_during_gap(
        "swipe 5 5 100; click 1 1; shot ignored.png; activate",
        "scroll",
        cx,
    );
}
#[gpui::test]
fn held_entity_cannot_keep_drag_or_held_click_worker_lease_alive(cx: &mut TestAppContext) {
    let (retained, root) = source(cx);
    let mut app = cx.to_async();
    let lease = input_lease(&retained.downgrade(), 1usize as *mut AnyObject, &mut app).unwrap();
    let mut sends = Vec::new();
    assert!(while_input_live(&lease.gate, || sends.push("down")));
    remove(root, cx);
    assert!(retained.downgrade().upgrade().is_some());
    // These are the exact scalar dispatch gates used after worker sleeps.
    assert!(!while_input_live(&lease.gate, || sends.push("dragged")));
    assert!(!while_input_live(&lease.gate, || sends.push("up")));
    assert_eq!(sends, ["down"]);
    retained.update(cx, |view, _| view.native_view = None);
}

#[gpui::test]
fn lease_survives_old_nominal_deadlines_until_delayed_worker_up_completes(cx: &mut TestAppContext) {
    use std::sync::{mpsc, Mutex};
    let (retained, _) = source(cx);
    let mut app = cx.to_async();
    let lease = input_lease(&retained.downgrade(), 1usize as *mut AnyObject, &mut app).unwrap();
    let gate = lease.gate.clone();
    let sent = Arc::new(Mutex::new(Vec::new()));
    assert!(gate.post(|| sent.lock().unwrap().push("down")));
    let (allow_up, wait_up) = mpsc::channel();
    let recorded = sent.clone();
    let worker = input_worker::spawn(gate.clone(), move |gate| {
        wait_up.recv_timeout(Duration::from_secs(3)).unwrap();
        gate.post(|| recorded.lock().unwrap().push("up"))
    });
    let observed = Rc::new(RefCell::new(None));
    let result = observed.clone();
    cx.spawn(async move |mut cx| {
        let completion = lease.wait(&mut cx).await;
        *result.borrow_mut() = Some(completion);
        // Dropping the lease is now authorized by actual worker completion.
    })
    .detach();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(
        observed.borrow().is_none(),
        "old hold+60/500ms deadlines cannot complete the wait"
    );
    assert_eq!(gate.status(), (true, Completion::Pending));
    assert_eq!(*sent.lock().unwrap(), ["down"]);
    allow_up.send(()).unwrap();
    worker.join().unwrap();
    cx.executor().advance_clock(Duration::from_millis(5));
    cx.run_until_parked();
    assert_eq!(*observed.borrow(), Some(Completion::Posted));
    assert_eq!(*sent.lock().unwrap(), ["down", "up"]);
    assert_eq!(gate.status(), (false, Completion::Posted)); // lease dropped after Up
    retained.update(cx, |view, _| view.native_view = None);
}
