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
        process_id: None,
        network_bytes_per_second: None,
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
            sample(3, "GPU Process", TaskKind::Gpu, 400),
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

#[derive(Clone)]
struct FakeSampler(std::rc::Rc<std::cell::RefCell<(Vec<TaskRecord>, Vec<i64>)>>);
impl engine::TaskSampler for FakeSampler {
    fn snapshot(&self) -> Result<Vec<TaskRecord>, String> {
        Ok(self.0.borrow().0.clone())
    }
    fn end(&self, id: i64) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        if !state.0.iter().any(|r| r.id == id && r.killable) {
            return Err("stale task".into());
        }
        state.1.push(id);
        Ok(())
    }
}
#[gpui::test]
fn task_manager_consumes_fake_process_metrics_and_keeps_shared_process_rows_together(
    cx: &mut TestAppContext,
) {
    cx.update(crate::text_input::bind_keys);
    let mut first = sample(10, "Tab: One", TaskKind::Renderer, 900);
    first.process_id = Some(700);
    first.cpu_percent = Some(245.5); // CEF reports CPU across cores; do not clamp to 100.
    first.network_bytes_per_second = Some(2048);
    let mut shared = first.clone();
    shared.id = 11;
    shared.title = "Extension: Shared process".into();
    shared.kind = TaskKind::Extension;
    let mut utility = sample(20, "Utility: Network Service", TaskKind::Utility, 500);
    utility.process_id = Some(900);
    utility.network_bytes_per_second = Some(4096);
    let state = std::rc::Rc::new(std::cell::RefCell::new((
        vec![first, utility, shared],
        Vec::new(),
    )));
    let sampler = FakeSampler(state.clone());
    let (view, cx) = cx.add_window_view(|window, cx| {
        TaskManagerWindow::with_sampler(window, cx, Some(Box::new(sampler)))
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, cx| v
            .visible(cx)
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()),
        vec![10, 11, 20]
    );
    assert_eq!(
        view.read_with(cx, |v, _| (
            v.rows[0].cpu_percent,
            v.rows[0].network_bytes_per_second,
            v.rows[0].process_id
        )),
        (Some(245.5), Some(2048), Some(700))
    );
    view.update(cx, |v, cx| {
        v.column = Column::Network;
        v.descending = true;
        cx.notify();
    });
    assert_eq!(
        view.read_with(cx, |v, cx| v
            .visible(cx)
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()),
        vec![20, 10, 11]
    );
    view.update(cx, |v, cx| {
        v.category = Category::Browser;
        cx.notify();
    });
    assert_eq!(
        view.read_with(cx, |v, cx| v
            .visible(cx)
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()),
        vec![20]
    );
    view.update(cx, |v, cx| {
        v.category = Category::Tabs;
        cx.notify();
    });
    assert_eq!(
        view.read_with(cx, |v, cx| v
            .visible(cx)
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()),
        vec![10, 11]
    );
    view.update(cx, |v, cx| {
        v.selected = Some(11);
        v.end_selected(cx);
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(state.borrow().1, vec![11]);
    state.borrow_mut().0.retain(|r| r.id == 20);
    state.borrow_mut().0[0].network_bytes_per_second = Some(0);
    state.borrow_mut().0[0].cpu_percent = None;
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.selected), None);
    assert_eq!(
        view.read_with(cx, |v, _| (
            v.rows[0].network_bytes_per_second,
            v.rows[0].cpu_percent
        )),
        (Some(0), None)
    );
}
