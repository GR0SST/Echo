//! Arc's local save-password request sheet. The view never owns a password.
use crate::icons::{self, Icon};
use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, Context, EventEmitter, FocusHandle, Focusable,
    Render, Window,
};
const CONTEXT: &str = "PasswordPrompt";
gpui::actions!(password_prompt, [Accept, Decline, Next, Previous, Activate]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("enter", Accept, Some(CONTEXT)),
        gpui::KeyBinding::new("escape", Decline, Some(CONTEXT)),
        gpui::KeyBinding::new("tab", Next, Some(CONTEXT)),
        gpui::KeyBinding::new("shift-tab", Previous, Some(CONTEXT)),
        gpui::KeyBinding::new("space", Activate, Some(CONTEXT)),
    ]);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Save,
    No,
    Never,
}
pub(crate) struct PasswordPrompt {
    host: String,
    focus: FocusHandle,
    buttons: [FocusHandle; 3],
    closed: bool,
}
impl EventEmitter<Decision> for PasswordPrompt {}
impl Focusable for PasswordPrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl PasswordPrompt {
    pub fn new(host: String, cx: &mut Context<Self>) -> Self {
        Self {
            host,
            focus: cx.focus_handle(),
            buttons: [cx.focus_handle(), cx.focus_handle(), cx.focus_handle()],
            closed: false,
        }
    }
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.closed = true;
        cx.notify();
    }
    fn choose(&mut self, decision: Decision, cx: &mut Context<Self>) {
        if !self.closed {
            self.closed = true;
            cx.emit(decision);
            cx.notify();
        }
    }
    fn selected(&self, window: &Window) -> Option<usize> {
        self.buttons.iter().position(|f| f.is_focused(window))
    }
    fn accept(&mut self, window: &Window, cx: &mut Context<Self>) {
        let choice = match self.selected(window) {
            Some(0) => Decision::Never,
            Some(1) => Decision::No,
            _ => Decision::Save,
        };
        self.choose(choice, cx);
    }
    fn move_focus(&self, backwards: bool, window: &mut Window) {
        let index = match self.selected(window) {
            None => {
                if backwards {
                    2
                } else {
                    0
                }
            }
            Some(i) => {
                if backwards {
                    (i + 2) % 3
                } else {
                    (i + 1) % 3
                }
            }
        };
        self.buttons[index].focus(window);
    }
    fn button(
        &self,
        label: &'static str,
        width: f32,
        index: usize,
        choice: Decision,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary = choice == Decision::Save;
        div()
            .id(label)
            .debug_selector(move || {
                match choice {
                    Decision::Save => "password-save",
                    Decision::No => "password-no",
                    Decision::Never => "password-never",
                }
                .into()
            })
            .w(px(width))
            .h(px(36.))
            .flex_none()
            .px(px(12.))
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(if primary { 0x474df2 } else { 0x676099 }))
            .bg(rgb(if primary { 0x3339f1 } else { 0x302f60 }))
            .text_size(px(13.))
            .line_height(px(18.))
            .text_color(rgb(0xffffff))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .cursor_pointer()
            .track_focus(&self.buttons[index])
            .focus(|style| style.bg(rgb(0x716be7)))
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.prevent_default()
            })
            .on_click(cx.listener(move |this, _, _, cx| this.choose(choice, cx)))
            .child(label)
            .when(choice != Decision::Never, |button| {
                button.child(
                    div()
                        .h(px(20.))
                        .w(px(if primary { 28. } else { 35. }))
                        .flex_none()
                        .rounded(px(4.))
                        .when(primary, |cap| {
                            cap.border_1()
                                .border_color(gpui::hsla(0., 0., 1., 0.28))
                                .bg(gpui::hsla(0., 0., 1., 0.02))
                        })
                        .when(!primary, |cap| {
                            cap.bg(rgb(0x4e4c77)).text_color(rgb(0xc4c2d7))
                        })
                        .text_size(px(10.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .line_height(px(14.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(primary, |cap| {
                            cap.child(icons::control_sized(Icon::Enter, 18.))
                        })
                        .when(!primary, |cap| cap.child("ESC")),
                )
            })
            .into_any_element()
    }
}
impl Render for PasswordPrompt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.closed {
            return div().into_any_element();
        }
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.55))
            .font_family(".SystemUIFont")
            .text_color(rgb(0xffffff))
            .text_size(px(14.))
            .line_height(px(20.))
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Accept, window, cx| this.accept(window, cx)))
            .on_action(cx.listener(|this, _: &Decline, _, cx| this.choose(Decision::No, cx)))
            .on_action(cx.listener(|this, _: &Activate, window, cx| {
                if this.selected(window).is_some() {
                    this.accept(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Next, window, _| this.move_focus(false, window)))
            .on_action(cx.listener(|this, _: &Previous, window, _| this.move_focus(true, window)))
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .child(
                div()
                    .id("password-save-sheet")
                    .debug_selector(|| "password-save-sheet".into())
                    .w(px(450.))
                    .max_w(window.viewport_size().width - px(20.))
                    .min_h(px(273.))
                    .rounded(px(18.))
                    .bg(rgb(0x161c2f))
                    .pt(px(38.))
                    .pb(px(28.))
                    .px(px(28.))
                    .flex()
                    .flex_col()
                    .child(div().px(px(10.)).child(key_icon()))
                    .child(
                        div()
                            .px(px(10.))
                            .mt(px(18.))
                            .text_size(px(18.))
                            .line_height(px(22.))
                            .font_family("SF Pro Display")
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(
                                div()
                                    .relative()
                                    .top(px(-1.))
                                    .child(format!("Save password for {}?", self.host)),
                            ),
                    )
                    .child(
                        div()
                            .px(px(10.))
                            .mt(px(12.))
                            .text_color(rgb(0xb8b8c0))
                            .child("That way, it’ll be auto-filled next time"),
                    )
                    .child(
                        div()
                            .mt(px(37.))
                            .h(px(36.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(self.button("Never on this site", 130., 0, Decision::Never, cx))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(self.button("No", 85., 1, Decision::No, cx))
                                    .child(self.button("Save", 89., 2, Decision::Save, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}
fn key_icon() -> impl IntoElement {
    div()
        .size(px(62.))
        .rounded(px(16.))
        .bg(rgb(0x393d57))
        .child(
            gpui::canvas(
                |_, _, _| (),
                |bounds, _, window, _| {
                    let at = |x: f32, y: f32| bounds.origin + gpui::point(px(x), px(y));
                    let circle = |x: f32, y: f32, r: f32, ink: gpui::Rgba, window: &mut Window| {
                        let mut p = gpui::PathBuilder::fill();
                        let k = r * 0.5522848;
                        p.move_to(at(x + r, y));
                        p.cubic_bezier_to(at(x, y + r), at(x + r, y + k), at(x + k, y + r));
                        p.cubic_bezier_to(at(x - r, y), at(x - k, y + r), at(x - r, y + k));
                        p.cubic_bezier_to(at(x, y - r), at(x - r, y - k), at(x - k, y - r));
                        p.cubic_bezier_to(at(x + r, y), at(x + k, y - r), at(x + r, y - k));
                        p.close();
                        if let Ok(p) = p.build() {
                            window.paint_path(p, ink);
                        }
                    };
                    circle(30.5, 22.5, 8.5, rgb(0xffffff), window);
                    let mut p = gpui::PathBuilder::fill();
                    for (i, (x, y)) in [
                        (28., 28.),
                        (33., 28.),
                        (33., 33.),
                        (37., 37.),
                        (33., 41.),
                        (36., 44.),
                        (32., 48.),
                        (28., 44.),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if i == 0 {
                            p.move_to(at(x, y));
                        } else {
                            p.line_to(at(x, y));
                        }
                    }
                    p.close();
                    if let Ok(p) = p.build() {
                        window.paint_path(p, rgb(0xffffff));
                    }
                    circle(30.5, 20.5, 2.5, rgb(0x393d57), window);
                },
            )
            .size_full(),
        )
}
