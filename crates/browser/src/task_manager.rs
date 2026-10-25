//! GPUI task manager measured from the installed Arc utility window.
use crate::{
    icons::{self, Icon},
    text_input::{Changed, TextInput},
};
use engine::{TaskKind, TaskRecord};
use gpui::{
    div, point, prelude::*, px, rgb, size, App, Bounds, Context, Entity, FocusHandle, Focusable,
    KeyBinding, Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use std::time::Duration;
gpui::actions!(task_manager, [OpenTaskManager, CloseTaskManager]);
#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Tabs,
    Browser,
    All,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    Task,
    Memory,
    Cpu,
}

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "cmd-w",
        CloseTaskManager,
        Some("TaskManager"),
    )]);
    cx.on_action(|_: &OpenTaskManager, cx| cx.defer(open));
}
pub(crate) fn open(cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    for handle in cx.windows() {
        if let Some(handle) = handle.downcast::<TaskManagerWindow>() {
            let _ = handle.update(cx, |view, window, cx| {
                window.activate_window();
                view.focus.focus(window);
                cx.notify();
            });
            return;
        }
    }
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1043.), px(838.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Task Manager".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(9.), px(9.))),
            }),
            window_min_size: Some(size(px(680.), px(400.))),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| TaskManagerWindow::new(window, cx)),
    );
}
pub(crate) struct TaskManagerWindow {
    focus: FocusHandle,
    search: Entity<TextInput>,
    _change: Subscription,
    rows: Vec<TaskRecord>,
    selected: Option<i64>,
    pending_end: Option<i64>,
    category: Category,
    column: Column,
    descending: bool,
}
impl TaskManagerWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextInput::new("Search", cx));
        let change = cx.subscribe(&search, |_, _, _: &Changed, cx| cx.notify());
        let focus = cx.focus_handle();
        focus.focus(window);
        cx.spawn(async move |view, cx| {
            let mut monitor = None;
            loop {
                if crate::quit::is_quitting() {
                    break;
                }
                let Ok(end) = view.update(cx, |view, _| view.pending_end.take()) else {
                    break;
                };
                if monitor.is_none() {
                    monitor = engine::Engine::task_monitor().ok();
                }
                if let (Some(monitor), Some(id)) = (&monitor, end) {
                    let _ = monitor.end(id);
                }
                let rows = monitor
                    .as_ref()
                    .and_then(|monitor| monitor.snapshot().ok())
                    .unwrap_or_default();
                if view
                    .update(cx, |view, cx| {
                        view.replace(rows);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();
        Self {
            focus,
            search,
            _change: change,
            rows: Vec::new(),
            selected: None,
            pending_end: None,
            category: Category::All,
            column: Column::Memory,
            descending: true,
        }
    }
    fn replace(&mut self, rows: Vec<TaskRecord>) {
        if self
            .selected
            .is_some_and(|id| !rows.iter().any(|row| row.id == id))
        {
            self.selected = None;
        }
        if self.rows.is_empty() && self.selected.is_none() {
            self.selected = rows.first().map(|row| row.id);
        }
        self.rows = rows;
    }
    fn visible(&self, cx: &App) -> Vec<TaskRecord> {
        let query = self.search.read(cx).text().to_lowercase();
        let mut rows: Vec<_> = self
            .rows
            .iter()
            .filter(|row| {
                let category = match self.category {
                    Category::All => true,
                    Category::Browser => matches!(row.kind, TaskKind::Browser | TaskKind::Other),
                    Category::Tabs => matches!(
                        row.kind,
                        TaskKind::Renderer | TaskKind::Extension | TaskKind::Worker
                    ),
                };
                category && row.title.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        rows.sort_by(|a, b| {
            let order = match self.column {
                Column::Task => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                Column::Memory => a.memory_bytes.cmp(&b.memory_bytes),
                Column::Cpu => a
                    .cpu_percent
                    .unwrap_or(-1.)
                    .total_cmp(&b.cpu_percent.unwrap_or(-1.)),
            };
            if self.descending {
                order.reverse()
            } else {
                order
            }
        });
        rows
    }
    fn end_selected(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.filter(|id| {
            self.visible(cx)
                .iter()
                .any(|row| row.id == *id && row.killable)
        }) else {
            return;
        };
        self.pending_end = Some(id);
        cx.notify();
    }
}
impl Focusable for TaskManagerWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for TaskManagerWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.visible(cx);
        let can_end = self
            .selected
            .is_some_and(|id| rows.iter().any(|r| r.id == id && r.killable));
        let cell = |width: f32, label: String| {
            div()
                .w(px(width))
                .flex_none()
                .px(px(12.))
                .overflow_hidden()
                .text_ellipsis()
                .text_right()
                .child(label)
        };
        div()
            .id("task-manager")
            .debug_selector(|| "task-manager".into())
            .key_context("TaskManager")
            .track_focus(&self.focus)
            .on_action(|_: &CloseTaskManager, window, _| window.remove_window())
            .size_full()
            .bg(rgb(0x1f1f1f))
            .text_color(rgb(0xe3e3e3))
            .font_family("Arial")
            .text_size(px(13.))
            .relative()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(33.))
                    .bg(rgb(0x303030))
                    .child(
                        div()
                            .ml(px(84.))
                            .mt(px(7.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0xb5b5b5))
                            .font_family(".SystemUIFont")
                            .child("Task Manager"),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top(px(33.))
                    .left(px(20.))
                    .right(px(20.))
                    .h(px(53.))
                    .border_b_2()
                    .border_color(rgb(0x616161))
                    .flex()
                    .items_center()
                    .children(
                        [
                            (Category::Tabs, "Tabs & extensions", Icon::NewTab),
                            (Category::Browser, "Browser", Icon::Reload),
                            (Category::All, "All tasks", Icon::Library),
                        ]
                        .map(|(category, label, icon)| {
                            div()
                                .id(label)
                                .debug_selector(move || format!("tasks-category-{label}"))
                                .h_full()
                                .px(px(20.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .border_b_2()
                                .border_color(if self.category == category {
                                    rgb(0xa8c7fa)
                                } else {
                                    rgb(0x1f1f1f)
                                })
                                .text_color(if self.category == category {
                                    rgb(0xa8c7fa)
                                } else {
                                    rgb(0xc4c4c4)
                                })
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .child(icons::control_sized(icon, 16.))
                                .child(label)
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.category = category;
                                    cx.notify();
                                }))
                        }),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .w(px(286.))
                            .h(px(36.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(icons::control_sized(Icon::Search, 16.))
                            .child(self.search.clone()),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top(px(102.))
                    .left(px(20.))
                    .right(px(20.))
                    .bottom(px(80.))
                    .rounded(px(8.))
                    .bg(rgb(0x323336))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(44.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .border_b_1()
                            .border_color(rgb(0x626268))
                            .children(
                                [
                                    (Column::Task, "Task", 556.),
                                    (Column::Memory, "Memory Footprint", 158.),
                                    (Column::Cpu, "CPU", 74.),
                                ]
                                .map(|(column, label, width)| {
                                    div()
                                        .id(label)
                                        .when(column == Column::Task, |v| v.flex_1().min_w_0())
                                        .when(column != Column::Task, |v| {
                                            v.w(px(width)).flex_none()
                                        })
                                        .px(px(12.))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .child(label)
                                        .when(self.column == column, |v| {
                                            v.child(icons::chevron(!self.descending))
                                        })
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            if view.column == column {
                                                view.descending = !view.descending;
                                            } else {
                                                view.column = column;
                                                view.descending = true;
                                            }
                                            cx.notify();
                                        }))
                                }),
                            )
                            .child(cell(100., "Network".into()))
                            .child(cell(114., "Process ID".into())),
                    )
                    .child(div().id("task-list").flex_1().overflow_y_scroll().children(
                        rows.into_iter().map(|row| {
                            let id = row.id;
                            let selected = self.selected == Some(id);
                            div()
                                .id(gpui::ElementId::Name(format!("task-{id}").into()))
                                .debug_selector(move || format!("task-{id}"))
                                .h(px(30.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .when(selected, |v| v.bg(rgb(0x004f78)))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.selected = Some(id);
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .pl(px(8.))
                                        .pr(px(8.))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(
                                            div()
                                                .size(px(24.))
                                                .rounded(px(4.))
                                                .bg(rgb(0x1f1f1f))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(icons::control_sized(
                                                    if row.kind == TaskKind::Browser {
                                                        Icon::Info
                                                    } else {
                                                        Icon::NewTab
                                                    },
                                                    16.,
                                                )),
                                        )
                                        .child(
                                            div()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .child(row.title),
                                        ),
                                )
                                .child(cell(
                                    158.,
                                    row.memory_bytes
                                        .map(|v| format!("{} MB", v / (1024 * 1024)))
                                        .unwrap_or_else(|| "–".into()),
                                ))
                                .child(cell(
                                    74.,
                                    row.cpu_percent
                                        .map(|v| format!("{v:.1}"))
                                        .unwrap_or_else(|| "–".into()),
                                ))
                                .child(cell(100., "–".into()))
                                .child(cell(114., "–".into()))
                        }),
                    )),
            )
            .child(
                div()
                    .id("end-task")
                    .debug_selector(|| "end-task".into())
                    .absolute()
                    .right(px(20.))
                    .bottom(px(20.))
                    .h(px(36.))
                    .px(px(18.))
                    .rounded(px(20.))
                    .flex()
                    .items_center()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .bg(rgb(if can_end { 0xa8c7fa } else { 0x303030 }))
                    .text_color(rgb(if can_end { 0x062e6f } else { 0x747474 }))
                    .when(can_end, |v| v.cursor_pointer())
                    .child("End Task")
                    .on_click(cx.listener(|view, _, _, cx| view.end_selected(cx))),
            )
    }
}

#[cfg(test)]
#[path = "task_manager_tests.rs"]
mod tests;
