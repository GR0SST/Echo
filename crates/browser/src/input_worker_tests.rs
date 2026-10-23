use super::*;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[test]
fn delayed_worker_does_not_finish_until_its_up_was_actually_enqueued() {
    let gate = PostGate::new();
    let sent = Arc::new(Mutex::new(Vec::new()));
    assert!(gate.post(|| sent.lock().unwrap().push("down")));
    let (allow_up, wait_up) = mpsc::channel();
    let posted = sent.clone();
    let worker = spawn(gate.clone(), move |gate| {
        wait_up.recv_timeout(Duration::from_secs(3)).unwrap();
        gate.post(|| posted.lock().unwrap().push("up"))
    });
    // No duration is completion. The worker is deliberately unable to post Up.
    assert_eq!(gate.status(), (true, Completion::Pending));
    assert_eq!(*sent.lock().unwrap(), ["down"]);
    allow_up.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(*sent.lock().unwrap(), ["down", "up"]);
    assert_eq!(gate.status(), (true, Completion::Posted));
}
#[test]
fn revoke_waits_for_already_admitted_enqueue_before_native_parent_can_drop() {
    let gate = PostGate::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    let (admitted, entered) = mpsc::channel();
    let (allow_post, wait_post) = mpsc::channel();
    let post_gate = gate.clone();
    let post_log = log.clone();
    let worker = std::thread::spawn(move || {
        post_gate.post(|| {
            // Pause precisely AFTER admission and BEFORE the event is queued.
            admitted.send(()).unwrap();
            wait_post.recv_timeout(Duration::from_secs(3)).unwrap();
            post_log.lock().unwrap().push("queued");
        })
    });
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    // Deterministic proof that admission still owns the serialization lock.
    assert!(matches!(
        gate.0.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    let close_gate = gate.clone();
    let close_log = log.clone();
    let (revoke_started, started) = mpsc::channel();
    let (revoked, done) = mpsc::channel();
    let closer = std::thread::spawn(move || {
        revoke_started.send(()).unwrap();
        close_gate.revoke();
        close_log.lock().unwrap().push("revoked-before-parent-drop");
        revoked.send(()).unwrap();
    });
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(matches!(
        done.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    allow_post.send(()).unwrap();
    assert!(worker.join().unwrap());
    done.recv_timeout(Duration::from_secs(3)).unwrap();
    closer.join().unwrap();
    assert_eq!(
        *log.lock().unwrap(),
        ["queued", "revoked-before-parent-drop"]
    );
    assert!(!gate.post(|| panic!("a not-yet-queued event crossed completed revocation")));
}
#[test]
fn revocation_wins_before_admission_and_worker_reports_cancellation() {
    let gate = PostGate::new();
    gate.revoke();
    let worker = spawn(gate.clone(), |gate| gate.post(|| panic!("late enqueue")));
    worker.join().unwrap();
    assert_eq!(gate.status(), (false, Completion::Cancelled));
}
