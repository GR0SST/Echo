use super::*;
use gpui::{Modifiers, TestAppContext};
fn sample(id: i64, title: &str, kind: TaskKind, memory: u64) -> TaskRecord {
    TaskRecord {
        id,
        title: title.into(),
        kind,
        killable: kind != TaskKind::Browser,
        memory_bytes: Some(memory),
        cpu_percent: Some(0.),
    }
}
#[gpui::test]
fn task_manager_filters_searches_and_drops_stale_selection(cx: &mut TestAppContext) {
    cx.update(crate::text_input::bind_keys);
    cx.update(super::init);
    let (view, cx) = cx.add_window_view(|window, cx| TaskManagerWindow::new(window, cx));
    cx.simulate_resize(size(px(1043.), px(838.)));
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        view.replace(vec![
            sample(1, "Browser", TaskKind::Browser, 900),
            sample(2, "Tab: Local fixture", TaskKind::Renderer, 500),
            sample(3, "GPU Process", TaskKind::Other, 400),
        ]);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view
            .visible(cx)
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()),
        vec![1, 2, 3]
    );
    let bounds = cx.debug_bounds("tasks-category-Tabs & extensions").unwrap();
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view
            .visible(cx)
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()),
        vec![2]
    );
    let bounds = cx.debug_bounds("task-2").unwrap();
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, _| view.selected), Some(2));
    let bounds = cx.debug_bounds("end-task").unwrap();
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(view.update(cx, |view, _| view.pending_end.take()), Some(2));

    view.update(cx, |view, cx| {
        view.search
            .update(cx, |input, cx| input.set_text("absent", cx));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, cx| view.visible(cx).is_empty()));
    let bounds = cx.debug_bounds("end-task").unwrap();
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, _| view.pending_end), None);

    view.update(cx, |view, _| {
        view.replace(vec![sample(1, "Browser", TaskKind::Browser, 900)])
    });
    assert_eq!(view.read_with(cx, |view, _| view.selected), None);
    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    assert!(cx.cx.update(|cx| cx
        .windows()
        .iter()
        .all(|handle| handle.downcast::<TaskManagerWindow>().is_none())));
}
