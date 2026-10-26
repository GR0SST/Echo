//! Arc's local quit confirmation, before any shutdown capability is sealed.
use crate::{profile::ProfileHandle, window::BrowserWindow};
use browser_core::settings::Toggle;
use gpui::{
    div, prelude::*, px, rgb, rgba, App, Context, FocusHandle, KeyBinding, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};

gpui::actions!(quit_confirmation, [Cancel, Accept]);
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some("QuitConfirmation")),
        KeyBinding::new("enter", Accept, Some("QuitConfirmation")),
    ]);
}

pub(crate) fn request(cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    for handle in cx.windows() {
        if let Some(handle) = handle.downcast::<Confirmation>() {
            let _ = handle.update(cx, |view, window, cx| {
                window.activate_window();
                view.focus.focus(window);
                cx.notify();
            });
            return;
        }
    }
    let active = cx.active_window();
    let settings_owner = active
        .and_then(|window| window.downcast::<crate::settings::SettingsWindow>())
        .and_then(|settings| settings.read(cx).ok()?.browser_owner(cx));
    let overlay = active.and_then(|w| w.downcast::<crate::window::Overlay>());
    let overlay_owner = overlay.and_then(|overlay| {
        cx.windows().into_iter().find_map(|window| {
            let handle = window.downcast::<BrowserWindow>()?;
            let identity = handle.update(cx, |_, _, cx| cx.entity().entity_id()).ok()?;
            overlay
                .read(cx)
                .is_ok_and(|overlay| overlay.belongs_to(identity))
                .then_some(handle)
        })
    });
    let owner = active
        .and_then(|w| w.downcast::<BrowserWindow>())
        .or(overlay_owner)
        .or(settings_owner)
        .or_else(|| {
            cx.windows()
                .into_iter()
                .find_map(|w| w.downcast::<BrowserWindow>())
        });
    let (mut profile, window_local, parent, bounds) = if let Some(owner) = owner {
        let Ok(state) = owner.update(cx, |view, window, _| {
            (
                view.profile.handle.clone(),
                view.browser.is_private() || view.browser.is_blank(),
                crate::native::view_of(window),
                window.bounds(),
            )
        }) else { return; };
        state
    } else {
        let Ok(normal) = crate::new_window::normal_source(cx) else {
            // Without a readable install preference, preserve the established
            // explicit-quit policy rather than leave a suspended OS request.
            crate::quit::request(cx);
            return;
        };
        (normal.profile, false, None,
            gpui::Bounds::centered(None, gpui::size(px(450.), px(248.)), cx))
    };
    if window_local {
        if let Ok(normal) = crate::new_window::normal_source(cx) {
            profile = normal.profile;
        }
    }
    if !profile.read_settings(|b| b.settings.enabled(Toggle::WarnBeforeQuitting)) {
        crate::quit::request(cx);
        return;
    }
    if let Some(owner) = owner {
        let _ = owner.update(cx, |view, window, cx| {
            view.close_command_bar(cx);
            view.close_site_controls(cx);
            view.close_find(cx);
            view.close_tab_menu(cx);
            if let Some(pages) = &view.pages {
                pages.borrow_mut().request_ui_focus();
            }
            window.activate_window();
        });
    }
    let confirmation = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            kind: WindowKind::Normal,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        move |window, cx| {
            let native = crate::native::view_of(window);
            if let (Some(parent), Some(child)) = (parent, native) {
                crate::native::attach_overlay(parent, child, false);
            }
            cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window);
                Confirmation {
                    owner,
                    profile,
                    focus,
                    native,
                }
            })
        },
    );
    if let Ok(confirmation) = confirmation {
        let _ = confirmation.update(cx, |view, window, cx| {
            window.activate_window();
            view.focus.focus(window);
            cx.on_next_frame(window, |view: &mut Confirmation, window, _| {
                window.activate_window();
                view.focus.focus(window);
            });
        });
    }
}

pub(crate) fn close_all(cx: &mut App) {
    for handle in cx.windows() {
        if let Some(handle) = handle.downcast::<Confirmation>() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }
}

pub(crate) struct Confirmation {
    owner: Option<WindowHandle<BrowserWindow>>,
    profile: ProfileHandle,
    focus: FocusHandle,
    native: Option<*mut std::ffi::c_void>,
}
impl Drop for Confirmation {
    fn drop(&mut self) {
        if let Some(native) = self.native {
            crate::native::forget_overlay(native);
        }
    }
}
impl Confirmation {
    #[cfg(test)]
    pub(crate) fn owner_id(&self) -> gpui::WindowId {
        self.owner.expect("confirmation has a browser owner").window_id()
    }
    fn finish(
        &mut self,
        accept: bool,
        remember: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if remember {
            self.profile
                .edit_settings(|b| b.settings.set_enabled(Toggle::WarnBeforeQuitting, false));
        }
        window.remove_window();
        if accept {
            cx.defer(crate::quit::request);
        } else {
            #[cfg(all(target_os = "macos", not(test)))]
            match crate::native::termination::prepare_cancel() {
                Ok(pending) => {
                    cx.spawn(async move |_, _| {
                        if let Err(error) = pending.finish() {
                            eprintln!("[quit-native] Cancellation failed: {error}");
                        }
                    }).detach();
                }
                Err(error) => eprintln!("[quit-native] Cancellation refused: {error}"),
            }
            let owner = self.owner;
            cx.defer(move |cx| {
                let Some(owner) = owner else { return; };
                let _ = owner.update(cx, |view, window, cx| {
                    window.activate_window();
                    if let Some(pages) = &view.pages {
                        pages.borrow_mut().request_page_focus();
                    }
                    cx.notify();
                });
            });
        }
    }
}
impl Render for Confirmation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let button = |label: &'static str, selector: &'static str, width: f32, primary: bool| {
            div()
                .id(selector)
                .debug_selector(move || selector.into())
                .h(px(37.))
                .w(px(width))
                .flex_none()
                .rounded(px(6.))
                .border_1()
                .border_color(rgb(if primary { 0x474df2 } else { 0x676099 }))
                .bg(rgb(if primary { 0x3339f1 } else { 0x302f60 }))
                .text_color(rgb(0xffffff))
                .text_size(px(13.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .hover(|s| s.bg(rgb(if primary { 0x4840ff } else { 0x443e75 })))
                .cursor_pointer()
                .child(label)
        };
        let key = |text: &'static str| {
            div()
                .h(px(20.))
                .w(px(35.))
                .rounded(px(4.))
                .bg(rgb(0x4e4c77))
                .text_color(rgb(0xc4c2d7))
                .text_size(px(10.))
                .font_weight(gpui::FontWeight::BOLD)
                .flex()
                .items_center()
                .justify_center()
                .child(text)
        };
        div()
            .id("quit-confirmation")
            .debug_selector(|| "quit-confirmation".into())
            .key_context("QuitConfirmation")
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &Cancel, window, cx| this.finish(false, false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Accept, window, cx| this.finish(true, false, window, cx)),
            )
            .font_family(".SystemUIFont")
            .size_full()
            .bg(rgba(0x17181899))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .debug_selector(|| "quit-confirmation-card".into())
                    .w(px(450.))
                    .h(px(248.))
                    .rounded(px(17.))
                    .bg(rgb(0x151c30))
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .left(px(44.))
                            .top(px(44.))
                            .size(px(50.))
                            .child(
                                gpui::img(crate::app_icons::preview_image(
                                    self.profile.read_settings(|b| b.settings.app_icon),
                                    cx,
                                ))
                                .size(px(50.)),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(38.))
                            .top(px(116.))
                            .text_size(px(18.))
                            .line_height(px(22.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0xffffff))
                            .child("Quit Echo?"),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(28.))
                            .bottom(px(27.))
                            .flex()
                            .gap(px(16.))
                            .child(
                                button("Quit, and don’t ask again", "quit-dont-ask", 176., false)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.finish(true, true, window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        button("Cancel", "quit-cancel", 110., false)
                                            .child(key("ESC"))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.finish(false, false, window, cx)
                                            })),
                                    )
                                    .child(
                                        button("Quit", "quit-accept", 85., true)
                                            .child(
                                                div()
                                                    .w(px(28.))
                                                    .h(px(20.))
                                                    .rounded(px(4.))
                                                    .border_1()
                                                    .border_color(rgba(0xffffff33))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(crate::icons::control_sized(
                                                        crate::icons::Icon::Enter,
                                                        18.,
                                                    )),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.finish(true, false, window, cx)
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}
