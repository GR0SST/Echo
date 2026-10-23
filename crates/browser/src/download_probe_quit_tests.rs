//! Native-effect driver regression: quit may seal from inside any engine call.
use super::*;
use gpui::TestAppContext;

#[gpui::test]
fn quit_reentered_from_drain_prevents_all_following_native_effects(cx: &mut TestAppContext) {
    cx.update(crate::quit::reset_for_test);
    let log = RefCell::new(Vec::new());
    let drained = while_running(|| {
        log.borrow_mut().push("drain");
        crate::quit::seal();
        vec![1, 2]
    })
    .unwrap();
    let running = apply_native_effects(
        drained,
        [3, 4],
        |_| log.borrow_mut().push("destination"),
        |_| log.borrow_mut().push("control"),
        || log.borrow_mut().push("prepare"),
    );
    assert!(!running);
    assert_eq!(*log.borrow(), ["drain"]);
    cx.update(crate::quit::reset_for_test);
}
#[gpui::test]
fn quit_inside_destination_or_control_stops_next_item_and_later_stages(cx: &mut TestAppContext) {
    for seal_at in [
        "destination-1",
        "destination-2",
        "control-1",
        "control-2",
        "prepare",
    ] {
        cx.update(crate::quit::reset_for_test);
        let log = RefCell::new(Vec::new());
        let effect = |name: String| {
            log.borrow_mut().push(name.clone());
            if name == seal_at {
                crate::quit::seal();
            }
        };
        assert!(!apply_native_effects(
            [1, 2],
            [1, 2],
            |id| effect(format!("destination-{id}")),
            |id| effect(format!("control-{id}")),
            || effect("prepare".into())
        ));
        let expected = [
            "destination-1",
            "destination-2",
            "control-1",
            "control-2",
            "prepare",
        ];
        let end = expected.iter().position(|name| *name == seal_at).unwrap();
        assert_eq!(*log.borrow(), expected[..=end]);
    }
    cx.update(crate::quit::reset_for_test);
}
#[gpui::test]
fn unsealed_effects_complete_in_order_and_borrow_nothing_across_native_work(
    cx: &mut TestAppContext,
) {
    cx.update(crate::quit::reset_for_test);
    let log = RefCell::new(Vec::new());
    assert!(apply_native_effects(
        [1, 2],
        [1, 2],
        |id| log.borrow_mut().push(format!("destination-{id}")),
        |id| log.borrow_mut().push(format!("control-{id}")),
        || log.borrow_mut().push("prepare".into())
    ));
    assert_eq!(
        *log.borrow(),
        [
            "destination-1",
            "destination-2",
            "control-1",
            "control-2",
            "prepare"
        ]
    );
}
