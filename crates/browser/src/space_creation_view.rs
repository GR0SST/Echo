//! Sidebar form only. Parent owns virtual-page visibility and the blank main
//! card. Return keeps the draft; Escape leaves the name field before cancelling.
use crate::{
    icons::{self, Icon},
    text_input::{self, TextInput},
};
use browser_core::space_creation::SpaceCreation;
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, Subscription, Window,
    actions, div, prelude::*, px, rgb,
};

actions!(space_creation_view, [Submit, Cancel, KeepDraft, EscapeForm]);

/// Arc 1.170 keeps a named draft on Return. Escape first leaves its input;
/// a subsequent Escape from the form root cancels it. No Return submission.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", KeepDraft, Some("SpaceCreationForm")),
        KeyBinding::new("escape", EscapeForm, Some("SpaceCreationForm")),
    ]);
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SpaceCreationEvent {
    NameChanged(String),
    ChooseIcon(SpaceCreation),
    ChooseProfile(SpaceCreation),
    ProfileInfo(SpaceCreation),
    ChooseTheme(SpaceCreation),
    Submit(SpaceCreation),
    Cancel,
}

pub(crate) struct SpaceCreationView {
    input: Entity<TextInput>,
    focus: FocusHandle,
    draft: SpaceCreation,
    profile_name: String,
    _changed: Subscription,
}

impl EventEmitter<SpaceCreationEvent> for SpaceCreationView {}
impl Focusable for SpaceCreationView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl SpaceCreationView {
    pub(crate) fn new(draft: &SpaceCreation, profile_name: &str, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new("Space name…", cx));
        input.update(cx, |input, cx| input.set_text(draft.name.clone(), cx));
        let changed = cx.subscribe(&input, |view, input, _: &text_input::Changed, cx| {
            view.draft.name = input.read(cx).text().to_owned();
            cx.emit(SpaceCreationEvent::NameChanged(view.draft.name.clone()));
            cx.notify();
        });
        Self {
            input,
            focus: cx.focus_handle(),
            draft: draft.clone(),
            profile_name: profile_name.into(),
            _changed: changed,
        }
    }

    pub(crate) fn focus_name(&self, window: &mut Window, cx: &App) {
        window.focus(&self.input.read(cx).focus_handle(cx));
    }

    /// Parent calls after an approved picker updates controller.edit_draft.
    /// Do not reset the text selection on unrelated profile/theme updates.
    pub(crate) fn sync_draft(
        &mut self,
        draft: &SpaceCreation,
        profile_name: &str,
        cx: &mut Context<Self>,
    ) {
        self.draft = draft.clone();
        self.profile_name = profile_name.into();
        if self.input.read(cx).text() != draft.name {
            self.input
                .update(cx, |input, cx| input.set_text(draft.name.clone(), cx));
        }
        cx.notify();
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        if !self.draft.name.trim().is_empty() {
            cx.emit(SpaceCreationEvent::Submit(self.draft.clone()));
        }
    }

    fn escape_form(&mut self, _: &EscapeForm, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.read(cx).focus_handle(cx).is_focused(window) {
            self.focus.focus(window);
            cx.notify();
        } else if self.focus.is_focused(window) {
            self.cancel(&Cancel, window, cx);
        }
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SpaceCreationEvent::Cancel);
    }
}

/// Original Echo stack artwork: three offset window outlines, no Arc assets.
fn illustration() -> impl IntoElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            for (x, y, width, height) in
                [(1., 1., 46., 31.), (9., 8., 46., 31.), (17., 15., 46., 31.)]
            {
                let mut path = gpui::PathBuilder::stroke(px(2.));
                for (index, (dx, dy)) in [
                    (x, y),
                    (x + width, y),
                    (x + width, y + height),
                    (x, y + height),
                    (x, y),
                    (x, y + 7.),
                    (x + width, y + 7.),
                ]
                .into_iter()
                .enumerate()
                {
                    let point = bounds.origin + gpui::point(px(dx), px(dy));
                    if index == 0 || index == 5 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, window.text_style().color);
                }
            }
        },
    )
    .w(px(64.))
    .h(px(48.))
}

/// Reuse the bundled, licensed Ionicons contours without changing the shared
/// icon API while another agent owns host integration.
fn form_icon(name: &'static str) -> gpui::AnyElement {
    type Contours = std::collections::BTreeMap<String, Vec<Vec<f32>>>;
    static ICONS: std::sync::LazyLock<Contours> = std::sync::LazyLock::new(|| {
        let mut catalog: Contours =
            serde_json::from_str(include_str!("../assets/ionicons/paths.json"))
                .expect("embedded Ionicons geometry is valid");
        ["person-outline", "brush-outline", "chevron-down-outline"]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    catalog.remove(name).expect("bundled form icon"),
                )
            })
            .collect()
    });
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let commands = &ICONS[name];
            let scale = f32::from(bounds.size.width) / 512.;
            let center = bounds.center();
            let at = |x, y| center + gpui::point(px((x - 256.) * scale), px((255. - y) * scale));
            let mut path = gpui::PathBuilder::fill();
            for command in commands {
                match command.as_slice() {
                    [0., x, y] => path.move_to(at(*x, *y)),
                    [1., x, y] => path.line_to(at(*x, *y)),
                    [2., cx, cy, x, y] => path.curve_to(at(*x, *y), at(*cx, *cy)),
                    [3., c1x, c1y, c2x, c2y, x, y] => {
                        path.cubic_bezier_to(at(*x, *y), at(*c1x, *c1y), at(*c2x, *c2y))
                    }
                    [4.] => path.close(),
                    _ => unreachable!("embedded contour command"),
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, window.text_style().color);
            }
        },
    )
    .size(px(12.))
    .flex_none()
    .into_any_element()
}

impl Render for SpaceCreationView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = !self.draft.name.trim().is_empty();
        div()
            .id("space-creation-form")
            .debug_selector(|| "space-creation-form".into())
            .key_context("SpaceCreationForm")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::escape_form))
            .on_action(cx.listener(|_, _: &KeepDraft, _, _| {}))
            .relative()
            .w_full()
            .h_full()
            .flex_none()
            .text_color(rgb(0x242334))
            .text_size(px(14.))
            .line_height(px(18.))
            .child(
                div()
                    .debug_selector(|| "space-creation-art".into())
                    .absolute()
                    .left(gpui::relative(0.5))
                    .ml(px(-32.))
                    .top(px(59.))
                    .w(px(64.))
                    .h(px(48.))
                    .child(illustration()),
            )
            .child(
                div()
                    .debug_selector(|| "space-creation-title".into())
                    .absolute()
                    .left_0()
                    .top(px(143.))
                    .w_full()
                    .h(px(22.))
                    .text_center()
                    .text_size(px(18.))
                    .line_height(px(22.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Create a Space"),
            )
            .child(
                div()
                    .debug_selector(|| "space-creation-subtitle".into())
                    .absolute()
                    .left_0()
                    .top(px(175.))
                    .w_full()
                    .h(px(36.))
                    .text_center()
                    .child(div().child("Separate your tabs for life,"))
                    .child(div().child("work, projects, and more.")),
            )
            .child(
                div()
                    .id("space-creation-name")
                    .debug_selector(|| "space-creation-name".into())
                    .absolute()
                    .left(px(14.))
                    .top(px(237.))
                    .right(px(13.))
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .rounded(px(7.))
                    .bg(gpui::rgba(0xffffff66))
                    .child(
                        div()
                            .id("space-creation-icon")
                            .debug_selector(|| "space-creation-icon".into())
                            .w(px(36.))
                            .h_full()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _, _, cx| {
                                cx.emit(SpaceCreationEvent::ChooseIcon(view.draft.clone()))
                            }))
                            .child(match &self.draft.custom_icon {
                                Some(icon) => icons::glyph(icon),
                                None => div()
                                    .debug_selector(|| "space-creation-add-icon".into())
                                    .size(px(18.))
                                    .border_1()
                                    .border_dashed()
                                    .rounded(px(3.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icons::control_sized(Icon::Plus, 12.))
                                    .into_any_element(),
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pr(px(8.))
                            .child(self.input.clone()),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "space-creation-profile-row".into())
                    .absolute()
                    .left(px(14.))
                    .top(px(279.))
                    .right(px(13.))
                    .h(px(37.))
                    .rounded(px(7.))
                    .bg(gpui::rgba(0xffffff66))
                    .child(
                        div()
                            .debug_selector(|| "space-creation-profile-icon".into())
                            .absolute()
                            .left(px(11.))
                            .top(px(12.5))
                            .size(px(12.))
                            .child(form_icon("person-outline")),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(36.))
                            .right(px(29.))
                            .top_0()
                            .h_full()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .debug_selector(|| "space-creation-profile-label".into())
                                    .flex_none()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Profile"),
                            )
                            .child(
                                div()
                                    .id("space-creation-profile")
                                    .debug_selector(|| "space-creation-profile".into())
                                    .min_w_0()
                                    .max_w(px(160.))
                                    .h(px(28.))
                                    .px(px(8.))
                                    .rounded(px(6.))
                                    .bg(gpui::rgba(0xffffff66))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        cx.emit(SpaceCreationEvent::ChooseProfile(
                                            view.draft.clone(),
                                        ))
                                    }))
                                    .child(
                                        div()
                                            .debug_selector(|| {
                                                "space-creation-profile-value".into()
                                            })
                                            .min_w_0()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .text_size(px(13.))
                                            .child(self.profile_name.clone()),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(|| {
                                                "space-creation-profile-chevron".into()
                                            })
                                            .size(px(12.))
                                            .flex_none()
                                            .child(form_icon("chevron-down-outline")),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("space-creation-profile-info")
                            .debug_selector(|| "space-creation-profile-info".into())
                            .absolute()
                            .right(px(1.))
                            .top(px(7.5))
                            .size(px(22.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _, _, cx| {
                                cx.emit(SpaceCreationEvent::ProfileInfo(view.draft.clone()))
                            }))
                            .child(icons::control(Icon::Info)),
                    ),
            )
            .child(
                div()
                    .id("space-creation-theme")
                    .debug_selector(|| "space-creation-theme".into())
                    .absolute()
                    .left(px(14.))
                    .top(px(323.))
                    .right(px(13.))
                    .h(px(37.))
                    .rounded(px(7.))
                    .bg(gpui::rgba(0xffffff66))
                    .cursor_pointer()
                    .on_click(cx.listener(|view, _, _, cx| {
                        cx.emit(SpaceCreationEvent::ChooseTheme(view.draft.clone()))
                    }))
                    .child(
                        div()
                            .debug_selector(|| "space-creation-theme-icon".into())
                            .absolute()
                            .left(px(11.))
                            .top(px(12.5))
                            .size(px(12.))
                            .child(form_icon("brush-outline")),
                    )
                    .child(
                        div()
                            .debug_selector(|| "space-creation-theme-label".into())
                            .absolute()
                            .left(px(36.))
                            .right(px(8.))
                            .top_0()
                            .h_full()
                            .flex()
                            .items_center()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Choose a Theme"),
                    ),
            )
            .child(
                div()
                    .id("space-creation-submit")
                    .debug_selector(|| "space-creation-submit".into())
                    .absolute()
                    .left(px(14.))
                    .bottom(px(96.))
                    .right(px(13.))
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .bg(if enabled {
                        rgb(0x363344)
                    } else {
                        rgb(0xd0ced7)
                    })
                    .text_color(if enabled {
                        rgb(0xffffff)
                    } else {
                        rgb(0x7d7989)
                    })
                    .when(enabled, |button| button.cursor_pointer())
                    .on_click(cx.listener(|view, _, window, cx| view.submit(&Submit, window, cx)))
                    .child("Create Space"),
            )
            .child(
                div()
                    .id("space-creation-cancel")
                    .debug_selector(|| "space-creation-cancel".into())
                    .absolute()
                    .left(px(14.))
                    .bottom(px(62.))
                    .right(px(13.))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(cx.listener(|view, _, window, cx| view.cancel(&Cancel, window, cx)))
                    .child("Cancel"),
            )
    }
}

#[cfg(test)]
#[path = "space_creation_view_tests.rs"]
mod tests;
