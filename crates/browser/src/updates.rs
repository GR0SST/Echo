//! App-owned updater; no model/engine borrow crosses network or installer work.
use gpui::{actions, div, prelude::*, px, rgb, App, Global, WindowOptions};
use std::{sync::Arc, time::Duration};
actions!(updates, [CheckForUpdates, RestartAndUpdate, CloseUpdate]);
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Status {
    #[default]
    Idle,
    Checking,
    Current,
    Ready(String),
    Failed,
    Downloads,
}
#[derive(Default)]
pub(crate) struct Updates {
    pub status: Status,
    ready: Option<Arc<echo_updater::Ready>>,
    restart: bool,
}
impl Global for Updates {}
pub(crate) fn init(cx: &mut App) {
    cx.set_global(Updates::default());
    cx.bind_keys([
        gpui::KeyBinding::new("escape", CloseUpdate, Some("UpdateWindow")),
        gpui::KeyBinding::new("cmd-w", CloseUpdate, Some("UpdateWindow")),
    ]);
    cx.on_action(|_: &CheckForUpdates, cx| {
        check(cx);
        show(cx);
    });
    cx.on_action(|_: &RestartAndUpdate, cx| restart(cx));
    #[cfg(not(test))]
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_secs(30))
            .await;
        loop {
            if cx
                .update(|cx| {
                    if !crate::quit::is_quitting() {
                        check(cx);
                    }
                })
                .is_err()
            {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_secs(6 * 3600))
                .await;
        }
    })
    .detach();
}
pub(crate) fn check(cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    let state = cx.global_mut::<Updates>();
    if state.status == Status::Checking || state.ready.is_some() {
        return;
    }
    state.status = Status::Checking;
    cx.refresh_windows();
    #[cfg(not(test))]
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async { echo_updater::prepare() })
            .await;
        let _ = cx.update(|cx| {
            if crate::quit::is_quitting() {
                return;
            }
            let state = cx.global_mut::<Updates>();
            state.status = match result {
                Ok(Some(ready)) => {
                    let version = ready.version().to_owned();
                    state.ready = Some(Arc::new(ready));
                    Status::Ready(version)
                }
                Ok(None) => Status::Current,
                Err(error) => {
                    eprintln!("Update check failed: {error}");
                    Status::Failed
                }
            };
            cx.refresh_windows();
        });
    })
    .detach();
}
pub(crate) fn restart(cx: &mut App) {
    if crate::quit::is_quitting() || cx.global::<Updates>().ready.is_none() {
        return;
    }
    // Conservative: includes queued callbacks, paused transfers, and retiring
    // download owners. Never cancel a user's download to apply an update.
    if engine::Engine::download_work_pending() {
        cx.global_mut::<Updates>().status = Status::Downloads;
        cx.refresh_windows();
        return;
    }
    cx.global_mut::<Updates>().restart = true;
    crate::quit::request(cx);
}
/// Called only after sessions, cookies, engine owners and Downloads have closed.
/// Failure pauses quit using the existing recovery/diagnostic path.
pub(crate) fn arm_after_shutdown(cx: &mut App) -> Result<(), String> {
    let Some(state) = cx.try_global::<Updates>() else {
        return Ok(());
    };
    if state.restart {
        state
            .ready
            .as_ref()
            .ok_or("Update is no longer ready")?
            .arm_after_shutdown()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn label(cx: &App) -> String {
    match cx.try_global::<Updates>().map(|s| &s.status) {
        Some(Status::Checking) => "Checking for updates…".into(),
        Some(Status::Current) => "Your browser is up to date".into(),
        Some(Status::Ready(_)) => "Restart and Update".into(),
        Some(Status::Downloads) => "Finish downloads, then Restart and Update".into(),
        Some(Status::Failed) => "Couldn’t check for updates. Try again.".into(),
        _ => String::new(),
    }
}
pub(crate) fn banner_visible(cx: &App) -> bool {
    cx.try_global::<Updates>()
        .is_some_and(|s| matches!(s.status, Status::Ready(_) | Status::Downloads))
}
pub(crate) fn banner(cx: &App) -> gpui::AnyElement {
    let text = label(cx);
    let visible = cx
        .try_global::<Updates>()
        .is_some_and(|s| matches!(s.status, Status::Ready(_) | Status::Downloads));
    div()
        .when(visible, |d| {
            d.child(
                div()
                    .id("update-banner")
                    .debug_selector(|| "update-banner".into())
                    .mx(px(8.))
                    .mb(px(6.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .bg(rgb(0xc9e4ff))
                    .text_color(rgb(0x183958))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0xb9dafb)))
                    .on_click(|_, _, cx| restart(cx))
                    .child(text),
            )
        })
        .into_any_element()
}
pub(crate) struct UpdateWindow {
    focus: gpui::FocusHandle,
}
impl gpui::Focusable for UpdateWindow {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus.clone()
    }
}
impl gpui::Render for UpdateWindow {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let ready = cx.global::<Updates>().ready.is_some();
        div()
            .key_context("UpdateWindow")
            .track_focus(&self.focus)
            .on_action(|_: &CloseUpdate, window, _| window.remove_window())
            .size_full()
            .relative()
            .bg(rgb(0x464a55))
            .text_color(rgb(0xe4e5e7))
            .child(
                gpui::img(Arc::new(gpui::Image::from_bytes(
                    gpui::ImageFormat::Png,
                    include_bytes!("../assets/echo-icon.png").to_vec(),
                )))
                .absolute()
                .left(px(109.))
                .top(px(69.))
                .size(px(52.)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(150.))
                    .w_full()
                    .text_center()
                    .line_height(px(30.))
                    .text_size(px(25.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Echo"),
            )
            .child(
                div()
                    .absolute()
                    .top(px(177.))
                    .w_full()
                    .text_center()
                    .line_height(px(16.))
                    .text_size(px(12.))
                    .child(
                        option_env!("ECHO_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top(px(210.))
                    .left(px(15.))
                    .w(px(240.))
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .text_center()
                    .child(label(cx)),
            )
            .child(
                div()
                    .id("update-action")
                    .debug_selector(|| "update-action".into())
                    .absolute()
                    .left(px(50.))
                    .top(px(256.))
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .w(px(170.))
                    .h(px(48.))
                    .rounded(px(6.))
                    .bg(rgb(0x000000))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        if ready {
                            restart(cx);
                        } else {
                            window.remove_window();
                        }
                    })
                    .child(if ready { "Restart and Update" } else { "Close" }),
            )
    }
}
fn show(cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    for handle in cx.windows() {
        if let Some(handle) = handle.downcast::<UpdateWindow>() {
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
            window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                None,
                gpui::size(px(270.), px(350.)),
                cx,
            ))),
            titlebar: Some(gpui::TitlebarOptions {
                appears_transparent: true,
                ..Default::default()
            }),
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        },
        |window, cx| {
            cx.new(|cx| {
                let view = UpdateWindow {
                    focus: cx.focus_handle(),
                };
                view.focus.focus(window);
                view
            })
        },
    );
}
